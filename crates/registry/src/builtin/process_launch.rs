//! `shell.run` 与 `exec.run` 共用的进程启动内核。

use corex_core::{ActionError, Value};
use encoding_rs::Encoding;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// 显式指定执行宿主。`Auto` 按脚本扩展名 / 命令形式推断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    /// 直接 `Command::new(program)` + args。
    None,
    /// Windows `cmd /C`，Unix `sh -c`。
    Cmd,
    /// Windows PowerShell 5.x（`powershell`）。
    Powershell,
    /// PowerShell 7+（`pwsh`）。
    Pwsh,
    /// 按上下文推断（脚本扩展名或命令形式 → None）。
    Auto,
}

impl Host {
    /// 解析 YAML 的 `host` 字符串。未知值 → 报错。
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

/// `program` 是否按脚本文件处理（供 Auto 推断用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Command,
    Script,
}

/// 同步等待退出，还是给 GUI 程序用“派生即返回”。
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

/// 同名可执行文件已在运行时的处理方式。
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

/// 启动前可选的窗口探测（Windows）。
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
    /// 写入子进程 stdin 的内容（写完即关闭）。`None` 时子进程继承父进程 stdin。
    pub input: Option<String>,
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

/// 把 `Auto`（并校验显式宿主）解析成具体宿主。
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
                    // 脚本必须走 CreateProcess 的多 argv 形式：把路径塞进单个 `/C`
                    // 字符串再包一层引号时，Windows 会二次转义成 `\"...\t.bat\"`，cmd 认不出。
                    // 编码靠管道侧 OEM/GBK 回退，不必在这里 chcp。
                    c.arg("/C").arg(program.as_os_str());
                    for a in &spec.args {
                        c.arg(a);
                    }
                } else {
                    // 单行命令：先切 UTF-8；听劝的少乱码，不听的仍靠 decode 回退。
                    let mut line = String::from("chcp 65001>NUL & ");
                    line.push_str(&prog_str);
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
            // 管道读方按字节解码；不设的话 Windows PowerShell 5 常按 OEM 吐中文。
            const UTF8_PREAMBLE: &str = "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = [Console]::OutputEncoding; ";
            if spec.kind == TargetKind::Script {
                let mut line = format!("{UTF8_PREAMBLE}& '{}'", prog_str.replace('\'', "''"));
                for a in &spec.args {
                    line.push(' ');
                    line.push_str(&powershell_single_quote(a));
                }
                c.arg("-Command").arg(line);
            } else {
                let mut line = format!("{UTF8_PREAMBLE}{prog_str}");
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
    // 从 GUI / 无窗口父进程（daemon、Tauri）启动时避免闪一下控制台窗口。
    // ⚠️ 父进程**有**控制台时绝不能加：该标志会让子进程挂到新建的隐藏控制台，
    // 交互式程序（corepack/pnpm、`Read-Host`、`set /p`）的提问既看不见，
    // 键盘输入也送不进去，只能一直等下去。
    #[cfg(windows)]
    if !has_console() {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(cmd)
}

/// 父进程是否连着控制台（任一标准流是终端即算）。
#[cfg(windows)]
fn has_console() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
        || std::io::stdout().is_terminal()
        || std::io::stderr().is_terminal()
}

/// 启动进程并映射成统一结果。会应用 `allow_nonzero`。
pub async fn launch(spec: LaunchSpec) -> Result<LaunchResult, ActionError> {
    // Windows 上被规范化成 `\\?\` 的路径会让 cmd.exe / 某些 shell 出错。
    let mut spec = LaunchSpec {
        program: corex_core::path::for_external_process(spec.program),
        cwd: spec.cwd.map(corex_core::path::for_external_process),
        ..spec
    };

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
    if spec.input.is_some() {
        cmd.stdin(Stdio::piped());
    }

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

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| ActionError::execution(format!("启动进程失败: {e}")))?;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    // `input` 走管道后必须显式关闭，子进程才会读到 EOF（`read_line` 类提问靠它返回）。
    let stdin_task = spec.input.take().map(|text| {
        let sink = child.stdin.take();
        tokio::spawn(async move {
            if let Some(mut sink) = sink {
                let _ = sink.write_all(text.as_bytes()).await;
                let _ = sink.shutdown().await;
            }
        })
    });
    let stdout_task =
        tokio::spawn(async move { pump_process_stream(stdout_pipe, ProcessStream::Stdout).await });
    let stderr_task =
        tokio::spawn(async move { pump_process_stream(stderr_pipe, ProcessStream::Stderr).await });
    let status = child
        .wait()
        .await
        .map_err(|e| ActionError::execution(format!("等待进程失败: {e}")))?;
    // 子进程已退出，剩余的 stdin 写入不再有意义（大输入还可能写满管道）。
    if let Some(task) = stdin_task {
        task.abort();
    }
    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let exit_code = status.code().unwrap_or(-1) as i64;
    let success = status.success();
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

#[derive(Debug, Clone, Copy)]
enum ProcessStream {
    Stdout,
    Stderr,
}

/// 分块读取进程输出，回显到终端，并收集起来作为动作结果。
///
/// Windows 上管道字节经常是 OEM（中文 CP936/GBK），不能当 UTF-8 硬解，
/// 也不能把原始字节直接 `write_all` 进控制台（Rust 走 `WriteConsoleW`，要合法 UTF-8）。
async fn pump_process_stream<R>(reader: Option<R>, stream: ProcessStream) -> String
where
    R: AsyncRead + Unpin,
{
    let Some(mut reader) = reader else {
        return String::new();
    };
    let mut collected = Vec::new();
    let mut echoed = 0usize;
    let mut buf = [0u8; 8192];
    loop {
        let n = match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        collected.extend_from_slice(&buf[..n]);
        let text = decode_process_output_partial(&collected);
        if text.len() > echoed {
            // `echoed` 始终是上一段解码结果的 UTF-8 字节长，落在字符边界上。
            let chunk = &text[echoed..];
            match stream {
                ProcessStream::Stdout => {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(chunk.as_bytes());
                    let _ = out.flush();
                }
                ProcessStream::Stderr => {
                    let mut err = std::io::stderr().lock();
                    let _ = err.write_all(chunk.as_bytes());
                    let _ = err.flush();
                }
            }
            echoed = text.len();
        } else if text.len() < echoed {
            // 中途从 UTF-8 切到 OEM 时前缀偶发变短：别再按旧下标切片。
            echoed = text.len();
        }
    }
    decode_process_output(&collected)
}

/// PowerShell `-Command` 里的单引号字面量：`'` → `''`。
fn powershell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// 子进程管道字节 → 文本。
///
/// 先严格按 UTF-8；失败时在 Windows 上按 OEM 代码页（中文常见 GBK）回退。
/// 这是「有些内容仍乱码」的主修点：CLI 自己的 `SetConsoleOutputCP` 管不到管道。
fn decode_process_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    #[cfg(windows)]
    {
        let oem = windows_pipe_encoding();
        // 中日韩 OEM：直接信它。
        if is_cjk_legacy(oem) {
            return oem.decode(bytes).0.into_owned();
        }
        // 系统 Beta UTF-8、或 en-US 的 CP1252 等单字节页：对 GBK 字节会
        // 「无错误地」解成拉丁乱码（CI 上就是 `ÄãºÃ`），必须再试 GBK。
        let (gbk, _, _) = encoding_rs::GBK.decode(bytes);
        gbk.into_owned()
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// 流式回显用：缓冲区末尾可能卡着半个多字节字符，先解「完整前缀」。
fn decode_process_output_partial(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(err) => {
            let valid = err.valid_up_to();
            // 仅末尾不完整：先把已完整的 UTF-8 吐出去，等下一截。
            if err.error_len().is_none() && valid > 0 {
                return std::str::from_utf8(&bytes[..valid])
                    .expect("valid_up_to prefix")
                    .to_owned();
            }
            // 中间就非法 → 不是 UTF-8，整段按管道编码解。
            decode_process_output(bytes)
        }
    }
}

/// Windows 管道侧默认代码页：控制台程序重定向后走 OEM，不是父进程的 Console CP。
/// （父进程可能已 `SetConsoleOutputCP(65001)`，用 GetConsoleOutputCP 会解错。）
#[cfg(windows)]
fn windows_pipe_encoding() -> &'static Encoding {
    encoding_for_code_page(unsafe { GetOEMCP() })
        .or_else(|| encoding_for_code_page(unsafe { GetACP() }))
        .unwrap_or(encoding_rs::GBK)
}

