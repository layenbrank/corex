//! `exec.run` — run a script/binary and optionally capture JSON.

use crate::builtin::util::{opt_str, require_map, require_str};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;

pub struct ExecRun;

#[async_trait]
impl Action for ExecRun {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "exec.run",
            "Exec",
            "运行脚本或二进制并捕获输出",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("script", SchemaType::File, true),
            ParamSchema::new("args", SchemaType::List, false),
            ParamSchema::new("cwd", SchemaType::Str, false),
            ParamSchema::new("capture", SchemaType::Str, false)
                .with_default("text")
                .with_description("text | json | none"),
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

        let mut cmd = build_command(&script_path);
        if let Some(Value::List(args)) = map.get("args") {
            for a in args {
                if let Some(s) = a.as_str() {
                    cmd.arg(s);
                } else {
                    cmd.arg(a.to_string());
                }
            }
        }
        if let Some(cwd) = opt_str(map, "cwd") {
            cmd.current_dir(cwd);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| ActionError::execution(format!("启动失败: {e}")))?;
        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if exit_code != 0 {
            return Err(ActionError::execution(format!(
                "脚本退出码 {exit_code}: {script}\nstderr: {stderr}"
            )));
        }

        let capture = opt_str(map, "capture").unwrap_or_else(|| "text".into());
        let mut out = BTreeMap::new();
        out.insert("exit_code".into(), Value::Int(exit_code as i64));
        out.insert("success".into(), Value::Bool(true));

        match capture.as_str() {
            "json" => {
                let line = last_non_empty_line(&stdout).ok_or_else(|| {
                    ActionError::execution("exec 输出为空，无法解析 JSON")
                })?;
                let parsed: serde_json::Value = serde_json::from_str(line)
                    .map_err(|e| ActionError::execution(format!("JSON 解析失败: {e}")))?;
                let path = parsed
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::execution("exec 输出缺少 path"))?;
                out.insert("path".into(), Value::File(PathBuf::from(path)));
                if let Some(data) = parsed.get("data") {
                    out.insert("data".into(), Value::from_json(data.clone()));
                }
            }
            "none" => {}
            _ => {
                out.insert("stdout".into(), Value::Str(stdout));
                if !stderr.is_empty() {
                    out.insert("stderr".into(), Value::Str(stderr));
                }
            }
        }
        Ok(Value::Map(out))
    }
}

fn build_command(script: &Path) -> Command {
    let ext = script
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "ps1" => {
            let host = if which("pwsh") { "pwsh" } else { "powershell" };
            let mut c = Command::new(host);
            c.args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script.to_string_lossy(),
            ]);
            c
        }
        "bat" | "cmd" => {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(script);
            c
        }
        "sh" | "bash" => {
            let mut c = Command::new("sh");
            c.arg(script);
            c
        }
        _ => Command::new(script),
    }
}

fn which(bin: &str) -> bool {
    env_path_has(bin)
}

fn env_path_has(bin: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        if dir.join(bin).is_file() || dir.join(format!("{bin}.exe")).is_file() {
            return true;
        }
    }
    false
}

fn last_non_empty_line(stdout: &str) -> Option<&str> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ExecRun));
}
