//! Directory actions: write / read / update / remove.

use crate::builtin::util::{confine_path, opt_bool, opt_i64, opt_str, require_map};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_DEPTH_HARD: i64 = 32;
const MAX_ENTRIES_DEFAULT: i64 = 10_000;
const MAX_ENTRIES_HARD: i64 = 10_000;

fn require_path(params: &Value, key: &str) -> Result<PathBuf, ActionError> {
    params
        .as_map()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| ActionError::MissingParam(key.into()))
}

fn entry_kind_from_ft(ft: &std::fs::FileType) -> &'static str {
    if ft.is_dir() {
        "dir"
    } else if ft.is_file() {
        "file"
    } else {
        "other"
    }
}

fn flat_entry(path: PathBuf, name: String, kind: &str, depth: i64) -> Value {
    let mut m = BTreeMap::new();
    m.insert("path".into(), Value::File(path));
    m.insert("name".into(), Value::Str(name));
    m.insert("kind".into(), Value::Str(kind.into()));
    m.insert("depth".into(), Value::Int(depth));
    Value::Map(m)
}

struct TreeNode {
    path: PathBuf,
    name: String,
    kind: String,
    children: Vec<TreeNode>,
}

impl TreeNode {
    fn into_value(self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("path".into(), Value::File(self.path));
        m.insert("name".into(), Value::Str(self.name));
        m.insert("kind".into(), Value::Str(self.kind.clone()));
        if self.kind == "dir" {
            let children: Vec<Value> = self.children.into_iter().map(|c| c.into_value()).collect();
            m.insert("children".into(), Value::List(children));
        }
        Value::Map(m)
    }
}

/// Bounded BFS. Returns either flat list or tree root depending on `as_tree`.
async fn read_dir_bounded(
    root: &Path,
    max_depth: usize,
    max_entries: usize,
    as_tree: bool,
) -> Result<Value, ActionError> {
    let root_meta = tokio::fs::metadata(root).await.map_err(|e| {
        ActionError::execution(format!("读取目录失败 {}: {e}", root.display()))
    })?;
    if !root_meta.is_dir() {
        return Err(ActionError::execution(format!(
            "不是目录: {}",
            root.display()
        )));
    }

    let root_name = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    if as_tree {
        let mut root_node = TreeNode {
            path: root.to_path_buf(),
            name: root_name,
            kind: "dir".into(),
            children: Vec::new(),
        };
        // queue: (parent children vec index path, parent path, depth)
        // Build iteratively: stack of (node_path, depth, children collector via path map)
        let mut entries_seen = 0usize;
        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
        queue.push_back((root.to_path_buf(), 0));

        // Map path -> children accumulated; assemble at end via recursive attach
        let mut children_map: BTreeMap<PathBuf, Vec<TreeNode>> = BTreeMap::new();

        while let Some((dir_path, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            let mut rd = tokio::fs::read_dir(&dir_path).await.map_err(|e| {
                ActionError::execution(format!("列举目录失败 {}: {e}", dir_path.display()))
            })?;
            while let Some(entry) = rd.next_entry().await.map_err(|e| {
                ActionError::execution(format!("列举目录失败 {}: {e}", dir_path.display()))
            })? {
                entries_seen += 1;
                if entries_seen > max_entries {
                    return Err(ActionError::execution(format!(
                        "dir.read 超过 max_entries 上限 ({max_entries})"
                    )));
                }
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let ft = entry.file_type().await.map_err(|e| {
                    ActionError::execution(format!("读取条目类型失败 {}: {e}", path.display()))
                })?;
                let kind = entry_kind_from_ft(&ft).to_string();
                let is_dir = ft.is_dir();
                let node = TreeNode {
                    path: path.clone(),
                    name,
                    kind: kind.clone(),
                    children: Vec::new(),
                };
                children_map.entry(dir_path.clone()).or_default().push(node);
                if is_dir && depth + 1 < max_depth {
                    queue.push_back((path, depth + 1));
                }
            }
        }

        fn attach(node: &mut TreeNode, map: &mut BTreeMap<PathBuf, Vec<TreeNode>>) {
            if let Some(mut kids) = map.remove(&node.path) {
                for kid in &mut kids {
                    if kid.kind == "dir" {
                        attach(kid, map);
                    }
                }
                node.children = kids;
            }
        }
        attach(&mut root_node, &mut children_map);
        Ok(root_node.into_value())
    } else {
        let mut out: Vec<Value> = Vec::new();
        let mut entries_seen = 0usize;
        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
        queue.push_back((root.to_path_buf(), 0));

        while let Some((dir_path, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            let mut rd = tokio::fs::read_dir(&dir_path).await.map_err(|e| {
                ActionError::execution(format!("列举目录失败 {}: {e}", dir_path.display()))
            })?;
            while let Some(entry) = rd.next_entry().await.map_err(|e| {
                ActionError::execution(format!("列举目录失败 {}: {e}", dir_path.display()))
            })? {
                entries_seen += 1;
                if entries_seen > max_entries {
                    return Err(ActionError::execution(format!(
                        "dir.read 超过 max_entries 上限 ({max_entries})"
                    )));
                }
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let ft = entry.file_type().await.map_err(|e| {
                    ActionError::execution(format!("读取条目类型失败 {}: {e}", path.display()))
                })?;
                let kind = entry_kind_from_ft(&ft);
                let child_depth = (depth + 1) as i64;
                let is_dir = ft.is_dir();
                out.push(flat_entry(path.clone(), name, kind, child_depth));
                if is_dir && depth + 1 < max_depth {
                    queue.push_back((path, depth + 1));
                }
            }
        }
        Ok(Value::List(out))
    }
}

pub struct DirWrite;
pub struct DirRead;
pub struct DirUpdate;
pub struct DirRemove;

#[async_trait]
impl Action for DirWrite {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dir.write",
            "Dir Write",
            "创建目录",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("parents", SchemaType::Bool, false).with_default(true),
            ParamSchema::new("exist_ok", SchemaType::Bool, false).with_default(true),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        let parents = opt_bool(map, "parents", true);
        let exist_ok = opt_bool(map, "exist_ok", true);

        if path.is_dir() {
            if exist_ok {
                return Ok(Value::File(path));
            }
            return Err(ActionError::execution(format!(
                "目录已存在: {}",
                path.display()
            )));
        }

        if parents {
            tokio::fs::create_dir_all(&path).await.map_err(|e| {
                ActionError::execution(format!("创建目录失败 {}: {e}", path.display()))
            })?;
        } else {
            tokio::fs::create_dir(&path).await.map_err(|e| {
                ActionError::execution(format!("创建目录失败 {}: {e}", path.display()))
            })?;
        }
        Ok(Value::File(path))
    }
}

#[async_trait]
impl Action for DirRead {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dir.read",
            "Dir Read",
            "列举目录（flat 或 tree）",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("mode", SchemaType::Str, false)
                .with_default("flat")
                .with_description("flat | tree"),
            ParamSchema::new("max_depth", SchemaType::Int, false),
            ParamSchema::new("max_entries", SchemaType::Int, false).with_default(MAX_ENTRIES_DEFAULT),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        let mode = opt_str(map, "mode").unwrap_or_else(|| "flat".into());
        let as_tree = match mode.as_str() {
            "flat" => false,
            "tree" => true,
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的 dir.read mode: {other}"
                )));
            }
        };
        let default_depth = if as_tree { 8 } else { 1 };
        let max_depth = opt_i64(map, "max_depth", default_depth).clamp(0, MAX_DEPTH_HARD) as usize;
        let max_entries = opt_i64(map, "max_entries", MAX_ENTRIES_DEFAULT)
            .clamp(1, MAX_ENTRIES_HARD) as usize;

        read_dir_bounded(&path, max_depth, max_entries, as_tree).await
    }
}

