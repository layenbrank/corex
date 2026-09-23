//! `app.launch` —— 启动应用（`process_launch` 的门面，默认派生即返回）。
//!
//! 与 `shell.run` / `exec.run` 的分工：这里只负责把应用跑起来——参数按数组原样传，
//! 不经 shell 解释、也不要输出；要拿 stdout/exit_code 用它们，只按文件关联打开
//! （文档、快捷方式、`.bat`、URL）用 `url.open`。

use crate::ActionRegistry;
use crate::builtin::process_launch::{
    Host, IfRunning, LaunchSpec, LaunchWait, TargetKind, args_from_params, if_running_from_params,
    launch,
};
use crate::builtin::util::{confine_path, opt_str, require_map};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// 应用商店应用只能由资源管理器转交：`shell:AppsFolder\<aumid>`。
const EXPLORER: &str = "explorer.exe";

pub struct AppLaunch;

#[async_trait]
impl Action for AppLaunch {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::SHELL
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "app.launch",
            "启动应用",
            "启动程序（path）或应用商店应用（aumid），默认派生即返回、不等退出",
            Bucket::System,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::Str, false)
                .with_description("可执行文件或命令名；与 aumid 二选一"),
            ParamSchema::new("aumid", SchemaType::Str, false)
                .with_description("应用商店应用的用户模型 ID（形如 Package!App）；与 path 二选一"),
            ParamSchema::new("args", SchemaType::Array, false)
                .with_description("参数列表（原样传递，不经 shell；aumid 模式忽略）")
                .with_default(Value::Array(vec![])),
            ParamSchema::new("cwd", SchemaType::Str, false)
                .with_description("工作目录（受 filesystem_roots 约束；aumid 模式忽略）"),
            ParamSchema::new("wait", SchemaType::Str, false)
                .with_default("detach")
                .with_description("sync | detach；aumid 模式固定 detach"),
            ParamSchema::new("if_running", SchemaType::Str, false)
                .with_default("launch")
                .with_description("launch | skip | fail；aumid 模式固定 launch"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let spec = launch_spec(&params, ctx)?;
        let result = launch(spec, ctx.reporter()).await?;
        Ok(result.into_value())
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(AppLaunch));
}

/// 由参数构建 [`LaunchSpec`]。抽成独立函数是为了能单测：真正启动进程要起子进程，
/// 不适合放进单元测试。
fn launch_spec(params: &Value, ctx: &ExecutionContext) -> Result<LaunchSpec, ActionError> {
    let map = require_map(params)?;
    // 先解析 wait / if_running，让拼错的值在任何模式下都报错，
    // 而不是在 aumid 模式里被固定值悄悄盖掉。
    let wait = wait_from(map)?;
    let if_running = if_running_from_params(map)?;
    match (opt_str(map, "path"), opt_str(map, "aumid")) {
        (Some(path), None) => path_spec(map, ctx, PathBuf::from(path), wait, if_running),
        (None, Some(aumid)) => Ok(aumid_spec(&aumid)),
        (Some(_), Some(_)) => Err(ActionError::InvalidParams(
            "path 与 aumid 只能给一个".into(),
        )),
        (None, None) => Err(ActionError::MissingParam("path 或 aumid".into())),
    }
}

/// `path` 模式：直连 CreateProcess（`Host::None`），参数原样传。
fn path_spec(
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
    path: PathBuf,
    wait: LaunchWait,
    if_running: IfRunning,
) -> Result<LaunchSpec, ActionError> {
    // 只查像路径的名字：裸命令名（`notepad`）要留给 PATH。
    // 提前报「程序不存在」比 CreateProcess 的错误码 2 好认。
    if has_separator(&path) && !path.is_file() {
        return Err(ActionError::execution(format!(
            "程序不存在: {}",
            path.display()
        )));
    }
    Ok(LaunchSpec {
        program: path,
        args: args_from_params(map),
        // 程序路径不做路径约束（要能启动系统程序）；只有 cwd 做，和 shell.run 一致。
        cwd: opt_str(map, "cwd")
            .map(PathBuf::from)
            .map(|cwd| confine_path(ctx, &cwd))
            .transpose()?,
        host: Host::None,
        kind: TargetKind::Command,
        input: None,
        allow_nonzero: false,
        wait,
        if_running,
        if_running_window: None,
    })
}

