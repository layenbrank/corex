//! 生成类动作：路径列表、uuid、cvid。

use crate::ActionRegistry;
use crate::builtin::filter::Filter;
use crate::builtin::util::{
    confine_path, ensure_parent, opt_bool, opt_i64, opt_str, opt_strs, require_map, require_path,
    require_str,
};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use rand::RngExt;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;
use walkdir::WalkDir;

pub fn generate_secure_cvid() -> String {
    let mut array = [0u8; 16];
    rand::rng().fill(&mut array);
    array[6] = (array[6] & 0x0f) | 0x40;
    array[8] = (array[8] & 0x3f) | 0x80;
    array.iter().map(|b| format!("{b:02X}")).collect()
}

pub struct GenerateUuid;
pub struct GenerateCvid;
pub struct GeneratePath;
pub struct GenerateTimestamp;

#[async_trait]
impl Action for GenerateUuid {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("generate.uuid", "生成 UUID", "生成 UUID v4", Bucket::Data).with_params(
            vec![
                ParamSchema::new("count", SchemaType::Int, false).with_default(1),
                ParamSchema::new("uppercase", SchemaType::Bool, false).with_default(false),
            ],
        )
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let empty = BTreeMap::new();
        let map = params.as_map().unwrap_or(&empty);
        let count = opt_i64(map, "count", 1).max(1) as usize;
        let uppercase = opt_bool(map, "uppercase", false);
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let id = Uuid::new_v4().to_string();
            items.push(Value::Str(if uppercase { id.to_uppercase() } else { id }));
        }
        let mut out = BTreeMap::new();
        out.insert("items".into(), Value::Array(items.clone()));
        out.insert(
            "value".into(),
            items.first().cloned().unwrap_or(Value::Null),
        );
        Ok(Value::Map(out))
    }
}

#[async_trait]
impl Action for GenerateCvid {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.cvid",
            "生成 CVID",
            "生成 GUID v4 大写 hex（CVID）",
            Bucket::Data,
        )
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        Ok(Value::Str(generate_secure_cvid()))
    }
}

#[async_trait]
impl Action for GeneratePath {
    fn permissions(&self) -> PermissionSet {
        // 会枚举源目录（`from` + `includes`），因此要读文件系统。
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.path",
            "生成路径列表",
            "遍历目录并按模板写出路径列表",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("transform", SchemaType::Str, true),
            ParamSchema::new("index", SchemaType::Int, false).with_default(0),
            ParamSchema::new("separator", SchemaType::Str, false).with_default(""),
            ParamSchema::new("includes", SchemaType::Array, false),
            ParamSchema::new("excludes", SchemaType::Array, false),
            ParamSchema::new("uppercase", SchemaType::Array, false),
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
        let transform = require_str(map, "transform")?;
        let index_start = opt_i64(map, "index", 0) as usize;
        let separator = opt_str(map, "separator").unwrap_or_default();
        let includes = opt_strs(map, "includes");
        let excludes = opt_strs(map, "excludes");
        let uppercase = opt_strs(map, "uppercase");

        if to.is_dir() {
            return Err(ActionError::InvalidParams(
                "目标路径应是一个文件路径".into(),
            ));
        }
        ensure_parent(&to)?;

        let filter = Filter::new(&includes, &excludes);
        let mut entries: Vec<_> = WalkDir::new(&from)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| {
                let raw = entry.path().strip_prefix(&from).unwrap_or(entry.path());
                !filter.is_filtered(raw) && entry.path().is_file()
            })
            .collect();

        entries.sort_by(|a, b| {
            let ext_a = a
                .path()
                .extension()
                .map(|e| e.to_string_lossy())
                .unwrap_or_default();
            let ext_b = b
                .path()
                .extension()
                .map(|e| e.to_string_lossy())
                .unwrap_or_default();
            match ext_a.cmp(&ext_b) {
                std::cmp::Ordering::Equal => a
                    .file_name()
                    .to_string_lossy()
                    .cmp(&b.file_name().to_string_lossy()),
                other => other,
            }
        });

        let pad_width = entries.len().to_string().len().max(1);
        let spec = NameSpec {
            transform: &transform,
            pad_width,
            uppercase: &uppercase,
            separator: &separator,
        };
        let mut file = std::fs::File::create(&to)?;
        let mut items = 0u64;
        for (key, entry) in entries.iter().enumerate() {
            let line = spec.render(
                entry.path(),
                entry.file_name().to_string_lossy().as_ref(),
                key + index_start,
                &from,
            );
            if key + 1 == entries.len() {
                write!(file, "{line}")?;
            } else {
                writeln!(file, "{line}")?;
            }
            items += 1;
        }

