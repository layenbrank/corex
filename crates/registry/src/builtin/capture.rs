//! 截图类动作——不支持的平台上软失败；能裁剪 / 进剪贴板的就做。

#[path = "capture_match.rs"]
mod match_img;

use crate::ActionRegistry;
use crate::builtin::util::{
    confine_path, ensure_parent, opt_i64, opt_str, require_map, require_path,
};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Unit, Value,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct CaptureScreenshot;
pub struct CaptureCrop;
pub struct CaptureMonitors;
pub struct CaptureOcr;
pub struct CaptureFind;

#[async_trait]
impl Action for CaptureScreenshot {
    fn permissions(&self) -> PermissionSet {
        // 会驱动屏幕并把 PNG 写到磁盘。
        PermissionSet::CAPTURE.union(PermissionSet::FILESYSTEM)
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("capture.screenshot", "截图", "截取主显示器画面", Bucket::Ui).with_params(
            vec![
                ParamSchema::new("to", SchemaType::File, true),
                ParamSchema::new("format", SchemaType::Str, false).with_default("png"),
                ParamSchema::new("quality", SchemaType::Int, false).with_default(90),
            ],
        )
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let mut map = require_map(&params)?.clone();
        let to = confine_path(ctx, &require_path(&map, "to")?)?;
        map.insert("to".into(), Value::Str(to.to_string_lossy().into()));
        capture_screenshot_impl(Value::Map(map)).await
    }
}

#[async_trait]
impl Action for CaptureCrop {
    fn permissions(&self) -> PermissionSet {
        // 两张图都是调用方已有的文件；完全不碰屏幕。
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("capture.crop", "裁剪图片", "裁剪图片文件并写出", Bucket::Ui).with_params(
            vec![
                ParamSchema::new("from", SchemaType::File, true),
                ParamSchema::new("to", SchemaType::File, true),
                ParamSchema::new("x", SchemaType::Int, true),
                ParamSchema::new("y", SchemaType::Int, true),
                ParamSchema::new("width", SchemaType::Int, true),
                ParamSchema::new("height", SchemaType::Int, true),
            ],
        )
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = confine_path(ctx, &require_path(map, "from")?)?;
        let to = confine_path(ctx, &require_path(map, "to")?)?;
        let x = opt_i64(map, "x", 0).max(0) as u32;
        let y = require_i64(map, "y")? as u32;
        let width = require_i64(map, "width")? as u32;
        let height = require_i64(map, "height")? as u32;

        let img =
            image::open(&from).map_err(|e| ActionError::execution(format!("打开图片失败: {e}")))?;
        let cropped = image::imageops::crop_imm(&img, x, y, width, height).to_image();
        ensure_parent(&to)?;
        cropped
            .save(&to)
            .map_err(|e| ActionError::execution(format!("保存裁剪图失败: {e}")))?;
        Ok(Value::File(to))
    }
}

fn require_i64(map: &BTreeMap<String, Value>, key: &str) -> Result<i64, ActionError> {
    map.get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam(key.into()))
}

#[async_trait]
impl Action for CaptureMonitors {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::CAPTURE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.monitors",
            "枚举显示器",
            "列出显示器信息",
            Bucket::Ui,
        )
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        capture_monitors_impl().await
    }
}

#[async_trait]
impl Action for CaptureOcr {
    fn permissions(&self) -> PermissionSet {
        // 对调用方指定的图片文件做 OCR；从不读取屏幕。
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.ocr",
            "识别文字（OCR）",
            "识别图片中的文字",
            Bucket::Ui,
        )
        .with_params(vec![
            ParamSchema::new("file", SchemaType::File, true),
            ParamSchema::new("language", SchemaType::Str, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let mut map = require_map(&params)?.clone();
        let file = confine_path(ctx, &require_path(&map, "file")?)?;
        map.insert("file".into(), Value::Str(file.to_string_lossy().into()));
        capture_ocr_impl(Value::Map(map)).await
    }
}

async fn capture_ocr_impl(params: Value) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        capture_ocr_windows(params).await
    }
    #[cfg(not(windows))]
    {
        let _ = params;
        Err(ActionError::execution(
            "capture.ocr 在当前平台不可用（需要 Windows Media OCR 后端）",
        ))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CaptureScreenshot));
    registry.register(Arc::new(CaptureCrop));
    registry.register(Arc::new(CaptureMonitors));
    registry.register(Arc::new(CaptureOcr));
    registry.register(Arc::new(CaptureFind));
}

