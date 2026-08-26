//! Shared process launch kernel for `shell.run` and `exec.run`.

use corex_core::{ActionError, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Explicit execution host. `Auto` resolves from script extension / command mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    /// Direct `Command::new(program)` + args.
    None,
    /// Windows `cmd /C`, Unix `sh -c`.
    Cmd,
    /// Windows PowerShell 5.x (`powershell`).
    Powershell,
    /// PowerShell 7+ (`pwsh`).
    Pwsh,
    /// Resolve from context (script ext or command → None).
    Auto,
}

impl Host {
    /// Parse YAML `host` string. Unknown → error.
    pub fn parse(s: &str) -> Result<Self, ActionError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "" => Ok(Host::None),
            "cmd" => Ok(Host::Cmd),
            "powershell" | "ps" => Ok(Host::Powershell),
            "pwsh" => Ok(Host::Pwsh),
            "auto" => Ok(Host::Auto),
            "sh" => Ok(Host::Cmd), // Unix shell-line mode uses same Cmd branch
            other => Err(ActionError::InvalidParams(format!(
                "未知 host: {other}（none|cmd|powershell|pwsh|auto）"
            ))),
        }
    }
}

/// Whether `program` is treated as a script file (for Auto resolution).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Command,
    Script,
}

/// Sync wait for exit vs spawn-and-return for GUI apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LaunchWait {
    #[default]
    Sync,
    Detach,
}

impl LaunchWait {
    pub fn parse(s: &str) -> Result<Self, ActionError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sync" => Ok(LaunchWait::Sync),
            "detach" => Ok(LaunchWait::Detach),
            other => Err(ActionError::InvalidParams(format!(
                "未知 wait: {other}（sync|detach）"
            ))),
        }
    }
}

/// When the same executable is already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IfRunning {
    #[default]
    Launch,
    Skip,
    Fail,
}

impl IfRunning {
    pub fn parse(s: &str) -> Result<Self, ActionError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "launch" | "always" => Ok(IfRunning::Launch),
            "skip" => Ok(IfRunning::Skip),
            "fail" => Ok(IfRunning::Fail),
            other => Err(ActionError::InvalidParams(format!(
                "未知 if_running: {other}（launch|skip|fail）"
            ))),
        }
    }
}

/// Optional window probe before launch (Windows).
#[derive(Debug, Clone, Default)]
pub struct IfRunningWindow {
    pub title_contains: String,
    pub title_excludes: Vec<String>,
    pub prefer_largest: bool,
}

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub host: Host,
    pub kind: TargetKind,
    pub allow_nonzero: bool,
    pub wait: LaunchWait,
    pub if_running: IfRunning,
    pub if_running_window: Option<IfRunningWindow>,
}

#[derive(Debug, Clone)]
pub struct LaunchResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
    pub success: bool,
    pub detached: bool,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub pid: Option<u32>,
}

impl LaunchResult {
    pub fn into_value(self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("stdout".into(), Value::Str(self.stdout));
        m.insert("stderr".into(), Value::Str(self.stderr));
        m.insert("exit_code".into(), Value::Int(self.exit_code));
        m.insert("success".into(), Value::Bool(self.success));
        m.insert("detached".into(), Value::Bool(self.detached));
        m.insert("skipped".into(), Value::Bool(self.skipped));
        if let Some(r) = self.skip_reason {
            m.insert("reason".into(), Value::Str(r));
        }
        if let Some(pid) = self.pid {
            m.insert("pid".into(), Value::Int(pid as i64));
        }
        Value::Map(m)
    }
}

/// Resolve `Auto` (and validate explicit hosts) into a concrete host.
pub fn resolve_host(host: Host, program: &Path, kind: TargetKind) -> Host {
    match host {
        Host::Auto => match kind {
            TargetKind::Command => Host::None,
            TargetKind::Script => host_for_script_ext(program),
        },
        other => other,
    }
}

