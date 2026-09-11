//! `corex validate`：指令 YAML 与配置的自检。
//!
//! `--watch` 把它变成编辑闭环：存一次盘就重回报一次结果，省掉「改一行、切窗口、
//! 再敲一遍命令」的来回。

use crate::build_registry;
use crate::output::{errln, is_stdout_closed, outln};
use crate::settings;
use crate::steps;
use crate::usage;
use anyhow::{Context, Result};
use corex_engine::{Directive, validate_permissions};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// 轮询间隔：盯的只有一个文件，一次 `stat` 的代价远低于起一套文件系统 watcher，
/// 而且编辑器「先写临时文件再改名」的保存方式对轮询天然友好。
const POLL: Duration = Duration::from_millis(400);

/// 校验指令；不给路径时校验配置。
pub(crate) async fn run(path: Option<&Path>, strict: bool, watch: bool) -> Result<()> {
    if watch {
        let Some(path) = path else {
            // 默默忽略它就会变成一个空转开关，正是本仓库想清掉的东西。
            return Err(usage("--watch 需要指定要盯的指令 YAML"));
        };
        return follow(path, strict).await;
    }
    check(path, strict)
}

/// 校验一次；`path` 为 `None` 表示检查生效配置。
///
/// 配置分支直接用 `settings` 已经读到的结果，不重新解析文件，
/// 这样报出的就是本进程实际用的那份。
fn check(path: Option<&Path>, strict: bool) -> Result<()> {
    let Some(path) = path else {
        if strict {
            return Err(usage("--strict 只用于指令校验，不适用于配置检查"));
        }
        return check_config();
    };

    let directive = Directive::from_yaml_file(path)?;
    let registry = build_registry();

    steps::require_registered(&directive.steps, &registry)?;
    if strict {
        // `validate_permissions` 只给出文字，所以需要一个带类型的载体——
        // 而门禁拒绝就是权限错误，退出码表已经认得它。
        validate_permissions(&registry, &directive)
            .map_err(|e| anyhow::Error::new(corex_core::ActionError::PermissionDenied(e)))?;
    }
    outln!(
        "校验通过: {}（{} 步）",
        directive.name,
        directive.steps.len()
    );
    Ok(())
}

/// 报告生效配置以及其中的问题。
fn check_config() -> Result<()> {
    match settings::source() {
        Some(path) => outln!("配置文件: {}", path.display()),
        None => outln!("未找到配置文件，使用默认值"),
    }
    let issues = corex_core::config::validate(settings::effective());
    if issues.is_empty() {
        outln!("配置有效");
        return Ok(());
    }
    // 这些问题本身在启动阶段已经作为告警报过；脚本在这里需要的是退出状态。
    // `EngineError::Config` 对应 usage 码，所以配置损坏与运行失败是可区分的。
    Err(corex_core::EngineError::Config(format!("配置有 {} 个问题", issues.len())).into())
}

/// 盯着文件改：每次保存后重新校验一次。
///
/// 循环本身永不返回，也不出错——校验失败只打一行错误。这正是一条「改坏了也要继续看」
/// 的命令：一旦 `?` 把失败抛出去，用户还得手动重启一次它。
async fn follow(path: &Path, strict: bool) -> Result<()> {
    outln!("盯着 {} 的改动，Ctrl+C 退出", path.display());
    let mut seen = modified(path)?;
    report(path, strict);

    loop {
        tokio::time::sleep(POLL).await;
        // 读方走后必须收手，否则 `corex validate --watch x.yaml | head` 永不返回。
        if is_stdout_closed() {
            return Ok(());
        }
        let now = modified(path)?;
        if now != seen {
            seen = now;
            report(path, strict);
        }
    }
}

/// 校验一次并把结论打出来；失败只报告，不打断循环。
fn report(path: &Path, strict: bool) {
    if let Err(err) = check(Some(path), strict) {
        errln!("错误: {err:?}");
    }
}

fn modified(path: &Path) -> Result<SystemTime> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .with_context(|| format!("无法读取修改时间: {}", path.display()))
}
