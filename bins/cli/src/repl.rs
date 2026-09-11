//! `corex repl`：交互式探索指令与动作。
//!
//! 两条设计原则：
//!
//! 1. **没有第二套语法**。一行文本经 [`split`] 切词后直接交给 clap 与 CLI 自己的
//!    `dispatch`，因此 REPL 里能敲的就是命令行里能敲的，两者不会漂移。
//! 2. **首屏就得说清楚这里有什么**。从前只打一行“输入 help”，用户还得先敲 `schedule`
//!    才知道有哪些指令——那正是 REPL 本该省掉的一步。

use crate::cli::Cli;
use crate::output::{errln, outln};
use crate::scheduler::Paths;
use crate::{build_registry, settings};
use anyhow::Result;
use clap::Parser;
use corex_engine::HistoryEntry;
use corex_ipc::data_dir;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, Helper};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

/// 首屏列几条最近跑过的指令。
const RECENT: usize = 5;

/// 读历史文件时只取末尾这么多字节；首屏要的是最后几条，不是整份账本。
const TAIL_BYTES: u64 = 64 * 1024;

/// 运行 `corex repl` 交互循环。
pub(crate) async fn run(dir: Option<PathBuf>) -> Result<()> {
    banner(dir.as_deref());
    if !std::io::stdin().is_terminal() {
        return piped(dir).await;
    }
    interactive(dir).await
}

/// 首屏：有什么可跑、跑过什么、怎么用。
fn banner(dir: Option<&Path>) {
    outln!("corex {} · repl", corex_core::VERSION);
    let actions = build_registry().len();
    match Paths::names(dir) {
        Ok(names) => {
            let own = names.iter().filter(|n| !n.example).count();
            outln!(
                "指令 {} 条（自有 {own} / examples {}）   动作 {actions} 个",
                names.len(),
                names.len() - own
            );
        }
        Err(err) => outln!("指令: 无法枚举（{err}）   动作 {actions} 个"),
    }
    let recent = recent();
    if !recent.is_empty() {
        outln!("最近: {}", recent.join(" · "));
    }
    outln!("`help` 看命令，Tab 补全指令名与动作 id，`quit` 退出");
    outln!("");
}

async fn interactive(dir: Option<PathBuf>) -> Result<()> {
    let config = Config::builder()
        .auto_add_history(true)
        // 候选多的时候（动作有几十个），列表比逐个补全好用得多。
        .completion_type(CompletionType::List)
        .build();
    let mut editor: Editor<Names, DefaultHistory> = Editor::with_config(config)?;
    editor.set_helper(Some(Names::collect(dir.as_deref())));

    // 历史跨会话保留：上一次试过的命令不该再打一遍。
    let history = data_dir().ok().map(|d| d.join("repl_history"));
    if let Some(path) = &history {
        let _ = editor.load_history(path);
    }

    let mut interrupted = false;
    loop {
        // 每轮刷新候选：上一轮可能刚 create 出一条新指令。
        if let Some(names) = editor.helper_mut() {
            *names = Names::collect(dir.as_deref());
        }
        match editor.readline("corex> ") {
            Ok(line) => {
                interrupted = false;
                if !run_line(&line, &dir).await {
                    break;
                }
            }
            // Ctrl+C 先提示、再按一次才退出；一次就退会让“想改上一行”变成重启 REPL。
            Err(ReadlineError::Interrupted) => {
                if interrupted {
                    outln!("");
                    break;
                }
                interrupted = true;
                outln!("（再按一次 Ctrl+C 退出）");
            }
            Err(ReadlineError::Eof) => {
                outln!("");
                break;
            }
            Err(err) => return Err(err.into()),
        }
    }

    if let Some(path) = &history {
        let _ = editor.save_history(path);
    }
    Ok(())
}

/// 非终端输入时的朴素循环：一次一行，读完即退。
///
/// 留着它有两个用处：`echo schedule | corex repl` 能跑通，
/// 集成测试也不必去驱动一个真终端。
async fn piped(dir: Option<PathBuf>) -> Result<()> {
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        if !run_line(&line, &dir).await {
            break;
        }
    }
    Ok(())
}

/// 执行一行；返回是否继续。
async fn run_line(line: &str, dir: &Option<PathBuf>) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    match trimmed {
        "quit" | "exit" | "q" => return false,
        "help" | "?" => {
            help();
            return true;
        }
        _ => {}
    }
    if let Err(err) = forward(trimmed, dir).await {
        errln!("错误: {err:?}");
    }
    true
}

/// 把一行文本送进 clap，再交给 CLI 自己的 `dispatch`。
async fn forward(line: &str, dir: &Option<PathBuf>) -> Result<()> {
    let words = split(line);
    let Some(first) = words.first() else {
        return Ok(());
    };
    if let Some(reason) = blocked(first, words.get(1).map(String::as_str)) {
        errln!("{reason}");
        return Ok(());
    }

    let mut argv = vec!["corex".to_string()];
    if let Some(dir) = dir {
        argv.push("--dir".to_string());
        argv.push(dir.display().to_string());
    }
    argv.extend(run_shorthand(&words, dir));
    // 装箱是必需的：`dispatch` 认得 `repl` 子命令，于是 `run` → … → `forward` → `dispatch`
    // 是一条环。`repl` 本身在 `blocked` 里就被拦下了，这里只是让类型能收敛。
    match Cli::try_parse_from(&argv) {
        Ok(cli) => Box::pin(crate::dispatch(cli)).await,
        Err(err) => {
            // `--help` / `--version` 也走这条路：它们不是错误，把文本打出来就好。
            let _ = err.print();
            Ok(())
        }
    }
}

