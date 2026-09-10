//! Corex 插件 SDK —— WIT world 定义与宿主 / 来宾辅助。
//!
//! 规范契约：`wit/corex-action.wit`（`package corex:plugin-sdk@0.1.0`）。
//! 宿主侧 wasmtime `bindgen!` 的生成在 `corex-registry`（feature `wasm`）。

#![allow(dead_code)]

/// 与 `wit/corex-action.wit` 一致的 WIT 包标识。
pub const WIT_PACKAGE: &str = "corex:plugin-sdk@0.1.0";

/// 插件导出的 world 名。
pub const WIT_WORLD: &str = "corex-action";

/// 来宾导出的接口名。
pub const WIT_INTERFACE: &str = "action";

/// 内嵌的 WIT 源码，供工具 / 诊断使用。
pub const WIT_SOURCE: &str = include_str!("../wit/corex-action.wit");

/// 面向来宾的元数据镜像（宿主侧用 `corex_core::ActionMeta`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginActionMeta {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// 来宾执行结果的镜像。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginExecResult {
    pub ok: bool,
    /// 成功时是 JSON 编码的 Value，失败时是错误消息。
    pub payload: String,
}
