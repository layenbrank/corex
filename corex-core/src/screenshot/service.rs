use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result, bail};
use arboard::ImageData;
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageReader, RgbaImage, imageops};
use serde_json::Value;
use xcap::{Monitor, Window};

use crate::screenshot::schema::{
    Args, CaptureArgs, ClipboardArgs, CropArgs, MonitorInfo, WindowInfo,
};
use crate::utils::paths::{validate_output_dir, validate_read_file, validate_read_path};

#[derive(Debug, Clone)]
pub struct Output {
    pub path: Option<PathBuf>,
    pub data: Option<Value>,
}

pub fn run(args: &Args) -> Result<()> {
    let output = execute(args, None)?;
    match &output.path {
        Some(p) => {
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis();
            println!("📸 截图时间戳: {timestamp}");
            println!("✅ 截图已保存: {}", p.display());
        }
        None => {}
    }
    if let Some(data) = &output.data {
        println!("{}", serde_json::to_string_pretty(data)?);
    }
    Ok(())
}

pub fn execute(args: &Args, cached_monitors: Option<&[Monitor]>) -> Result<Output> {
    match args {
        Args::Capture(a) => {
            validate_read_path(&a.to)?;
            let path = capture(a, cached_monitors)?;
            Ok(Output {
                path: Some(path),
                data: None,
            })
        }
        Args::Monitors => {
            let monitors = list_monitors(cached_monitors)?;
            Ok(Output {
                path: None,
                data: Some(serde_json::to_value(monitors)?),
            })
        }
        Args::Windows => Ok(Output {
            path: None,
            data: Some(serde_json::to_value(list_windows()?)?),
        }),
        Args::Crop(a) => {
            let path = crop_and_save(a)?;
            Ok(Output {
                path: Some(path),
                data: None,
            })
        }
        Args::Clipboard(a) => {
            copy_crop_to_clipboard(a)?;
            Ok(Output {
                path: None,
                data: None,
            })
        }
    }
}

pub fn capture(args: &CaptureArgs, cached_monitors: Option<&[Monitor]>) -> Result<PathBuf> {
    let to = Path::new(&args.to);
    let start = Instant::now();
    let owned_monitors;
    let monitors = match cached_monitors {
        Some(monitors) if !monitors.is_empty() => monitors,
        _ => {
            owned_monitors = Monitor::all().map_err(|e| anyhow::anyhow!(e.to_string()))?;
            &owned_monitors
        }
    };
    validate_output_dir(&args.to)?;
    let target = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or_else(|| anyhow::anyhow!("没有找到可用显示器"))?;
    let image = target
        .capture_image()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_millis();
    let monitor_name = normalized(
        target
            .friendly_name()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );
    let filename = format!("screenshot-{monitor_name}-{timestamp}.png");
    let output_path = to.join(&filename);
    image
        .save(&output_path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    eprintln!(
        "screenshot saved: {} ({:?})",
        output_path.display(),
        start.elapsed()
    );
    Ok(output_path)
}

pub fn list_monitors(cached_monitors: Option<&[Monitor]>) -> Result<Vec<MonitorInfo>> {
    let owned;
    let monitors = match cached_monitors {
        Some(m) if !m.is_empty() => m,
        _ => {
            owned = Monitor::all().map_err(|e| anyhow::anyhow!(e.to_string()))?;
            &owned
        }
    };
    Ok(monitors.iter().filter_map(to_monitor_info).collect())
}

pub fn list_windows() -> Result<Vec<WindowInfo>> {
    let windows = Window::all().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(windows
        .into_iter()
        .filter_map(|w| {
            let title = w.title().ok().unwrap_or_default();
            let app_name = w.app_name().ok().unwrap_or_default();
            let is_minimized = w.is_minimized().ok().unwrap_or(false);
            let x = w.x().ok()?;
            let y = w.y().ok()?;
            let width = w.width().ok()?;
            let height = w.height().ok()?;
            if is_minimized || width < 10 || height < 10 || title.is_empty() {
                return None;
            }
            Some(WindowInfo {
                id: w.id().ok()?,
                title,
                app_name,
                x,
                y,
                width,
                height,
                is_minimized,
            })
        })
        .collect())
}

pub fn crop_and_save(args: &CropArgs) -> Result<PathBuf> {
    validate_read_file(&args.source)?;
    validate_output_dir(&args.to)?;
    let (rgba, w, h) = load_cropped_rgba(args)?;
    let save_dir = Path::new(&args.to);
    let save_path = save_dir.join(format!(
        "screenshot-crop-{}.png",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis()
    ));
    let img = RgbaImage::from_raw(w, h, rgba).ok_or_else(|| anyhow::anyhow!("无效 RGBA 尺寸"))?;
    img.save(&save_path)?;
    Ok(save_path)
}

pub fn copy_crop_to_clipboard(args: &ClipboardArgs) -> Result<()> {
    validate_read_file(&args.source)?;
    let crop = CropArgs {
        source: args.source.clone(),
        to: String::new(),
        x: args.x,
        y: args.y,
        w: args.w,
        h: args.h,
        image_file: args.image_file.clone(),
        final_image_base64: args.final_image_base64.clone(),
    };
    let (rgba, w, h) = load_cropped_rgba(&crop)?;
    let expected = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| anyhow::anyhow!("无效图片尺寸"))?;
    if rgba.len() != expected {
        bail!(
            "RGBA 字节数 {actual} 与 {w}x{h} 不匹配",
            actual = rgba.len()
        );
    }
    let mut clipboard = arboard::Clipboard::new().context("打开剪贴板失败")?;
    clipboard
        .set_image(ImageData {
            width: w as usize,
            height: h as usize,
            bytes: rgba.into(),
        })
        .context("写入剪贴板失败")?;
    Ok(())
}