#[async_trait]
impl Action for DirUpdate {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dir.update",
            "Dir Update",
            "重命名或移动目录",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("create_dirs", SchemaType::Bool, false).with_default(true),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = require_path(&params, "from")?;
        let to = require_path(&params, "to")?;
        let from = confine_path(ctx, &from)?;
        let to = confine_path(ctx, &to)?;
        let create_dirs = opt_bool(map, "create_dirs", true);
        if create_dirs {
            if let Some(parent) = to.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await?;
                }
            }
        }
        tokio::fs::rename(&from, &to)
            .await
            .map_err(|e| ActionError::execution(format!("移动目录失败: {e}")))?;
        Ok(Value::File(to))
    }
}

#[async_trait]
impl Action for DirRemove {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "dir.remove",
            "Dir Remove",
            "删除目录（默认仅空目录）",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("recursive", SchemaType::Bool, false).with_default(false),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        let recursive = opt_bool(map, "recursive", false);
        if recursive {
            tokio::fs::remove_dir_all(&path).await.map_err(|e| {
                ActionError::execution(format!("删除目录失败 {}: {e}", path.display()))
            })?;
        } else {
            tokio::fs::remove_dir(&path).await.map_err(|e| {
                ActionError::execution(format!("删除目录失败 {}: {e}", path.display()))
            })?;
        }
        Ok(Value::Bool(true))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(DirWrite));
    registry.register(Arc::new(DirRead));
    registry.register(Arc::new(DirUpdate));
    registry.register(Arc::new(DirRemove));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;
    use tempfile::tempdir;

    #[tokio::test]
    async fn write_read_flat_tree_update_remove() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("root");
        let mut ctx = ExecutionContext::default();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(root.display().to_string()));
        DirWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        tokio::fs::write(root.join("a.txt"), b"a").await.unwrap();
        let sub = root.join("sub");
        tokio::fs::create_dir(&sub).await.unwrap();
        tokio::fs::write(sub.join("b.txt"), b"b").await.unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(root.display().to_string()));
        params.insert("mode".into(), Value::Str("flat".into()));
        let flat = DirRead
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let list = flat.as_list().unwrap();
        assert_eq!(list.len(), 2); // a.txt + sub (depth 1)

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(root.display().to_string()));
        params.insert("mode".into(), Value::Str("tree".into()));
        params.insert("max_depth".into(), Value::Int(8));
        let tree = DirRead
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let tm = tree.as_map().unwrap();
        assert_eq!(tm.get("kind").unwrap().as_str().unwrap(), "dir");
        let children = tm.get("children").unwrap().as_list().unwrap();
        assert_eq!(children.len(), 2);

        let renamed = tmp.path().join("renamed");
        let mut params = BTreeMap::new();
        params.insert("from".into(), Value::Str(root.display().to_string()));
        params.insert("to".into(), Value::Str(renamed.display().to_string()));
        DirUpdate
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(renamed.display().to_string()));
        assert!(DirRemove
            .execute(Value::Map(params.clone()), &mut ctx)
            .await
            .is_err());

        params.insert("recursive".into(), Value::Bool(true));
        DirRemove
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(!renamed.exists());
    }

    #[tokio::test]
    async fn max_entries_errors() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("many");
        tokio::fs::create_dir_all(&root).await.unwrap();
        for i in 0..5 {
            tokio::fs::write(root.join(format!("{i}.txt")), b"x")
                .await
                .unwrap();
        }
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(root.display().to_string()));
        params.insert("max_entries".into(), Value::Int(3));
        let err = DirRead
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("max_entries"), "got: {err}");
    }
}
