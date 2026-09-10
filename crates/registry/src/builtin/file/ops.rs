//! `file.copy` / `file.update` / `file.remove`。

use super::*;
#[async_trait]
impl Action for FileCopy {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.copy",
            "文件复制",
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
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let from = require_path(&params, "from")?;
        let to = require_path(&params, "to")?;
        let from = confine_path(ctx, &from)?;
        let to = confine_path(ctx, &to)?;
        if let Some(parent) = to.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(&from, &to)
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        Ok(Value::File(to))
    }
}

#[async_trait]
impl Action for FileUpdate {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.update",
            "文件更新",
            "重命名或移动文件",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("create_dirs", SchemaType::Bool, false).with_default(true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = require_path(&params, "from")?;
        let to = require_path(&params, "to")?;
        let from = confine_path(ctx, &from)?;
        let to = confine_path(ctx, &to)?;
        let create_dirs = opt_bool(map, "create_dirs", true);
        if create_dirs
            && let Some(parent) = to.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::rename(&from, &to)
            .await
            .map_err(|e| ActionError::execution(format!("移动失败: {e}")))?;
        Ok(Value::File(to))
    }
}

#[async_trait]
impl Action for FileRemove {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.remove",
            "文件删除",
            "删除文件（目录则递归删除）",
            ActionCategory::Data,
        )
        .with_params(vec![ParamSchema::new("path", SchemaType::File, true)])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await?;
        } else {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(Value::Bool(true))
    }
}
