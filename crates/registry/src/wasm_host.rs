//! 第三方 WASM 动作插件的 Wasmtime 宿主（feature `wasm`）。
//!
//! 会创建一个真正的 [`Engine`]（async + 组件模型配置）与一个
//! 面向 WASI 的 store 桩。`corex:plugin-sdk/action` 的完整 WIT bindgen
//! 还没生成——[`WasmPluginHost::instantiate`] 因此在尽可能校验组件字节之后
//! 返回一个明确的错误。

use corex_core::{Action, ActionError};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p2::add_to_linker_async;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

/// 每个 store 的宿主状态：WASI 上下文 + 资源表（组件模型模式）。
pub struct HostState {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl HostState {
    /// 构建默认 WASI 状态（stdio 继承；暂不做 preopen）。
    pub fn new() -> Self {
        let ctx = WasiCtxBuilder::new().inherit_stdio().inherit_env().build();
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

/// 经 wasmtime 加载 WIT world 的宿主（组件模型）。
pub struct WasmPluginHost {
    engine: Engine,
}

impl WasmPluginHost {
    /// 创建启用了 async 与 Wasm 组件模型的引擎。
    pub fn new() -> Result<Self, ActionError> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        let engine = Engine::new(&config)
            .map_err(|e| ActionError::other(format!("wasmtime Engine 初始化失败: {e}")))?;
        Ok(Self { engine })
    }

    /// 共享的引擎引用（给进阶调用方用）。
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// 构建一个带 [`HostState`] 的全新 [`Store`]（WasiCtxBuilder 模式）。
    pub fn new_store(&self) -> Store<HostState> {
        Store::new(&self.engine, HostState::new())
    }

    /// 构建 linker 并挂上 WASI Preview 2 的宿主函数。
    pub fn new_linker(&self) -> Result<Linker<HostState>, ActionError> {
        let mut linker = Linker::new(&self.engine);
        add_to_linker_async(&mut linker)
            .map_err(|e| ActionError::other(format!("WASI linker 配置失败: {e}")))?;
        Ok(linker)
    }

    /// 加载一个 `.wasm` 组件插件。
    ///
    /// 先校验该文件能被解析成组件、备好 store/linker，
    /// 然后在 `corex:plugin-sdk@0.1.0` 的 WIT bindgen 生成并接好之前返回错误
    /// （见 `crates/plugin-sdk/wit/corex-action.wit`）。
    pub fn instantiate(&self, path: &Path) -> Result<Arc<dyn Action>, ActionError> {
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

        // 先碰一下 store 与 linker，让宿主路径在 bindgen 就绪之前也能被走到。
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

/// 早期脚手架用过的向后兼容别名。
pub type WasmHost = WasmPluginHost;
