//! Process helpers for supervised jobs.

use crate::supervisor::JobMeta;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::time::Duration;

/// Maximum drift allowed when comparing process start timestamps (ms).
const START_TIME_TOLERANCE_MS: u64 = 2_000;

/// Returns true when `pid` appears to be running.
pub fn is_pid_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    process_exists(pid)
}

/// Capture identity fields for the current supervisor process.
pub fn current_supervisor_identity() -> (PathBuf, u64) {
    let pid = std::process::id();
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("corex"));
    let started_at_ms = process_started_at_ms(pid).unwrap_or(0);
    (exe, started_at_ms)
}

/// Capture identity fields for a spawned child pid.
pub fn child_supervisor_identity(pid: u32, exe: &Path) -> (PathBuf, u64) {
    let started_at_ms = process_started_at_ms(pid).unwrap_or(0);
    (exe.to_path_buf(), started_at_ms)
}

/// Returns true when `meta` still refers to the original supervisor process.
pub fn is_supervisor_alive(meta: &JobMeta) -> bool {
    if meta.pid == 0 {
        return false;
    }
    if !process_exists(meta.pid) {
        return false;
    }
    let Some(expected_exe) = meta.supervisor_exe.as_ref() else {
        return false;
    };
    let Some(actual_exe) = process_exe_path(meta.pid) else {
        return false;
    };
    if !exe_paths_match(expected_exe, &actual_exe) {
        return false;
    }
    match meta.started_at_ms {
        None => false,
        Some(0) => true,
        Some(expected) => match process_started_at_ms(meta.pid) {
            Some(actual) => time_diff_ms(expected, actual) <= START_TIME_TOLERANCE_MS,
            None => false,
        },
    }
}

fn time_diff_ms(a: u64, b: u64) -> u64 {
    a.abs_diff(b)
}

fn exe_paths_match(expected: &Path, actual: &Path) -> bool {
    if expected == actual {
        return true;
    }
    if expected.file_name().is_some()
        && expected.file_name() == actual.file_name()
        && expected
            .to_string_lossy()
            .eq_ignore_ascii_case(&actual.to_string_lossy())
    {
        return true;
    }
    match (expected.canonicalize(), actual.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn process_exists(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        unsafe {
            windows::Win32::System::Threading::OpenProcess(
                windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            )
            .ok()
            .map(|handle| {
                let _ = windows::Win32::Foundation::CloseHandle(handle);
                true
            })
            .unwrap_or(false)
        }
    }
}

fn process_started_at_ms(pid: u32) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        unix_process_started_at_ms(pid)
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        let _ = pid;
        None
    }
    #[cfg(windows)]
    {
        win_process_started_at_ms(pid)
    }
}

fn process_exe_path(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        let _ = pid;
        None
    }
    #[cfg(windows)]
    {
        win_process_exe_path(pid)
    }
}

#[cfg(target_os = "linux")]
fn unix_process_started_at_ms(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(')')?.1.trim();
    let starttime: u64 = rest.split_whitespace().nth(19)?.parse().ok()?;
    let clk_tck = unix_clock_ticks();
    let boot_ms = unix_boot_time_ms()?;
    Some(boot_ms + (starttime * 1000 / clk_tck))
}

#[cfg(target_os = "linux")]
fn unix_clock_ticks() -> u64 {
    100
}

#[cfg(target_os = "linux")]
fn unix_boot_time_ms() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/stat").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("btime ") {
            let secs: u64 = rest.trim().parse().ok()?;
            return Some(secs * 1000);
        }
    }
    None
}

#[cfg(windows)]
fn win_process_started_at_ms(pid: u32) -> Option<u64> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut creation = FILETIME::default();
        let ok = GetProcessTimes(
            handle,
            &mut creation,
            &mut FILETIME::default(),
            &mut FILETIME::default(),
            &mut FILETIME::default(),
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        Some(filetime_to_unix_ms(&creation))
    }
}

#[cfg(windows)]
fn win_process_exe_path(pid: u32) -> Option<PathBuf> {
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
        QueryFullProcessImageNameW,
    };
    use windows::core::PWSTR;

    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        )
        .ok()?;
        let mut buf = vec![0u16; MAX_PATH as usize];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        Some(PathBuf::from(String::from_utf16_lossy(
            &buf[..size as usize],
        )))
    }
}

#[cfg(windows)]
fn filetime_to_unix_ms(ft: &windows::Win32::Foundation::FILETIME) -> u64 {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    ticks
        .saturating_div(10_000)
        .saturating_sub(11_644_473_600_000)
}

/// Terminate `pid` and its child process tree.
pub fn kill_process_tree(pid: u32) -> std::io::Result<()> {
    if pid == 0 {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("taskkill 失败 (pid {pid})"),
            ))
        }
    }
    #[cfg(unix)]
    {
        let _ = Command::new("pkill")
            .args(["-TERM", "-P", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(Duration::from_millis(300));
        let _ = Command::new("pkill")
            .args(["-KILL", "-P", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let status = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() || !process_exists(pid) {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("kill 失败 (pid {pid})"),
            ))
        }
    }
}

/// Spawn a detached child process for a supervisor run loop.
pub fn spawn_detached(exe: &Path, args: &[&str], log_path: Option<&Path>) -> std::io::Result<u32> {
    let mut cmd = Command::new(exe);
    cmd.args(args).stdin(Stdio::null());
    if let Some(log) = log_path {
        let file = OpenOptions::new().create(true).append(true).open(log)?;
        let stderr = file.try_clone()?;
        cmd.stdout(Stdio::from(file)).stderr(Stdio::from(stderr));
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    let child = cmd.spawn()?;
    Ok(child.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::{JobKind, JobMeta};

    #[test]
    fn pid_zero_is_not_running() {
        assert!(!is_pid_running(0));
        assert!(!is_supervisor_alive(&JobMeta {
            id: "x".into(),
            kind: JobKind::Watch,
            directive_name: "x".into(),
            directive_path: PathBuf::from("x.yaml"),
            pid: 0,
            expr: None,
            paths: vec![],
            supervisor_exe: None,
            started_at_ms: None,
        }));
    }

    #[test]
    fn legacy_meta_without_identity_is_not_alive() {
        let meta = JobMeta {
            id: "x".into(),
            kind: JobKind::Watch,
            directive_name: "x".into(),
            directive_path: PathBuf::from("x.yaml"),
            pid: 999_999,
            expr: None,
            paths: vec![],
            supervisor_exe: None,
            started_at_ms: None,
        };
        assert!(!is_supervisor_alive(&meta));
    }

    #[test]
    fn current_process_identity_matches() {
        let (exe, started_at_ms) = current_supervisor_identity();
        let meta = JobMeta {
            id: "x".into(),
            kind: JobKind::Watch,
            directive_name: "x".into(),
            directive_path: PathBuf::from("x.yaml"),
            pid: std::process::id(),
            expr: None,
            paths: vec![],
            supervisor_exe: Some(exe),
            started_at_ms: Some(started_at_ms),
        };
        assert!(is_supervisor_alive(&meta));
    }
}