#[async_trait]
impl Action for CaptureFind {
    fn permissions(&self) -> PermissionSet {
        // 从磁盘读 haystack 与 needle；匹配本身只是算术。
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.find",
            "模板匹配",
            "在大图中查找模板（灰度 NCC）",
            Bucket::Ui,
        )
        .with_params(vec![
            ParamSchema::new("haystack", SchemaType::File, true),
            ParamSchema::new("needle", SchemaType::File, true),
            ParamSchema::new("threshold", SchemaType::Float, false).with_default(0.9),
            ParamSchema::new("step", SchemaType::Int, false).with_default(2),
            ParamSchema::new("x", SchemaType::Int, false),
            ParamSchema::new("y", SchemaType::Int, false),
            ParamSchema::new("width", SchemaType::Int, false),
            ParamSchema::new("height", SchemaType::Int, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let haystack = confine_path(ctx, &require_path(map, "haystack")?)?;
        let needle = confine_path(ctx, &require_path(map, "needle")?)?;
        let threshold = map.get("threshold").and_then(|v| v.as_f64()).unwrap_or(0.9);
        let step = opt_i64(map, "step", 2).max(1) as u32;
        let region = match (
            map.get("x").and_then(|v| v.as_i64()),
            map.get("y").and_then(|v| v.as_i64()),
            map.get("width").and_then(|v| v.as_i64()),
            map.get("height").and_then(|v| v.as_i64()),
        ) {
            (Some(x), Some(y), Some(w), Some(h)) if w > 0 && h > 0 => {
                Some((x.max(0) as u32, y.max(0) as u32, w as u32, h as u32))
            }
            _ => None,
        };

        // 匹配跑在阻塞线程上，那里借不到 `ctx`：先把上报句柄取出来一起搬进去。
        let report = ctx.reporter();
        tokio::task::spawn_blocking(move || {
            let hay = image::open(&haystack)
                .map_err(|e| ActionError::execution(format!("打开 haystack 失败: {e}")))?;
            let ndl = image::open(&needle)
                .map_err(|e| ActionError::execution(format!("打开 needle 失败: {e}")))?;
            let hay_g = match_img::to_gray(hay);
            let ndl_g = match_img::to_gray(ndl);
            let mut on_row = |done: u32, rows: u32| {
                if let Some(report) = &report {
                    report.chunk(done as u64, Some(rows as u64), Unit::Items);
                }
            };
            let m = match_img::find_template(&hay_g, &ndl_g, region, step, threshold, &mut on_row)?;
            let mut out = BTreeMap::new();
            out.insert("found".into(), Value::Bool(m.found));
            out.insert("score".into(), Value::Float(m.score));
            out.insert("x".into(), Value::Int(m.x as i64));
            out.insert("y".into(), Value::Int(m.y as i64));
            out.insert("width".into(), Value::Int(m.width as i64));
            out.insert("height".into(), Value::Int(m.height as i64));
            Ok(Value::Map(out))
        })
        .await
        .map_err(|e| ActionError::execution(format!("capture.find 失败: {e}")))?
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::collections::BTreeMap;

    pub async fn capture_screenshot(params: Value) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let to = require_path(map, "to")?;
        let format = opt_str(map, "format").unwrap_or_else(|| "png".into());
        ensure_parent(&to)?;
        let to_path = to.clone();
        let _format = format;
        tokio::task::spawn_blocking(move || screenshot_to_file(&to_path, &_format))
            .await
            .map_err(|e| ActionError::execution(format!("截图任务失败: {e}")))??;
        Ok(Value::File(to))
    }

    fn screenshot_to_file(to: &std::path::Path, _format: &str) -> Result<(), ActionError> {
        use screenshots::Screen;
        let screens =
            Screen::all().map_err(|e| ActionError::execution(format!("枚举显示器失败: {e}")))?;
        let screen = screens
            .into_iter()
            .next()
            .ok_or_else(|| ActionError::execution("未找到显示器"))?;
        let img = screen
            .capture()
            .map_err(|e| ActionError::execution(format!("截图失败: {e}")))?;
        img.save(to)
            .map_err(|e| ActionError::execution(format!("保存截图失败: {e}")))?;
        Ok(())
    }

    pub async fn capture_monitors() -> Result<Value, ActionError> {
        tokio::task::spawn_blocking(|| {
            use screenshots::Screen;
            let screens = Screen::all()
                .map_err(|e| ActionError::execution(format!("枚举显示器失败: {e}")))?;
            let monitors: Vec<Value> = screens
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let mut m = BTreeMap::new();
                    m.insert("index".into(), Value::Int(i as i64));
                    m.insert("width".into(), Value::Int(s.display_info.width as i64));
                    m.insert("height".into(), Value::Int(s.display_info.height as i64));
                    Value::Map(m)
                })
                .collect();
            let mut out = BTreeMap::new();
            out.insert("monitors".into(), Value::Array(monitors));
            Ok(Value::Map(out))
        })
        .await
        .map_err(|e| ActionError::execution(format!("枚举任务失败: {e}")))?
    }

