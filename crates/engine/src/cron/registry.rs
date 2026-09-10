//! 供 `cron.schedule` 动作使用的全局 cron 引擎句柄。

use super::engine::CronEngine;
use std::sync::{Arc, OnceLock};

static CRON_ENGINE: OnceLock<Arc<CronEngine>> = OnceLock::new();

/// 绑定当前 supervisor 的 [`CronEngine`]。
pub fn bind_cron_engine(engine: Arc<CronEngine>) {
    let _ = CRON_ENGINE.set(engine);
}

/// 取出已绑定的 cron 引擎（仅 supervisor 进程内）。
pub fn find_cron_engine() -> Option<Arc<CronEngine>> {
    CRON_ENGINE.get().cloned()
}
