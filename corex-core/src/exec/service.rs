use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::exec::schema::{Args, CaptureMode, RunArgs};

const STDERR_TAIL_MAX: usize = 2048;

#[derive(Debug, Clone)]
pub struct Output {
    pub path: Option<PathBuf>,
    pub data: HashMap<String, Value>,
    pub stdout: Option<String>,
    pub exit_code: i32,
}

impl Output {
    pub fn into_invoke_result(self) -> crate::invoke::InvokeResult {
        use crate::invoke::{Artifact, InvokeResult};

        let mut artifact = match self.path {
            Some(p) => Artifact::from_path(p),
            None => Artifact::default(),
        };
        for (k, v) in self.data {
            artifact = artifact.with_data(k, v);
        }
        InvokeResult::from_artifact(artifact)
    }
}

#[derive(Debug, Deserialize)]
struct ScriptResult {
    path: String,
    data: Map<String, Value>,
}

/// CLI 入口
pub fn run(args: &Args) -> Result<()> {
    let output = execute(args)?;
    if !crate::runtime::is_quiet() && !crate::runtime::is_json_output() {
        println!("exit code: {}", output.exit_code);
        if let Some(path) = &output.path {
            println!("path: {}", path.display());
        }
        if !output.data.is_empty() {
            let json = serde_json::to_string_pretty(&output.data)?;
            println!("data:\n{json}");
        }
        if let Some(stdout) = &output.stdout {
            print!("{stdout}");
        }
    }
    Ok(())
}

/// Pipeline / IPC 复用
pub fn execute(args: &Args) -> Result<Output> {
    match args {
        Args::Run(run_args) => run_script(run_args),
    }
}

fn run_script(args: &RunArgs) -> Result<Output> {
    let script = Path::new(&args.script);
    if !script.exists() {
        bail!("脚本不存在: {}", args.script);
    }

    let mut cmd = build_command(script, &args.args)?;
    if let Some(cwd) = &args.cwd {
        cmd.current_dir(cwd);
    }

    let live = !crate::runtime::is_quiet() && !crate::runtime::is_json_output();
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("启动脚本失败: {}", args.script))?;

    let stdout_pipe = child
        .stdout
        .take()
        .context("无法捕获脚本 stdout")?;
    let stderr_pipe = child
        .stderr
        .take()
        .context("无法捕获脚本 stderr")?;

    let stdout_thread = thread::spawn(move || stream_pipe(stdout_pipe, live, false));
    let stderr_thread = thread::spawn(move || stream_pipe(stderr_pipe, live, true));

    let status = child
        .wait()
        .with_context(|| format!("等待脚本结束失败: {}", args.script))?;
    let exit_code = status.code().unwrap_or(-1);

    let stdout = stdout_thread
        .join()
        .unwrap_or_else(|_| String::new());
    let stderr = stderr_thread
        .join()
        .unwrap_or_else(|_| String::new());

    if exit_code != 0 {
        let tail = truncate_tail(&stderr, STDERR_TAIL_MAX);
        bail!(
            "脚本退出码 {exit_code}: {}{}",
            args.script,
            if tail.is_empty() {
                String::new()
            } else {
                format!("\nstderr: {tail}")
            }
        );
    }

    match args.capture {
        CaptureMode::Json => parse_json_capture(&stdout),
        CaptureMode::Text => Ok(Output {
            path: None,
            data: HashMap::new(),
            stdout: Some(stdout),
            exit_code,
        }),
        CaptureMode::None => Ok(Output {
            path: None,
            data: HashMap::new(),
            stdout: None,
            exit_code,
        }),
    }
}

/// 边读边回显，同时收集完整输出供 capture 解析。
fn stream_pipe(mut pipe: impl Read, live: bool, is_stderr: bool) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let piece = &chunk[..n];
                bytes.extend_from_slice(piece);
                if live {
                    let text = String::from_utf8_lossy(piece);
                    if is_stderr {
                        eprint!("{text}");
                        let _ = std::io::stderr().flush();
                    } else {
                        print!("{text}");
                        let _ = std::io::stdout().flush();
                    }
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn build_command(script: &Path, args: &[String]) -> Result<Command> {
    let ext = script
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let cmd = match ext.as_str() {
        "ps1" => {
            let mut c = Command::new("powershell");
            c.args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script.to_string_lossy(),
            ]);
            c.args(args);
            c
        }
        "bat" | "cmd" => {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(script);
            c.args(args);
            c
        }
        _ => {
            let mut c = Command::new(script);
            c.args(args);
            c
        }
    };
    Ok(cmd)
}

fn parse_json_capture(stdout: &str) -> Result<Output> {
    let line = last_non_empty_line(stdout)
        .ok_or_else(|| anyhow::anyhow!("exec 输出为空，无法解析 JSON"))?;

    let parsed: ScriptResult = serde_json::from_str(line).with_context(|| {
        format!("exec 输出 JSON 解析失败（须含 path + data）: {line}")
    })?;

    if parsed.path.trim().is_empty() {
        bail!("exec 输出 path 不能为空");
    }
    Ok(Output {
        path: Some(PathBuf::from(parsed.path)),
        data: parsed.data.into_iter().collect(),
        stdout: None,
        exit_code: 0,
    })
}

/// 取 stdout 最后一个非空行（trim 后）
pub fn last_non_empty_line(stdout: &str) -> Option<&str> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()
}

fn truncate_tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    s[s.len().saturating_sub(max)..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn last_non_empty_line_skips_trailing_blanks() {
        let stdout = "log line\n\n{\"path\":\"a\",\"data\":{\"v\":1}}\n\n";
        assert_eq!(
            last_non_empty_line(stdout),
            Some("{\"path\":\"a\",\"data\":{\"v\":1}}")
        );
    }

    #[test]
    fn parse_json_capture_ok() {
        let stdout = "info\n{\"path\":\"C:/out.json\",\"data\":{\"version\":\"20260713\"}}\n";
        let out = parse_json_capture(stdout).unwrap();
        assert_eq!(out.path.as_ref().unwrap().to_str().unwrap(), "C:/out.json");
        assert_eq!(
            out.data.get("version"),
            Some(&Value::String("20260713".into()))
        );
    }

    #[test]
    fn parse_json_capture_nested_data() {
        let stdout = r#"{"path":"D:/r.json","data":{"count":42,"meta":{"sha":"abc"}}}"#;
        let out = parse_json_capture(stdout).unwrap();
        assert_eq!(out.data.get("count"), Some(&json!(42)));
        assert!(out.data.get("meta").unwrap().is_object());
    }

    #[test]
    fn parse_json_capture_missing_path_fails() {
        let err = parse_json_capture(r#"{"data":{"v":1}}"#).unwrap_err();
        assert!(err.to_string().contains("path"));
    }

    #[test]
    fn parse_json_capture_missing_data_fails() {
        let err = parse_json_capture(r#"{"path":"C:/a.json"}"#).unwrap_err();
        assert!(err.to_string().contains("data"));
    }

    #[test]
    fn parse_json_capture_empty_data_ok() {
        let out = parse_json_capture(r#"{"path":"C:/a.json","data":{}}"#).unwrap();
        assert!(out.data.is_empty());
    }

    #[test]
    fn parse_json_capture_data_not_object_fails() {
        let err = parse_json_capture(r#"{"path":"C:/a.json","data":"nope"}"#).unwrap_err();
        assert!(err.to_string().contains("data"));
    }
}
