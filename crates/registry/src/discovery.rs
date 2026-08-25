//! Plugin discovery stub (wasm feature).

use std::path::Path;
use tracing::info;

/// Scan `plugin_dir` for wasm plugins. Skeleton — returns Ok without loading.
pub fn discover(plugin_dir: &Path) -> Result<Vec<String>, corex_core::ActionError> {
    info!(dir = %plugin_dir.display(), "wasm discovery 骨架：跳过加载");
    if plugin_dir.exists() {
        let mut found = Vec::new();
        if let Ok(rd) = std::fs::read_dir(plugin_dir) {
            for entry in rd.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                    found.push(path.display().to_string());
                }
            }
        }
        Ok(found)
    } else {
        Ok(Vec::new())
    }
}