#[cfg(windows)]
unsafe extern "system" {
    fn GetOEMCP() -> u32;
    fn GetACP() -> u32;
}

/// 中日韩遗留多字节页：对这些 locale 的管道输出应优先按 OEM 解，而不是一律 GBK。
#[cfg(windows)]
fn is_cjk_legacy(enc: &'static Encoding) -> bool {
    enc == encoding_rs::GBK
        || enc == encoding_rs::GB18030
        || enc == encoding_rs::BIG5
        || enc == encoding_rs::SHIFT_JIS
        || enc == encoding_rs::EUC_KR
}

/// 常见 Windows 代码页 → encoding_rs；未知则 `None`。
#[cfg_attr(not(windows), allow(dead_code))]
fn encoding_for_code_page(cp: u32) -> Option<&'static Encoding> {
    Some(match cp {
        65001 => encoding_rs::UTF_8,
        936 => encoding_rs::GBK,
        54936 => encoding_rs::GB18030,
        950 => encoding_rs::BIG5,
        932 => encoding_rs::SHIFT_JIS,
        949 => encoding_rs::EUC_KR,
        1250 => encoding_rs::WINDOWS_1250,
        1251 => encoding_rs::WINDOWS_1251,
        1252 => encoding_rs::WINDOWS_1252,
        1253 => encoding_rs::WINDOWS_1253,
        1254 => encoding_rs::WINDOWS_1254,
        1255 => encoding_rs::WINDOWS_1255,
        1256 => encoding_rs::WINDOWS_1256,
        1257 => encoding_rs::WINDOWS_1257,
        1258 => encoding_rs::WINDOWS_1258,
        874 => encoding_rs::WINDOWS_874,
        20866 => encoding_rs::KOI8_R,
        21866 => encoding_rs::KOI8_U,
        _ => return None,
    })
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
            // 配了窗口查询但没匹配上：不要回退到按进程名跳过。
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
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
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
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
    };
    use windows::core::BOOL;

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

