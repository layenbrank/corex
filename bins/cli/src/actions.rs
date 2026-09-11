//! `corex actions` 与 `corex actions <id>`：把注册表里的事实摊开给人看。
//!
//! 光有 id 列表是不够的——写指令的人真正需要知道的是「这个动作要哪些参数、什么类型、
//! 要不要声明权限」。这些本来就躺在 `ActionMeta` 里，只是以前没有人把它打出来，
//! 于是作者只能去翻 `docs/reference/内置Action.md`。

use crate::build_registry;
use crate::fuzzy;
use crate::output::outln;
use anyhow::Result;
use corex_core::{ActionMeta, Bucket, EngineError, ParamSchema, SchemaType};
use corex_registry::ActionRegistry;

/// 列出已注册动作（按 bucket 分组）；给了 id 就展开它的参数表、权限与一段可粘贴的步骤片段。
pub(crate) fn cmd_actions(id: Option<&str>, bucket: Option<&str>) -> Result<()> {
    let registry = build_registry();
    let Some(id) = id else {
        return list(&registry, bucket);
    };

    let Some(meta) = registry.actions().into_iter().find(|m| m.id == id) else {
        return Err(unknown(id, &registry));
    };
    outln!("{} — {}", meta.id, meta.name);
    outln!("{}", meta.description);
    outln!("bucket {}   权限 {}", meta.bucket, granted(&registry, id));
    outln!("");
    outln!("参数");
    if meta.params.is_empty() {
        outln!("  （无）");
    }
    for param in &meta.params {
        outln!("  {}", describe(param));
    }
    outln!("");
    outln!("步骤片段（粘进 steps:）");
    for line in snippet(&meta) {
        outln!("{line}");
    }
    Ok(())
}

/// 按 bucket 分组列出；`wanted` 给了就只列那一组。
fn list(registry: &ActionRegistry, wanted: Option<&str>) -> Result<()> {
    let only = match wanted {
        Some(name) => Some(find_bucket(name)?),
        None => None,
    };
    for bucket in Bucket::ALL {
        if only.is_some_and(|only| only != bucket) {
            continue;
        }
        let metas: Vec<ActionMeta> = registry
            .actions()
            .into_iter()
            .filter(|meta| meta.bucket == bucket)
            .collect();
        if metas.is_empty() {
            continue;
        }
        outln!("{}（{}）", bucket, metas.len());
        for meta in metas {
            outln!("  {:<22} {}", meta.id, meta.description);
        }
    }
    Ok(())
}

/// 解析 `--bucket`；不认识的写法属于调用失误，顺手把可选值列出来。
fn find_bucket(name: &str) -> Result<Bucket> {
    Bucket::parse(name).ok_or_else(|| {
        let names: Vec<&str> = Bucket::ALL.iter().map(|b| b.as_str()).collect();
        crate::usage(format!("未知 bucket: {name}（可选: {}）", names.join("、")))
    })
}

/// 动作运行前必须声明的权限类别。
fn granted(registry: &ActionRegistry, id: &str) -> String {
    match registry.get(id) {
        Some(action) => {
            let kinds: Vec<&str> = action.permissions().iter().map(|k| k.name()).collect();
            if kinds.is_empty() {
                "无".to_string()
            } else {
                kinds.join("、")
            }
        }
        None => "无".to_string(),
    }
}

/// 一行参数说明：`  from  file  必填`，带上默认值与描述。
fn describe(param: &ParamSchema) -> String {
    let requirement = if param.required { "必填" } else { "可选" };
    let mut line = format!("{:<16} {:<6} {requirement}", param.name, param.ty.as_str());
    if let Some(default) = &param.default {
        line.push_str(&format!("  默认 {}", compact(default)));
    }
    if let Some(description) = &param.description {
        line.push_str(&format!("  {description}"));
    }
    line
}

/// 默认值的紧凑写法：字符串不带引号，其余用 JSON。
fn compact(value: &corex_core::Value) -> String {
    match value {
        corex_core::Value::Str(s) => s.clone(),
        other => other.to_json().to_string(),
    }
}

/// 可直接粘进 `steps:` 的骨架：必填参数留空占位，可选参数注释掉。
fn snippet(meta: &ActionMeta) -> Vec<String> {
    let mut lines = vec![
        format!("  - id: {}", short_id(&meta.id)),
        format!("    action: {}", meta.id),
        "    params:".to_string(),
    ];
    for param in &meta.params {
        let mut row = format!("{}: {}", param.name, placeholder(param.ty));
        if param.ty == SchemaType::Secret {
            row.push_str("   # 密钥：用 keyring.get 取值，别写死在 YAML 里");
        }
        if param.required {
            lines.push(format!("      {row}"));
        } else {
            lines.push(format!("      # {row}"));
        }
    }
    lines
}

/// `file.copy` → `copy`，只用作步骤 id 的建议值。
pub(crate) fn short_id(id: &str) -> &str {
    id.split('.').next_back().unwrap_or(id)
}

/// 占位值：类型决定长什么样，写指令的人照着改就行。
pub(crate) fn placeholder(ty: SchemaType) -> &'static str {
    match ty {
        SchemaType::Bool => "true",
        SchemaType::Int | SchemaType::Float => "0",
        SchemaType::Array => "[]",
        SchemaType::Map => "{}",
        _ => "\"\"",
    }
}

/// 未知动作属于调用失误（退出码 2），顺手给出最接近的名字。
fn unknown(id: &str, registry: &ActionRegistry) -> anyhow::Error {
    let names: Vec<String> = registry.actions().into_iter().map(|m| m.id).collect();
    let near = fuzzy::nearest(id, &names);
    let hint = if near.is_empty() {
        "（`corex actions` 看全部）".to_string()
    } else {
        format!("（最接近的: {}）", near.join("、"))
    };
    anyhow::Error::new(EngineError::ActionNotRegistered(format!("{id}{hint}")))
}
