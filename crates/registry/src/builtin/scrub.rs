//! `scrub.run` — delete named targets under a source tree.

use crate::builtin::util::{opt_bool, require_map, require_path, require_str};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

pub struct ScrubRun;

#[async_trait]
impl Action for ScrubRun {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "scrub.run",
            "Scrub",
            "删除源目录下指定名称的目标",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("source", SchemaType::File, true),
            ParamSchema::new("target", SchemaType::Str, true),
            ParamSchema::new("recursive", SchemaType::Bool, false).with_default(false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let source = require_path(map, "source")?;
        let target = require_str(map, "target")?;
        let recursive = opt_bool(map, "recursive", false);

        if !source.exists() {
            return Err(ActionError::execution(format!(
                "未找到指定路径: {}",
                source.display()
            )));
        }

        let matches = if recursive {
            if !source.is_dir() {
                return Err(ActionError::execution(format!(
                    "路径不是目录: {}",
                    source.display()
                )));
            }
            WalkDir::new(&source)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy() == target)
                .map(|e| e.path().to_path_buf())
                .collect::<Vec<_>>()
        } else {
            match std::fs::read_dir(&source) {
                Ok(rd) => rd
                    .filter_map(|e| e.ok().map(|ent| ent.path()))
                    .filter(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy() == target)
                            .unwrap_or(false)
                    })
                    .collect(),
                Err(_) => Vec::new(),
            }
        };

        let mut removed = 0i64;
        for path in matches {
            remove_path(&path)?;
            removed += 1;
        }

        let mut out = std::collections::BTreeMap::new();
        out.insert("path".into(), Value::File(source));
        out.insert("removed".into(), Value::Int(removed));
        Ok(Value::Map(out))
    }
}

fn remove_path(path: &Path) -> Result<(), ActionError> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(ScrubRun));
}
