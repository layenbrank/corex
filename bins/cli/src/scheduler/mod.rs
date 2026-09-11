//! `watch` / `cron` 共用的指令查找与作业登记。
//!
//! 两族的机制完全一致——解析指令、每个指令最多保留一个 supervisor、通过控制套接字与
//! 它通信——因此共用一套实现，只在传入的 [`JobKind`] 上不同。

mod control;
mod logs;
mod start;

use crate::fuzzy;
use anyhow::{Result, bail};
use corex_core::EngineError;
use corex_engine::{Directive, JobKind, JobMeta};
use corex_ipc::data_dir;
use corex_registry::ActionRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) use control::{ps, restart, send, stop};
pub(crate) use logs::{attach, logs};
pub(crate) use start::{Spec, run};

/// 仓库自带的演示指令目录，也是解析失败前的最后一站。
const EXAMPLES: &str = "examples/directives";

/// 一条可运行的指令：名字与它来自哪里。
pub(crate) struct Named {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    /// 来自 `examples/directives`（仓库自带），而不是用户的指令目录。
    pub(crate) example: bool,
}

impl Named {
    /// 列在选单 / `schedule` 里的一行。
    pub(crate) fn label(&self) -> String {
        if self.example {
            format!("{}  (examples)", self.name)
        } else {
            self.name.clone()
        }
    }
}

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

    /// 像 [`Self::resolve`] 一样解析，但失败时把最接近的名字一并说出来。
    ///
    /// 手敲名字必然会有错别字，而“指令不存在”本身并不告诉用户拼错了哪个字母。
    pub(crate) fn resolve_near(target: &str, dir: Option<&Path>) -> Result<PathBuf> {
        if let Ok(path) = Self::resolve(target, dir) {
            return Ok(path);
        }
        let near = Self::nearby(target, dir).unwrap_or_default();
        // 报错文本只写「哪条指令 + 最接近的候选」：`EngineError` 的 Display 已经说了
        // 「指令未找到」，这里再写一遍前缀就成了双层前缀。
        let hint = if near.is_empty() {
            format!("{target}（`corex schedule` 看全部）")
        } else {
            format!("{target}（最接近的: {}）", near.join("、"))
        };
        Err(anyhow::Error::new(EngineError::DirectiveNotFound(hint)))
    }

    /// 指令目录与 `examples/directives` 里的全部指令，按名字排序。
    ///
    /// 同名时以自有目录为准：用户自己的指令不应该被仓库里的演示遮住。
    pub(crate) fn names(dir: Option<&Path>) -> Result<Vec<Named>> {
        let mut found: Vec<Named> = Vec::new();
        for (base, example) in [(Self::dir(dir)?, false), (PathBuf::from(EXAMPLES), true)] {
            for path in yaml_files(&base)? {
                let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if found.iter().any(|n| n.name == name) {
                    continue;
                }
                found.push(Named {
                    name: name.to_string(),
                    path,
                    example,
                });
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    /// 最像 `target` 的几个指令名（不区分大小写）。
    pub(crate) fn nearby(target: &str, dir: Option<&Path>) -> Result<Vec<String>> {
        let names: Vec<String> = Self::names(dir)?.into_iter().map(|n| n.name).collect();
        Ok(fuzzy::nearest(target, &names))
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

/// `base` 下的 `*.yaml` / `*.yml`（不递归）；目录不存在就是空列表。
fn yaml_files(base: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if !base.exists() {
        return Ok(found);
    }
    for entry in std::fs::read_dir(base)? {
        let path = entry?.path();
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) {
            found.push(path);
        }
    }
    Ok(found)
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
