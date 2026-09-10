//! `watch` / `cron` 共用的指令查找与作业登记。
//!
//! 两族的机制完全一致——解析指令、每个指令最多保留一个 supervisor、通过控制套接字与
//! 它通信——因此共用一套实现，只在传入的 [`JobKind`] 上不同。

mod control;
mod logs;
mod start;

use anyhow::{Result, bail};
use corex_core::EngineError;
use corex_engine::{Directive, JobKind, JobMeta};
use corex_ipc::data_dir;
use corex_registry::ActionRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) use control::{cmd_ps, cmd_restart, cmd_send, cmd_stop};
pub(crate) use logs::{cmd_attach, cmd_logs};
pub(crate) use start::{Spec, cmd_run};

/// 指令文件所在位置，`run` / `validate` / `schedule` 与两个调度器共用。
pub(crate) struct Paths;

impl Paths {
    /// 把 CLI 目标变成文件：已存在的路径、`<dir>/<target>.yaml|yml`，或 examples 里的指令。
    pub(crate) fn resolve(target: &str, dir: Option<&Path>) -> Result<PathBuf> {
        let as_path = PathBuf::from(target);
        if as_path.exists() {
            return Ok(as_path);
        }
        let base = Self::dir(dir)?;
        for ext in ["yaml", "yml"] {
            let p = base.join(format!("{target}.{ext}"));
            if p.exists() {
                return Ok(p);
            }
        }
        let examples = PathBuf::from("examples/directives");
        for ext in ["yaml", "yml"] {
            let p = examples.join(format!("{target}.{ext}"));
            if p.exists() {
                return Ok(p);
            }
        }
        // 复用引擎的错误，使“指令不存在”在本地和走引擎的路径上报同样的退出码。
        Err(anyhow::Error::new(EngineError::DirectiveNotFound(
            target.to_string(),
        )))
    }

    /// 指令目录：给了 `--dir` 就用它，否则是 `<data-dir>/directives`。
    pub(crate) fn dir(override_dir: Option<&Path>) -> Result<PathBuf> {
        if let Some(d) = override_dir {
            return Ok(d.to_path_buf());
        }
        let d = data_dir()?.join("directives");
        std::fs::create_dir_all(&d)?;
        Ok(d)
    }
}

/// `watch` 与 `cron` 共用的登记逻辑；两者只在 [`JobKind`] 上不同。
pub(crate) struct Jobs;

impl Jobs {
    /// 注册好内置动作并应用生效配置的注册表。
    pub(crate) fn store() -> Arc<ActionRegistry> {
        // 整个二进制只有一个注册表构造器：`corex run` 与调度器不能在“注册了哪些内置动作 / 生效配置”上产生分歧。
        Arc::new(crate::build_registry())
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
}
