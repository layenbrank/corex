//! Built-in actions, each behind a Cargo feature.

use crate::ActionRegistry;

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
#[cfg(feature = "act-template")]
pub mod template;
#[cfg(feature = "act-cron")]
pub mod cron;
#[cfg(feature = "act-keyring")]
pub mod keyring;

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
    #[cfg(feature = "act-template")]
    template::register(registry);
    #[cfg(feature = "act-cron")]
    cron::register(registry);
    #[cfg(feature = "act-keyring")]
    keyring::register(registry);
}
