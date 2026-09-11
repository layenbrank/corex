//! `file.read` 动作门面。
//!
//! 文本处理在 `super::mode`；共用辅助函数与类型定义在 `super`。

use super::*;
#[async_trait]
impl Action for FileRead {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.read",
            "文件读取",
            "读取文件内容、行窗，或轻量 exists/stat",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("mode", SchemaType::Str, false)
                .with_default("content")
                .with_description("content | lines | stat | exists | bytes"),
            ParamSchema::new("start_line", SchemaType::Int, false),
            ParamSchema::new("end_line", SchemaType::Int, false),
            ParamSchema::new("limit", SchemaType::Int, false),
            ParamSchema::new("offset", SchemaType::Int, false)
                .with_description("bytes 模式的起点字节"),
            ParamSchema::new("length", SchemaType::Int, false)
                .with_description("bytes 模式只读这么多；不填 = 读到结尾"),
            ParamSchema::new("max_bytes", SchemaType::Int, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        let mode = opt_str(map, "mode").unwrap_or_else(|| "content".into());
        let max_bytes = opt_usize(map, "max_bytes")?.unwrap_or(DEFAULT_MAX_READ_BYTES);

        match mode.as_str() {
            "content" | "lines" => {
                let text = tokio::fs::read_to_string(&path).await.map_err(|e| {
                    ActionError::execution(format!("读取文件失败 {}: {e}", path.display()))
                })?;
                enforce_max_bytes(&text, max_bytes)?;
                let rope = Rope::from_str(&text);
                let start_line = opt_usize(map, "start_line")?;
                let end_line = opt_usize(map, "end_line")?;
                let limit = opt_usize(map, "limit")?;
                let (start0, end0) =
                    if start_line.is_some() || end_line.is_some() || limit.is_some() {
                        line_window(&rope, start_line, end_line, limit)?
                    } else if mode == "lines" {
                        line_window(&rope, Some(1), None, None)?
                    } else {
                        (0, 0)
                    };

                if mode == "lines" {
                    Ok(lines_value(&rope, start0, end0))
                } else if start_line.is_some() || end_line.is_some() || limit.is_some() {
                    Ok(Value::Str(slice_lines_text(&rope, start0, end0)))
                } else {
                    Ok(Value::Str(text))
                }
            }
            // 二进制：文本模式会把它读坏，而「把这一段原样交给下一个动作」是
            // 上传、校验、拼接都用得到的动作。上限就是 `max_bytes`。
            "bytes" => {
                let (offset, length) = range_params(map)?;
                let bytes = read_range(&path, offset, length, max_bytes as u64).await?;
                Ok(Value::Bytes(bytes))
            }
            "exists" => {
                let exists = tokio::fs::try_exists(&path).await.map_err(|e| {
                    ActionError::execution(format!("检查存在失败 {}: {e}", path.display()))
                })?;
                Ok(Value::Bool(exists))
            }
            "stat" => {
                let meta = tokio::fs::metadata(&path).await.map_err(|e| {
                    ActionError::execution(format!("读取元数据失败 {}: {e}", path.display()))
                })?;
                Ok(stat_value(path, meta))
            }
            other => Err(ActionError::InvalidParams(format!(
                "不支持的 file.read mode: {other}"
            ))),
        }
    }
}
