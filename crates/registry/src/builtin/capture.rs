//! Capture actions — soft-fail on unsupported platforms; crop/clipboard where possible.

use crate::builtin::util::{
    ensure_parent, opt_i64, opt_str, require_map, require_path, require_str,
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
pub struct CaptureClipboard;
pub struct CaptureCrop;
pub struct CaptureMonitors;

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
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        Err(ActionError::execution(
            "capture.screenshot 在当前平台不可用（需要原生截图后端）",
        ))
    }
}

#[async_trait]
impl Action for CaptureClipboard {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "capture.clipboard",
            "Capture Clipboard",
            "读取/写入剪贴板文本",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("mode", SchemaType::Str, false)
                .with_default("get")
                .with_description("get | set"),
            ParamSchema::new("text", SchemaType::Str, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let empty = BTreeMap::new();
        let map = params.as_map().unwrap_or(&empty);
        let mode = opt_str(map, "mode").unwrap_or_else(|| "get".into());
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| ActionError::execution(format!("打开剪贴板失败: {e}")))?;
        match mode.as_str() {
            "set" => {
                let text = require_str(map, "text")?;
                clipboard
                    .set_text(text)
                    .map_err(|e| ActionError::execution(format!("写入剪贴板失败: {e}")))?;
                Ok(Value::Bool(true))
            }
            _ => {
                let text = clipboard
                    .get_text()
                    .map_err(|e| ActionError::execution(format!("读取剪贴板失败: {e}")))?;
                Ok(Value::Str(text))
            }
        }
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
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = require_path(map, "from")?;
        let to = require_path(map, "to")?;
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
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        Err(ActionError::execution(
            "capture.monitors 在当前平台不可用（需要原生截图后端）",
        ))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CaptureScreenshot));
    registry.register(Arc::new(CaptureClipboard));
    registry.register(Arc::new(CaptureCrop));
    registry.register(Arc::new(CaptureMonitors));
}
