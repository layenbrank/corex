//! PC 系统辅助动作：dialog / url / process / sleep（`act-sys`）。

mod dialog;
mod process;
mod sleep;
mod url;

use crate::ActionRegistry;

pub fn register(registry: &mut ActionRegistry) {
    dialog::register(registry);
    url::register(registry);
    process::register(registry);
    sleep::register(registry);
}