fn host_for_script_ext(program: &Path) -> Host {
    let ext = program
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "ps1" => {
            if env_path_has("pwsh") {
                Host::Pwsh
            } else {
                Host::Powershell
            }
        }
        "bat" | "cmd" => Host::Cmd,
        "sh" | "bash" => Host::Cmd,
        _ => Host::None,
    }
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

fn build_command(spec: &LaunchSpec, host: Host) -> Result<Command, ActionError> {
    let program = &spec.program;
    let prog_str = program.to_string_lossy();
    let mut cmd = match host {
        Host::None | Host::Auto => {
            let mut c = Command::new(program);
            for a in &spec.args {
                c.arg(a);
            }
            c
        }
        Host::Cmd => {
            #[cfg(windows)]
            {
                let mut c = Command::new("cmd");
                if spec.kind == TargetKind::Script {
                    c.arg("/C").arg(program.as_os_str());
                    for a in &spec.args {
                        c.arg(a);
                    }
                } else {
                    // Single command line: join program + args for /C
                    let mut line = prog_str.to_string();
                    for a in &spec.args {
                        line.push(' ');
                        line.push_str(a);
                    }
                    c.arg("/C").arg(line);
                }
                c
            }
            #[cfg(not(windows))]
            {
                let mut c = Command::new("sh");
                if spec.kind == TargetKind::Script {
                    c.arg(program.as_os_str());
                    for a in &spec.args {
                        c.arg(a);
                    }
                } else {
                    let mut line = prog_str.to_string();
                    for a in &spec.args {
                        line.push(' ');
                        line.push_str(a);
                    }
                    c.arg("-c").arg(line);
                }
                c
            }
        }
        Host::Powershell | Host::Pwsh => {
            let exe = if host == Host::Pwsh {
                "pwsh"
            } else {
                "powershell"
            };
            let mut c = Command::new(exe);
            c.args(["-NoProfile", "-ExecutionPolicy", "Bypass"]);
            if spec.kind == TargetKind::Script {
                c.arg("-File").arg(program.as_os_str());
                for a in &spec.args {
                    c.arg(a);
                }
            } else {
                let mut line = prog_str.to_string();
                for a in &spec.args {
                    line.push(' ');
                    line.push_str(a);
                }
                c.arg("-Command").arg(line);
            }
            c
        }
    };
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    Ok(cmd)
}

/// Launch process and map to a uniform result. Applies `allow_nonzero`.
pub async fn launch(spec: LaunchSpec) -> Result<LaunchResult, ActionError> {
    if let Some(reason) = should_skip_launch(&spec)? {
        return Ok(LaunchResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            success: true,
            detached: false,
            skipped: true,
            skip_reason: Some(reason),
            pid: None,
        });
    }

    let host = resolve_host(spec.host, &spec.program, spec.kind);
    tracing::debug!(
        program = %spec.program.display(),
        ?host,
        kind = ?spec.kind,
        wait = ?spec.wait,
        "process_launch"
    );
    let mut cmd = build_command(&spec, host)?;

    if spec.wait == LaunchWait::Detach {
        let child = cmd
            .spawn()
            .map_err(|e| ActionError::execution(format!("启动进程失败: {e}")))?;
        let pid = child.id();
        return Ok(LaunchResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            success: true,
            detached: true,
            skipped: false,
            skip_reason: None,
            pid,
        });
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| ActionError::execution(format!("启动进程失败: {e}")))?;
    let exit_code = output.status.code().unwrap_or(-1) as i64;
    let success = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let result = LaunchResult {
        stdout,
        stderr: stderr.clone(),
        exit_code,
        success,
        detached: false,
        skipped: false,
        skip_reason: None,
        pid: None,
    };
    if !success && !spec.allow_nonzero {
        return Err(ActionError::execution(format!(
            "命令非零退出 exit={exit_code}: {stderr}"
        )));
    }
    Ok(result)
}

