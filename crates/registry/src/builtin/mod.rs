//! Built-in actions, each behind a Cargo feature.

use crate::ActionRegistry;

pub mod util;

#[cfg(any(feature = "act-shell", feature = "act-exec"))]
pub mod process_launch;

#[cfg(any(
    feature = "act-copy",
    feature = "act-generate",
    feature = "act-compression"
))]
pub mod filter;

#[cfg(feature = "act-shell")]
pub mod shell;
#[cfg(feature = "act-http")]
pub mod http;
#[cfg(feature = "act-clipboard")]
pub mod clipboard;
#[cfg(feature = "act-notify")]
pub mod notify;
#[cfg(feature = "act-file")]
pub mod file;
#[cfg(feature = "act-file")]
pub mod dir;
#[cfg(feature = "act-template")]
pub mod template;
#[cfg(feature = "act-cron")]
pub mod cron;
#[cfg(feature = "act-keyring")]
pub mod keyring;
#[cfg(feature = "act-copy")]
pub mod copy;
#[cfg(feature = "act-scrub")]
pub mod scrub;
#[cfg(feature = "act-shade")]
pub mod shade;
#[cfg(feature = "act-compression")]
pub mod compression;
#[cfg(feature = "act-generate")]
pub mod generate;
#[cfg(feature = "act-exec")]
pub mod exec;
#[cfg(feature = "act-bootstrap")]
pub mod bootstrap;
#[cfg(feature = "act-codec")]
pub mod codec;
#[cfg(feature = "act-scan")]
pub mod scan;
#[cfg(feature = "act-capture")]
pub mod capture;
#[cfg(feature = "act-morph")]
pub mod morph;
#[cfg(feature = "act-ui")]
pub mod ui;
#[cfg(feature = "act-ui")]
pub mod ui_kernel;
#[cfg(feature = "act-sys")]
pub mod sys;

/// Register every feature-enabled builtin into `registry`.
pub fn register_all(registry: &mut ActionRegistry) {
    #[cfg(feature = "act-shell")]
    shell::register(registry);
    #[cfg(feature = "act-http")]
    http::register(registry);
    #[cfg(feature = "act-clipboard")]
    clipboard::register(registry);
    #[cfg(feature = "act-notify")]
    notify::register(registry);
    #[cfg(feature = "act-file")]
    file::register(registry);
    #[cfg(feature = "act-file")]
    dir::register(registry);
    #[cfg(feature = "act-template")]
    template::register(registry);
    #[cfg(feature = "act-cron")]
    cron::register(registry);
    #[cfg(feature = "act-keyring")]
    keyring::register(registry);
    #[cfg(feature = "act-copy")]
    copy::register(registry);
    #[cfg(feature = "act-scrub")]
    scrub::register(registry);
    #[cfg(feature = "act-shade")]
    shade::register(registry);
    #[cfg(feature = "act-compression")]
    compression::register(registry);
    #[cfg(feature = "act-generate")]
    generate::register(registry);
    #[cfg(feature = "act-exec")]
    exec::register(registry);
    #[cfg(feature = "act-bootstrap")]
    bootstrap::register(registry);
    #[cfg(feature = "act-codec")]
    codec::register(registry);
    #[cfg(feature = "act-scan")]
    scan::register(registry);
    #[cfg(feature = "act-capture")]
    capture::register(registry);
    #[cfg(feature = "act-morph")]
    morph::register(registry);
    #[cfg(feature = "act-ui")]
    ui::register(registry);
    #[cfg(feature = "act-sys")]
    sys::register(registry);
}
