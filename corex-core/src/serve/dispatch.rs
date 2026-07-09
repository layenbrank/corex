use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::serve::state::DaemonState;

/// 模块 dispatch 结果
pub struct DispatchResult {
    pub path: Option<PathBuf>,
}

/// 按 module 名分发到对应 `run` 入口
pub fn dispatch(state: &DaemonState, module: &str, args: &Value) -> Result<DispatchResult> {
    match module {
        #[cfg(feature = "screenshot")]
        "screenshot" => {
            let parsed: crate::screenshot::schema::Args =
                serde_json::from_value(args.clone()).context("screenshot args 解析失败")?;
            let path = crate::screenshot::service::capture(
                &parsed,
                state.monitors.as_deref(),
            )?;
            Ok(DispatchResult {
                path: Some(path),
            })
        }
        #[cfg(feature = "copy")]
        "copy" => {
            let parsed: crate::copy::schema::Args =
                serde_json::from_value(args.clone()).context("copy args 解析失败")?;
            crate::copy::run(&parsed)?;
            Ok(DispatchResult {
                path: Some(PathBuf::from(&parsed.to)),
            })
        }
        #[cfg(feature = "scrub")]
        "scrub" => {
            let parsed: crate::scrub::schema::Args =
                serde_json::from_value(args.clone()).context("scrub args 解析失败")?;
            crate::scrub::run(&parsed)?;
            Ok(DispatchResult {
                path: Some(PathBuf::from(&parsed.target)),
            })
        }
        #[cfg(feature = "shade")]
        "shade" => {
            let parsed: crate::shade::schema::Args =
                serde_json::from_value(args.clone()).context("shade args 解析失败")?;
            crate::shade::run(&parsed)?;
            Ok(DispatchResult {
                path: Some(PathBuf::from(&parsed.to)),
            })
        }
        #[cfg(feature = "compression")]
        "compression" => {
            let parsed: crate::compression::schema::Args =
                serde_json::from_value(args.clone()).context("compression args 解析失败")?;
            let path = match &parsed {
                crate::compression::schema::Args::Zip(a) => Some(PathBuf::from(&a.to)),
                crate::compression::schema::Args::Unzip(a) => Some(PathBuf::from(&a.to)),
            };
            crate::compression::run(&parsed)?;
            Ok(DispatchResult { path })
        }
        #[cfg(feature = "generate")]
        "generate" => {
            let parsed: crate::generate::schema::Args =
                serde_json::from_value(args.clone()).context("generate args 解析失败")?;
            let path = match &parsed {
                crate::generate::schema::Args::Path(a) => Some(PathBuf::from(&a.to)),
                crate::generate::schema::Args::File(a) => Some(PathBuf::from(&a.to)),
                crate::generate::schema::Args::Uuid(_) => None,
            };
            crate::generate::run(&parsed)?;
            Ok(DispatchResult { path })
        }
        #[cfg(feature = "bootstrap")]
        "bootstrap" => {
            let parsed: crate::bootstrap::schema::Args =
                serde_json::from_value(args.clone()).context("bootstrap args 解析失败")?;
            crate::bootstrap::run(&parsed)?;
            Ok(DispatchResult { path: None })
        }
        _ => anyhow::bail!("未知或未启用的模块: {module}"),
    }
}

/// 处理单条 invoke 请求
pub fn handle_invoke(
    state: &DaemonState,
    id: u64,
    module: &str,
    args: &Value,
) -> crate::serve::protocol::Response {
    let start = Instant::now();
    match dispatch(state, module, args) {
        Ok(result) => crate::serve::protocol::Response::success(
            id,
            result.path.map(|p| p.to_string_lossy().into_owned()),
            start.elapsed().as_millis() as u64,
        ),
        Err(err) => crate::serve::protocol::Response::failure(
            id,
            err.to_string(),
            start.elapsed().as_millis() as u64,
        ),
    }
}
