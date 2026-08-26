//! `shell.run` — execute a shell command.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;
use tokio::process::Command;

pub struct ShellRun;

#[async_trait]
impl Action for ShellRun {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "shell.run",
            "Shell",
            "执行 shell 命令并返回 stdout/stderr/exit_code",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("command", SchemaType::Str, true)
                .with_description("要执行的命令"),
            ParamSchema::new("args", SchemaType::List, false)
                .with_description("参数列表")
                .with_default(Value::List(vec![])),
            ParamSchema::new("cwd", SchemaType::Str, false).with_description("工作目录"),
            ParamSchema::new("shell", SchemaType::Bool, false)
                .with_description("通过系统 shell 执行（不信任输入时勿开启）")
                .with_default(false),
            ParamSchema::new("allow_nonzero", SchemaType::Bool, false)
                .with_description("非零退出时仍返回 Ok（默认 false，报错）")
                .with_default(false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".to_string()))?;

        let command = map
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("command".to_string()))?;

        let use_shell = map
            .get("shell")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let allow_nonzero = map
            .get("allow_nonzero")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut cmd = if use_shell {
            #[cfg(windows)]
            {
                let mut c = Command::new("cmd");
                c.arg("/C").arg(command);
                c
            }
            #[cfg(not(windows))]
            {
                let mut c = Command::new("sh");
                c.arg("-c").arg(command);
                c
            }
        } else {
            let mut c = Command::new(command);
            if let Some(Value::List(args)) = map.get("args") {
                for a in args {
                    if let Some(s) = a.as_str() {
                        c.arg(s);
                    } else {
                        c.arg(a.to_string());
                    }
                }
            }
            c
        };

        if let Some(cwd) = map.get("cwd").and_then(|v| v.as_str()) {
            cmd.current_dir(cwd);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| ActionError::execution(format!("启动进程失败: {e}")))?;

        let mut result = std::collections::BTreeMap::new();
        result.insert(
            "stdout".into(),
            Value::Str(String::from_utf8_lossy(&output.stdout).into_owned()),
        );
        result.insert(
            "stderr".into(),
            Value::Str(String::from_utf8_lossy(&output.stderr).into_owned()),
        );
        result.insert(
            "exit_code".into(),
            Value::Int(output.status.code().unwrap_or(-1) as i64),
        );
        result.insert("success".into(), Value::Bool(output.status.success()));

        if !output.status.success() && !allow_nonzero {
            let code = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ActionError::execution(format!(
                "命令非零退出 exit={code}: {stderr}"
            )));
        }

        Ok(Value::Map(result))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ShellRun));
}
