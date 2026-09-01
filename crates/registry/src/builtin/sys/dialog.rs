//! Native dialogs (Windows MessageBox / simple prompt).

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct DialogAlert;
pub struct DialogConfirm;
pub struct DialogPrompt;

#[async_trait]
impl Action for DialogAlert {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dialog.alert",
            "Alert",
            "模态提示框（确定）",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("message", SchemaType::Str, true),
            ParamSchema::new("title", SchemaType::Str, false).with_default("corex"),
        ])
    }
    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_alert(params).await
    }
}

#[async_trait]
impl Action for DialogConfirm {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dialog.confirm",
            "Confirm",
            "是/否确认框",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("message", SchemaType::Str, true),
            ParamSchema::new("title", SchemaType::Str, false).with_default("corex"),
        ])
    }
    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_confirm(params).await
    }
}

#[async_trait]
impl Action for DialogPrompt {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dialog.prompt",
            "Prompt",
            "简单文本输入框",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("message", SchemaType::Str, true),
            ParamSchema::new("title", SchemaType::Str, false).with_default("corex"),
            ParamSchema::new("default", SchemaType::Str, false),
        ])
    }
    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        dialog_prompt(params).await
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(DialogAlert));
    registry.register(Arc::new(DialogConfirm));
    registry.register(Arc::new(DialogPrompt));
}

fn require_message(params: &Value) -> Result<(String, String), ActionError> {
    let map = params
        .as_map()
        .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".into()))?;
    let message = map
        .get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionError::MissingParam("message".into()))?
        .to_string();
    let title = map
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("corex")
        .to_string();
    Ok((message, title))
}

async fn dialog_alert(params: Value) -> Result<Value, ActionError> {
    let (message, title) = require_message(&params)?;
    #[cfg(windows)]
    {
        return tokio::task::spawn_blocking(move || win::message_box(&title, &message, false))
            .await
            .map_err(|e| ActionError::execution(format!("dialog.alert 失败: {e}")))?
            .map(|_| Value::Bool(true));
    }
    #[cfg(not(windows))]
    {
        let _ = (message, title);
        Err(ActionError::execution("dialog.* 需要 Windows"))
    }
}

async fn dialog_confirm(params: Value) -> Result<Value, ActionError> {
    let (message, title) = require_message(&params)?;
    #[cfg(windows)]
    {
        return tokio::task::spawn_blocking(move || win::message_box(&title, &message, true))
            .await
            .map_err(|e| ActionError::execution(format!("dialog.confirm 失败: {e}")))?
            .map(Value::Bool);
    }
    #[cfg(not(windows))]
    {
        let _ = (message, title);
        Err(ActionError::execution("dialog.* 需要 Windows"))
    }
}

async fn dialog_prompt(params: Value) -> Result<Value, ActionError> {
    let map = params
        .as_map()
        .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".into()))?;
    let message = map
        .get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionError::MissingParam("message".into()))?
        .to_string();
    let title = map
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("corex")
        .to_string();
    let default = map
        .get("default")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    #[cfg(windows)]
    {
        return tokio::task::spawn_blocking(move || win::prompt_box(&title, &message, &default))
            .await
            .map_err(|e| ActionError::execution(format!("dialog.prompt 失败: {e}")))?;
    }
    #[cfg(not(windows))]
    {
        let _ = (message, title, default);
        Err(ActionError::execution("dialog.* 需要 Windows"))
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_OK, MB_YESNO, MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, MessageBoxW,
    };
    use windows::core::PCWSTR;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn message_box(title: &str, message: &str, yes_no: bool) -> Result<bool, ActionError> {
        let t = wide(title);
        let m = wide(message);
        let style: MESSAGEBOX_STYLE = if yes_no { MB_YESNO } else { MB_OK };
        let r: MESSAGEBOX_RESULT =
            unsafe { MessageBoxW(None, PCWSTR(m.as_ptr()), PCWSTR(t.as_ptr()), style) };
        if yes_no { Ok(r == IDYES) } else { Ok(true) }
    }

    /// Minimal prompt: show message with default text hint; user confirms via Yes to accept default,
    /// No to cancel. Full Edit dialog avoided to keep footprint small — returns default on Yes.
    /// For richer input, combine with clipboard in Directive.
    pub fn prompt_box(title: &str, message: &str, default: &str) -> Result<Value, ActionError> {
        let body = if default.is_empty() {
            format!("{message}\n\n（确认后返回空文本；可先把内容放入剪贴板）")
        } else {
            format!("{message}\n\n默认值: {default}\n（是=使用默认值，否=取消）")
        };
        let ok = message_box(title, &body, true)?;
        let mut out = BTreeMap::new();
        out.insert("ok".into(), Value::Bool(ok));
        out.insert(
            "text".into(),
            Value::Str(if ok {
                default.to_string()
            } else {
                String::new()
            }),
        );
        Ok(Value::Map(out))
    }
}
