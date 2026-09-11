//! `corex run`：挑指令、收输入、挂进度、给结果。
//!
//! 三条输出纪律，改这个文件时不能破：
//! - stdout 只放**结果**（最终的 JSON，或 `--json-events` 的事件流）；
//! - 进度一律走 stderr（见 `crate::progress`）；
//! - 交互追问只在终端里发生，脚本里缺什么就直接失败。
//!
//! 执行有两条路：默认在**本进程内**跑（只有内置 Action），`--remote` 则交给
//! `corex-daemon`（插件 Action 只在它那里可用）。两条路共用同一个 [`Channel`]——
//! daemon 推回来的帧经 [`Replay`] 重放进同一个上报口，所以进度与结论只写一份。

use crate::output::outln;
use crate::progress::{self, Events};
use crate::scheduler::{Named, Paths};
use crate::steps;
use crate::{ask, build_registry, parse_inputs, usage};
use anyhow::{Context, Result};
use corex_core::{EngineError, ExecutionContext, Observer, RuntimeConfig, Value};
use corex_engine::{
    Directive, ExecutionAudit, ExecutionHistory, InputDecl, Pipeline, is_input_unset,
};
use corex_ipc::protocol::{Request, Response, RpcError};
use corex_ipc::{Replay, Transport, data_dir, ipc_connect};
use corex_registry::ActionRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// `corex run` 的开关。
pub(crate) struct Options {
    pub(crate) inputs: Vec<String>,
    pub(crate) dry_run: bool,
    pub(crate) json_events: bool,
    pub(crate) quiet: bool,
    pub(crate) yes: bool,
    pub(crate) remote: bool,
    /// 覆盖本次运行的 `step_timeout`。
    pub(crate) timeout: Option<u64>,
    /// 覆盖本次运行的 `max_parallel`。
    pub(crate) jobs: Option<usize>,
}

/// `corex run` 的输出通道：进度挂哪、结论往哪写。
///
/// 本地执行与 `--remote` 要做的是**同一件事**——挑一种进度渲染、跑完补一条结论——
/// 所以这件事只写一份，两种输出模式的区别也只在这里。
struct Channel {
    /// 事件流模式的输出句柄；`None` 表示走人类可读的路径。
    events: Option<Arc<Events>>,
    /// 挂给流水线、或喂给 [`Replay`] 的上报口；`None` = 完全不上报。
    observer: Option<Arc<dyn Observer>>,
}

impl Channel {
    /// 按 `--quiet` / `--json-events` 挑形态。
    ///
    /// 事件流模式下不再另画进度：事件本身就是进度，混两套只会让人对不上号。
    fn pick(opts: &Options) -> Self {
        if opts.json_events {
            let events = Arc::new(Events::new());
            return Self {
                observer: Some(Arc::clone(&events) as Arc<dyn Observer>),
                events: Some(events),
            };
        }
        Self {
            events: None,
            observer: progress::human(opts.quiet),
        }
    }

    /// 挂给流水线。`None` 时引擎那边是零开销的空操作。
    fn observer(&self) -> Option<Arc<dyn Observer>> {
        self.observer.clone()
    }

    /// 把 daemon 推回来的帧重放进同一个上报口。
    fn frames(&self) -> Replay {
        Replay::new(self.observer.clone())
    }

    /// 这次运行要不要收 daemon 推回来的帧。
    ///
    /// 与「有没有上报口」同义：`--quiet` 时连帧都不要，daemon 那边于是连观察者都不挂，
    /// `file.copy` 之类会走回平台最优路径。
    fn wants_frames(&self) -> bool {
        self.observer.is_some()
    }

