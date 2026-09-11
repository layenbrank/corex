//! 交互追问的薄包装。
//!
//! 只在终端里问，其余情况一律让调用方报错——**不回退到默认值**：脚本里少给一个输入，
//! 悄悄用别的值跑完，比当场失败危险得多。所有提示都走 stderr（dialoguer 的默认），
//! stdout 始终只留给结果。

use anyhow::{Result, bail};
use dialoguer::{Confirm, Input, Select};
use std::io::IsTerminal;

/// 现在能不能问问题：stdin 与 stderr 都得是终端。
///
/// 只看 stdin 不够——把 stderr 重定向进日志时，覆盖式提示会把日志弄脏。
pub(crate) fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// 从列表里选一个，返回下标。
pub(crate) fn select(prompt: &str, items: &[String]) -> Result<usize> {
    if items.is_empty() {
        bail!("没有可选项");
    }
    let index = Select::new()
        .with_prompt(prompt)
        .items(items)
        .default(0)
        .interact()?;
    Ok(index)
}

/// 问一行文本。
pub(crate) fn text(prompt: &str, default: Option<&str>) -> Result<String> {
    let mut ask = Input::<String>::new().with_prompt(prompt);
    if let Some(default) = default {
        ask = ask.default(default.to_string());
    }
    Ok(ask.interact_text()?)
}

/// 问一个是 / 否。
pub(crate) fn confirm(prompt: &str, default: bool) -> Result<bool> {
    Ok(Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact()?)
}
