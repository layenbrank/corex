//! `exec.run` — script-file runner (facade over process_launch).

use crate::ActionRegistry;
use crate::builtin::process_launch::{TargetKind, launch, launch_spec_from_command_params};
use crate::builtin::util::{confine_path, require_map, require_str};
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::path::Path;
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
                .with_description("脚本路径（作者自选，支持变量解析；受 filesystem_roots 约束）"),
            ParamSchema::new("args", SchemaType::List, false),
            ParamSchema::new("cwd", SchemaType::Str, false)
                .with_description("工作目录（受 filesystem_roots 约束）"),
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
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let script = require_str(map, "script")?;
        let script_path = confine_path(ctx, Path::new(&script))?;
        if !script_path.exists() {
            return Err(ActionError::execution(format!(
                "脚本不存在: {}",
                script_path.display()
            )));
        }
        info!(script = %script_path.display(), "exec.run");
        let mut spec = launch_spec_from_command_params(map, script_path, TargetKind::Script)?;
        if let Some(cwd) = spec.cwd.take() {
            spec.cwd = Some(confine_path(ctx, &cwd)?);
        }
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
    use corex_core::{ExecutionContext, RuntimeConfig};
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
        m.insert("script".into(), Value::Str(path.to_string_lossy().into()));
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

    #[tokio::test]
    async fn filesystem_roots_rejects_script_outside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("allowed");
        let outside = dir.path().join("denied");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        #[cfg(windows)]
        let script = {
            let p = outside.join("t.bat");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "@echo hello").unwrap();
            p
        };
        #[cfg(unix)]
        let script = {
            let p = outside.join("t.sh");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "#!/bin/sh\necho hello").unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms).unwrap();
            p
        };

        let mut cfg = RuntimeConfig::default();
        cfg.filesystem_roots = vec![root];
        let mut ctx = ExecutionContext::new(cfg);
        let mut m = BTreeMap::new();
        m.insert("script".into(), Value::Str(script.to_string_lossy().into()));
        let err = ExecRun
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect_err("outside script");
        let msg = err.to_string();
        assert!(
            msg.contains("越界") || msg.contains("不在") || msg.contains("无法解析"),
            "got: {msg}"
        );
    }

    #[tokio::test]
    async fn filesystem_roots_allows_script_inside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("allowed");
        std::fs::create_dir_all(&root).unwrap();
        #[cfg(windows)]
        let script = {
            let p = root.join("t.bat");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "@echo hello").unwrap();
            p
        };
        #[cfg(unix)]
        let script = {
            let p = root.join("t.sh");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "#!/bin/sh\necho hello").unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms).unwrap();
            p
        };

        let mut cfg = RuntimeConfig::default();
        cfg.filesystem_roots = vec![root.clone()];
        let mut ctx = ExecutionContext::new(cfg);
        let mut m = BTreeMap::new();
        m.insert("script".into(), Value::Str(script.to_string_lossy().into()));
        m.insert("host".into(), Value::Str("auto".into()));
        let out = ExecRun
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect("inside script");
        assert_eq!(
            out.as_map().unwrap().get("success"),
            Some(&Value::Bool(true))
        );
    }

    #[tokio::test]
    async fn filesystem_roots_rejects_cwd_outside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("allowed");
        let outside = dir.path().join("denied");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        #[cfg(windows)]
        let script = {
            let p = root.join("t.bat");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "@echo hello").unwrap();
            p
        };
        #[cfg(unix)]
        let script = {
            let p = root.join("t.sh");
            let mut f = std::fs::File::create(&p).unwrap();
            writeln!(f, "#!/bin/sh\necho hello").unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms).unwrap();
            p
        };

        let mut cfg = RuntimeConfig::default();
        cfg.filesystem_roots = vec![root];
        let mut ctx = ExecutionContext::new(cfg);
        let mut m = BTreeMap::new();
        m.insert("script".into(), Value::Str(script.to_string_lossy().into()));
        m.insert("cwd".into(), Value::Str(outside.to_string_lossy().into()));
        let err = ExecRun
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect_err("outside cwd");
        let msg = err.to_string();
        assert!(
            msg.contains("越界") || msg.contains("不在") || msg.contains("无法解析"),
            "got: {msg}"
        );
    }
}