fn load_cropped_rgba(args: &CropArgs) -> Result<(Vec<u8>, u32, u32)> {
    if args.image_file.is_some() && args.final_image_base64.is_some() {
        bail!("请只指定 image_file 或 final_image_base64 之一");
    }
    if args.image_file.is_some() && (args.x != 0 || args.y != 0 || args.w != 0 || args.h != 0) {
        bail!("指定 image_file 时忽略 x/y/w/h 裁剪坐标");
    }
    if let Some(ref path) = args.image_file {
        validate_read_file(path)?;
        let img = ImageReader::open(path)
            .with_context(|| format!("打开图片失败: {path}"))?
            .decode()?;
        let rgba = img.to_rgba8();
        let (cw, ch) = rgba.dimensions();
        return Ok((rgba.into_raw(), cw, ch));
    }
    if let Some(ref b64) = args.final_image_base64 {
        let bytes = STANDARD.decode(b64).context("base64 解码失败")?;
        let img = image::load_from_memory(&bytes)?;
        let rgba = img.to_rgba8();
        let (cw, ch) = rgba.dimensions();
        return Ok((rgba.into_raw(), cw, ch));
    }
    if args.w == 0 || args.h == 0 {
        bail!("裁剪宽高 w/h 必须大于 0");
    }
    validate_read_file(&args.source)?;
    let img = ImageReader::open(&args.source)
        .with_context(|| format!("打开源图失败: {}", args.source))?
        .decode()?;
    let cropped = imageops::crop_imm(&img, args.x, args.y, args.w, args.h).to_image();
    let (cw, ch) = cropped.dimensions();
    Ok((cropped.into_raw(), cw, ch))
}

fn to_monitor_info(m: &Monitor) -> Option<MonitorInfo> {
    Some(MonitorInfo {
        id: m.id().ok()?,
        name: m.friendly_name().unwrap_or_else(|_| {
            m.name()
                .unwrap_or_else(|_| format!("Monitor-{}", m.id().unwrap_or(0)))
        }),
        x: m.x().ok()?,
        y: m.y().ok()?,
        width: m.width().ok()?,
        height: m.height().ok()?,
        is_primary: m.is_primary().unwrap_or(false),
    })
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_rejects_dual_image_sources() {
        let args = CropArgs {
            source: "a.png".to_string(),
            to: "out".to_string(),
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            image_file: Some("b.png".to_string()),
            final_image_base64: Some("abc".to_string()),
        };
        assert!(load_cropped_rgba(&args).is_err());
    }
}
