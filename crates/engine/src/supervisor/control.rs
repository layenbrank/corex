//! supervisor 控制消息。

use std::fmt;
use std::str::FromStr;

/// 发给 watch/cron supervisor 的控制消息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlMsg {
    RunNow,
    Status,
    Stop,
    /// 停止 supervisor，并终止进行中的流水线 / 子进程。
    StopForce,
}

impl fmt::Display for ControlMsg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunNow => write!(f, "RUN_NOW"),
            Self::Status => write!(f, "STATUS"),
            Self::Stop => write!(f, "STOP"),
            Self::StopForce => write!(f, "STOP_FORCE"),
        }
    }
}

impl FromStr for ControlMsg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "run_now" => Ok(Self::RunNow),
            "status" => Ok(Self::Status),
            "stop" => Ok(Self::Stop),
            "stop_force" => Ok(Self::StopForce),
            other => Err(format!(
                "未知 control 消息: {other}（支持 run-now、status、stop、stop-force）"
            )),
        }
    }
}

/// 把控制消息写进作业目录。
pub fn send_control(job_dir: &std::path::Path, msg: ControlMsg) -> std::io::Result<()> {
    std::fs::write(job_dir.join("control.cmd"), msg.to_string())
}

/// 轮询并消费一条待处理的控制消息。
pub fn poll_control(job_dir: &std::path::Path) -> Option<ControlMsg> {
    let path = job_dir.join("control.cmd");
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    ControlMsg::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lowercase_aliases() {
        assert_eq!(ControlMsg::from_str("run-now").unwrap(), ControlMsg::RunNow);
        assert_eq!(ControlMsg::from_str("status").unwrap(), ControlMsg::Status);
        assert_eq!(ControlMsg::from_str("stop").unwrap(), ControlMsg::Stop);
        assert_eq!(
            ControlMsg::from_str("stop-force").unwrap(),
            ControlMsg::StopForce
        );
    }
}
