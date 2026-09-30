//! 指令库：CLI 侧取指令与改指令的唯一门面。
//!
//! v12 及以前 CLI 的「指令根」是目录（`<数据目录>/directives` 加 `--dir` 加
//! `examples/directives`），`run` / `validate` / `watch` / `cron` / REPL 各查各的。v13 起指令的
//! 真相在 `<数据目录>/directives.db`，这一层就是全 CLI 唯一的取指令处——谁要一条指令都经过它，
//! 「同一条指令」于是在所有命令里都是同一份。
//!
//! 库之外只留一条**只读**回退：调用方直接给的文件路径，以及 `--dir` / `examples/directives`
//! 里同名的 YAML。它们不进库、不改变库——`corex directive import` 才是把它们收进来的那道门。
//!
//! 读写都收在这里还有第二个理由：库的错误码要折成 [`EngineError`]（CLI 的退出码只认它），
//! 散在各个命令里转换迟早会漏掉一处，于是「指令未找到」在不同命令上给出不同的数字。

use crate::fuzzy;
use anyhow::{Context, Result};
use corex_core::EngineError;
use corex_engine::{Directive, DirectiveSource, HistorySink, admission};
use corex_ipc::data_dir;
use corex_registry::ActionRegistry;
use corex_store::{
    BootstrapOptions, DirectiveMeta, DirectiveRecord, DirectiveStore, ImportOptions, ImportReport,
    StoreDirectiveSource, StoreError, history_sink,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 仓库自带的演示指令目录。库之外的唯一来源，权限上只读。
pub(crate) const EXAMPLES: &str = "examples/directives";

/// 一条指令，以及它从哪来。
pub(crate) struct Located {
    pub(crate) directive: Directive,
    /// `Some` = 来自磁盘上的 YAML（调用方给的文件，或回退目录里的同名文件）；`None` = 来自库。
    pub(crate) file: Option<PathBuf>,
}

/// 选单 / `corex schedule` 里的一行。
pub(crate) struct Named {
    pub(crate) name: String,
    /// 库里没有、从文件找到的那份。
    pub(crate) file: Option<PathBuf>,
    /// 来自 `examples/directives`。
    pub(crate) is_example: bool,
}

impl Named {
    pub(crate) fn label(&self) -> String {
        if self.is_example {
            format!("{}  (examples)", self.name)
        } else {
            self.name.clone()
        }
    }
}

/// 指令库、注册表、生效配置：CLI 里凡是要动指令的地方都从它出发。
pub(crate) struct Library {
    store: Arc<DirectiveStore>,
    registry: Arc<ActionRegistry>,
}

impl Library {
    /// 打开指令库（含一次性迁移与空库播种）。
    pub(crate) fn open() -> Result<Self> {
        let data = data_dir()?;
        let config = crate::settings::effective();
        let registry = Arc::new(crate::build_registry());
        let (store, _report) = DirectiveStore::open_in_data_dir(
            &data,
            BootstrapOptions::from_config(&data, config),
            &admission(Arc::clone(&registry)),
        )
        .context("无法打开指令库")?;
        Ok(Self {
            store: Arc::new(store),
            registry,
        })
    }

    pub(crate) fn registry(&self) -> Arc<ActionRegistry> {
        Arc::clone(&self.registry)
    }

    /// 入库要过的那两道门（动作已注册 + 权限声明够）。
    pub(crate) fn admission(&self) -> impl Fn(&Directive) -> Result<(), String> + use<> {
        admission(self.registry())
    }

    /// 执行日志：`[history] enabled = false` 时是 `None`（不记账，而不是换个地方记）。
    pub(crate) fn history(&self) -> Option<Arc<dyn HistorySink>> {
        history_sink(Arc::clone(&self.store), crate::settings::effective())
    }

    /// 交给引擎的「按名字取指令」口：触发器（watch / cron）每次触发都经过它。
    pub(crate) fn source(&self) -> Arc<dyn DirectiveSource> {
        Arc::new(StoreDirectiveSource::new(Arc::clone(&self.store)))
    }

    /// 库里的全部条目（不含模型）。
    pub(crate) fn records(&self) -> Result<Vec<DirectiveMeta>> {
        Ok(self.store.list().map_err(EngineError::from)?)
    }

    /// 取一条指令的完整记录。
    pub(crate) fn fetch(&self, name: &str) -> Result<DirectiveRecord> {
        Ok(self.store.fetch(name).map_err(EngineError::from)?)
    }

    pub(crate) fn exists(&self, name: &str) -> Result<bool> {
        Ok(self.store.find(name).map_err(EngineError::from)?.is_some())
    }

    /// 写一条指令（`previous` 与 `name` 不同就是改名）。
    pub(crate) fn save(
        &self,
        previous: Option<&str>,
        name: &str,
        definition: &Directive,
    ) -> Result<DirectiveRecord> {
        Ok(self
            .store
            .save_with_rename(previous, name, definition)
            .map_err(EngineError::from)?)
    }

    pub(crate) fn delete(&self, name: &str) -> Result<()> {
        Ok(self.store.delete(name).map_err(EngineError::from)?)
    }

    pub(crate) fn rename(&self, from: &str, to: &str) -> Result<DirectiveRecord> {
        Ok(self.store.rename(from, to).map_err(EngineError::from)?)
    }

    /// 从文件或目录导入；`opts` 里的分组 / 覆盖 / dry-run 原样交给库。
    pub(crate) fn import(&self, path: &Path, opts: &ImportOptions) -> Result<ImportReport> {
        Ok(self
            .store
            .import_path(path, opts, &self.admission())
            .map_err(EngineError::from)?)
    }

    pub(crate) fn export_yaml(&self, name: &str) -> Result<String> {
        Ok(self.store.export_yaml(name).map_err(EngineError::from)?)
    }

    pub(crate) fn export_dir(&self, out: &Path, is_overwrite: bool) -> Result<Vec<PathBuf>> {
        Ok(self
            .store
            .export_dir(out, is_overwrite)
            .map_err(EngineError::from)?)
    }

    /// 按名或按路径取一条指令。
    ///
    /// 顺序是「文件 → 库 → 回退目录」：调用方明确给的路径（`corex run ./x.yaml`）应当照跑，
    /// 其余一律以库为准。回退只为 `examples/` 这类演示留一条只读的路。
    pub(crate) fn find(&self, target: &str, dir: Option<&Path>) -> Result<Located> {
        if let Some(path) = existing_file(target) {
            return Ok(Located {
                directive: Directive::from_yaml_file(&path)?,
                file: Some(path),
            });
        }
        match self.store.fetch(target) {
            Ok(record) => {
                return Ok(Located {
                    directive: record.definition,
                    file: None,
                });
            }
            // 名字不是裸名（`a/b`）说明它压根不像库里的键，直接往下找文件。
            Err(StoreError::NotFound(_) | StoreError::InvalidName(_)) => {}
            Err(error) => return Err(EngineError::from(error).into()),
        }
        if let Some((path, _)) = self.find_file(target, dir)? {
            return Ok(Located {
                directive: Directive::from_yaml_file(&path)?,
                file: Some(path),
            });
        }
        Err(EngineError::DirectiveNotFound(target.to_string()).into())
    }

    /// 同 [`Self::find`]，但失败时把最接近的名字一并说出来。
    ///
    /// 手敲名字必然会有错别字，而「指令不存在」本身并不告诉用户拼错了哪个字母。
    pub(crate) fn find_or_near(&self, target: &str, dir: Option<&Path>) -> Result<Located> {
        match self.find(target, dir) {
            Ok(located) => Ok(located),
            Err(original) => match self.nearby(target, dir) {
                // 一个候选都没有时保留原始错误：那多半不是拼错，而是路径写错了。
                Ok(near) if !near.is_empty() => Err(EngineError::DirectiveNotFound(format!(
                    "{target}（最接近的: {}）",
                    near.join("、")
                ))
                .into()),
                _ => Err(original),
            },
        }
    }

    /// 库里的指令名，加上 `--dir` / `examples/directives` 里还没进库的那几个。
    pub(crate) fn names(&self, dir: Option<&Path>) -> Result<Vec<Named>> {
        let mut found: Vec<Named> = self
            .records()?
            .into_iter()
            .map(|meta| Named {
                name: meta.name,
                file: None,
                is_example: false,
            })
            .collect();
        for base in [dir, Some(Path::new(EXAMPLES))] {
            let Some(base) = base else { continue };
            let is_example = base == Path::new(EXAMPLES);
            for (name, file) in yaml_files(base)? {
                // 库里已经有一条同名指令时以库为准：用户自己那份不该被演示遮住。
                if !found.iter().any(|named| named.name == name) {
                    found.push(Named {
                        name,
                        file: Some(file),
                        is_example,
                    });
                }
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    /// 最像 `target` 的几个指令名（不区分大小写）。
    pub(crate) fn nearby(&self, target: &str, dir: Option<&Path>) -> Result<Vec<String>> {
        let names: Vec<String> = self
            .names(dir)?
            .into_iter()
            .map(|named| named.name)
            .collect();
        Ok(fuzzy::nearest(target, &names))
    }

    /// 回退目录里的同名 YAML；返回文件与它所在的根目录。
    pub(crate) fn find_file(
        &self,
        target: &str,
        dir: Option<&Path>,
    ) -> Result<Option<(PathBuf, PathBuf)>> {
        for base in [dir, Some(Path::new(EXAMPLES))] {
            let Some(base) = base else { continue };
            for ext in ["yaml", "yml"] {
                let path = base.join(format!("{target}.{ext}"));
                if path.is_file() {
                    return Ok(Some((path, base.to_path_buf())));
                }
            }
        }
        Ok(None)
    }
}

/// `target` 作为**路径**存在时的那份文件。
fn existing_file(target: &str) -> Option<PathBuf> {
    let path = PathBuf::from(target);
    path.is_file().then_some(path)
}

/// 一个目录里的 `*.yaml` / `*.yml`（不递归）；目录不存在就是空列表。
fn yaml_files(base: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut found = Vec::new();
    if !base.is_dir() {
        return Ok(found);
    }
    for entry in std::fs::read_dir(base)? {
        let path = entry?.path();
        if !matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yaml") | Some("yml")
        ) {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        found.push((name.to_string(), path));
    }
    Ok(found)
}