        let mut out = BTreeMap::new();
        out.insert("path".into(), Value::File(to));
        out.insert("items".into(), Value::Int(items as i64));
        Ok(Value::Map(out))
    }
}

/// `generate.path` 如何渲染每一行输出。
///
/// 用一个值代替四个并列参数：命名策略每次运行只构建一次，
/// 之后每个条目复用。
struct NameSpec<'a> {
    /// 带 `{{name}}` / `{name}` 占位符的模板。
    transform: &'a str,
    /// `index` 占位符的补零宽度。
    pad_width: usize,
    /// 需要转大写的占位符名。
    uppercase: &'a [String],
    /// 路径分隔符的替换字符；为空则原样保留。
    separator: &'a str,
}

impl NameSpec<'_> {
    fn render(&self, entry_path: &Path, filename: &str, index: usize, from: &Path) -> String {
        let extension = entry_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let relative = entry_path.strip_prefix(from).unwrap_or(entry_path);
        let dirpart = relative
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let fullpath = if dirpart.is_empty() {
            filename.to_string()
        } else {
            let sep = if self.separator.is_empty() {
                std::path::MAIN_SEPARATOR_STR
            } else {
                self.separator
            };
            format!("{dirpart}{sep}{filename}")
        };
        let index_str = format!("{:0pad_width$}", index, pad_width = self.pad_width);
        let filename_v = up(self.uppercase, "filename", filename);
        let extension_v = up(self.uppercase, "extension", &extension);
        let path_v = up(self.uppercase, "path", &dirpart);
        let fullpath_v = up(self.uppercase, "fullpath", &fullpath);
        let mut out = self.transform.to_string();
        // 优先 `{{name}}`，再试 `{name}`（单层大括号可避开指令 `{{ }}` 解析器的冲突）。
        for (key, val) in [
            ("index", index_str.as_str()),
            ("filename", filename_v.as_str()),
            ("extension", extension_v.as_str()),
            ("path", path_v.as_str()),
            ("fullpath", fullpath_v.as_str()),
        ] {
            out = out.replace(&format!("{{{{{key}}}}}"), val);
            out = out.replace(&format!("{{{key}}}"), val);
        }
        if !self.separator.is_empty() {
            out = out.replace(['\\', '/'], self.separator);
        }
        out
    }
}

fn up(uppercase: &[String], field: &str, value: &str) -> String {
    if uppercase.iter().any(|s| s == field) {
        value.to_uppercase()
    } else {
        value.to_string()
    }
}

#[async_trait]
impl Action for GenerateTimestamp {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.timestamp",
            "生成时间戳",
            "生成当前时间戳字符串",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("format", SchemaType::Str, false).with_default("%Y-%m-%d %H:%M:%S"),
            ParamSchema::new("utc", SchemaType::Bool, false).with_default(false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        use chrono::{Local, Utc};
        let empty = BTreeMap::new();
        let map = params.as_map().unwrap_or(&empty);
        let format = opt_str(map, "format").unwrap_or_else(|| "%Y-%m-%d %H:%M:%S".into());
        let utc = opt_bool(map, "utc", false);
        let (formatted, unix, iso8601) = if utc {
            let dt = Utc::now();
            (
                dt.format(&format).to_string(),
                dt.timestamp(),
                dt.to_rfc3339(),
            )
        } else {
            let dt = Local::now();
            (
                dt.format(&format).to_string(),
                dt.timestamp(),
                dt.to_rfc3339(),
            )
        };
        let mut out = BTreeMap::new();
        out.insert("value".into(), Value::Str(formatted));
        out.insert("unix".into(), Value::Int(unix));
        out.insert("iso8601".into(), Value::Str(iso8601));
        Ok(Value::Map(out))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(GenerateUuid));
    registry.register(Arc::new(GenerateCvid));
    registry.register(Arc::new(GeneratePath));
    registry.register(Arc::new(GenerateTimestamp));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;

    #[tokio::test]
    async fn uuid_and_cvid() {
        let mut ctx = ExecutionContext::default();
        let out = GenerateUuid
            .execute(Value::Map(BTreeMap::new()), &mut ctx)
            .await
            .unwrap();
        let items = out
            .as_map()
            .unwrap()
            .get("items")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str().unwrap().len(), 36);

        let cvid = GenerateCvid.execute(Value::Null, &mut ctx).await.unwrap();
        let s = cvid.as_str().unwrap();
        assert_eq!(s.len(), 32);
        assert!(
            s.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        );
    }
}
