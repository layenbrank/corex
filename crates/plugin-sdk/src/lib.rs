//! Corex plugin SDK — WIT world definitions and host/guest helpers.
//!
//! The canonical contract lives in `wit/corex-action.wit`. Full wasmtime
//! binding generation is deferred until the wasm host is wired in P3.

#![allow(dead_code)]

/// WIT package identifier matching `wit/corex-action.wit`.
pub const WIT_PACKAGE: &str = "corex:plugin-sdk@0.1.0";

/// Embedded WIT source for tooling / diagnostics.
pub const WIT_SOURCE: &str = include_str!("../wit/corex-action.wit");

/// Guest-facing metadata mirror (host side uses `corex_core::ActionMeta`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginActionMeta {
    pub id: String,
    pub name: String,
    pub description: String,
}
