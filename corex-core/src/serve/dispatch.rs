use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use serde_json::Value;

use crate::invoke::{InvokeContext, WireArgs, invoke, ipc_data};
use crate::serve::state::DaemonState;

/// 模块 dispatch 结果
pub struct DispatchResult {
    pub path: Option<PathBuf>,
    pub data: Option<Value>,
}

/// 按 module 名分发（统一 invoke 层）
pub fn dispatch(
    state: &mut DaemonState,
    module: &str,
    wire: WireArgs,
) -> Result<DispatchResult> {
    #[cfg(feature = "screenshot")]
    if module == "screenshot" {
        if matches!(
            wire.action.as_deref(),
            Some("capture") | Some("monitors")
        ) {
            let _ = state.refresh_monitors();
        }
    }

    let ctx = InvokeContext::daemon(state);
    let result = invoke(module, wire, &ctx)?;
    Ok(DispatchResult {
        path: result.artifact.as_ref().and_then(|a| a.path.clone()),
        data: ipc_data(&result).or(result.data),
    })
}

/// 处理单条 invoke 请求
pub fn handle_invoke(
    state: &mut DaemonState,
    id: u64,
    module: &str,
    action: Option<String>,
    format: Option<String>,
    algorithm: Option<String>,
    args: Value,
) -> crate::serve::protocol::Response {
    let start = Instant::now();
    let wire = WireArgs {
        action,
        format,
        algorithm,
        flags: args,
    };
    match dispatch(state, module, wire) {
        Ok(result) => crate::serve::protocol::Response::success(
            id,
            result.path.map(|p| p.to_string_lossy().into_owned()),
            result.data,
            start.elapsed().as_millis() as u64,
        ),
        Err(err) => crate::serve::protocol::Response::failure(
            id,
            err.to_string(),
            start.elapsed().as_millis() as u64,
        ),
    }
}
