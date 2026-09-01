//! `cron.schedule` — register jobs on the active cron supervisor.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct CronSchedule;

#[async_trait]
impl Action for CronSchedule {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "cron.schedule",
            "Cron Schedule",
            "向 cron 守护注册表达式并关联指令",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("expr", SchemaType::Str, true).with_description("cron 表达式"),
            ParamSchema::new("timezone", SchemaType::Str, false)
                .with_description("时区：local / utc / ±HH:MM（默认用 runtime.cron_timezone）"),
            ParamSchema::new("directive", SchemaType::Str, false)
                .with_description("关联的指令名或路径"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".to_string()))?;
        let expr = map
            .get("expr")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("expr".to_string()))?
            .to_string();
        let timezone_param = map
            .get("timezone")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let directive = map
            .get("directive")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                ctx.variables
                    .get("directive_name")
                    .and_then(|v| v.as_str().map(str::to_string))
            })
            .ok_or_else(|| ActionError::MissingParam("directive".to_string()))?;

        #[cfg(feature = "act-cron")]
        {
            use corex_engine::{CronJobSpec, effective_cron_timezone, find_cron_engine};
            let engine = find_cron_engine().ok_or_else(|| {
                ActionError::execution("cron 守护未运行：请先执行 corex cron run")
            })?;
            let path = resolve_directive_path(&directive)?;
            let job_id = format!("dyn-{}", uuid::Uuid::new_v4());
            let timezone =
                effective_cron_timezone(timezone_param.as_deref(), &ctx.config.cron_timezone);
            engine
                .register(CronJobSpec {
                    id: job_id.clone(),
                    expr: expr.clone(),
                    timezone: timezone.clone(),
                    directive_path: path,
                    directive_name: directive.clone(),
                })
                .await
                .map_err(|e| ActionError::execution(e.to_string()))?;
            let mut out = BTreeMap::new();
            out.insert("job_id".into(), Value::Str(job_id));
            out.insert("expr".into(), Value::Str(expr));
            out.insert("timezone".into(), Value::Str(timezone));
            out.insert("registered".into(), Value::Bool(true));
            return Ok(Value::Map(out));
        }

        #[cfg(not(feature = "act-cron"))]
        {
            let _ = (expr, timezone_param, directive, ctx);
            Err(ActionError::execution("act-cron feature 未启用"))
        }
    }
}

fn resolve_directive_path(name: &str) -> Result<std::path::PathBuf, ActionError> {
    let as_path = std::path::PathBuf::from(name);
    if as_path.exists() {
        return Ok(as_path);
    }
    let data =
        corex_ipc::data_dir().map_err(|e| ActionError::execution(format!("data dir: {e}")))?;
    let base = data.join("directives");
    for ext in ["yaml", "yml"] {
        let p = base.join(format!("{name}.{ext}"));
        if p.exists() {
            return Ok(p);
        }
    }
    Err(ActionError::execution(format!("指令未找到: {name}")))
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CronSchedule));
}
