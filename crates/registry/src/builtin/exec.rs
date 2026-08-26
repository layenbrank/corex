//! `exec.run` — script-file runner (facade over process_launch).

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
use tracing::info;

pub struct ExecRun;

#[async_trait]
impl Action for ExecRun {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "exec.run",
            "Exec",
            "运行脚本文件并返回 stdout/stderr/exit_code",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("script", SchemaType::File, true)
                .with_description("脚本路径（作者自选，支持变量解析）"),
            ParamSchema::new("args", SchemaType::List, false),
            ParamSchema::new("cwd", SchemaType::Str, false),
            ParamSchema::new("host", SchemaType::Str, false)
                .with_default("auto")
                .with_description("none | cmd | powershell | pwsh | auto"),
            ParamSchema::new("allow_nonzero", SchemaType::Bool, false)
                .with_description("非零退出时仍返回 Ok")
                .with_default(false),
            ParamSchema::new("wait", SchemaType::Str, false).with_default("sync"),
            ParamSchema::new("if_running", SchemaType::Str, false).with_default("launch"),
            ParamSchema::new("if_running_window", SchemaType::Map, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let script = require_str(map, "script")?;
        let script_path = PathBuf::from(&script);
        if !script_path.exists() {
            return Err(ActionError::execution(format!("脚本不存在: {script}")));
        }
        info!(script = %script_path.display(), "exec.run");
        let spec = launch_spec_from_command_params(map, script_path, TargetKind::Script)?;
        let result = launch(spec).await?;
        Ok(result.into_value())
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ExecRun));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;
    use std::collections::BTreeMap;
    use std::io::Write;

    #[tokio::test]
    async fn missing_script_errors() {
        let mut ctx = ExecutionContext::default();
        let mut m = BTreeMap::new();
        m.insert(
            "script".into(),
            Value::Str("C:/definitely-missing-corex-script-xyz.bat".into()),
        );
        let err = ExecRun
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect_err("missing script");
        assert!(err.to_string().contains("不存在"));
    }

    #[tokio::test]
    async fn script_bat_or_sh_ok() {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let path = {
            let p = dir.path().join("t.bat");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "@echo hello").unwrap();
            p
        };
        #[cfg(unix)]
        let path = {
            let p = dir.path().join("t.sh");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "#!/bin/sh\necho hello").unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms).unwrap();
            p
        };
        let mut ctx = ExecutionContext::default();
        let mut m = BTreeMap::new();
        m.insert(
            "script".into(),
            Value::Str(path.to_string_lossy().into()),
        );
        m.insert("host".into(), Value::Str("auto".into()));
        let out = ExecRun
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect("script run");
        let map = out.as_map().unwrap();
        assert_eq!(map.get("success"), Some(&Value::Bool(true)));
        assert!(map.contains_key("stdout"));
        assert!(map.contains_key("exit_code"));
    }
}