/// 从参数 map 里解析可选的宿主（`host` 键）。默认 `Auto`。
pub fn host_from_params(map: &BTreeMap<String, Value>) -> Result<Host, ActionError> {
    match map.get("host").and_then(|v| v.as_str()) {
        None => Ok(Host::Auto),
        Some(s) => Host::parse(s),
    }
}

pub fn args_from_params(map: &BTreeMap<String, Value>) -> Vec<String> {
    match map.get("args") {
        Some(Value::Array(items)) => items
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
    let m = v
        .as_map()
        .ok_or_else(|| ActionError::InvalidParams("if_running_window 必须为 map".into()))?;
    let title_contains = m
        .get("title_contains")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionError::MissingParam("if_running_window.title_contains".into()))?
        .to_string();
    let title_excludes = match m.get("title_excludes") {
        Some(Value::Array(items)) => items
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

/// 由解析好的动作参数构建 [`LaunchSpec`]（shell.run / exec.run 共用）。
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
        input: opt_str(map, "input"),
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
        use crate::builtin::process_launch::{LaunchSpec, LaunchWait, TargetKind, launch};
        use std::path::PathBuf;
        let spec = LaunchSpec {
            program: PathBuf::from("cmd"),
            args: vec!["/C".into(), "timeout /t 30".into()],
            cwd: None,
            host: Host::Cmd,
            kind: TargetKind::Command,
            input: None,
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
        assert_eq!(resolve_host(Host::Auto, &p, TargetKind::Script), Host::Cmd);
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
        assert_eq!(resolve_host(Host::Cmd, &p, TargetKind::Script), Host::Cmd);
    }

    #[test]
    fn input_param_is_parsed() {
        let mut m = BTreeMap::new();
        m.insert("command".into(), Value::Str("echo".into()));
        m.insert("input".into(), Value::Str("y\n".into()));
        let spec = launch_spec_from_command_params(&m, PathBuf::from("echo"), TargetKind::Command)
            .expect("spec");
        assert_eq!(spec.input.as_deref(), Some("y\n"));
    }

    /// `input` 要真正送到子进程 stdin，并且在写完后关闭（否则子进程等 EOF 会挂住）。
    #[tokio::test]
    async fn input_reaches_child_stdin() {
        let (program, args) = stdin_echo_command();
        let spec = LaunchSpec {
            program,
            args,
            cwd: None,
            host: Host::None,
            kind: TargetKind::Command,
            input: Some("corex-stdin\n".into()),
            allow_nonzero: false,
            wait: LaunchWait::Sync,
            if_running: Default::default(),
            if_running_window: None,
        };
        let out = launch(spec).await.expect("stdin roundtrip");
        assert!(out.stdout.contains("corex-stdin"), "got: {}", out.stdout);
    }

    /// 把 stdin 原样回显到 stdout 的命令。
    fn stdin_echo_command() -> (PathBuf, Vec<String>) {
        #[cfg(windows)]
        {
            (PathBuf::from("findstr"), vec![".".into()])
        }
        #[cfg(not(windows))]
        {
            (PathBuf::from("cat"), Vec::new())
        }
    }

    /// GBK「你好」不能当 UTF-8 解；回退解码后应是中文。
    #[test]
    fn decode_falls_back_from_gbk_bytes() {
        let (gbk, _, _) = encoding_rs::GBK.encode("你好");
        assert!(std::str::from_utf8(&gbk).is_err());
        let text = decode_process_output(&gbk);
        #[cfg(windows)]
        assert_eq!(text, "你好");
        #[cfg(not(windows))]
        {
            // 非 Windows 无 OEM 回退，至少不能 panic；保留 lossy 行为。
            assert!(!text.is_empty());
        }
    }

    /// 合法 UTF-8 不能被误判成 GBK。
    #[test]
    fn decode_keeps_utf8_chinese() {
        assert_eq!(decode_process_output("你好".as_bytes()), "你好");
    }

    #[test]
    fn decode_partial_holds_incomplete_utf8_tail() {
        let mut bytes = "你好".as_bytes().to_vec();
        // 砍掉最后一字节，留下半个 UTF-8 序列。
        bytes.pop();
        let partial = decode_process_output_partial(&bytes);
        assert_eq!(partial, "你");
    }

    #[test]
    fn code_page_936_maps_to_gbk() {
        assert_eq!(encoding_for_code_page(936).map(|e| e.name()), Some("GBK"));
    }

    #[test]
    fn powershell_quotes_embed_single_quotes() {
        assert_eq!(powershell_single_quote("a'b"), "'a''b'");
    }
}
