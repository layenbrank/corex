//! `clipboard.get` / `clipboard.set` via arboard.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;

pub struct ClipboardGet;
pub struct ClipboardSet;

#[async_trait]
impl Action for ClipboardGet {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "clipboard.get",
            "Clipboard Get",
            "读取系统剪贴板文本",
            ActionCategory::Ui,
        )
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| ActionError::execution(format!("打开剪贴板失败: {e}")))?;
        let text = clipboard
            .get_text()
            .map_err(|e| ActionError::execution(format!("读取剪贴板失败: {e}")))?;
        Ok(Value::Str(text))
    }
}

#[async_trait]
impl Action for ClipboardSet {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "clipboard.set",
            "Clipboard Set",
            "写入系统剪贴板文本",
            ActionCategory::Ui,
        )
        .with_params(vec![ParamSchema::new("text", SchemaType::Str, true)])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let text = params
            .as_map()
            .and_then(|m| m.get("text"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("text".into()))?;
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|e| ActionError::execution(format!("打开剪贴板失败: {e}")))?;
        clipboard
            .set_text(text)
            .map_err(|e| ActionError::execution(format!("写入剪贴板失败: {e}")))?;
        Ok(Value::Bool(true))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ClipboardGet));
    registry.register(Arc::new(ClipboardSet));
}
