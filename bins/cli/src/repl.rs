//! 用于探索指令与动作的交互式 REPL。

use crate::output::{self, errln, outln};
use anyhow::Result;
use std::io;
use std::path::PathBuf;

/// 运行 `corex repl` 交互循环。
pub async fn run(dir: Option<PathBuf>) -> Result<()> {
    outln!("corex repl — 输入 `help` 查看命令，`quit` 退出");
    let stdin = io::stdin();
    let mut line = String::new();

    loop {
        output::prompt("corex> ");
        line.clear();
        let n = stdin.read_line(&mut line)?;
        if n == 0 {
            outln!("");
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        match cmd {
            "help" | "?" => print_help(),
            "quit" | "exit" | "q" => break,
            "actions" => crate::cmd_actions()?,
            "schedule" => crate::cmd_schedule(dir.as_deref())?,
            "run" => {
                let name = parts.next();
                match name {
                    Some(target) => {
                        let rest: Vec<String> = parts.map(|s| s.to_string()).collect();
                        if let Err(e) = crate::cmd_run(target, &rest, dir.as_deref()).await {
                            errln!("错误: {e:#}");
                        }
                    }
                    None => errln!("用法: run <name> [KEY=VALUE ...]"),
                }
            }
            "edit" => {
                let name = parts.next();
                match name {
                    Some(target) => {
                        if let Err(e) = crate::cmd_edit(target, dir.as_deref()) {
                            errln!("错误: {e:#}");
                        }
                    }
                    None => errln!("用法: edit <name>"),
                }
            }
            other => errln!("未知命令: {other}（输入 `help`）"),
        }
    }
    Ok(())
}

fn print_help() {
    outln!(
        "命令:
  help              显示本帮助
  actions           列出已注册动作
  schedule          列出可用指令
  edit <name>       用编辑器打开指令 YAML
  run <name> [...]  运行指令（可跟 KEY=VALUE 输入）
  quit              退出 REPL"
    );
}
