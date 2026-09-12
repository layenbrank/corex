//! `file.write` 动作门面：把参数接到 `super::mode` 的策略上。

use super::mode::{
    json_set, regex, rope_delete_lines, rope_insert_lines, rope_replace_lines, splice,
    str_replace_exact, unified_patch,
};
use super::*;
#[async_trait]
impl Action for FileWrite {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.write",
            "文件写入",
            "写入或局部更新文本/JSON 文件（迷你 IDE）",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("content", SchemaType::Any, false).with_description(
                "文本内容；也可接 `file.read` / `http.send` 的 Bytes（含 IPC 往返后的整数数组），此时只支持 overwrite / append",
            ),
            ParamSchema::new("mode", SchemaType::Str, false)
                .with_default("overwrite")
                .with_description(
                    "overwrite | append | str_replace | replace_lines | insert_lines | delete_lines | splice | regex | json_set | patch",
                ),
            ParamSchema::new("start", SchemaType::Str, false),
            ParamSchema::new("end", SchemaType::Str, false),
            ParamSchema::new("old", SchemaType::Str, false),
            ParamSchema::new("new", SchemaType::Str, false),
            ParamSchema::new("replace_all", SchemaType::Bool, false).with_default(false),
            ParamSchema::new("start_line", SchemaType::Int, false),
            ParamSchema::new("end_line", SchemaType::Int, false),
            ParamSchema::new("after_line", SchemaType::Int, false),
            ParamSchema::new("nth", SchemaType::Int, false).with_default(1),
            ParamSchema::new("include_markers", SchemaType::Bool, false).with_default(false),
            ParamSchema::new("on_missing", SchemaType::Str, false)
                .with_default("error")
                .with_description("error | noop"),
            ParamSchema::new("pattern", SchemaType::Str, false),
            ParamSchema::new("replacement", SchemaType::Str, false),
            ParamSchema::new("pointer", SchemaType::Str, false),
            ParamSchema::new("value", SchemaType::Any, false),
            ParamSchema::new("diff", SchemaType::Str, false),
            ParamSchema::new("newline", SchemaType::Str, false)
                .with_default("preserve")
                .with_description("preserve | lf | crlf"),
            ParamSchema::new("create_dirs", SchemaType::Bool, false).with_default(true),
            ParamSchema::new("backup", SchemaType::Bool, false).with_default(false),
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
        let mode = opt_str(map, "mode").unwrap_or_else(|| "overwrite".into());
        let create_dirs = opt_bool(map, "create_dirs", true);
        let backup = opt_bool(map, "backup", false);
        let newline_mode = opt_str(map, "newline").unwrap_or_else(|| "preserve".into());

