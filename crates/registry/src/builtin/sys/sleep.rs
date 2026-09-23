//! `sys.sleep`：与界面无关的异步等待。
//!
//! `ui.wait` 服务的是「等界面稳定」，累计耗时受 `ui_settle_limit` 约束；这里只是让流程停一会儿
//! （轮询外部系统、错开重试），所以**不占用** `ui_session.settle_ms_used`，也不设上限——
//! 该等多久由写指令的人决定，不由 UI 预算决定。

use crate::ActionRegistry;
use crate::builtin::util::require_map;
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use std::sync::Arc;
use std::time::Duration;

pub struct SysSleep;

#[async_trait]
impl Action for SysSleep {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "sys.sleep",
            "休眠",
            "异步等待毫秒（不占 ui_settle_limit）",
            Bucket::System,
        )
        .with_params(vec![ParamSchema::new("ms", SchemaType::Int, true)])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let ms = map
            .get("ms")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ActionError::MissingParam("ms".into()))?;
        if ms < 0 {
            // 负数在 `Duration::from_millis` 前会被静默 clamp 成 0；宁可报错。
            return Err(ActionError::InvalidParams(format!("ms 不能为负: {ms}")));
        }
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
        Ok(Value::Bool(true))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(SysSleep));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::Instant;

    fn params(ms: i64) -> Value {
        let mut map = BTreeMap::new();
        map.insert("ms".to_string(), Value::Int(ms));
        Value::Map(map)
    }

    #[tokio::test]
    async fn waits_without_spending_ui_settle_budget() {
        let mut ctx = ExecutionContext::default();
        let before = ctx.ui_session.settle_ms_used;
        let started = Instant::now();
        let out = SysSleep.execute(params(30), &mut ctx).await.unwrap();
        assert_eq!(out, Value::Bool(true));
        assert!(started.elapsed() >= Duration::from_millis(30));
        assert_eq!(ctx.ui_session.settle_ms_used, before);
    }

    #[tokio::test]
    async fn negative_ms_is_rejected() {
        let mut ctx = ExecutionContext::default();
        let err = SysSleep.execute(params(-1), &mut ctx).await.unwrap_err();
        assert!(matches!(err, ActionError::InvalidParams(_)), "错误: {err}");
    }

    #[tokio::test]
    async fn missing_ms_is_reported_as_missing_param() {
        let mut ctx = ExecutionContext::default();
        let err = SysSleep
            .execute(Value::Map(BTreeMap::new()), &mut ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::MissingParam(name) if name == "ms"));
    }

    #[test]
    fn registered_without_permissions() {
        let mut registry = ActionRegistry::new();
        registry.register_builtins();
        assert!(registry.contains("sys.sleep"));
        assert_eq!(
            registry.get("sys.sleep").unwrap().permissions(),
            PermissionSet::NONE
        );
    }
}
