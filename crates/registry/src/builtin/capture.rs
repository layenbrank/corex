//! Capture actions — soft-fail on unsupported platforms; crop/clipboard where possible.

#[path = "capture_match.rs"]
mod match_img;

use crate::builtin::util::{
    confine_path, ensure_parent, opt_i64, opt_str, require_map, require_path,
};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
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
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.screenshot",
            "Screenshot",
            "截取主显示器画面",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("format", SchemaType::Str, false).with_default("png"),
            ParamSchema::new("quality", SchemaType::Int, false).with_default(90),
        ])
    }

    async fn execute(
        &self, params: Value,
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
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.crop",
            "Crop Image",
            "裁剪图片文件并写出",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("x", SchemaType::Int, true),
            ParamSchema::new("y", SchemaType::Int, true),
            ParamSchema::new("width", SchemaType::Int, true),
            ParamSchema::new("height", SchemaType::Int, true),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = confine_path(ctx, &require_path(map, "from")?)?;
        let to = confine_path(ctx, &require_path(map, "to")?)?;
        let x = opt_i64(map, "x", 0).max(0) as u32;
        let y = require_i64(map, "y")? as u32;
        let width = require_i64(map, "width")? as u32;
        let height = require_i64(map, "height")? as u32;

        let img = image::open(&from)
            .map_err(|e| ActionError::execution(format!("打开图片失败: {e}")))?;
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
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.monitors",
            "List Monitors",
            "列出显示器信息",
            ActionCategory::Ui,
        )
    }

    async fn execute(
        &self, _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        capture_monitors_impl().await
    }
}

#[async_trait]
impl Action for CaptureOcr {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.ocr",
            "Capture OCR",
            "识别图片中的文字",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("file", SchemaType::File, true),
            ParamSchema::new("language", SchemaType::Str, false),
        ])
    }

    async fn execute(
        &self, params: Value,
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
        return capture_ocr_windows(params).await;
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
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.find",
            "Find Template",
            "在大图中查找模板（灰度 NCC）",
            ActionCategory::Ui,
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
        let threshold = map
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.9);
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

        tokio::task::spawn_blocking(move || {
            let hay = image::open(&haystack)
                .map_err(|e| ActionError::execution(format!("打开 haystack 失败: {e}")))?;
            let ndl = image::open(&needle)
                .map_err(|e| ActionError::execution(format!("打开 needle 失败: {e}")))?;
            let hay_g = match_img::to_gray(hay);
            let ndl_g = match_img::to_gray(ndl);
            let m = match_img::find_template(&hay_g, &ndl_g, region, step, threshold)?;
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
        let screens = Screen::all()
            .map_err(|e| ActionError::execution(format!("枚举显示器失败: {e}")))?;
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
            let list: Vec<Value> = screens
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let mut m = BTreeMap::new();
                    m.insert("index".into(), Value::Int(i as i64));
                    m.insert(
                        "width".into(),
                        Value::Int(s.display_info.width as i64),
                    );
                    m.insert(
                        "height".into(),
                        Value::Int(s.display_info.height as i64),
                    );
                    Value::Map(m)
                })
                .collect();
            let mut out = BTreeMap::new();
            out.insert("monitors".into(), Value::List(list));
            Ok(Value::Map(out))
        })
        .await
        .map_err(|e| ActionError::execution(format!("枚举任务失败: {e}")))?
    }

    pub async fn capture_ocr_windows(params: Value) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let file = require_path(map, "file")?;
        let lang = opt_str(map, "language");
        // WinRT OCR types are typically !Send; keep COM work on one blocking thread.
        tokio::task::spawn_blocking(move || ocr_file(&file, lang.as_deref()))
            .await
            .map_err(|e| ActionError::execution(format!("OCR 任务失败: {e}")))?
    }

    fn ocr_file(path: &std::path::Path, _language: Option<&str>) -> Result<Value, ActionError> {
        use windows::core::HSTRING;
        use windows::Graphics::Imaging::BitmapDecoder;
        use windows::Media::Ocr::OcrEngine;
        use windows::Storage::FileAccessMode;
        use windows::Storage::StorageFile;

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
        // Line geometry APIs vary by windows crate version; expose text + empty lines for now.
        let lines = vec![Value::Map({
            let mut lm = BTreeMap::new();
            lm.insert("text".into(), Value::Str(text.clone()));
            lm
        })];
        let mut out = BTreeMap::new();
        out.insert("text".into(), Value::Str(text));
        out.insert("lines".into(), Value::List(lines));
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
