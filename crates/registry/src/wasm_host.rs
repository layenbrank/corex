//! Wasmtime host for third-party WASM action plugins (feature `wasm`).
//!
//! Creates a real [`Engine`] with async + component-model config and a
//! WASI-ready store stub. Full WIT bindgen for `corex:plugin-sdk/action`
//! is not generated yet — [`WasmPluginHost::load_plugin`] therefore returns
//! a clear error after validating the component bytes when possible.

use corex_core::{Action, ActionError};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p2::{
    add_to_linker_async, IoView, WasiCtx, WasiCtxBuilder, WasiView,
};

/// Per-store host state: WASI context + resource table (component-model pattern).
pub struct HostState {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl IoView for HostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl HostState {
    /// Build default WASI state (stdio inherited; no preopens yet).
    pub fn new() -> Self {
        let ctx = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_env()
            .build();
        Self {
            ctx,
            table: ResourceTable::new(),
        }
    }
}

impl Default for HostState {
    fn default() -> Self {
        Self::new()
    }
}

/// Host that loads WIT worlds via wasmtime (component model).
pub struct WasmPluginHost {
    engine: Engine,
}

impl WasmPluginHost {
    /// Create an engine with async support and the Wasm component model enabled.
    pub fn new() -> Result<Self, ActionError> {
        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        let engine = Engine::new(&config).map_err(|e| {
            ActionError::other(format!("wasmtime Engine 初始化失败: {e}"))
        })?;
        Ok(Self { engine })
    }

    /// Shared engine reference (for advanced callers).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Build a fresh [`Store`] with [`HostState`] (WasiCtxBuilder pattern).
    pub fn new_store(&self) -> Store<HostState> {
        Store::new(&self.engine, HostState::new())
    }

    /// Build a linker and attach WASI Preview 2 host functions.
    pub fn new_linker(&self) -> Result<Linker<HostState>, ActionError> {
        let mut linker = Linker::new(&self.engine);
        add_to_linker_async(&mut linker).map_err(|e| {
            ActionError::other(format!("WASI linker 配置失败: {e}"))
        })?;
        Ok(linker)
    }

    /// Load a `.wasm` component plugin.
    ///
    /// Validates that the file parses as a component, prepares store/linker,
    /// then returns an error until WIT bindgen for `corex:plugin-sdk@0.1.0`
    /// is generated and wired (see `crates/plugin-sdk/wit/corex-action.wit`).
    pub fn load_plugin(&self, path: &Path) -> Result<Arc<dyn Action>, ActionError> {
        let path_str = path.display().to_string();
        info!(path = %path_str, "wasm_host: 加载插件组件");

        if !path.exists() {
            return Err(ActionError::NotFound(format!(
                "WASM 插件不存在: {path_str}"
            )));
        }

        let component = Component::from_file(&self.engine, path).map_err(|e| {
            ActionError::execution(format!(
                "无法将 {} 解析为 Wasm 组件（需要 component model / WASI-P2）: {e}",
                path_str
            ))
        })?;

        // Touch store + linker so the host path is exercised even before bindgen.
        let _store = self.new_store();
        let _linker = self.new_linker()?;
        let _ = (&component, &_store, &_linker);

        debug!(
            path = %path_str,
            "wasm_host: 组件已解析；WIT bindgen 尚未生成，无法实例化 action 接口"
        );

        Err(ActionError::other(format!(
            "WASM 插件 {path_str} 已解析为组件，但 WIT bindgen（corex:plugin-sdk/action）尚未生成，无法实例化；请先运行 wit-bindgen / wasmtime::component::bindgen! 后再注册"
        )))
    }
}

impl Default for WasmPluginHost {
    fn default() -> Self {
        Self::new().expect("wasmtime Engine 默认初始化不应失败")
    }
}

/// Back-compat alias used by early scaffolding.
pub type WasmHost = WasmPluginHost;
