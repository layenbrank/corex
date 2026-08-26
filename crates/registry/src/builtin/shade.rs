//! `shade.convert` — image format conversion / compression.

use crate::builtin::util::{
    confine_path, ensure_parent, opt_i64, opt_str, require_map, require_path,
};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{DynamicImage, ImageEncoder, ImageFormat};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tiff", "tif"];

pub struct ShadeConvert;

#[async_trait]
impl Action for ShadeConvert {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "shade.convert",
            "Shade Convert",
            "图片格式转换与压缩",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("format", SchemaType::Str, false),
            ParamSchema::new("quality", SchemaType::Int, false).with_default(100),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = confine_path(ctx, &require_path(map, "from")?)?;
        let to = confine_path(ctx, &require_path(map, "to")?)?;
        let quality = opt_i64(map, "quality", 100).clamp(1, 100) as u8;
        let format_hint = opt_str(map, "format");

        let is_single = from.is_file();
        let entries: Vec<PathBuf> = if is_single {
            vec![from.clone()]
        } else if from.is_dir() {
            WalkDir::new(&from)
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
            return Err(ActionError::execution(format!(
                "源路径不存在: {}",
                from.display()
            )));
        };

        if entries.is_empty() {
            return Err(ActionError::execution("没有找到图片文件"));
        }

        let out_format = if let Some(ref fmt) = format_hint {
            parse_format(fmt)?
        } else if is_single {
            to.extension()
                .and_then(|e| e.to_str())
                .map(parse_format)
                .transpose()?
                .unwrap_or(ImageFormat::Png)
        } else {
            ImageFormat::Png
        };

        if is_single {
            ensure_parent(&to)?;
        } else {
            std::fs::create_dir_all(&to)?;
        }

        let mut count = 0i64;
        for entry_path in &entries {
            let out_path = if is_single {
                to.clone()
            } else {
                let rel = entry_path.strip_prefix(&from).unwrap_or(entry_path);
                let new_name = rel.with_extension(format_ext(&out_format));
                to.join(new_name)
            };
            ensure_parent(&out_path)?;
            let img = image::open(entry_path)
                .map_err(|e| ActionError::execution(format!("打开图片失败: {e}")))?;
            save_image(&img, &out_path, &out_format, quality)?;
            count += 1;
        }

        let mut out = BTreeMap::new();
        out.insert("path".into(), Value::File(to));
        out.insert("count".into(), Value::Int(count));
        Ok(Value::Map(out))
    }
}

fn save_image(
    img: &DynamicImage,
    path: &Path,
    format: &ImageFormat,
    quality: u8,
) -> Result<(), ActionError> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    match format {
        ImageFormat::Jpeg => {
            let encoder = JpegEncoder::new_with_quality(writer, quality);
            encoder
                .write_image(
                    img.as_bytes(),
                    img.width(),
                    img.height(),
                    img.color().into(),
                )
                .map_err(|e| ActionError::execution(format!("JPEG 编码失败: {e}")))?;
        }
        ImageFormat::WebP => {
            let encoder = WebPEncoder::new_lossless(writer);
            encoder
                .write_image(
                    img.as_bytes(),
                    img.width(),
                    img.height(),
                    img.color().into(),
                )
                .map_err(|e| ActionError::execution(format!("WebP 编码失败: {e}")))?;
        }
        ImageFormat::Png => {
            let encoder = PngEncoder::new(writer);
            encoder
                .write_image(
                    img.as_bytes(),
                    img.width(),
                    img.height(),
                    img.color().into(),
                )
                .map_err(|e| ActionError::execution(format!("PNG 编码失败: {e}")))?;
        }
        _ => {
            img.save_with_format(path, *format)
                .map_err(|e| ActionError::execution(format!("保存失败: {e}")))?;
        }
    }
    Ok(())
}

fn parse_format(s: &str) -> Result<ImageFormat, ActionError> {
    match s.to_lowercase().as_str() {
        "png" => Ok(ImageFormat::Png),
        "jpg" | "jpeg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::WebP),
        "bmp" => Ok(ImageFormat::Bmp),
        "gif" => Ok(ImageFormat::Gif),
        "tiff" | "tif" => Ok(ImageFormat::Tiff),
        other => Err(ActionError::InvalidParams(format!(
            "不支持的图片格式: {other}"
        ))),
    }
}

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

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ShadeConvert));
}
