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
            "复制文件（单文件，可上报分块进度）",
            Bucket::Data,
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
        let total = std::fs::metadata(&from).map(|m| m.len()).unwrap_or(0);
        copy_bytes(&from, &to, 0, total, ctx).await?;
        Ok(Value::File(to))
    }
}

#[async_trait]
impl Action for FileUpdate {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("file.update", "文件更新", "重命名或移动文件", Bucket::Data).with_params(
            vec![
                ParamSchema::new("from", SchemaType::File, true),
                ParamSchema::new("to", SchemaType::File, true),
                ParamSchema::new("create_dirs", SchemaType::Bool, false).with_default(true),
            ],
        )
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
            Bucket::Data,
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
            // 删除本身不可中断，但先点一下条目数，用户至少知道要处理多大规模。
            let total = count_entries(&path);
            ctx.chunk(0, Some(total), Unit::Items);
            tokio::fs::remove_dir_all(&path).await?;
            ctx.chunk(total, Some(total), Unit::Items);
        } else {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(Value::Bool(true))
    }
}
