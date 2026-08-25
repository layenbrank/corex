//! Corex plugin SDK — WIT world definitions and host/guest helpers.
//!
//! Canonical contract: `wit/corex-action.wit` (`package corex:plugin-sdk@0.1.0`).
//! Host-side wasmtime `bindgen!` generation lives in `corex-registry` (feature `wasm`).

#![allow(dead_code)]

/// WIT package identifier matching `wit/corex-action.wit`.
pub const WIT_PACKAGE: &str = "corex:plugin-sdk@0.1.0";

/// World name exported by plugins.
pub const WIT_WORLD: &str = "corex-action";

/// Interface name guests export.
pub const WIT_INTERFACE: &str = "action";

/// Embedded WIT source for tooling / diagnostics.
pub const WIT_SOURCE: &str = include_str!("../wit/corex-action.wit");

/// Guest-facing metadata mirror (host side uses `corex_core::ActionMeta`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginActionMeta {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Guest execute result mirror.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginExecResult {
    pub ok: bool,
    /// JSON-encoded Value on success, or error message on failure.
    pub payload: String,
}
