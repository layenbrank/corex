//! Process helpers for supervised jobs.

use std::fs::OpenOptions;
use std::path::Path;
use std::process::{Command, Stdio};

/// Returns true when `pid` appears to be running.
pub fn is_pid_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .any(|l| l.contains(&pid.to_string()))
            })
            .unwrap_or(false)
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
