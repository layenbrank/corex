//! `corex doctor`：把「跑不起来」的常见原因一次问完。
//!
//! 只做只读检查，外加一次可写性探测；不修任何东西。最后一项有事就退 1，
//! 于是 `if ! corex doctor` 这种写法能直接当健康检查用。
//! 提示项（比如守护进程没起来）不算问题——不启动它不是错误。

use crate::output::{self, Role, outln};
use crate::scheduler::Paths;
use crate::schema;
use crate::{build_registry, daemon_state, settings};
use anyhow::{Result, bail};
use corex_core::VERSION;
use corex_ipc::data_dir;
use std::path::Path;

pub(crate) async fn run() -> Result<()> {
    let mut report = Report::default();
    outln!("corex {VERSION}");
    outln!("");

    // 数据目录排第一：别的检查项基本都住在它下面。
    match data_dir() {
        Ok(dir) => match probe_writable(&dir) {
            Ok(()) => report.ok("数据目录", dir.display().to_string()),
            Err(err) => report.bad("数据目录", format!("{} —— {err}", dir.display())),
        },
        Err(err) => report.bad("数据目录", err.to_string()),
    }
    report.ok(
        "schema",
        format!(
            "内嵌 {} 字节（corex schema 可取用）",
            schema::DIRECTIVE.len()
        ),
    );

    match settings::source() {
        Some(path) => {
            let issues = corex_core::config::validate(settings::effective());
            if issues.is_empty() {
                report.ok("配置文件", path.display().to_string());
            } else {
                report.bad(
                    "配置文件",
                    format!("{} —— {} 个问题", path.display(), issues.len()),
                );
                for issue in &issues {
                    report.detail(format!("{}: {}", issue.key, issue.message));
                }
            }
        }
        None => report.note("配置文件", "未找到，使用默认值".into()),
    }

    // 配置语法没问题，端点却可能根本用不了（典型：Windows 上沿用了 Unix 的
    // `socket_path = "corex.sock"`）。这类错误只会在启动 daemon 时才炸出来，
    // 而 doctor 正是问「跑不起来的常见原因」的地方。
    match crate::resolve_endpoint() {
        Ok(endpoint) => report.ok("IPC 端点", endpoint.display().to_string()),
        Err(err) => report.bad("IPC 端点", err.to_string()),
    }

    match daemon_state().await {
        Ok(state) if state.starts_with("运行中") => report.ok("守护进程", state),
        Ok(state) => report.note("守护进程", format!("{state}（corex daemon start 可启动）")),
        // 连不上是正常状态，探测本身坏了才值得说一句。
        Err(err) => report.note("守护进程", err.to_string()),
    }

    report.ok("已注册动作", format!("{} 个", build_registry().len()));

    match Paths::names(None) {
        Ok(names) => {
            let own = names.iter().filter(|n| !n.example).count();
            let detail = format!(
                "{} 条（自有 {own} / examples {}）",
                names.len(),
                names.len() - own
            );
            if own == 0 {
                report.note(
                    "可用指令",
                    format!("{detail}；corex create <名称> 建一条自己的"),
                );
            } else {
                report.ok("可用指令", detail);
            }
        }
        Err(err) => report.bad("可用指令", err.to_string()),
    }

    outln!("");
    report.finish()
}

/// 数据目录能不能写：写一个探针文件再删掉。
///
/// `create_dir_all` 成功只说明目录在，不说明这个进程写得进去——只读挂载、
/// 被安全软件拦住、目录属于另一个用户，在它眼里长得一模一样。
fn probe_writable(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(".doctor-probe");
    std::fs::write(&probe, b"")?;
    std::fs::remove_file(&probe)?;
    Ok(())
}

/// 自检结果的收集器：负责排版，并记住哪些算问题。
#[derive(Default)]
struct Report {
    problems: Vec<String>,
}

impl Report {
    fn ok(&mut self, title: &str, detail: String) {
        outln!("{} {title:<10} {detail}", output::mark(Role::Ok));
    }

    fn note(&mut self, title: &str, detail: String) {
        outln!("{} {title:<10} {detail}", output::mark(Role::Note));
    }

    fn bad(&mut self, title: &str, detail: String) {
        outln!("{} {title:<10} {detail}", output::mark(Role::Bad));
        self.problems.push(title.to_string());
    }

    /// 挂在上一项下面的补充行。
    fn detail(&self, line: String) {
        outln!("             {line}");
    }

    fn finish(self) -> Result<()> {
        if self.problems.is_empty() {
            return Ok(());
        }
        // 裸 `bail!` 不带类型化错误 → 退出码 1（执行了但失败），正合自检的语义。
        bail!(
            "自检发现 {} 个问题: {}",
            self.problems.len(),
            self.problems.join("、")
        )
    }
}
