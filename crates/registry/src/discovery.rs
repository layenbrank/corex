//! 扫描插件目录里的 `*.wasm` 组件，并注册加载成功的那些。

use crate::ActionRegistry;
use crate::wasm_host::WasmPluginHost;
use std::path::Path;
use tracing::{info, warn};

/// 在 `plugin_dir` 下发现 `*.wasm` 文件，逐个尝试经 [`WasmPluginHost`] 加载，
/// 并把加载成功的注册为动作。
///
/// 失败的会记日志并跳过（比如 WIT bindgen 还没生成）。
/// 返回尝试过的路径列表。
pub fn discover(
    plugin_dir: &Path,
    registry: &mut ActionRegistry,
) -> Result<Vec<String>, corex_core::ActionError> {
    if !plugin_dir.exists() {
        info!(dir = %plugin_dir.display(), "插件目录不存在，跳过发现");
        return Ok(Vec::new());
    }

    let host = match WasmPluginHost::new() {
        Ok(h) => h,
        Err(e) => {
            warn!(error = %e, "WasmPluginHost 初始化失败，跳过插件发现");
            return Ok(Vec::new());
        }
    };

    let mut found = Vec::new();
    let rd = std::fs::read_dir(plugin_dir).map_err(corex_core::ActionError::from)?;
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("wasm") {
            continue;
        }
        let path_s = path.display().to_string();
        found.push(path_s.clone());
        match host.instantiate(&path) {
            Ok(action) => {
                let id = action.meta().id.clone();
                info!(path = %path_s, action_id = %id, "WASM 插件已加载并注册");
                registry.register(action);
            }
            Err(e) => {
                warn!(path = %path_s, error = %e, "WASM 插件加载失败，已跳过");
            }
        }
    }

    info!(
        dir = %plugin_dir.display(),
        scanned = found.len(),
        registered = registry.len(),
        "插件发现完成"
    );
    Ok(found)
}

/// 只列出 `*.wasm` 路径、不做加载（工具 / 测试用）。
pub fn wasm_files(plugin_dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    if !plugin_dir.exists() {
        return found;
    }
    if let Ok(rd) = std::fs::read_dir(plugin_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                found.push(path.display().to_string());
            }
        }
    }
    found
}