    /// 成功。事件流模式补一条 `result`，否则打 pretty JSON——两者绝不同时出现。
    ///
    /// 载荷是泛型的：本地路径给 `serde_json::Value`，远程路径给 IPC 回话里的
    /// `corex_core::Value`，两者都只被序列化一次。
    fn result<T: serde::Serialize>(&self, payload: &T) -> Result<()> {
        match &self.events {
            Some(events) => events.emit(&serde_json::json!({
                "kind": "result",
                "ok": true,
                "value": payload,
            })),
            None => outln!("{}", serde_json::to_string_pretty(payload)?),
        }
        Ok(())
    }

    /// 失败。事件流的读方也要看到结论，否则它只能靠「流突然断了」来猜。
    fn failure(&self, message: &str) {
        if let Some(events) = &self.events {
            events.emit(&serde_json::json!({"kind": "error", "message": message}));
        }
    }

    /// `--dry-run` 的提纲：将要执行什么、用什么输入、每步要什么权限。
    fn plan(&self, directive: &Directive, rows: &[String], input: &HashMap<String, Value>) {
        let resolved = resolved_input(input);
        match &self.events {
            Some(events) => events.emit(&serde_json::json!({
                "kind": "plan",
                "directive": directive.name,
                "description": directive.description,
                "input": resolved,
                "steps": rows,
            })),
            None => {
                outln!("指令 {}（{} 步）", directive.name, rows.len());
                if !directive.description.is_empty() {
                    outln!("{}", directive.description);
                }
                if !resolved.is_empty() {
                    outln!("输入");
                    for (key, value) in &resolved {
                        outln!("  {key} = {value}");
                    }
                }
                outln!("步骤");
                for row in rows {
                    outln!("  {row}");
                }
                outln!("未执行（--dry-run）。");
            }
        }
    }
}

/// 输入摘要：按键排序（`HashMap` 自身的顺序不稳定），值用 JSON 表示。
fn resolved_input(input: &HashMap<String, Value>) -> serde_json::Map<String, serde_json::Value> {
    let mut pairs: Vec<(&String, &Value)> = input.iter().collect();
    pairs.sort_by_key(|(key, _)| key.as_str());
    pairs
        .into_iter()
        .map(|(key, value)| (key.clone(), value.to_json()))
        .collect()
}

pub(crate) async fn directive(
    target: Option<&str>,
    opts: &Options,
    dir: Option<&Path>,
) -> Result<()> {
    // `--remote` 把名字原样交给 daemon（由它做路径沙箱与解析），所以本地不碰文件。
    if opts.remote {
        let Some(target) = target else {
            return Err(usage("--remote 需要指令名（daemon 不接受本地路径）"));
        };
        return remote(target, opts).await;
    }

    let path = match target {
        Some(target) => Paths::resolve_near(target, dir)?,
        None => choose_directive(dir, opts.yes)?,
    };
    let directive = Directive::from_yaml_file(&path)?;
    let mut input = parse_inputs(&opts.inputs)?;
    report_unknown_inputs(&directive, &input);
    fill_missing(&directive, &mut input, opts.yes)?;

    let registry = Arc::new(build_registry());
    let channel = Channel::pick(opts);
    if opts.dry_run {
        // 预览的输入要和真正开跑时看到的一致：默认值是引擎在开跑前填的，
        // 这里自己补上，不去动真实那条路径。
        return dry_run(
            &directive,
            &registry,
            &channel,
            &with_defaults(&directive, &input),
        );
    }

    let config = {
        let mut config = crate::settings::effective().clone();
        apply_overrides(&mut config, opts);
        config
    };
    let ctx = ExecutionContext::new(config.clone()).with_input(input);
    let mut pipeline = Pipeline::new(registry);

    if config.history.enabled {
        let hist_path = if config.history.file.is_absolute() {
            config.history.file.clone()
        } else {
            data_dir()?.join(&config.history.file)
        };
        pipeline =
            pipeline.with_history(ExecutionHistory::open(hist_path).context("无法打开执行历史")?);
    }
    {
        let audit_path = data_dir()?.join("audit.jsonl");
        let audit_display = audit_path.display().to_string();
        match ExecutionAudit::open(audit_path) {
            Ok(audit) => pipeline = pipeline.with_audit(audit),
            // 刻意只做尽力而为，但绝不静默：用户以为已记录的运行，
            // 不能一声不喘地没被记录。
            Err(err) => crate::output::error_line(&format!(
                "警告: 无法打开审计日志 {audit_display}（{err}）"
            )),
        }
    }
    if let Some(observer) = channel.observer() {
        pipeline = pipeline.with_observer(observer);
    }

    match pipeline.execute(&directive, ctx).await {
        Ok(value) => channel.result(&value.to_json()),
        Err(err) => {
            channel.failure(&err.to_string());
            Err(anyhow::Error::new(err))
        }
    }
}

