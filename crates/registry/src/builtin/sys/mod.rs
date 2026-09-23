//! PC 系统辅助动作：dialog / url / process / launch / lock / sleep（`act-sys`）。

mod dialog;
mod launch;
mod lock;
mod process;
mod sleep;
mod url;

use crate::ActionRegistry;

pub fn register(registry: &mut ActionRegistry) {
    dialog::register(registry);
    url::register(registry);
    process::register(registry);
    launch::register(registry);
    lock::register(registry);
    sleep::register(registry);
}
