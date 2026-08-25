//! File actions: read / write / copy / delete.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::path::PathBuf;
use std::sync::Arc;

fn require_path(params: &Value, key: &str) -> Result<PathBuf, ActionError> {
    params
        .as_map()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| ActionError::MissingParam(key.into()))
}

pub struct FileRead;
pub struct FileWrite;
pub struct FileCopy;
pub struct FileDelete;

#[async_trait]
impl Action for FileRead {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.read",
            "File Read",
            "读取文本文件内容",
            ActionCategory::Data,
        )
        .with_params(vec![ParamSchema::new("path", SchemaType::File, true)])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let path = require_path(&params, "path")?;
        let text = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ActionError::execution(format!("读取文件失败 {}: {e}", path.display())))?;
        Ok(Value::Str(text))
    }
}

#[async_trait]
impl Action for FileWrite {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.write",
            "File Write",
            "写入文本文件",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("content", SchemaType::Str, true),
            ParamSchema::new("create_dirs", SchemaType::Bool, false).with_default(true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".into()))?;
        let path = require_path(&params, "path")?;
        let content = map
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("content".into()))?;
        let create_dirs = map
            .get("create_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if create_dirs {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await?;
                }
            }
        }

        tokio::fs::write(&path, content).await?;
        Ok(Value::File(path))
    }
}

#[async_trait]
impl Action for FileCopy {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.copy",
            "File Copy",
            "复制文件（单文件 std::fs::copy）",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let from = require_path(&params, "from")?;
        let to = require_path(&params, "to")?;
        if let Some(parent) = to.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        tokio::fs::copy(&from, &to)
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        Ok(Value::File(to))
    }
}

#[async_trait]
impl Action for FileDelete {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.delete",
            "File Delete",
            "删除文件",
            ActionCategory::Data,
        )
        .with_params(vec![ParamSchema::new("path", SchemaType::File, true)])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let path = require_path(&params, "path")?;
        if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await?;
        } else {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(Value::Bool(true))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(FileRead));
    registry.register(Arc::new(FileWrite));
    registry.register(Arc::new(FileCopy));
    registry.register(Arc::new(FileDelete));
}
