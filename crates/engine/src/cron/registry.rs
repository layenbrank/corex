//! Global cron engine handle for `cron.schedule` action.

use super::engine::CronEngine;
use std::sync::{Arc, OnceLock};

static CRON_ENGINE: OnceLock<Arc<CronEngine>> = OnceLock::new();

/// Bind the active supervisor's [`CronEngine`].
pub fn bind_cron_engine(engine: Arc<CronEngine>) {
    let _ = CRON_ENGINE.set(engine);
}

/// Find the bound cron engine (supervisor process only).
pub fn find_cron_engine() -> Option<Arc<CronEngine>> {
    CRON_ENGINE.get().cloned()
}
