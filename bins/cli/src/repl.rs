//! Interactive REPL for exploring directives and actions.

use anyhow::Result;
use std::io::{self, Write};
use std::path::PathBuf;

/// Run the interactive `corex repl` loop.
pub async fn run(dir: Option<PathBuf>) -> Result<()> {
    println!("corex repl — type `help` for commands, `quit` to exit");
    let stdin = io::stdin();
    let mut line = String::new();

    loop {
        print!("corex> ");
        io::stdout().flush()?;
        line.clear();
        let n = stdin.read_line(&mut line)?;
        if n == 0 {
            println!();
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
                            eprintln!("error: {e:#}");
                        }
                    }
                    None => eprintln!("usage: run <name> [KEY=VALUE ...]"),
                }
            }
            "edit" => {
                let name = parts.next();
                match name {
                    Some(target) => {
                        if let Err(e) = crate::cmd_edit(target, dir.as_deref()) {
                            eprintln!("error: {e:#}");
                        }
                    }
                    None => eprintln!("usage: edit <name>"),
                }
            }
            other => eprintln!("unknown command: {other} (type `help`)"),
        }
    }
    Ok(())
}

fn print_help() {
    println!(
        "Commands:
  help              Show this help
  actions           List registered actions
  schedule          List available directives
  edit <name>       Open Directive YAML in your editor
  run <name> [...]  Run a Directive (optional KEY=VALUE inputs)
  quit              Exit the REPL"
    );
}
