//! `cron.schedule` — stub that acknowledges a schedule registration.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;

pub struct CronSchedule;

#[async_trait]
impl Action for CronSchedule {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "cron.schedule",
            "Cron Schedule",
            "注册 cron 表达式（骨架，返回确认值）",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("expr", SchemaType::Str, true)
                .with_description("cron 表达式"),
            ParamSchema::new("shortcut", SchemaType::Str, false)
                .with_description("关联的快捷指令名"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".into()))?;
        let expr = map
            .get("expr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("expr".into()))?;
        let shortcut = map
            .get("shortcut")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        tracing::info!(expr, shortcut, "cron.schedule 骨架注册");

        let mut out = std::collections::BTreeMap::new();
        out.insert("scheduled".into(), Value::Bool(true));
        out.insert("expr".into(), Value::Str(expr.into()));
        out.insert("shortcut".into(), Value::Str(shortcut.into()));
        Ok(Value::Map(out))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CronSchedule));
}
