//! `corex schema`：把指令 YAML 的 JSON Schema 交出去。
//!
//! schema 的源头是 `schemas/directive.schema.json`，由 `corex-engine` 的 `schema` 特性
//! 快照测试生成。这里刻意**编译期内嵌**而不是运行时去磁盘找：
//!
//! 1. CLI 不能链接 `schemars`（`quality.yml` 断言过它的依赖树里没有这个 crate），
//!    内嵌是唯一不引入该依赖的拿法；
//! 2. 内嵌之后 `corex schema` 在任何工作目录、任何安装形态下答出的都是同一份东西，
//!    不会因为找不到仓库相对路径而悄悄退化。
//!
//! 改了 `crates/engine/src/definition.rs` 的类型（含 doc 注释）要重新生成快照，
//! 否则这里的副本会旧掉。

use crate::output::{bytes, outln};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// 编译期嵌入的指令 schema。
pub(crate) const DIRECTIVE: &str = include_str!("../../../schemas/directive.schema.json");

/// 在指令目录里放一份副本时用的文件名。
pub(crate) const COPY: &str = "directive.schema.json";

/// YAML 文件头里的 schema 提示。yaml-language-server 认这一行，路径相对文件本身。
pub(crate) const HINT: &str = "# yaml-language-server: $schema=./directive.schema.json";

/// 把 schema 打到 stdout，或写到指定路径。
pub(crate) fn run(write: Option<&Path>) -> Result<()> {
    let Some(path) = write else {
        // 走字节通道：这是一份 JSON 文档，不是一行文本，多补一个换行就是另一份文档了。
        bytes(DIRECTIVE.as_bytes())?;
        return Ok(());
    };
    store(path)?;
    outln!("已写入 {}", path.display());
    outln!(
        "编辑器里把 yaml.schemas 指向它就能得到补全与校验；\
         VS Code 的 YAML 扩展也认文件头那行 {HINT}。"
    );
    Ok(())
}

/// 在指令目录里放一份 schema 副本，供同一目录下的 YAML 用相对路径引用。
///
/// 内容一致时不动它：每次 `create` 都重写一遍会让编辑器里的 schema 缓存反复失效。
pub(crate) fn seed(dir: &Path) -> Result<PathBuf> {
    let path = dir.join(COPY);
    if std::fs::read_to_string(&path).is_ok_and(|existing| existing == DIRECTIVE) {
        return Ok(path);
    }
    store(&path)?;
    Ok(path)
}

fn store(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, DIRECTIVE).with_context(|| format!("无法写入 {}", path.display()))
}
