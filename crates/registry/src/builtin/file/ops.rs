//! `file.copy` / `file.update` / `file.remove`。

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 分块复制的缓冲区：兼顾吞吐与进度上报粒度。
const COPY_CHUNK: usize = 1024 * 1024;

/// 复制单个文件，并在有上报口时按块报告进度。
///
/// 没人看进度时交给 `tokio::fs::copy`——它会走平台的最优路径
/// （`copy_file_range` / `CopyFileEx`），没必要为了一个没人看的百分比放弃它。
async fn copy_file(from: &Path, to: &Path, ctx: &ExecutionContext) -> Result<(), ActionError> {
    if ctx.observer.is_none() {
        tokio::fs::copy(from, to)
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        return Ok(());
    }

    let total = tokio::fs::metadata(from).await.map(|m| m.len()).ok();
    let mut src = tokio::fs::File::open(from)
        .await
        .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
    let mut dst = tokio::fs::File::create(to)
        .await
        .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
    let mut buf = vec![0u8; COPY_CHUNK];
    let mut done = 0u64;
    loop {
        let n = src
            .read(&mut buf)
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n])
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        done += n as u64;
        ctx.chunk(done, total, Unit::Bytes);
    }
    dst.flush()
        .await
        .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
    Ok(())
}

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
        copy_file(&from, &to, ctx).await?;
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
