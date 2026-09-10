//! `corex update` —— 从 GitHub Releases 自更新。

use crate::output::{errln, outln};
use anyhow::{Result, bail};
use clap::Args;
use corex_core::UpdateChannel;
use corex_updater::{
    Notice, NotifierEnv, Options, Plan, Reporter, RunResult, Updater, human_size, notifier_allowed,
    run_background,
};
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// 命令结束后等待后台检查的时长，保证网络不可达时 `corex` 也绝不会挂住。
const NOTICE_WAIT: Duration = Duration::from_millis(1_500);

/// `corex update` 的参数。
#[derive(Args, Debug, Clone, Default)]
pub struct UpdateArgs {
    /// 只报告是否存在新版本
    #[arg(long)]
    pub check: bool,
    /// 安装指定版本，而不取通道头部版本
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,
    /// 发布通道：stable | alpha | beta | rc
    #[arg(long, value_name = "CHANNEL")]
    pub channel: Option<UpdateChannel>,
    /// 下载并校验，然后停下，不替换任何文件
    #[arg(long)]
    pub dry_run: bool,
    /// 即使目标版本已装也重装
    #[arg(long)]
    pub force: bool,
    /// 不询问确认
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// 隐藏下载进度条
    #[arg(long)]
    pub no_progress: bool,
    /// 以 JSON 输出结果
    #[arg(long)]
    pub json: bool,
}

/// 运行 `corex update`。
pub async fn run(args: UpdateArgs) -> Result<()> {
    let mut config = crate::settings::effective().update.clone();
    if let Some(channel) = args.channel {
        config.channel = channel;
    }
    if !config.enabled {
        bail!(
            "自更新已被禁用（[update].enabled = false）。\n\
             企业 / 离线部署请手工安装：{}",
            releases_url(&config.repository)
        );
    }
    let updater = Updater::from_config(config.clone())?;
    let options = Options {
        force: args.force,
        target: args.version.clone(),
        dry_run: args.dry_run,
    };

    if args.check {
        let plan = updater.check(&options).await?;
        return report_check(plan, &updater.current().to_string(), args.json);
    }

    // 没有终端还要确认会挂住，所以要求显式 `--yes`。
    let assume_yes = args.yes || !config.require_confirmation;
    if !assume_yes && !std::io::stdin().is_terminal() {
        bail!("当前不是交互式终端，无法确认；请加 --yes 以非交互方式更新");
    }

    let reporter = TerminalReporter::new(progress_wanted(&args), assume_yes);
    let outcome = updater.run(&options, &reporter).await;
    reporter.finish();
    report_run(&outcome?, args.json)
}

/// 已配置仓库的 release 页面。
fn releases_url(repository: &str) -> String {
    format!("https://github.com/{repository}/releases/latest")
}

/// 本次调用是否适合显示进度条。
fn progress_wanted(args: &UpdateArgs) -> bool {
    !args.no_progress && !args.json && std::io::stderr().is_terminal()
}

/// “有新版本”检查的后台句柄。
pub(crate) type NoticeTask = tokio::task::JoinHandle<Option<Notice>>;

/// 随命令一起启动限流的版本检查。
///
/// 通知不适用时返回 `None`，这种情况下一个请求也不会发出。门禁规则见
/// [`corex_updater::notifier_allowed`]。
pub(crate) fn start_notice(wanted: bool) -> Option<NoticeTask> {
    if !wanted {
        return None;
    }
    // 从进程配置里读：提示绝不能成为命令失败或多输出东西的原因。
    let update = crate::settings::effective().update.clone();
    let env = NotifierEnv::from_process();
    if !notifier_allowed(&update, &env) {
        return None;
    }
    let data_dir = corex_ipc::data_dir().ok()?;
    Some(tokio::spawn(async move {
        let updater = Updater::from_config(update).ok()?;
        run_background(&updater, &data_dir).await
    }))
}

/// 检查及时跑完时，把提示打到 stderr。
///
/// 在命令已经产出自己的输出之后才调用，因此提示永远不会被当成命令的结果。
pub(crate) async fn finish_notice(task: Option<NoticeTask>) {
    let Some(task) = task else { return };
    if let Ok(Ok(Some(notice))) = tokio::time::timeout(NOTICE_WAIT, task).await {
        errln!("");
        errln!("{}", notice.message());
    }
}

/// 要么打 JSON，要么打人类可读的行，绝不同时打。
///
/// 本命令里唯一知道存在两种输出模式的地方。
fn emit(json: bool, payload: serde_json::Value, lines: Vec<String>) -> Result<()> {
    if json {
        outln!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for line in lines {
            outln!("{line}");
        }
    }
    Ok(())
}

/// “无活可干”的结果，`--check` 与 `run` 都会产出。
fn up_to_date_report(current: &str) -> (serde_json::Value, Vec<String>) {
    (
        serde_json::json!({ "status": "up_to_date", "current": current }),
        vec![format!("corex {current} 已是最新版本。")],
    )
}