/// `--remote`：让 daemon 跑这条指令，把它推回来的帧喂给同一套渲染器。
///
/// 本地不解析 YAML，也不追问输入——指令文件住在 daemon 的指令目录里，
/// 只有它知道那份文件长什么样。
async fn remote(target: &str, opts: &Options) -> Result<()> {
    let endpoint = crate::resolve_endpoint()?;
    let token = crate::auth_token()?;
    let channel = Channel::pick(opts);

    let request = Request::RunDirective {
        id: 1,
        auth_token: None,
        name: target.to_string(),
        input: parse_inputs(&opts.inputs)?,
        path: None,
        stream: channel.wants_frames(),
    }
    .with_auth_token(token);

    let mut transport = ipc_connect(&endpoint);
    let response = transport.send_events(&request, &channel.frames()).await?;
    match response {
        Response::Ok { data, .. } => channel.result(&data),
        Response::Error { error, .. } => {
            channel.failure(&error.message);
            Err(from_rpc(error))
        }
        // `read_final` 不会把中间帧当终帧返回，所以这里到不了。
        other => Err(usage(format!("意外的响应: {other:?}"))),
    }
}

/// 把 IPC 的错误码折回 CLI 的退出码表。
///
/// daemon 那边已经把错误压成了字符串，这里只能按码分桶；分桶的意义在于
/// `corex run --remote` 与本地 `corex run` 对同一种失败给出**同一个**数字。
/// 文案一律原样透传，不再叠一层「指令未找到: 动作未注册: …」之类的前缀。
fn from_rpc(error: RpcError) -> anyhow::Error {
    match error.code {
        403 => anyhow::Error::new(corex_core::ActionError::PermissionDenied(error.message)),
        // 400 / 404 都是「你给的东西不对」：动作未注册、指令不存在、参数非法。
        400 | 404 => anyhow::Error::new(EngineError::Usage(error.message)),
        // 401（token 不对）与 500 归为运行失败：环境没配好不是命令写错了。
        _ => anyhow::anyhow!(error.message),
    }
}

/// 没给名字时在终端里挑一条；不交互就问不出答案。
fn choose_directive(dir: Option<&Path>, yes: bool) -> Result<PathBuf> {
    let named = Paths::names(dir)?;
    if named.is_empty() {
        return Err(usage(
            "没有可用指令（`corex create <名称>` 先建一条，或用 `--dir` 指定目录）",
        ));
    }
    if yes || !ask::is_interactive() {
        return Err(usage("需要指令名；`corex schedule` 可以看全部"));
    }
    let labels: Vec<String> = named.iter().map(Named::label).collect();
    let chosen = ask::select("要运行哪条指令？", &labels)?;
    Ok(named[chosen].path.clone())
}