/// 只敲指令名时补上 `run`：REPL 里最高频的动作，不该要求先打三个字母再打名字。
fn run_shorthand(words: &[String], dir: &Option<PathBuf>) -> Vec<String> {
    let is_command = commands().iter().any(|c| c == &words[0]);
    if !is_command && Paths::resolve(&words[0], dir.as_deref()).is_ok() {
        let mut with_run = vec!["run".to_string()];
        with_run.extend(words.iter().cloned());
        return with_run;
    }
    words.to_vec()
}

/// 顶层子命令名，直接从 clap 定义里取，不再维护第二份列表。
fn commands() -> Vec<String> {
    <Cli as clap::CommandFactory>::command()
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect()
}

/// REPL 里不该跑的命令：它们会占住终端，或者替换正在运行的二进制。
fn blocked(first: &str, second: Option<&str>) -> Option<&'static str> {
    match (first, second) {
        ("repl", _) => Some("已经在 REPL 里了"),
        ("daemon", _) => Some("守护进程要独占终端，回 shell 里跑 `corex daemon ...`"),
        ("update", _) => Some("自更新会替换正在运行的二进制，回 shell 里跑 `corex update`"),
        ("watch" | "cron", Some("attach")) => {
            Some("日志跟随会占住终端，回 shell 里跑 `corex watch attach` / `corex cron attach`")
        }
        _ => None,
    }
}

fn help() {
    outln!(
        "quit / exit    退出
help / ?       显示本帮助
<指令名>       运行该指令（等价于 run <指令名>）
其余按 corex 的命令行来，例如:
  schedule              列出指令
  actions file.copy     看某个动作的参数表与步骤片段
  run <名称> -i k=v     带输入运行
  create / edit         新建 / 打开指令
  validate <路径> --strict
  doctor                自检数据目录、配置与守护进程"
    );
}

/// 按 shell 的习惯切词：引号内的空格不切分，引号本身不进结果。
///
/// 刻意**不**处理反斜杠转义——Windows 上它是路径分隔符，把 `-i path=C:\a\b`
/// 里的反斜杠吃掉，比不支持转义糟得多。需要字面引号时用另一种引号包起来。
fn split(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in line.chars() {
        match (quote, ch) {
            (Some(open), c) if c == open => quote = None,
            (None, c @ ('"' | '\'')) => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (_, c) => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// 最近跑过的指令名，新的排前面、同名只留一次。
fn recent() -> Vec<String> {
    let Some(text) = history_tail() else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    for line in text.lines().rev() {
        // 末尾那几行里的第一行很可能是被截断的半个 JSON，解析失败跳过即可。
        let Ok(entry) = serde_json::from_str::<HistoryEntry>(line) else {
            continue;
        };
        if names.contains(&entry.directive) {
            continue;
        }
        names.push(entry.directive);
        if names.len() == RECENT {
            break;
        }
    }
    names
}

fn history_tail() -> Option<String> {
    let config = settings::effective();
    if !config.history.enabled {
        return None;
    }
    let path = if config.history.file.is_absolute() {
        config.history.file.clone()
    } else {
        data_dir().ok()?.join(&config.history.file)
    };
    tail(&path, TAIL_BYTES)
}

/// 文件末尾 `bytes` 个字节能读到的文本；读不动就是 `None`。
fn tail(path: &Path, bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len > bytes {
        // 起点可能落在多字节字符中间，所以按字节读、再宽松解码，而不是 `read_to_string`。
        file.seek(SeekFrom::Start(len - bytes)).ok()?;
    }
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// 补全候选：REPL 自己能敲的命令 + 指令名。
struct Names {
    words: Vec<String>,
    directives: Vec<String>,
}

impl Names {
    fn collect(dir: Option<&Path>) -> Self {
        let mut words = vec!["help".to_string(), "quit".to_string()];
        words.extend(commands());
        words.sort();
        words.dedup();
        let directives = Paths::names(dir)
            .map(|named| named.into_iter().map(|n| n.name).collect())
            .unwrap_or_default();
        Self { words, directives }
    }
}

impl Helper for Names {}

impl Highlighter for Names {}

impl Validator for Names {}

impl Hinter for Names {
    type Hint = String;
}

impl Completer for Names {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let head = &line[..pos];
        let start = head
            .rfind(char::is_whitespace)
            .map(|at| at + 1)
            .unwrap_or(0);
        let word = &head[start..];
        // 第一个词只可能是命令或指令名；之后的参数才轮得到指令名。
        let pool = if start == 0 {
            self.words.iter().chain(&self.directives)
        } else {
            self.directives.iter().chain(&self.words)
        };
        let pairs = pool
            .filter(|candidate| candidate.starts_with(word))
            .map(|candidate| Pair {
                display: candidate.clone(),
                replacement: candidate.clone(),
            })
            .collect();
        Ok((start, pairs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_keeps_quoted_spaces_together() {
        assert_eq!(
            split(r#"run hello -i who="Alice Smith""#),
            vec!["run", "hello", "-i", "who=Alice Smith"]
        );
    }

    #[test]
    fn split_leaves_windows_paths_alone() {
        // 反斜杠是路径分隔符，不是转义符。
        assert_eq!(
            split(r"run x -i path=C:\tmp\a b"),
            vec!["run", "x", "-i", r"path=C:\tmp\a", "b"]
        );
    }

    #[test]
    fn split_ignores_padding() {
        assert_eq!(split("  run   hello  "), vec!["run", "hello"]);
        assert!(split("   ").is_empty());
    }
}
