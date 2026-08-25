//! Wasm host stub (wasm feature). Avoids hard wasmtime dependency for now.

use tracing::info;

/// Placeholder host that would load WIT worlds via wasmtime.
pub struct WasmHost;

impl WasmHost {
    pub fn new() -> Self {
        Self
    }

    /// Discover / register wasm actions. Stub always succeeds.
    pub fn load_plugin(&self, path: &str) -> Result<(), corex_core::ActionError> {
        info!(path, "wasm_host.load_plugin 骨架");
        Ok(())
    }
}

impl Default for WasmHost {
    fn default() -> Self {
        Self::new()
    }
}
