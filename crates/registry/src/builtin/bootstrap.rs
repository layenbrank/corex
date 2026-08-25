//! Bootstrap actions: env / inspect / force (PATH helpers).

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, Value,
};
use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

fn exe_dir() -> Result<String, ActionError> {
    let path = env::current_exe()
        .map_err(|e| ActionError::execution(format!("无法获取可执行文件路径: {e}")))?;
    let parent = path
        .parent()
        .ok_or_else(|| ActionError::execution("无法获取可执行文件目录"))?;
    Ok(parent.to_string_lossy().into_owned())
}

fn bootstrap_script() -> PathBuf {
    // Prefer scripts next to workspace when developing; fall back to CWD.
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/bootstrap.ps1"),
        PathBuf::from("scripts/bootstrap.ps1"),
    ];
    candidates
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("scripts/bootstrap.ps1"))
}

fn run_ps_script(action: &str, target: &str) -> Result<(), ActionError> {
    let script = bootstrap_script();
    if !script.exists() {
        return Err(ActionError::execution(format!(
            "bootstrap 脚本不存在: {}",
            script.display()
        )));
    }
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
            "-Action",
            action,
            "-Target",
            target,
        ])
        .output()
        .map_err(|e| ActionError::execution(format!("执行 PowerShell 失败: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ActionError::execution(format!(
            "PowerShell 执行失败: {stderr}"
        )));
    }
    Ok(())
}

fn inspect_path(exe_dir: &str) -> Result<Value, ActionError> {
    let current_path = env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ';' } else { ':' };
    let contains = current_path.split(sep).any(|path| {
        Path::new(path).canonicalize().ok() == Path::new(exe_dir).canonicalize().ok()
    });
    let mut out = BTreeMap::new();
    out.insert("in_path".into(), Value::Bool(contains));
    out.insert("exe_dir".into(), Value::Str(exe_dir.into()));
    Ok(Value::Map(out))
}

pub struct BootstrapEnv;
pub struct BootstrapInspect;
pub struct BootstrapForce;

#[async_trait]
impl Action for BootstrapEnv {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "bootstrap.env",
            "Bootstrap Env",
            "将工具目录写入用户 PATH（Windows PowerShell）",
            ActionCategory::System,
        )
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let dir = exe_dir()?;
        if cfg!(windows) {
            run_ps_script("insert", &dir)?;
        } else {
            return Err(ActionError::execution(
                "bootstrap.env 当前仅支持 Windows",
            ));
        }
        let mut out = BTreeMap::new();
        out.insert("exe_dir".into(), Value::Str(dir));
        out.insert("ok".into(), Value::Bool(true));
        Ok(Value::Map(out))
    }
}

#[async_trait]
impl Action for BootstrapInspect {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "bootstrap.inspect",
            "Bootstrap Inspect",
            "检查工具目录是否已在 PATH 中",
            ActionCategory::System,
        )
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let dir = exe_dir()?;
        inspect_path(&dir)
    }
}

#[async_trait]
impl Action for BootstrapForce {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "bootstrap.force",
            "Bootstrap Force",
            "强制刷新 PATH 中的工具目录（Windows）",
            ActionCategory::System,
        )
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let dir = exe_dir()?;
        if cfg!(windows) {
            run_ps_script("force", &dir)?;
        } else {
            return Err(ActionError::execution(
                "bootstrap.force 当前仅支持 Windows",
            ));
        }
        let mut out = BTreeMap::new();
        out.insert("exe_dir".into(), Value::Str(dir));
        out.insert("ok".into(), Value::Bool(true));
        Ok(Value::Map(out))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(BootstrapEnv));
    registry.register(Arc::new(BootstrapInspect));
    registry.register(Arc::new(BootstrapForce));
}