/// `aumid` 模式：运行权在资源管理器手里，于是 `wait` / `if_running` 固定为
/// 「派生即返回 / 总是启动」——`explorer.exe` 常年在线，照参数判 `if_running`
/// 会把每一次启动都当成「已在运行」吞掉。
fn aumid_spec(aumid: &str) -> LaunchSpec {
    LaunchSpec {
        program: PathBuf::from(EXPLORER),
        args: vec![format!("shell:AppsFolder\\{aumid}")],
        cwd: None,
        host: Host::None,
        kind: TargetKind::Command,
        input: None,
        allow_nonzero: false,
        wait: LaunchWait::Detach,
        if_running: IfRunning::Launch,
        if_running_window: None,
    }
}

/// 带路径分隔符（即用户明确指向某个文件，而非交给 PATH 找）。
fn has_separator(path: &std::path::Path) -> bool {
    path.is_absolute() || path.components().count() > 1
}

/// `wait` 默认 `detach`：`shell.run` 那边默认 sync，这里方向相反——
/// 启动应用不该阻塞指令。
fn wait_from(map: &BTreeMap<String, Value>) -> Result<LaunchWait, ActionError> {
    match opt_str(map, "wait") {
        None => Ok(LaunchWait::Detach),
        Some(spec) => LaunchWait::parse(&spec),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, Value)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        )
    }

    fn text(value: &str) -> Value {
        Value::Str(value.to_string())
    }

    fn spec(pairs: &[(&str, Value)]) -> Result<LaunchSpec, ActionError> {
        launch_spec(&params(pairs), &ExecutionContext::default())
    }

    #[test]
    fn path_and_aumid_are_exclusive() {
        assert!(matches!(spec(&[]), Err(ActionError::MissingParam(_))));
        let both = spec(&[("path", text("cmd")), ("aumid", text("X!App"))]);
        assert!(matches!(both, Err(ActionError::InvalidParams(_))));
    }

    #[test]
    fn missing_program_path_is_reported_before_spawning() {
        let err = spec(&[("path", text(r"C:\corex-no-such-dir\no-such-app.exe"))]).unwrap_err();
        assert!(err.to_string().contains("程序不存在"), "got: {err}");
    }

    #[test]
    fn bare_name_is_left_to_path_lookup() {
        let spec = spec(&[("path", text("notepad"))]).unwrap();
        assert_eq!(spec.program, PathBuf::from("notepad"));
        assert_eq!(spec.host, Host::None);
        assert_eq!(spec.wait, LaunchWait::Detach, "启动应用默认不等待");
        assert_eq!(spec.if_running, IfRunning::Launch);
    }

    #[test]
    fn args_stay_whole_and_wait_is_honoured() {
        let spec = spec(&[
            ("path", text("cmd")),
            (
                "args",
                Value::Array(vec![text("/C"), text("echo hello  world")]),
            ),
            ("wait", text("sync")),
            ("if_running", text("skip")),
        ])
        .unwrap();
        assert_eq!(
            spec.args,
            vec!["/C".to_string(), "echo hello  world".to_string()],
            "带空格的参数不能被拆开"
        );
        assert_eq!(spec.wait, LaunchWait::Sync);
        assert_eq!(spec.if_running, IfRunning::Skip);
    }

    #[test]
    fn aumid_goes_through_explorer_and_ignores_process_options() {
        let aumid = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
        let spec = spec(&[
            ("aumid", text(aumid)),
            ("wait", text("sync")),
            ("if_running", text("skip")),
            ("cwd", text(r"C:\")),
        ])
        .unwrap();
        assert_eq!(spec.program, PathBuf::from(EXPLORER));
        assert_eq!(spec.args, vec![format!(r"shell:AppsFolder\{aumid}")]);
        assert_eq!(spec.wait, LaunchWait::Detach);
        assert_eq!(spec.if_running, IfRunning::Launch);
        assert!(spec.cwd.is_none());
    }

    #[test]
    fn bad_enum_values_are_rejected_in_either_mode() {
        assert!(matches!(
            spec(&[("path", text("notepad")), ("wait", text("background"))]),
            Err(ActionError::InvalidParams(_))
        ));
        assert!(matches!(
            spec(&[("aumid", text("X!App")), ("if_running", text("maybe"))]),
            Err(ActionError::InvalidParams(_))
        ));
    }
}