    pub async fn capture_ocr_windows(params: Value) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let file = require_path(map, "file")?;
        let lang = opt_str(map, "language");
        // WinRT 的 OCR 类型通常不是 Send；COM 操作全部留在同一个阻塞线程上。
        tokio::task::spawn_blocking(move || ocr_file(&file, lang.as_deref()))
            .await
            .map_err(|e| ActionError::execution(format!("OCR 任务失败: {e}")))?
    }

    fn ocr_file(path: &std::path::Path, _language: Option<&str>) -> Result<Value, ActionError> {
        use windows::Graphics::Imaging::BitmapDecoder;
        use windows::Media::Ocr::OcrEngine;
        use windows::Storage::FileAccessMode;
        use windows::Storage::StorageFile;
        use windows::core::HSTRING;

        let path_str = path.to_string_lossy();
        let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path_str.as_ref()))
            .map_err(|e| ActionError::execution(format!("打开文件失败: {e}")))?
            .join()
            .map_err(|e| ActionError::execution(format!("GetFileFromPathAsync 失败: {e}")))?;
        let stream = file
            .OpenAsync(FileAccessMode::Read)
            .map_err(|e| ActionError::execution(format!("OpenAsync 失败: {e}")))?
            .join()
            .map_err(|e| ActionError::execution(format!("OpenAsync 等待失败: {e}")))?;
        let decoder = BitmapDecoder::CreateAsync(&stream)
            .map_err(|e| ActionError::execution(format!("BitmapDecoder 失败: {e}")))?
            .join()
            .map_err(|e| ActionError::execution(format!("BitmapDecoder 等待失败: {e}")))?;
        let software = decoder
            .GetSoftwareBitmapAsync()
            .map_err(|e| ActionError::execution(format!("GetSoftwareBitmap 失败: {e}")))?
            .join()
            .map_err(|e| ActionError::execution(format!("GetSoftwareBitmap 等待失败: {e}")))?;
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .map_err(|e| ActionError::execution(format!("OcrEngine 失败: {e}")))?;
        let result = engine
            .RecognizeAsync(&software)
            .map_err(|e| ActionError::execution(format!("RecognizeAsync 失败: {e}")))?
            .join()
            .map_err(|e| ActionError::execution(format!("OCR 识别失败: {e}")))?;
        let text = result
            .Text()
            .map_err(|e| ActionError::execution(format!("读取 OCR 文本失败: {e}")))?
            .to_string();
        // 行几何 API 随 windows crate 版本变动；目前只暴露文本与空行。
        let lines = vec![Value::Map({
            let mut lm = BTreeMap::new();
            lm.insert("text".into(), Value::Str(text.clone()));
            lm
        })];
        let mut out = BTreeMap::new();
        out.insert("text".into(), Value::Str(text));
        out.insert("lines".into(), Value::Array(lines));
        Ok(Value::Map(out))
    }
}