/// 报告 `--check` 的结果。
fn report_check(plan: Option<Plan>, current: &str, json: bool) -> Result<()> {
    let Some(plan) = plan else {
        let (payload, lines) = up_to_date_report(current);
        return emit(json, payload, lines);
    };
    emit(
        json,
        serde_json::json!({ "status": "update_available", "plan": plan_json(&plan) }),
        vec![
            plan.summary(),
            format!("Release: {}", plan.html_url),
            "运行 `corex update` 安装。".to_string(),
        ],
    )
}

/// 报告 `run` 干了什么。
fn report_run(outcome: &RunResult, json: bool) -> Result<()> {
    let (payload, lines) = match outcome {
        RunResult::UpToDate { current } => up_to_date_report(&current.to_string()),
        RunResult::Declined(plan) => (
            serde_json::json!({ "status": "declined", "plan": plan_json(plan) }),
            vec!["已取消，未做任何更改。".to_string()],
        ),
        RunResult::DryRun(plan) => (
            serde_json::json!({ "status": "dry_run", "plan": plan_json(plan) }),
            vec![
                format!(
                    "校验通过：{}（{}）。",
                    plan.asset.name,
                    human_size(plan.asset.size)
                ),
                "--dry-run：未替换任何文件。".to_string(),
            ],
        ),
        RunResult::Installed(plan) => {
            let mut lines = vec![format!("已更新 corex {} → {}。", plan.current, plan.target)];
            lines.extend(plan.notes().into_iter().map(|note| format!("注意：{note}")));
            lines.push("当前进程仍运行旧版本；重新执行命令即可使用新版本。".to_string());
            (
                serde_json::json!({
                    "status": "installed",
                    "plan": plan_json(plan),
                    "notes": plan.notes(),
                }),
                lines,
            )
        }
    };
    emit(json, payload, lines)
}

/// 把计划序列化成 `--json`。
///
/// 手写而不是 derive，使对外格式与内部结构体解耦。
fn plan_json(plan: &Plan) -> serde_json::Value {
    serde_json::json!({
        "current": plan.current.to_string(),
        "target": plan.target.to_string(),
        "tag": plan.tag,
        "asset": plan.asset.name,
        "asset_size": plan.asset.size,
        "kind": plan.kind.as_str(),
        "install_path": plan.install_path.display().to_string(),
        "daemon_outdated": plan.daemon_outdated,
        "downgrade": plan.downgrade,
        "html_url": plan.html_url,
    })
}

/// 终端报告者：提示用 `dialoguer`，进度条用 `indicatif`。
struct TerminalReporter {
    /// 进度输出被抑制时为 `None`。
    bar: Option<ProgressBar>,
    /// 进度条是否已被显示出来。
    ///
    /// 进度条先以隐藏状态创建，这样不会画到确认提示上，
    /// 由第一次进度回调负责显示它。不用 `Reporter::info` 来做这件事：它的职责只是一行状态。
    revealed: AtomicBool,
    /// 跳过确认提示。
    assume_yes: bool,
}

impl TerminalReporter {
    fn new(show_progress: bool, assume_yes: bool) -> Self {
        let bar = show_progress.then(|| {
            let bar = ProgressBar::hidden();
            bar.set_style(
                ProgressStyle::with_template("{msg} [{bar:32}] {bytes}/{total_bytes} ({eta})")
                    .expect("模板是静态字面量")
                    .progress_chars("=> "),
            );
            bar
        });
        Self {
            bar,
            revealed: AtomicBool::new(false),
            assume_yes,
        }
    }

    /// 首次使用时把进度条显示出来。
    fn reveal(&self) {
        let Some(bar) = &self.bar else { return };
        if self.revealed.swap(true, Ordering::Relaxed) {
            return;
        }
        bar.set_message("下载中");
        bar.set_draw_target(ProgressDrawTarget::stderr());
    }

    /// 清掉进度条，免得最终总结被追加在它后面。
    fn finish(&self) {
        if let Some(bar) = &self.bar {
            bar.finish_and_clear();
        }
    }
}

impl Reporter for TerminalReporter {
    fn confirm(&self, plan: &Plan) -> bool {
        if self.assume_yes {
            return true;
        }
        // 用 stderr 而不是 stdout：带 `--json` 时提示不能落在 JSON 文档里，
        // 而且进度条已经画在那里了。
        errln!("{}", plan.summary());
        errln!("目标文件：{}", plan.install_path.display());
        Confirm::new()
            .with_prompt("继续？")
            .default(false)
            .interact_opt()
            .ok()
            .flatten()
            .unwrap_or(false)
    }

    fn progress(&self, downloaded: u64, total: Option<u64>) {
        self.reveal();
        let Some(bar) = &self.bar else { return };
        if let Some(total) = total {
            bar.set_length(total);
        }
        bar.set_position(downloaded);
    }

    fn info(&self, message: &str) {
        errln!("{message}");
    }
}
