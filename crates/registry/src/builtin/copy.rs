//! `copy.run` — recursive directory/file copy with includes/excludes.

use crate::builtin::filter::Filter;
use crate::builtin::util::{
    ensure_parent, opt_bool, opt_str_list, require_map, require_path,
};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

pub struct CopyRun;

#[async_trait]
impl Action for CopyRun {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "copy.run",
            "Copy",
            "复制文件或目录（支持 includes/excludes）",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("empty", SchemaType::Bool, false).with_default(false),
            ParamSchema::new("includes", SchemaType::List, false),
            ParamSchema::new("excludes", SchemaType::List, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = require_path(map, "from")?;
        let to = require_path(map, "to")?;
        let empty = opt_bool(map, "empty", false);
        let includes = opt_str_list(map, "includes");
        let excludes = opt_str_list(map, "excludes");

        let path = if from.is_file() {
            copy_single_file(&from, &to)?
        } else if from.is_dir() {
            copy_directory(&from, &to, empty, &includes, &excludes)?
        } else {
            return Err(ActionError::execution(format!(
                "源路径不存在: {}",
                from.display()
            )));
        };
        Ok(Value::File(path))
    }
}

fn copy_single_file(from: &Path, to: &Path) -> Result<PathBuf, ActionError> {
    let target = if to.is_dir() {
        to.join(from.file_name().unwrap_or_default())
    } else {
        ensure_parent(to)?;
        to.to_path_buf()
    };
    std::fs::copy(from, &target)?;
    Ok(target)
}

fn copy_directory(
    from: &Path,
    to: &Path,
    empty: bool,
    includes: &[String],
    excludes: &[String],
) -> Result<PathBuf, ActionError> {
    let filter = Filter::new(includes, excludes);
    std::fs::create_dir_all(to)?;
    if empty {
        empty_dir(to)?;
    }
    let mut files = 0u64;
    for entry in WalkDir::new(from).into_iter().filter_map(Result::ok) {
        let source = entry.path();
        let relative = source
            .strip_prefix(from)
            .map_err(|e| ActionError::execution(e.to_string()))?;
        if filter.is_filtered(relative) {
            continue;
        }
        let target = to.join(relative);
        if source.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if source.is_file() {
            ensure_parent(&target)?;
            std::fs::copy(source, &target)?;
            files += 1;
        }
    }
    if files == 0 {
        return Err(ActionError::execution("没有文件需要复制"));
    }
    Ok(to.to_path_buf())
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
    use std::collections::BTreeMap;
    use corex_core::ExecutionContext;
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
            Value::List(vec![Value::Str("**/*.tmp".into())]),
        );

        let mut ctx = ExecutionContext::default();
        CopyRun
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(dst.join("a/keep.txt").exists());
        assert!(!dst.join("a/skip.tmp").exists());
    }
}
