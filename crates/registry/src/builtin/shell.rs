//! `shell.run` — general process launcher (facade over process_launch).

use crate::builtin::process_launch::{
    launch, launch_spec_from_command_params, TargetKind,
};
use crate::builtin::util::{require_map, require_str};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::path::PathBuf;
use std::sync::Arc;

pub struct ShellRun;

#[async_trait]
impl Action for ShellRun {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "shell.run",
            "Shell",
            "执行进程/命令并返回 stdout/stderr/exit_code",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("command", SchemaType::Str, true)
                .with_description("可执行文件或命令名"),
            ParamSchema::new("args", SchemaType::List, false)
                .with_description("参数列表")
                .with_default(Value::List(vec![])),
            ParamSchema::new("cwd", SchemaType::Str, false).with_description("工作目录"),
            ParamSchema::new("host", SchemaType::Str, false)
                .with_default("auto")
                .with_description("none | cmd | powershell | pwsh | auto"),
            ParamSchema::new("allow_nonzero", SchemaType::Bool, false)
                .with_description("非零退出时仍返回 Ok（默认 false，报错）")
                .with_default(false),
            ParamSchema::new("wait", SchemaType::Str, false)
                .with_default("sync")
                .with_description("sync | detach（GUI 应用建议 detach）"),
            ParamSchema::new("if_running", SchemaType::Str, false)
                .with_default("launch")
                .with_description("launch | skip | fail"),
            ParamSchema::new("if_running_window", SchemaType::Map, false)
                .with_description("窗口已存在则 skip：title_contains, title_excludes?, prefer_largest?"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let command = require_str(map, "command")?;
        let spec = launch_spec_from_command_params(
            map,
            PathBuf::from(command),
            TargetKind::Command,
        )?;
        let result = launch(spec).await?;
        Ok(result.into_value())
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ShellRun));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;
    use std::collections::BTreeMap;

    fn failing_command_params(allow_nonzero: bool) -> Value {
        let mut m = BTreeMap::new();
        #[cfg(unix)]
        {
            m.insert("command".into(), Value::Str("false".into()));
        }
        #[cfg(windows)]
        {
            m.insert("command".into(), Value::Str("cmd".into()));
            m.insert(
                "args".into(),
                Value::List(vec![
                    Value::Str("/C".into()),
                    Value::Str("exit 1".into()),
                ]),
            );
        }
        if allow_nonzero {
            m.insert("allow_nonzero".into(), Value::Bool(true));
        }
        Value::Map(m)
    }

    #[tokio::test]
    async fn nonzero_without_allow_errors() {
        let mut ctx = ExecutionContext::default();
        let err = ShellRun
            .execute(failing_command_params(false), &mut ctx)
            .await
            .expect_err("non-zero exit must Err without allow_nonzero");
        let msg = err.to_string();
        assert!(
            msg.contains("非零") || msg.contains("exit"),
            "got: {msg}"
        );
    }

    #[tokio::test]
    async fn nonzero_with_allow_ok() {
        let mut ctx = ExecutionContext::default();
        let out = ShellRun
            .execute(failing_command_params(true), &mut ctx)
            .await
            .expect("allow_nonzero should return Ok");
        let map = out.as_map().expect("map");
        assert_eq!(map.get("success"), Some(&Value::Bool(false)));
        assert_ne!(map.get("exit_code"), Some(&Value::Int(0)));
    }

    #[tokio::test]
    async fn host_cmd_runs_line() {
        let mut ctx = ExecutionContext::default();
        let mut m = BTreeMap::new();
        #[cfg(windows)]
        {
            m.insert("command".into(), Value::Str("echo ok".into()));
            m.insert("host".into(), Value::Str("cmd".into()));
        }
        #[cfg(unix)]
        {
            m.insert("command".into(), Value::Str("echo ok".into()));
            m.insert("host".into(), Value::Str("cmd".into()));
        }
        let out = ShellRun
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect("host:cmd echo");
        let map = out.as_map().unwrap();
        assert_eq!(map.get("success"), Some(&Value::Bool(true)));
    }
}
