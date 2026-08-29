//! UI automation domain: window / element / input facades + Windows adapter.
//!
//! Layers: Action facade → domain (`win` services) → Win32/UIA.
//! CLI probes call the same `win::*` entry points (or Action::execute).

#[macro_use]
mod macros;

pub mod element;
pub mod input;
pub mod window;

#[cfg(windows)]
pub(crate) mod win;

use crate::ActionRegistry;

/// Register all `ui.*` actions.
pub fn register(registry: &mut ActionRegistry) {
    window::register(registry);
    element::register(registry);
    input::register(registry);
}
