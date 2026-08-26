//! `notify.send` — desktop notification via notify-rust.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;

pub struct NotifySend;

#[async_trait]
impl Action for NotifySend {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "notify.send",
            "Notify",
            "发送桌面通知",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("summary", SchemaType::Str, true),
            ParamSchema::new("body", SchemaType::Str, false).with_default(""),
            ParamSchema::new("appname", SchemaType::Str, false).with_default("corex"),
        ])
    }

    async fn execute(
        &self, params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".to_string()))?;
        let summary = map
            .get("summary")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("summary".to_string()))?;
        let body = map
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let appname = map
            .get("appname")
            .and_then(|v| v.as_str())
            .unwrap_or("corex");

        notify_rust::Notification::new()
            .summary(summary)
            .body(body)
            .appname(appname)
            .show()
            .map_err(|e| ActionError::execution(format!("发送通知失败: {e}")))?;

        Ok(Value::Bool(true))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(NotifySend));
}