fn should_skip_launch(spec: &LaunchSpec) -> Result<Option<String>, ActionError> {
    if spec.if_running == IfRunning::Launch && spec.if_running_window.is_none() {
        return Ok(None);
    }

    if let Some(win) = &spec.if_running_window {
        #[cfg(windows)]
        {
            if window_probe_matches(win) {
                return match spec.if_running {
                    IfRunning::Skip => Ok(Some("window_exists".into())),
                    IfRunning::Fail => Err(ActionError::execution(format!(
                        "窗口已存在: {}",
                        win.title_contains
                    ))),
                    IfRunning::Launch => Ok(None),
                };
            }
            // Window query configured but no match: do not fall back to process-name skip.
            return Ok(None);
        }
        #[cfg(not(windows))]
        {
            let _ = win;
            return Ok(None);
        }
    }

    if spec.if_running != IfRunning::Launch && process_running_for_exe(&spec.program) {
        return match spec.if_running {
            IfRunning::Skip => Ok(Some("process_running".into())),
            IfRunning::Fail => Err(ActionError::execution(format!(
                "进程已运行: {}",
                spec.program.display()
            ))),
            IfRunning::Launch => Ok(None),
        };
    }

    Ok(None)
}

fn process_running_for_exe(program: &Path) -> bool {
    let want = program
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if want.is_empty() {
        return false;
    }
    #[cfg(windows)]
    {
        process_running_windows(&want)
    }
    #[cfg(not(windows))]
    {
        let _ = want;
        false
    }
}

#[cfg(windows)]
fn process_running_windows(want_exe: &str) -> bool {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let name = OsString::from_wide(&entry.szExeFile)
                .to_string_lossy()
                .to_ascii_lowercase();
            if name == want_exe {
                let _ = CloseHandle(snap);
                return true;
            }
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    false
}

#[cfg(windows)]
fn window_probe_matches(probe: &IfRunningWindow) -> bool {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    };

    fn title_of(hwnd: HWND) -> String {
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let read = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if read <= 0 {
            return String::new();
        }
        OsString::from_wide(&buf[..read as usize])
            .to_string_lossy()
            .into_owned()
    }

    fn area(hwnd: HWND) -> i64 {
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect).is_err() } {
            return 0;
        }
        let w = (rect.right - rect.left).max(0) as i64;
        let h = (rect.bottom - rect.top).max(0) as i64;
        w * h
    }

    let needle = probe.title_contains.to_lowercase();
    let excludes: Vec<String> = probe
        .title_excludes
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    let mut matches: Vec<HWND> = Vec::new();

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = lparam.0 as *mut (String, Vec<String>, Vec<HWND>);
        if ctx.is_null() {
            return BOOL(0);
        }
        let (needle, excludes, out) = unsafe { &mut *ctx };
        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return BOOL(1);
        }
        let title = title_of(hwnd);
        if title.is_empty() {
            return BOOL(1);
        }
        let lower = title.to_lowercase();
        if !lower.contains(needle.as_str()) {
            return BOOL(1);
        }
        if excludes.iter().any(|ex| lower.contains(ex.as_str())) {
            return BOOL(1);
        }
        out.push(hwnd);
        BOOL(1)
    }

    let mut ctx = (needle, excludes, matches);
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    }
    matches = ctx.2;
    if matches.is_empty() {
        return false;
    }
    if probe.prefer_largest {
        matches.sort_by_key(|h| area(*h));
        return matches.last().is_some();
    }
    true
}

/// Parse optional host from params map (`host` key). Default `Auto`.
pub fn host_from_params(map: &BTreeMap<String, Value>) -> Result<Host, ActionError> {
    match map.get("host").and_then(|v| v.as_str()) {
        None => Ok(Host::Auto),
        Some(s) => Host::parse(s),
    }
}

