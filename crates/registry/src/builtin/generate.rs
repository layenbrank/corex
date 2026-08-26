//! Generate actions: path list, uuid, cvid.

use crate::builtin::filter::Filter;
use crate::builtin::util::{
    ensure_parent, opt_bool, opt_i64, opt_str, opt_str_list, require_map, require_path, require_str,
};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
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

#[async_trait]
impl Action for GenerateUuid {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.uuid",
            "Generate UUID",
            "生成 UUID v4",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("count", SchemaType::Int, false).with_default(1),
            ParamSchema::new("uppercase", SchemaType::Bool, false).with_default(false),
        ])
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
        let mut list = Vec::with_capacity(count);
        for _ in 0..count {
            let id = Uuid::new_v4().to_string();
            list.push(Value::Str(if uppercase {
                id.to_uppercase()
            } else {
                id
            }));
        }
        let mut out = BTreeMap::new();
        out.insert("items".into(), Value::List(list.clone()));
        out.insert(
            "value".into(),
            list.first().cloned().unwrap_or(Value::Null),
        );
        Ok(Value::Map(out))
    }
}

#[async_trait]
impl Action for GenerateCvid {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.cvid",
            "Generate CVID",
            "生成 GUID v4 大写 hex（CVID）",
            ActionCategory::Data,
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
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "generate.path",
            "Generate Path List",
            "遍历目录并按模板写出路径列表",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("transform", SchemaType::Str, true),
            ParamSchema::new("index", SchemaType::Int, false).with_default(0),
            ParamSchema::new("separator", SchemaType::Str, false).with_default(""),
            ParamSchema::new("includes", SchemaType::List, false),
            ParamSchema::new("excludes", SchemaType::List, false),
            ParamSchema::new("uppercase", SchemaType::List, false),
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
        let transform = require_str(map, "transform")?;
        let index_start = opt_i64(map, "index", 0) as usize;
        let separator = opt_str(map, "separator").unwrap_or_default();
        let includes = opt_str_list(map, "includes");
        let excludes = opt_str_list(map, "excludes");
        let uppercase = opt_str_list(map, "uppercase");

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
        let mut file = std::fs::File::create(&to)?;
        let mut items = 0u64;
        for (key, entry) in entries.iter().enumerate() {
            let line = path_transform_line(
                &transform,
                entry.path(),
                entry.file_name().to_string_lossy().as_ref(),
                key + index_start,
                pad_width,
                &uppercase,
                &separator,
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

fn path_transform_line(
    transform: &str,
    entry_path: &Path,
    filename: &str,
    index: usize,
    pad_width: usize,
    uppercase: &[String],
    separator: &str,
    from: &Path,
) -> String {
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
        let sep = if !separator.is_empty() {
            separator
        } else {
            std::path::MAIN_SEPARATOR_STR
        };
        format!("{dirpart}{sep}{filename}")
    };
    let index_str = format!("{:0pad_width$}", index, pad_width = pad_width);
    let filename_v = up(uppercase, "filename", filename);
    let extension_v = up(uppercase, "extension", &extension);
    let path_v = up(uppercase, "path", &dirpart);
    let fullpath_v = up(uppercase, "fullpath", &fullpath);
    let mut out = transform.to_string();
    // Prefer `{{name}}` then `{name}` (single braces avoid Shortcut `{{ }}` resolver clash).
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
    if !separator.is_empty() {
        out = out.replace('\\', separator).replace('/', separator);
    }
    out
}

fn up(uppercase: &[String], field: &str, value: &str) -> String {
    if uppercase.iter().any(|s| s == field) {
        value.to_uppercase()
    } else {
        value.to_string()
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(GenerateUuid));
    registry.register(Arc::new(GenerateCvid));
    registry.register(Arc::new(GeneratePath));
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
            .as_list()
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str().unwrap().len(), 36);

        let cvid = GenerateCvid.execute(Value::Null, &mut ctx).await.unwrap();
        let s = cvid.as_str().unwrap();
        assert_eq!(s.len(), 32);
        assert!(s
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
    }
}
