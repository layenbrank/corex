use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::artifact::Artifact;
use super::context::InvokeContext;
use super::result::InvokeResult;

/// 按 module 名调用业务模块（CLI / Pipeline / IPC 共用）
pub fn invoke(module: &str, args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    match module {
        #[cfg(feature = "copy")]
        "copy" => invoke_copy(args, ctx),
        #[cfg(feature = "scrub")]
        "scrub" => invoke_scrub(args, ctx),
        #[cfg(feature = "compression")]
        "compression" => invoke_compression(args, ctx),
        #[cfg(feature = "shade")]
        "shade" => invoke_shade(args, ctx),
        #[cfg(feature = "generate")]
        "generate" => invoke_generate(args, ctx),
        #[cfg(feature = "screenshot")]
        "screenshot" => invoke_screenshot(args, ctx),
        #[cfg(feature = "codec")]
        "codec" => invoke_codec(args, ctx),
        #[cfg(feature = "scan")]
        "scan" => invoke_scan(args, ctx),
        #[cfg(feature = "morph")]
        "morph" => invoke_morph(args, ctx),
        #[cfg(feature = "bootstrap")]
        "bootstrap" => invoke_bootstrap(args, ctx),
        #[cfg(feature = "exec")]
        "exec" => invoke_exec(args, ctx),
        _ => anyhow::bail!("未知或未启用的模块: {module}"),
    }
}

/// 已知 module 列表（用于 validate）
pub fn known_modules() -> &'static [&'static str] {
    &[
        #[cfg(feature = "copy")]
        "copy",
        #[cfg(feature = "scrub")]
        "scrub",
        #[cfg(feature = "compression")]
        "compression",
        #[cfg(feature = "shade")]
        "shade",
        #[cfg(feature = "generate")]
        "generate",
        #[cfg(feature = "screenshot")]
        "screenshot",
        #[cfg(feature = "codec")]
        "codec",
        #[cfg(feature = "scan")]
        "scan",
        #[cfg(feature = "morph")]
        "morph",
        #[cfg(feature = "bootstrap")]
        "bootstrap",
        #[cfg(feature = "exec")]
        "exec",
    ]
}

fn decode_json<T: DeserializeOwned>(args: Value, module: &str) -> Result<T> {
    serde_json::from_value(args).with_context(|| format!("{module} params 解析失败"))
}

fn path_result(path: PathBuf) -> InvokeResult {
    InvokeResult::from_artifact(Artifact::from_path(path))
}

fn optional_path_result(path: Option<PathBuf>) -> InvokeResult {
    InvokeResult::from_artifact(match path {
        Some(p) => Artifact::from_path(p),
        None => Artifact::default(),
    })
}

fn path_str_result(path: Option<String>) -> InvokeResult {
    optional_path_result(path.map(PathBuf::from))
}

#[cfg(feature = "copy")]
fn invoke_copy(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::copy::schema::Args = decode_json(args, "copy")?;
    let args = crate::copy::parse_args(raw, ctx);
    let output = crate::copy::service::execute(&args)?;
    Ok(path_result(output.path))
}

#[cfg(feature = "scrub")]
fn invoke_scrub(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::scrub::schema::Args = decode_json(args, "scrub")?;
    let args = crate::scrub::parse_args(raw, ctx);
    let output = crate::scrub::service::execute(&args)?;
    Ok(path_result(output.path))
}

#[cfg(feature = "compression")]
fn invoke_compression(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::compression::schema::Args = decode_json(args, "compression")?;
    let args = crate::compression::parse_args(raw, ctx);
    let output = crate::compression::execute(&args)?;
    Ok(path_str_result(output.path))
}

#[cfg(feature = "shade")]
fn invoke_shade(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::shade::schema::Args = decode_json(args, "shade")?;
    let args = crate::shade::parse_args(raw, ctx);
    let output = crate::shade::service::execute(&args)?;
    Ok(path_result(output.path))
}

#[cfg(feature = "generate")]
fn invoke_generate(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::generate::schema::Args = decode_json(args, "generate")?;
    let args = crate::generate::parse_args(raw, ctx);
    let output = crate::generate::service::execute(&args)?;
    Ok(optional_path_result(output.path))
}

#[cfg(feature = "screenshot")]
fn invoke_screenshot(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::screenshot::schema::Args = decode_json(args, "screenshot")?;
    let args = crate::screenshot::parse_args(raw, ctx);
    let output = crate::screenshot::service::execute(&args, ctx.cached_monitors())?;
    Ok(optional_path_result(output.path).with_ipc_data(output.data))
}

#[cfg(feature = "codec")]
fn invoke_codec(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::codec::schema::Args = decode_json(args, "codec")?;
    let args = crate::codec::parse_args(raw, ctx);
    Ok(crate::codec::service::execute(&args)?.into_invoke_result())
}

#[cfg(feature = "scan")]
fn invoke_scan(args: Value, _ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::scan::schema::Args = decode_json(args, "scan")?;
    Ok(crate::scan::service::execute(&raw)?.into_invoke_result())
}

#[cfg(feature = "morph")]
fn invoke_morph(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::morph::schema::Args = decode_json(args, "morph")?;
    let args = crate::morph::parse_args(raw, ctx);
    let output = crate::morph::service::execute(&args)?;
    Ok(path_str_result(output.path).with_ipc_data(output.data))
}

#[cfg(feature = "bootstrap")]
fn invoke_bootstrap(args: Value, _ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::bootstrap::schema::Args = decode_json(args, "bootstrap")?;
    crate::bootstrap::service::execute(&raw)?;
    Ok(InvokeResult::default())
}

#[cfg(feature = "exec")]
fn invoke_exec(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::exec::schema::Args = decode_json(args, "exec")?;
    let args = crate::exec::parse_args(raw, ctx);
    Ok(crate::exec::service::execute(&args)?.into_invoke_result())
}

/// 将 InvokeResult 转为 IPC data 字段（scan/codec/morph 等）
pub fn ipc_data(result: &InvokeResult) -> Option<Value> {
    if let Some(ref data) = result.data {
        return Some(data.clone());
    }
    result.artifact.as_ref().and_then(|a| {
        a.data.get("data").cloned().or_else(|| {
            if a.data.is_empty() {
                None
            } else {
                Some(Value::Object(
                    a.data.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                ))
            }
        })
    })
}
