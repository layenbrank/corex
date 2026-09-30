//! `watch` / `cron` 共用的作业登记。
//!
//! 两族的机制完全一致——解析指令、每个指令最多保留一个 supervisor、通过控制套接字与
//! 它通信——因此共用一套实现，只在传入的 [`JobKind`] 上不同。
//!
//! 指令从**库**里取：v13 起真相在 `<数据目录>/directives.db`，supervisor 每次触发也按名字
//! 从库读（[`Jobs::io`]），于是「用户改的是库里那份、跑的还是磁盘上那份」不会再发生。

mod control;
mod logs;
mod start;

use crate::library::Library;
use anyhow::{Result, bail};
use corex_engine::{Directive, JobKind, JobMeta, SupervisorIo};
use corex_ipc::data_dir;
use corex_registry::ActionRegistry;
use corex_store::directives_db_path;
use std::path::Path;
use std::sync::Arc;

pub(crate) use control::{ps, restart, send, stop};
pub(crate) use logs::{attach, logs};
pub(crate) use start::{Spec, run};

/// `watch` 与 `cron` 共用的登记逻辑；两者只在 [`JobKind`] 上不同。
pub(crate) struct Jobs;

impl Jobs {
    /// 注册好内置动作并应用生效配置的注册表。
    pub(crate) fn store() -> Arc<ActionRegistry> {
        // 整个二进制只有一个注册表构造器：`corex run` 与调度器不能在“注册了哪些内置动作 / 生效配置”上产生分歧。
        Arc::new(crate::build_registry())
    }

    /// supervisor 要接的两个口：按名取指令（库）与记执行日志（库里的 `runs` 表）。
    ///
    /// 两者总是同时给（有库就都接库）：触发器每次触发都重新取一次指令，所以库里的改动会
    /// 从**下一次**触发开始生效，而账本与 `corex history` 读的是同一份。
    pub(crate) fn io() -> Result<SupervisorIo> {
        let library = Library::open()?;
        Ok(SupervisorIo {
            source: Some(library.source()),
            history: library.history(),
        })
    }

    /// 子命令名，用于面向用户的提示。
    pub(crate) fn sub(kind: JobKind) -> &'static str {
        match kind {
            JobKind::Watch => "watch",
            JobKind::Cron => "cron",
        }
    }

    /// 该指令名下已注册的作业。
    pub(crate) fn find(kind: JobKind, name: &str) -> Result<JobMeta> {
        let data = data_dir()?;
        JobMeta::resolve_by_name(&data, kind, name).map_err(|e| anyhow::anyhow!(e))
    }

    /// 指令必须声明本族调度所依赖的触发器。
    pub(crate) fn ensure(kind: JobKind, directive: &Directive) -> Result<()> {
        match kind {
            JobKind::Watch => {
                if corex_engine::find_watch_trigger(&directive.triggers)?.is_none() {
                    bail!("指令 `{}` 未声明 watch 触发器", directive.name);
                }
            }
            JobKind::Cron => {
                if corex_engine::find_cron_trigger(&directive.triggers)?.is_none() {
                    bail!("指令 `{}` 未声明 cron 触发器", directive.name);
                }
            }
        }
        Ok(())
    }

    /// 该指令当前已在运行的作业（若有）。
    pub(crate) fn running(data: &Path, kind: JobKind, directive: &str) -> Option<JobMeta> {
        JobMeta::find_running_by_directive(data, kind, directive)
    }

    /// 作业的 `directive_path`：指令住在库里的哪个文件。
    ///
    /// 这个字段只剩「这条指令从哪来」的展示用途——真有回退文件（`examples/` 那种）时给文件，
    /// 否则给库自己的路径：`watch ps` 打出来的是实话，而不是一个装了样子的空值。
    pub(crate) fn origin(file: Option<&Path>, data: &Path) -> std::path::PathBuf {
        match file {
            Some(file) => file.to_path_buf(),
            None => directives_db_path(data),
        }
    }
}