        if create_dirs
            && let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent).await?;
        }

        // 二进制内容没有行、也没有换行策略可言：`mode` 只能是落盘方式。
        // （下载图片 / 压缩包就是这条路：`http.send` 的 `response: binary` 直接接进来。）
        if let Some(bytes) = map.get("content").and_then(as_bytes) {
            return write_bytes(&path, &bytes, &mode, backup).await;
        }

        let existing = if mode == "overwrite" {
            String::new()
        } else {
            tokio::fs::read_to_string(&path).await.unwrap_or_default()
        };

        let mut meta = WriteMeta::default();
        let (raw_content, changed) = match mode.as_str() {
            "overwrite" => {
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                (content.to_string(), true)
            }
            "append" => {
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let mut rope = Rope::from_str(&existing);
                rope.insert(rope.len_chars(), content);
                (rope.to_string(), true)
            }
            "str_replace" => {
                let old = require_str(map, "old")?;
                let new = map
                    .get("new")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("new".into()))?;
                let replace_all = opt_bool(map, "replace_all", false);
                let (out, matches) = str_replace_exact(&existing, &old, new, replace_all)?;
                meta.matches = Some(matches as i64);
                let changed = out != existing;
                (out, changed)
            }
            "replace_lines" => {
                let start_line = opt_usize(map, "start_line")?
                    .ok_or_else(|| ActionError::MissingParam("start_line".into()))?;
                let end_line = opt_usize(map, "end_line")?.unwrap_or(start_line);
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let mut rope = Rope::from_str(&existing);
                let (s, e, n) = rope_replace_lines(&mut rope, start_line, end_line, content)?;
                meta.start_line = Some(s as i64);
                meta.end_line = Some(e as i64);
                meta.lines_affected = Some(n as i64);
                let out = rope.to_string();
                let changed = out != existing;
                (out, changed)
            }
            "insert_lines" => {
                let after_line = opt_usize(map, "after_line")?
                    .ok_or_else(|| ActionError::MissingParam("after_line".into()))?;
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let mut rope = Rope::from_str(&existing);
                let (s, e, n) = rope_insert_lines(&mut rope, after_line, content)?;
                meta.start_line = Some(s as i64);
                meta.end_line = Some(e as i64);
                meta.lines_affected = Some(n as i64);
                (rope.to_string(), true)
            }
            "delete_lines" => {
                let start_line = opt_usize(map, "start_line")?
                    .ok_or_else(|| ActionError::MissingParam("start_line".into()))?;
                let end_line = opt_usize(map, "end_line")?.unwrap_or(start_line);
                let mut rope = Rope::from_str(&existing);
                let (s, e, n) = rope_delete_lines(&mut rope, start_line, end_line)?;
                meta.start_line = Some(s as i64);
                meta.end_line = Some(e as i64);
                meta.lines_affected = Some(n as i64);
                let out = rope.to_string();
                let changed = out != existing;
                (out, changed)
            }
            "splice" => {
                let start = require_str(map, "start")?;
                let end = require_str(map, "end")?;
                let replacement = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let nth = opt_usize(map, "nth")?.unwrap_or(1);
                let include_markers = opt_bool(map, "include_markers", false);
                let on_missing = opt_str(map, "on_missing").unwrap_or_else(|| "error".into());
                splice(
                    &existing,
                    &start,
                    &end,
                    replacement,
                    nth,
                    include_markers,
                    &on_missing,
                )?
            }
            "regex" => {
                let pattern = require_str(map, "pattern")?;
                let replacement = opt_str(map, "replacement").unwrap_or_default();
                let (out, matches) = regex(&existing, &pattern, &replacement)?;
                meta.matches = Some(matches as i64);
                let changed = out != existing;
                (out, changed)
            }
            "json_set" => {
                let pointer = require_str(map, "pointer")?;
                let value = map
                    .get("value")
                    .cloned()
                    .or_else(|| map.get("content").cloned())
                    .ok_or_else(|| ActionError::MissingParam("value".into()))?;
                let out = json_set(&existing, &pointer, &value)?;
                let changed = out != existing;
                (out, changed)
            }
            "patch" => {
                let diff = require_str(map, "diff")?;
                let out = unified_patch(&existing, &diff)?;
                let changed = out != existing;
                (out, changed)
            }
            "replace_between" => {
                return Err(ActionError::InvalidParams(
                    "file.write mode `replace_between` 已重命名为 `splice`".into(),
                ));
            }
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的 file.write mode: {other}"
                )));
            }
        };

        let (final_content, nl) = with_newline(raw_content, &newline_mode, &existing)?;
        meta.newline = Some(nl);
        let bytes = final_content.as_bytes();
        if changed || mode == "overwrite" {
            atomic_write(&path, bytes, backup).await?;
        }
        Ok(write_result(path, changed, bytes.len(), meta))
    }
}

/// 二进制内容落盘。
///
/// 文本模式那些花活（行窗、正则、换行归一）对字节流都无意义，因此这里只留两条：
/// 覆盖与追加。选错了就直说，不默默当文本处理。
async fn write_bytes(
    path: &Path,
    bytes: &[u8],
    mode: &str,
    backup: bool,
) -> Result<Value, ActionError> {
    use tokio::io::AsyncWriteExt;

    match mode {
        "overwrite" => atomic_write(path, bytes, backup).await?,
        "append" => {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await?;
            file.write_all(bytes)
                .await
                .map_err(|e| ActionError::execution(format!("追加写入失败: {e}")))?;
        }
        other => {
            return Err(ActionError::InvalidParams(format!(
                "content 是 Bytes 时只支持 mode: overwrite / append，收到 {other}"
            )));
        }
    }

    let meta = WriteMeta {
        // 字节流没有换行风格可说，显式写 none 以免调用方误以为被改过。
        newline: Some("none".into()),
        ..WriteMeta::default()
    };
    Ok(write_result(path.to_path_buf(), true, bytes.len(), meta))
}
