use std::{
    fs::{File, create_dir_all},
    io::BufWriter,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{DynamicImage, ImageEncoder, ImageFormat};
use walkdir::WalkDir;

use crate::shade::schema::Args;
use crate::utils::{file, notify, progress};

pub fn run(args: &Args) -> Result<()> {
    match image_task(args) {
        Ok(_) => {
            let _ = notify::success("图片处理成功", "图片处理操作已成功完成");
        }
        Err(e) => {
            let _ = notify::error("图片处理失败", &format!("图片处理过程中发生错误: {}", e));
            return Err(e);
        }
    }
    Ok(())
}

// ─── 图片处理 ────────────────────────────────────────────────────────────────

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tiff", "tif"];

/// 图片格式转换与压缩
pub fn image_task(args: &Args) -> Result<()> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    let is_single_file = from.is_file();

    // 收集需要处理的图片文件
    let entries: Vec<PathBuf> = if is_single_file {
        vec![from.to_path_buf()]
    } else if from.is_dir() {
        WalkDir::new(from)
            .min_depth(1)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file()
                    && e.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
                        .unwrap_or(false)
            })
            .map(|e| e.path().to_path_buf())
            .collect()
    } else {
        anyhow::bail!("源路径不存在: {}", from.display());
    };

    let file_count = entries.len();
    if file_count == 0 {
        println!("没有找到图片文件");
        return Ok(());
    }

    println!("找到 {} 个图片文件", file_count);

    // 确定输出格式
    let out_format = if let Some(ref fmt) = args.format {
        parse_format(fmt)?
    } else if is_single_file {
        to.extension()
            .and_then(|e| e.to_str())
            .map(|e| parse_format(e))
            .transpose()?
            .unwrap_or(ImageFormat::Png)
    } else {
        // 目录模式：格式参数必须指定，否则默认 PNG
        args.format
            .as_ref()
            .map(|f| parse_format(f))
            .transpose()?
            .unwrap_or(ImageFormat::Png)
    };

    // 确保输出目录存在
    if is_single_file {
        if let Some(parent) = to.parent() {
            create_dir_all(parent)?;
        }
    } else {
        create_dir_all(to)?;
    }

    let pb = progress::progress(file_count as u64);
    pb.set_message("正在处理图片...");
    pb.tick();
    let start = Instant::now();
    let mut total_bytes: u64 = 0;

    for entry_path in &entries {
        if let Some(name) = entry_path.file_name() {
            pb.set_message(file::truncate(&name.to_string_lossy(), 30));
        }

        let out_path = if is_single_file {
            to.to_path_buf()
        } else {
            let rel = entry_path.strip_prefix(from).unwrap_or(entry_path);
            let new_name = rel.with_extension(format_ext(&out_format));
            to.join(&new_name)
        };

        if let Some(parent) = out_path.parent() {
            create_dir_all(parent)?;
        }

        let img = image::open(entry_path)
            .with_context(|| format!("打开图片: {}", entry_path.display()))?;

        save_image(&img, &out_path, &out_format, args.quality)
            .with_context(|| format!("保存图片: {}", out_path.display()))?;

        total_bytes += out_path.metadata().map(|m| m.len()).unwrap_or(0);
        pb.inc(1);
    }

    let elapsed = start.elapsed();
    let avg_speed = file::speed(total_bytes, elapsed);
    pb.finish_with_message(format!(
        "处理 {} 张图片, {}, 用时 {}, 平均 {}/s",
        file_count,
        file::size(total_bytes),
        file::duration(elapsed),
        file::size(avg_speed)
    ));

    Ok(())
}

/// 保存图片到指定路径，根据格式和质量参数编码
///
/// - JPEG: quality 控制压缩质量 (1-100)
/// - WebP: image 0.25 仅支持无损编码，quality 参数忽略
/// - PNG:  始终无损，quality 参数忽略
fn save_image(img: &DynamicImage, path: &Path, format: &ImageFormat, quality: u8) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);

    match format {
        ImageFormat::Jpeg => {
            let encoder = JpegEncoder::new_with_quality(writer, quality);
            encoder.write_image(
                img.as_bytes(),
                img.width(),
                img.height(),
                img.color().into(),
            )?;
        }
        ImageFormat::WebP => {
            // image 0.25 的 WebPEncoder 仅支持无损编码
            let encoder = WebPEncoder::new_lossless(writer);
            encoder.write_image(
                img.as_bytes(),
                img.width(),
                img.height(),
                img.color().into(),
            )?;
        }
        ImageFormat::Png => {
            let encoder = PngEncoder::new(writer);
            encoder.write_image(
                img.as_bytes(),
                img.width(),
                img.height(),
                img.color().into(),
            )?;
        }
        _ => {
            // BMP / GIF / TIFF 等使用默认编码器
            img.save_with_format(path, format.clone())
                .with_context(|| format!("保存 {:?} 格式", format))?;
        }
    }

    Ok(())
}

/// 解析格式字符串为 ImageFormat
fn parse_format(s: &str) -> Result<ImageFormat> {
    match s.to_lowercase().as_str() {
        "png" => Ok(ImageFormat::Png),
        "jpg" | "jpeg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::WebP),
        "bmp" => Ok(ImageFormat::Bmp),
        "gif" => Ok(ImageFormat::Gif),
        "tiff" | "tif" => Ok(ImageFormat::Tiff),
        _ => anyhow::bail!("不支持的图片格式: {}（支持 png/jpg/webp/bmp/gif/tiff）", s),
    }
}

/// ImageFormat -> 文件扩展名
fn format_ext(format: &ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::WebP => "webp",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Gif => "gif",
        ImageFormat::Tiff => "tiff",
        _ => "png",
    }
}
