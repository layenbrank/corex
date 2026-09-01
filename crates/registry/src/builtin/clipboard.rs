//! `clipboard.get` / `clipboard.set` via arboard (text and image).

use crate::ActionRegistry;
use crate::builtin::util::{opt_str, require_map, require_str};
use arboard::{Clipboard, ImageData};
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use image::GenericImageView;
use std::collections::BTreeMap;
use std::sync::Arc;

fn open_clipboard() -> Result<Clipboard, ActionError> {
    Clipboard::new().map_err(|e| ActionError::execution(format!("打开剪贴板失败: {e}")))
}

fn clipboard_format(map: &BTreeMap<String, Value>) -> String {
    opt_str(map, "format").unwrap_or_else(|| "text".into())
}

fn load_rgba_image(path: &str) -> Result<(Vec<u8>, u32, u32), ActionError> {
    let img = image::open(path)
        .map_err(|e| ActionError::execution(format!("打开图片失败 {path}: {e}")))?;
    let (width, height) = img.dimensions();
    let rgba = img.to_rgba8().into_raw();
    Ok((rgba, width, height))
}

pub struct ClipboardGet;
pub struct ClipboardSet;

#[async_trait]
impl Action for ClipboardGet {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "clipboard.get",
            "Clipboard Get",
            "读取系统剪贴板（text 或 image）",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("format", SchemaType::Str, false)
                .with_default("text")
                .with_description("text | image"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let format = clipboard_format(map);
        let mut clipboard = open_clipboard()?;
        match format.as_str() {
            "text" => {
                let text = clipboard
                    .get_text()
                    .map_err(|e| ActionError::execution(format!("读取剪贴板文本失败: {e}")))?;
                Ok(Value::Str(text))
            }
            "image" => {
                let img = clipboard
                    .get_image()
                    .map_err(|e| ActionError::execution(format!("读取剪贴板图片失败: {e}")))?;
                let mut out = BTreeMap::new();
                out.insert("width".into(), Value::Int(img.width as i64));
                out.insert("height".into(), Value::Int(img.height as i64));
                out.insert("bytes".into(), Value::Bytes(img.bytes.into()));
                Ok(Value::Map(out))
            }
            other => Err(ActionError::InvalidParams(format!(
                "不支持的 clipboard format: {other}"
            ))),
        }
    }
}

#[async_trait]
impl Action for ClipboardSet {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "clipboard.set",
            "Clipboard Set",
            "写入系统剪贴板（text 或 image）",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("format", SchemaType::Str, false)
                .with_default("text")
                .with_description("text | image"),
            ParamSchema::new("text", SchemaType::Str, false),
            ParamSchema::new("file", SchemaType::File, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let format = clipboard_format(map);
        let mut clipboard = open_clipboard()?;
        match format.as_str() {
            "text" => {
                let text = require_str(map, "text")?;
                clipboard
                    .set_text(text)
                    .map_err(|e| ActionError::execution(format!("写入剪贴板文本失败: {e}")))?;
            }
            "image" => {
                let path =
                    opt_str(map, "file").ok_or_else(|| ActionError::MissingParam("file".into()))?;
                let (rgba, width, height) = load_rgba_image(&path)?;
                let data = ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: rgba.into(),
                };
                clipboard
                    .set_image(data)
                    .map_err(|e| ActionError::execution(format!("写入剪贴板图片失败: {e}")))?;
            }
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的 clipboard format: {other}"
                )));
            }
        }
        Ok(Value::Bool(true))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ClipboardGet));
    registry.register(Arc::new(ClipboardSet));
}
