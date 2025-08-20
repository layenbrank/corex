use anyhow::Result;
use notify_rust;

/// 统一的通知服务
pub struct Notification;

impl Notification {
    /// 显示成功通知
    pub fn success(title: &str, message: &str) -> Result<()> {
        notify_rust::Notification::new()
            .summary(title)
            .body(message)
            .icon("dialog-information")
            .show()
            .map_err(|e| anyhow::anyhow!("显示成功通知失败: {}", e))?;
        Ok(())
    }

    /// 显示错误通知
    pub fn error(title: &str, message: &str) -> Result<()> {
        notify_rust::Notification::new()
            .summary(title)
            .body(message)
            .icon("dialog-error")
            .show()
            .map_err(|e| anyhow::anyhow!("显示错误通知失败: {}", e))?;
        Ok(())
    }

    /// 显示信息通知
    pub fn info(title: &str, message: &str) -> Result<()> {
        notify_rust::Notification::new()
            .summary(title)
            .body(message)
            .icon("dialog-information")
            .show()
            .map_err(|e| anyhow::anyhow!("显示信息通知失败: {}", e))?;
        Ok(())
    }
}
