//! `copy.run` —— 带 includes/excludes 的递归目录 / 文件复制。

use crate::ActionRegistry;
use crate::builtin::filter::Filter;
use crate::builtin::util::{
    Sink, confine_path, copy_file, ensure_parent, opt_bool, opt_strs, require_map, require_path,
};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Unit, Value,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

pub struct CopyRun;

#[async_trait]
impl Action for CopyRun {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "copy.run",
            "复制",
            "复制文件或目录（支持 includes/excludes，可上报分块进度）",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("empty", SchemaType::Bool, false).with_default(false),
            ParamSchema::new("includes", SchemaType::Array, false),
            ParamSchema::new("excludes", SchemaType::Array, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = confine_path(ctx, &require_path(map, "from")?)?;
        let to = confine_path(ctx, &require_path(map, "to")?)?;
        let empty = opt_bool(map, "empty", false);
        let includes = opt_strs(map, "includes");
        let excludes = opt_strs(map, "excludes");

        let path = if from.is_file() {
            copy_single_file(&from, &to, ctx).await?
        } else if from.is_dir() {
            copy_directory(&from, &to, empty, &includes, &excludes, ctx).await?
        } else {
            return Err(ActionError::execution(format!(
                "源路径不存在: {}",
                from.display()
            )));
        };
        Ok(Value::File(path))
    }
}

/// 单个文件：进度就是这个文件自己的字节数。
async fn copy_single_file(
    from: &Path,
    to: &Path,
    ctx: &ExecutionContext,
) -> Result<PathBuf, ActionError> {
    let target = if to.is_dir() {
        to.join(from.file_name().unwrap_or_default())
    } else {
        ensure_parent(to)?;
        to.to_path_buf()
    };
    let total = std::fs::metadata(from).map(|m| m.len()).unwrap_or(0);
    copy_reported(from, &target, 0, total, ctx).await?;
    Ok(target)
}

/// 目录：先量一遍，再逐文件拷，进度报在**整棵树的字节量**上。
///
/// 只按文件计数是不够的：一个大文件从 0 跳到 1，中间什么都没有。按字节报与 `file.copy`
/// 是同一套观感，百分比也才会真的动。
async fn copy_directory(
    from: &Path,
    to: &Path,
    empty: bool,
    includes: &[String],
    excludes: &[String],
    ctx: &ExecutionContext,
) -> Result<PathBuf, ActionError> {
    let filter = Filter::new(includes, excludes);
    let tree = Tree::scan(from, &filter)?;
    if tree.files.is_empty() {
        return Err(ActionError::execution("没有文件需要复制"));
    }
    ctx.chunk(0, Some(tree.bytes), Unit::Bytes);
    std::fs::create_dir_all(to)?;
    if empty {
        empty_dir(to)?;
    }
    // 目录先全部建出来：空目录也是目录树的一部分，不该因为「没有文件」而消失。
    for relative in &tree.dirs {
        std::fs::create_dir_all(to.join(relative))?;
    }
    let mut copied = 0u64;
    for (relative, size) in &tree.files {
        let target = to.join(relative);
        ensure_parent(&target)?;
        copy_reported(&from.join(relative), &target, copied, tree.bytes, ctx).await?;
        copied += size;
    }
    Ok(to.to_path_buf())
}

/// 用 `file.copy` 的那套分块实现拷一个文件，把进度报在**整批**的字节量上：
/// `offset` 是本批已完成的字节，`total` 是本批总量。
async fn copy_reported(
    from: &Path,
    to: &Path,
    offset: u64,
    total: u64,
    ctx: &ExecutionContext,
) -> Result<(), ActionError> {
    let mut report = |done: u64, _file_total: Option<u64>| {
        ctx.chunk(offset + done, Some(total), Unit::Bytes);
    };
    // 没人看进度就不挂上报口：`copy_file` 会改走平台最优路径。
    let sink = ctx.observer.is_some().then_some(&mut report as Sink);
    copy_file(from, to, sink).await
}

/// 一次遍历量出的目录树：要建的目录、要拷的文件，以及文件总字节数。
struct Tree {
    /// 相对源根的目录（不含源根本身）。
    dirs: Vec<PathBuf>,
    /// 相对源根的文件及其大小。
    files: Vec<(PathBuf, u64)>,
    /// 所有文件字节之和，用作进度的分母。
    bytes: u64,
}

