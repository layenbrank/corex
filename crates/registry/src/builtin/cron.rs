//! `cron.schedule` — not implemented (returns explicit error).

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
            "注册 cron 表达式（尚未实现，调用将报错）",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("expr", SchemaType::Str, true).with_description("cron 表达式"),
            ParamSchema::new("Directive", SchemaType::Str, false)
                .with_description("关联的指令名"),
        ])
    }

    async fn execute(
        &self, params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".to_string()))?;
        let _expr = map
            .get("expr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("expr".to_string()))?;

        Err(ActionError::execution(
            "cron.schedule 尚未实现：请使用外部调度器或等待后续版本",
        ))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CronSchedule));
}