/// 声明里没见过的输入键：多半是笔误，只提醒不拦。
///
/// 不报错是因为 `{{input.x}}` 允许按键取值，指令可以刻意收未声明的输入；
/// 但「键名打错、默认值顶上、脚本照跑」这种事必须让用户看见。
///
/// 只在本进程执行这条路径上做：`--remote` 的指令住在 daemon 那边，
/// 本地不知道它声明了什么。
fn report_unknown_inputs(directive: &Directive, input: &HashMap<String, Value>) {
    let unknown: Vec<&str> = input
        .keys()
        .map(String::as_str)
        .filter(|key| !directive.inputs.iter().any(|decl| decl.name == *key))
        .collect();
    if unknown.is_empty() {
        return;
    }
    let declared = if directive.inputs.is_empty() {
        "该指令没有声明输入".to_string()
    } else {
        let names: Vec<&str> = directive
            .inputs
            .iter()
            .map(|decl| decl.name.as_str())
            .collect();
        format!("已声明: {}", names.join("、"))
    };
    crate::output::error_line(&format!(
        "警告: 未声明的输入 {}（{declared}）",
        unknown.join("、")
    ));
}

/// 声明为必填、又没有默认值的输入，缺了就问一句。
fn fill_missing(
    directive: &Directive,
    input: &mut HashMap<String, Value>,
    yes: bool,
) -> Result<()> {
    let missing: Vec<&InputDecl> = directive
        .inputs
        .iter()
        .filter(|decl| decl.required && decl.default.is_none())
        .filter(|decl| match input.get(&decl.name) {
            Some(value) => is_input_unset(value),
            None => true,
        })
        .collect();
    if missing.is_empty() {
        return Ok(());
    }

    let names = missing
        .iter()
        .map(|decl| decl.name.as_str())
        .collect::<Vec<_>>()
        .join("、");
    if yes || !ask::is_interactive() {
        return Err(usage(format!(
            "缺少必填输入: {names}（用 -i <名称>=<值> 提供）"
        )));
    }

    for decl in missing {
        let prompt = if decl.description.is_empty() {
            decl.name.clone()
        } else {
            format!("{}（{}）", decl.name, decl.description)
        };
        let answer = ask::text(&prompt, None)?;
        input.insert(decl.name.clone(), Value::from_cli_literal(&answer));
    }
    Ok(())
}

/// 把声明里的默认值补进输入，用于预览（默认值本身可能是 `{{env.TEMP}}` 之类的模板，
/// 原样展示：预览要的是「这次用的哪个值」，不是把它渲染一遍）。
fn with_defaults(directive: &Directive, input: &HashMap<String, Value>) -> HashMap<String, Value> {
    let mut merged = input.clone();
    for decl in &directive.inputs {
        let unset = match merged.get(&decl.name) {
            Some(value) => is_input_unset(value),
            None => true,
        };
        if unset && let Some(default) = &decl.default {
            merged.insert(decl.name.clone(), default.clone());
        }
    }
    merged
}

/// `--timeout` / `--jobs` 只作用于本次运行，不落盘：CI 里给一次运行上紧发条，
/// 不必去改全机配置（那会遯往后台跑的所有东西）。
fn apply_overrides(config: &mut RuntimeConfig, opts: &Options) {
    if let Some(timeout) = opts.timeout {
        config.step_timeout = timeout;
    }
    if let Some(jobs) = opts.jobs {
        config.max_parallel = jobs.max(1);
    }
}

/// `--dry-run`：把将要执行的步骤摊开，并把两道门各查一遍。
///
/// 不执行任何步骤，但**两道门的判定照做**：动作没注册、权限会拒的，现在就说出来。
/// 否则 `--dry-run` 通过、真跑却退 2 / 3，这个开关就骗人了。
fn dry_run(
    directive: &Directive,
    registry: &ActionRegistry,
    channel: &Channel,
    input: &HashMap<String, Value>,
) -> Result<()> {
    steps::require_registered(&directive.steps, registry)?;
    // 走运行时那道门，而不是 `validate --strict` 的企业门禁：后者会要求
    // 「必须声明 permissions」，而真实运行并不要求，拿它判预览会让能跑的指令失败。
    steps::require_allowed(&directive.steps, &directive.permissions, registry)?;
    channel.plan(
        directive,
        &steps::outline(&directive.steps, registry),
        input,
    );
    Ok(())
}
