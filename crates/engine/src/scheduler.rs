//! Optional cron trigger scheduling (feature `cron`).

use crate::definition::Trigger;
use tracing::warn;

/// Stub / optional scheduler for cron triggers.
///
/// Not wired into CLI/daemon yet. Prefer external schedulers until this is complete.
#[doc(hidden)]
pub struct Scheduler;

impl Scheduler {
    /// Register cron triggers from a directive. Without the `cron` feature this
    /// only logs and returns Ok.
    pub async fn register_triggers(
        directive_name: &str,
        triggers: &[Trigger],
    ) -> Result<(), corex_core::EngineError> {
        for t in triggers {
            if let Trigger::Cron { expr } = t {
                Self::register_cron(directive_name, expr).await?;
            }
        }
        Ok(())
    }

    #[cfg(feature = "cron")]
    async fn register_cron(name: &str, expr: &str) -> Result<(), corex_core::EngineError> {
        use tracing::info;
        let _sched = tokio_cron_scheduler::JobScheduler::new()
            .await
            .map_err(|e| corex_core::EngineError::other(format!("cron 调度器初始化失败: {e}")))?;
        info!(directive = name, expr, "已注册 cron 触发器（骨架）");
        Ok(())
    }

    #[cfg(not(feature = "cron"))]
    async fn register_cron(name: &str, expr: &str) -> Result<(), corex_core::EngineError> {
        warn!(
            directive = name,
            expr,
            "cron feature 未启用，跳过触发器注册"
        );
        Ok(())
    }
}
