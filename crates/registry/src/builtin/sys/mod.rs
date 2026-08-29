//! PC system helpers: dialog / url / process (`act-sys`).

mod dialog;
mod process;
mod url;

use crate::ActionRegistry;

pub fn register(registry: &mut ActionRegistry) {
    dialog::register(registry);
    url::register(registry);
    process::register(registry);
}
