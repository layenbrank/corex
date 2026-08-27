//! Supervisor control messages.

use std::fmt;
use std::str::FromStr;

/// Control message for watch/cron supervisors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMsg {
    RunNow,
    Status,
    Stop,
}

impl fmt::Display for ControlMsg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunNow => write!(f, "RUN_NOW"),
            Self::Status => write!(f, "STATUS"),
            Self::Stop => write!(f, "STOP"),
        }
    }
}

impl FromStr for ControlMsg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "RUN_NOW" => Ok(Self::RunNow),
            "STATUS" => Ok(Self::Status),
            "STOP" => Ok(Self::Stop),
            other => Err(format!("未知 control 消息: {other}")),
        }
    }
}

/// Write a control message into a job directory.
pub fn send_control(job_dir: &std::path::Path, msg: ControlMsg) -> std::io::Result<()> {
    std::fs::write(job_dir.join("control.cmd"), msg.to_string())
}

/// Poll and consume a pending control message.
pub fn poll_control(job_dir: &std::path::Path) -> Option<ControlMsg> {
    let path = job_dir.join("control.cmd");
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    ControlMsg::from_str(&text).ok()
}