pub fn args_from_params(map: &BTreeMap<String, Value>) -> Vec<String> {
    match map.get("args") {
        Some(Value::List(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string())
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub fn wait_from_params(map: &BTreeMap<String, Value>) -> Result<LaunchWait, ActionError> {
    match map.get("wait").and_then(|v| v.as_str()) {
        None => Ok(LaunchWait::Sync),
        Some(s) => LaunchWait::parse(s),
    }
}

pub fn if_running_from_params(map: &BTreeMap<String, Value>) -> Result<IfRunning, ActionError> {
    match map.get("if_running").and_then(|v| v.as_str()) {
        None => Ok(IfRunning::Launch),
        Some(s) => IfRunning::parse(s),
    }
}

pub fn if_running_window_from_params(
    map: &BTreeMap<String, Value>,
) -> Result<Option<IfRunningWindow>, ActionError> {
    let Some(v) = map.get("if_running_window") else {
        return Ok(None);
    };
    let m = v.as_map().ok_or_else(|| {
        ActionError::InvalidParams("if_running_window 必须为 map".into())
    })?;
    let title_contains = m
        .get("title_contains")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionError::MissingParam("if_running_window.title_contains".into()))?
        .to_string();
    let title_excludes = match m.get("title_excludes") {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    };
    let prefer_largest = m
        .get("prefer_largest")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(Some(IfRunningWindow {
        title_contains,
        title_excludes,
        prefer_largest,
    }))
}

/// Build a [`LaunchSpec`] from resolved action params (shared by shell.run / exec.run).
pub fn launch_spec_from_command_params(
    map: &BTreeMap<String, Value>,
    program: PathBuf,
    kind: TargetKind,
) -> Result<LaunchSpec, ActionError> {
    Ok(LaunchSpec {
        program,
        args: args_from_params(map),
        cwd: opt_str(map, "cwd").map(PathBuf::from),
        host: host_from_params(map)?,
        kind,
        allow_nonzero: map
            .get("allow_nonzero")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        wait: wait_from_params(map)?,
        if_running: if_running_from_params(map)?,
        if_running_window: if_running_window_from_params(map)?,
    })
}

fn opt_str(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[tokio::test]
    async fn detach_does_not_block() {
        use crate::builtin::process_launch::{
            launch, LaunchSpec, LaunchWait, TargetKind,
        };
        use std::path::PathBuf;
        let spec = LaunchSpec {
            program: PathBuf::from("cmd"),
            args: vec!["/C".into(), "timeout /t 30".into()],
            cwd: None,
            host: Host::Cmd,
            kind: TargetKind::Command,
            allow_nonzero: true,
            wait: LaunchWait::Detach,
            if_running: Default::default(),
            if_running_window: None,
        };
        let out = launch(spec).await.expect("detach spawn");
        assert!(out.detached);
        assert!(out.success);
    }

    #[test]
    fn parse_host_names() {
        assert_eq!(Host::parse("pwsh").unwrap(), Host::Pwsh);
        assert_eq!(Host::parse("powershell").unwrap(), Host::Powershell);
        assert_eq!(Host::parse("cmd").unwrap(), Host::Cmd);
        assert_eq!(Host::parse("none").unwrap(), Host::None);
        assert_eq!(Host::parse("auto").unwrap(), Host::Auto);
        assert!(Host::parse("zsh").is_err());
    }

    #[test]
    fn parse_wait_mode() {
        assert_eq!(LaunchWait::parse("sync").unwrap(), LaunchWait::Sync);
        assert_eq!(LaunchWait::parse("detach").unwrap(), LaunchWait::Detach);
    }

    #[test]
    fn auto_command_is_none() {
        let p = PathBuf::from("npm");
        assert_eq!(
            resolve_host(Host::Auto, &p, TargetKind::Command),
            Host::None
        );
    }

    #[test]
    fn auto_bat_is_cmd() {
        let p = PathBuf::from("deploy.bat");
        assert_eq!(
            resolve_host(Host::Auto, &p, TargetKind::Script),
            Host::Cmd
        );
    }

    #[test]
    fn auto_ps1_is_powershell_family() {
        let p = PathBuf::from("build.ps1");
        let h = resolve_host(Host::Auto, &p, TargetKind::Script);
        assert!(matches!(h, Host::Pwsh | Host::Powershell));
    }

    #[test]
    fn explicit_host_overrides_auto() {
        let p = PathBuf::from("build.ps1");
        assert_eq!(
            resolve_host(Host::Cmd, &p, TargetKind::Script),
            Host::Cmd
        );
    }
}