#[cfg(windows)]
use win::capture_monitors as capture_monitors_impl;
#[cfg(windows)]
use win::capture_ocr_windows;
#[cfg(windows)]
use win::capture_screenshot as capture_screenshot_impl;

#[cfg(not(windows))]
async fn capture_screenshot_impl(_params: Value) -> Result<Value, ActionError> {
    Err(ActionError::execution(
        "capture.screenshot 在当前平台不可用（需要原生截图后端）",
    ))
}

#[cfg(not(windows))]
async fn capture_monitors_impl() -> Result<Value, ActionError> {
    Err(ActionError::execution(
        "capture.monitors 在当前平台不可用（需要原生截图后端）",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::util::probe::Probe;
    use corex_core::Observer;
    use image::{GrayImage, Luma};
    use std::sync::Arc;
    use tempfile::tempdir;

    const NEEDLE: u32 = 24;

    /// 在 `img` 的 `(ox, oy)` 处画一块棋盘：纯色块没有方差，NCC 恒为 0，匹配不上。
    fn stamp(img: &mut GrayImage, ox: u32, oy: u32) {
        for j in 0..NEEDLE {
            for i in 0..NEEDLE {
                let v = if (i + j) % 2 == 0 { 50 } else { 250 };
                img.put_pixel(ox + i, oy + j, Luma([v]));
            }
        }
    }

    /// 模板匹配是唯一真的能算进度的地方：报的是「扫到第几行」，分母是总行数。
    #[tokio::test]
    async fn find_reports_scanned_rows() {
        let dir = tempdir().unwrap();
        let hay_path = dir.path().join("hay.png");
        let needle_path = dir.path().join("needle.png");

        let mut hay = GrayImage::from_pixel(400, 300, Luma([50]));
        stamp(&mut hay, 100, 120);
        hay.save(&hay_path).unwrap();
        let mut needle = GrayImage::from_pixel(NEEDLE, NEEDLE, Luma([50]));
        stamp(&mut needle, 0, 0);
        needle.save(&needle_path).unwrap();

        let probe = Probe::items();
        let mut ctx = ExecutionContext::default();
        ctx.observer = Some(Arc::clone(&probe) as Arc<dyn Observer>);
        ctx.enter_step("find", "capture.find");

        let out = CaptureFind
            .execute(
                Value::Map(BTreeMap::from([
                    (
                        "haystack".into(),
                        Value::Str(hay_path.to_string_lossy().into()),
                    ),
                    (
                        "needle".into(),
                        Value::Str(needle_path.to_string_lossy().into()),
                    ),
                    ("step".into(), Value::Int(2)),
                ])),
                &mut ctx,
            )
            .await
            .unwrap();

        let map = out.as_map().unwrap();
        assert_eq!(map.get("found"), Some(&Value::Bool(true)), "{map:?}");
        assert_eq!(map.get("x"), Some(&Value::Int(100)));
        assert_eq!(map.get("y"), Some(&Value::Int(120)));

        // 行数与 `find_template` 用同一套算法：从 0 起每 2 行一行，站得住的都算。
        let rows = (300 - NEEDLE) / 2 + 1;
        let marks = probe.marks();
        assert_eq!(
            marks.last(),
            Some(&(rows as u64, Some(rows as u64))),
            "{marks:?}"
        );
        assert!(marks.len() > 100, "应当逐行上报：{}", marks.len());
    }
}