impl Tree {
    /// 按同一套过滤规则扫一遍。
    ///
    /// 先量后拷是为了给进度一个分母——一边拷一边数，就永远只有「已拷多少」。
    fn scan(from: &Path, filter: &Filter) -> Result<Self, ActionError> {
        let mut tree = Self {
            dirs: Vec::new(),
            files: Vec::new(),
            bytes: 0,
        };
        for entry in WalkDir::new(from).into_iter().filter_map(Result::ok) {
            let source = entry.path();
            let relative = source
                .strip_prefix(from)
                .map_err(|e| ActionError::execution(e.to_string()))?;
            // 源根本身由调用方创建。
            if relative.as_os_str().is_empty() || filter.is_filtered(relative) {
                continue;
            }
            if source.is_dir() {
                tree.dirs.push(relative.to_path_buf());
            } else if source.is_file() {
                let size = std::fs::metadata(source).map(|m| m.len()).unwrap_or(0);
                tree.bytes += size;
                tree.files.push((relative.to_path_buf(), size));
            }
        }
        Ok(tree)
    }
}

fn empty_dir(dir: &Path) -> Result<(), ActionError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CopyRun));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::{ExecutionContext, Mark, Observer, Spot};
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    #[tokio::test]
    async fn copy_dir_with_exclude() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        std::fs::create_dir_all(src.join("a")).unwrap();
        std::fs::write(src.join("a/keep.txt"), b"ok").unwrap();
        std::fs::write(src.join("a/skip.tmp"), b"no").unwrap();

        let mut params = BTreeMap::new();
        params.insert("from".into(), Value::Str(src.to_string_lossy().into()));
        params.insert("to".into(), Value::Str(dst.to_string_lossy().into()));
        params.insert(
            "excludes".into(),
            Value::Array(vec![Value::Str("**/*.tmp".into())]),
        );

        let mut ctx = ExecutionContext::default();
        CopyRun.execute(Value::Map(params), &mut ctx).await.unwrap();
        assert!(dst.join("a/keep.txt").exists());
        assert!(!dst.join("a/skip.tmp").exists());
    }

    /// 进度要按**整棵树的字节**报：文件计数在大文件上等于没有进度。
    #[tokio::test]
    async fn reports_bytes_for_the_whole_tree() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::create_dir_all(src.join("empty")).unwrap();
        std::fs::write(src.join("a.bin"), vec![7u8; 4096]).unwrap();
        std::fs::write(src.join("nested/b.bin"), vec![9u8; 8192]).unwrap();

        let recorder = Arc::new(Bytes::default());
        let mut ctx = ExecutionContext::default();
        ctx.observer = Some(Arc::clone(&recorder) as Arc<dyn Observer>);
        // 引擎只在动作步骤内上报；这里模拟那一步。
        ctx.enter_step("copy", "copy.run");

        let mut params = BTreeMap::new();
        params.insert("from".into(), Value::Str(src.to_string_lossy().into()));
        params.insert("to".into(), Value::Str(dst.to_string_lossy().into()));
        CopyRun.execute(Value::Map(params), &mut ctx).await.unwrap();

        let total = 4096 + 8192;
        let marks = recorder.marks.lock().unwrap().clone();
        assert_eq!(marks.first(), Some(&(0, Some(total))), "{marks:?}");
        assert_eq!(marks.last(), Some(&(total, Some(total))), "{marks:?}");
        assert!(
            marks.iter().all(|(_, each)| *each == Some(total)),
            "分母要始终是整棵树：{marks:?}"
        );
        // 空目录也是目录树的一部分。
        assert!(dst.join("empty").is_dir());
        assert!(dst.join("nested/b.bin").is_file());
    }

    /// 只记字节进度：要钉住的正是「copy.run 与 file.copy 一样按字节报」。
    #[derive(Debug, Default)]
    struct Bytes {
        marks: std::sync::Mutex<Vec<(u64, Option<u64>)>>,
    }

    impl Observer for Bytes {
        fn chunk(&self, _at: Spot<'_>, mark: Mark) {
            if mark.unit == Unit::Bytes {
                self.marks.lock().unwrap().push((mark.done, mark.total));
            }
        }
    }

    #[tokio::test]
    async fn filesystem_roots_rejects_outside() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("x.txt"), b"x").unwrap();

        let cfg = corex_core::RuntimeConfig {
            filesystem_roots: vec![root.clone()],
            ..Default::default()
        };
        let mut ctx = ExecutionContext::new(cfg);

        let mut params = BTreeMap::new();
        params.insert(
            "from".into(),
            Value::Str(outside.join("x.txt").to_string_lossy().into()),
        );
        params.insert(
            "to".into(),
            Value::Str(root.join("y.txt").to_string_lossy().into()),
        );
        let err = CopyRun
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("越界") || msg.contains("不在") || msg.contains("无法解析"),
            "got: {msg}"
        );
    }
}
