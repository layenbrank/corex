//! 命令行界面：clap 推导出来的一切，别的什么都没有。
//!
//! 与 `main.rs` 分开，使“用户到底能敲什么”这件事能单独读，
//! 不被每个命令的实现挡住。命令的*行为*在 `main.rs` 与 `scheduler` 里。

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "corex", version, about = "Corex —— 可组合的指令与动作")]
pub(crate) struct Cli {
    /// 指令 / 配置的搜索目录
    #[arg(long, global = true)]
    pub(crate) dir: Option<PathBuf>,

    /// 提高日志详细程度
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub(crate) verbose: u8,

    /// 使用指定配置文件，而非默认搜索路径
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) config: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// 按名称或文件路径运行指令
    #[command(long_about = "按名称或文件路径运行指令。\n\n\
        省略名称时在终端里交互挑选；给出名称时会做模糊匹配，唯一命中就直接跑。\n\n\
        只有内置 Action 可用：`corex run` 在进程内执行，且刻意从不加载 WASM 插件，\
        所以使用插件提供的 Action 的指令会以 `动作未注册` 失败。这类指令请用守护进程\
        （`corex daemon start`，再通过 IPC 调用）。")]
    Run {
        /// 指令名（不含 .yaml）或 YAML 文件路径；省略则交互挑选
        target: Option<String>,
        /// 输入，形式为 KEY=VALUE
        #[arg(short, long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
        /// 只解析与校验，打印将要执行的步骤，不真的执行
        #[arg(long)]
        dry_run: bool,
        /// 把步骤事件按 NDJSON 写到 stdout（最后一条是 result）
        #[arg(long = "json-events", conflicts_with = "quiet")]
        json_events: bool,
        /// 不打进度；结果照常输出
        #[arg(short, long)]
        quiet: bool,
        /// 不要交互追问：缺指令名或必填输入就直接报错
        #[arg(short = 'y', long)]
        yes: bool,
        /// 交给 corex-daemon 执行（插件 Action 只在它那里可用），进度按帧流回
        #[arg(long, conflicts_with = "dry_run")]
        remote: bool,
        /// 覆盖本次运行的单步超时秒数（0 = 不限）
        #[arg(long, value_name = "SECS", conflicts_with = "remote")]
        timeout: Option<u64>,
        /// 覆盖本次运行的 parallel 最大并发
        #[arg(long, value_name = "N", conflicts_with = "remote")]
        jobs: Option<usize>,
    },
    /// 列出可用指令名
    Schedule {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// 列出已注册动作（按 bucket 分组）；给 id 时打印它的参数表与步骤片段
    Actions {
        /// 动作 id，如 file.copy
        id: Option<String>,
        /// 只看某个 bucket：system / network / data / ui / logic / plugin
        #[arg(long)]
        bucket: Option<String>,
        /// 输出机器可读的目录 JSON（参数表、权限与 inputSchema）
        #[arg(long)]
        json: bool,
    },
    /// 生成新的指令骨架（交互向导，或 -t 选模板）
    Create {
        /// 指令名；省略则在交互里问
        name: Option<String>,
        /// 模板：内置名（blank / hello / http / file / cron / watch / ui）、目录或 YAML 路径
        #[arg(short, long)]
        template: Option<String>,
        /// 目标文件已存在时覆盖
        #[arg(short, long)]
        force: bool,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// 用 $COREX_EDITOR / $EDITOR 或系统默认程序打开指令 YAML
    Edit {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// 校验指令 YAML；不给路径时校验配置
    Validate {
        /// 要检查的指令 YAML；省略则检查配置
        path: Option<PathBuf>,
        /// 要求声明的权限覆盖全部步骤（仅指令）
        #[arg(long)]
        strict: bool,
        /// 盯着文件改：每次保存后重新校验，Ctrl+C 退出
        #[arg(long)]
        watch: bool,
    },
    /// 输出指令 YAML 的 JSON Schema，供编辑器补全与校验
    Schema {
        /// 写到该路径；省略则打到 stdout
        #[arg(long, value_name = "PATH")]
        write: Option<PathBuf>,
    },
    /// 打印某个 shell 的补全注册脚本（候选由 corex 现算，升级后无需重生成）
    Completions {
        /// 目标 shell
        shell: clap_complete::Shell,
    },
    /// 列出最近的执行记录
    History {
        /// 只看这条指令
        name: Option<String>,
        /// 最多列出几条
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
        /// 只看失败的
        #[arg(long = "failed")]
        is_failed_only: bool,
    },
    /// 自检：数据目录、配置、守护进程、动作与指令
    Doctor,
    /// 交互式 REPL
    Repl,
    /// 守护进程控制
    Daemon {
        #[command(subcommand)]
        command: DaemonCmd,
    },
    /// 文件监听 supervisor（PM2 风格）
    Watch {
        #[command(subcommand)]
        command: crate::watch::WatchCommands,
    },
    /// cron 调度 supervisor
    Cron {
        #[command(subcommand)]
        command: crate::cron::CronCommands,
    },
    /// UI 元素探测（Windows UIAutomation）
    Ui {
        #[command(subcommand)]
        command: crate::ui::UiCommands,
    },
    /// 从 GitHub Releases 自更新
    Update {
        #[command(flatten)]
        args: crate::update::UpdateArgs,
    },
}

impl Commands {
    /// 本命令是否应该跑后台版本检查。
    ///
    /// 长时间运行的 supervisor 与面向机器的命令都跳过：它们的输出由脚本或宿主程序读取，
    /// 那里弹一条主动发起的 stderr 提示只是噪声。`update` 跳过是因为它自己报告版本；
    /// `schema` / `completions` 的输出会被重定向进文件；`doctor` 自己就在做检查。
    pub(crate) fn wants_update_notice(&self) -> bool {
        !matches!(
            self,
            Commands::Update { .. }
                | Commands::Daemon { .. }
                | Commands::Watch { .. }
                | Commands::Cron { .. }
                | Commands::Ui { .. }
                | Commands::Schema { .. }
                | Commands::Completions { .. }
                | Commands::Doctor
                | Commands::Repl
        )
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum DaemonCmd {
    /// 后台启动 corex-daemon
    Start,
    /// 停止运行中的守护进程
    Stop,
    /// 查看守护进程状态
    Status,
    /// 在当前终端前台运行守护进程
    Run,
}
