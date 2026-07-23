//! 将线格式（小写路由 + 扁平 flags）组装为各模块 clap Args 的 serde 视图

use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::wire::WireArgs;

/// 无参子命令（externally tagged 为 `null`）
const UNIT_ACTIONS: &[&str] = &["monitors", "windows", "env", "inspect", "force"];

/// 将 wire 组装为可供 `serde_json::from_value::<ModuleArgs>` 的 Value
pub fn assemble_typed(module: &str, wire: &WireArgs) -> Result<Value> {
    match module {
        #[cfg(feature = "copy")]
        "copy" => assemble_leaf("copy", wire),
        #[cfg(feature = "scrub")]
        "scrub" => assemble_leaf("scrub", wire),
        #[cfg(feature = "shade")]
        "shade" => assemble_leaf("shade", wire),
        #[cfg(feature = "generate")]
        "generate" => {
            reject_action_module_routing("generate", wire)?;
            assemble_action_module("generate", wire)
        }
        #[cfg(feature = "scan")]
        "scan" => {
            reject_action_module_routing("scan", wire)?;
            assemble_action_module("scan", wire)
        }
        #[cfg(feature = "screenshot")]
        "screenshot" => {
            reject_action_module_routing("screenshot", wire)?;
            assemble_action_module("screenshot", wire)
        }
        #[cfg(feature = "morph")]
        "morph" => {
            reject_action_module_routing("morph", wire)?;
            assemble_action_module("morph", wire)
        }
        #[cfg(feature = "bootstrap")]
        "bootstrap" => {
            reject_action_module_routing("bootstrap", wire)?;
            assemble_action_module("bootstrap", wire)
        }
        #[cfg(feature = "exec")]
        "exec" => {
            reject_action_module_routing("exec", wire)?;
            assemble_action_module("exec", wire)
        }
        #[cfg(feature = "compression")]
        "compression" => {
            if wire.algorithm.is_some() {
                bail!("compression 使用 format，不应设置 algorithm");
            }
            assemble_compression(wire)
        }
        #[cfg(feature = "codec")]
        "codec" => {
            if wire.format.is_some() {
                bail!("codec 使用 algorithm，不应设置 format");
            }
            assemble_codec(wire)
        }
        _ => bail!("未知或未启用的模块: {module}"),
    }
}

fn assemble_leaf(module: &str, wire: &WireArgs) -> Result<Value> {
    if wire.action.is_some() {
        bail!("{module} 为单操作模块，不应设置 action");
    }
    if wire.format.is_some() {
        if module == "shade" {
            bail!(
                "shade 不应设置步骤级 format（归档 format 仅 compression 使用）；图片输出格式请写在 params.format"
            );
        }
        bail!("{module} 不应设置 format（仅 compression 使用）");
    }
    if wire.algorithm.is_some() {
        bail!("{module} 不应设置 algorithm（仅 codec 使用）");
    }
    Ok(normalize_flags(&wire.flags))
}

fn reject_action_module_routing(module: &str, wire: &WireArgs) -> Result<()> {
    if wire.format.is_some() {
        bail!("{module} 不应设置 format（仅 compression 使用）");
    }
    if wire.algorithm.is_some() {
        bail!("{module} 不应设置 algorithm（仅 codec 使用）");
    }
    Ok(())
}

fn assemble_action_module(module: &str, wire: &WireArgs) -> Result<Value> {
    let action = wire
        .action
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("{module} 需要 action（CLI 子命令，kebab-case）"))?;
    validate_action(module, action)?;
    let key = kebab_to_pascal(action);
    let body = if UNIT_ACTIONS.contains(&action) {
        ensure_empty_unit_flags(module, action, &wire.flags)?;
        Value::Null
    } else {
        normalize_flags(&wire.flags)
    };
    Ok(json!({ key: body }))
}

fn ensure_empty_unit_flags(module: &str, action: &str, flags: &Value) -> Result<()> {
    let empty = match flags {
        Value::Null => true,
        Value::Object(map) => map.is_empty(),
        _ => false,
    };
    if !empty {
        bail!("{module} action `{action}` 无参数，params/args 必须为空对象");
    }
    Ok(())
}

fn assemble_compression(wire: &WireArgs) -> Result<Value> {
    let action = wire
        .action
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("compression 需要 action: compress | decompress"))?;
    if !matches!(action, "compress" | "decompress") {
        bail!("compression 未知 action: {action}（允许: compress, decompress）");
    }
    let format = wire
        .format
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("compression 需要 format: zip | tar-gz | 7z"))?;
    let format_key = format_to_pascal(format)?;
    let action_key = kebab_to_pascal(action);
    let flags = normalize_flags(&wire.flags);
    Ok(json!({ action_key: { format_key: flags } }))
}

fn assemble_codec(wire: &WireArgs) -> Result<Value> {
    let action = wire
        .action
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("codec 需要 action: encode | decode | hash"))?;
    if !matches!(action, "encode" | "decode" | "hash") {
        bail!("codec 未知 action: {action}（允许: encode, decode, hash）");
    }
    let allowed_algo = match action {
        "hash" => "md5",
        _ => "base64",
    };
    let algorithm = wire.algorithm.as_deref().ok_or_else(|| {
        anyhow::anyhow!("codec action `{action}` 需要 algorithm: {allowed_algo}")
    })?;
    let algo_key = algorithm_to_pascal(action, algorithm)?;
    let action_key = kebab_to_pascal(action);
    let flags = normalize_flags(&wire.flags);
    Ok(json!({ action_key: { algo_key: flags } }))
}

fn format_to_pascal(format: &str) -> Result<&'static str> {
    match format {
        "zip" => Ok("Zip"),
        "tar-gz" => Ok("TarGz"),
        "7z" | "seven-z" => Ok("SevenZ"),
        other => bail!("compression 未知 format: {other}（允许: zip, tar-gz, 7z）"),
    }
}

fn algorithm_to_pascal(action: &str, algorithm: &str) -> Result<&'static str> {
    match (action, algorithm) {
        ("encode" | "decode", "base64") => Ok("Base64"),
        ("hash", "md5") => Ok("Md5"),
        ("encode" | "decode", other) => {
            bail!("codec {action} 未知 algorithm: {other}（允许: base64）")
        }
        ("hash", other) => bail!("codec hash 未知 algorithm: {other}（允许: md5）"),
        _ => bail!("codec 未知 action/algorithm 组合"),
    }
}

fn validate_action(module: &str, action: &str) -> Result<()> {
    let allowed: &[&str] = match module {
        "generate" => &["path", "uuid"],
        "scan" => &["os"],
        "screenshot" => &["capture", "monitors", "windows", "crop", "clipboard"],
        "morph" => &[
            "meta",
            "render-page",
            "render-thumbnails",
            "search",
            "export",
            "merge",
            "split",
            "split-by-count",
            "to-images",
            "to-office",
        ],
        "bootstrap" => &["env", "inspect", "force"],
        "exec" => &["run"],
        _ => return Ok(()),
    };
    if !allowed.contains(&action) {
        bail!(
            "{module} 未知 action: {action}（允许: {}）",
            allowed.join(", ")
        );
    }
    Ok(())
}

/// kebab-case → PascalCase（`render-page` → `RenderPage`）
pub fn kebab_to_pascal(s: &str) -> String {
    s.split('-')
        .filter(|p| !p.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

fn normalize_flags(flags: &Value) -> Value {
    match flags {
        Value::Null => json!({}),
        other => other.clone(),
    }
}

/// 校验线格式：assemble +（无 `${` 占位符时）尝试反序列化为模块 Args
pub fn validate_wire(module: &str, wire: &WireArgs) -> Result<()> {
    let typed = assemble_typed(module, wire)?;
    if typed.to_string().contains("${") {
        // 占位符在运行时由 PipelineContext 解析，静态阶段无法严格 decode
        return Ok(());
    }
    decode_module_args(module, typed)
}

fn decode_module_args(module: &str, typed: Value) -> Result<()> {
    match module {
        #[cfg(feature = "copy")]
        "copy" => {
            let _: crate::copy::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("copy params 无效: {e}"))?;
        }
        #[cfg(feature = "scrub")]
        "scrub" => {
            let _: crate::scrub::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("scrub params 无效: {e}"))?;
        }
        #[cfg(feature = "shade")]
        "shade" => {
            let _: crate::shade::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("shade params 无效: {e}"))?;
        }
        #[cfg(feature = "generate")]
        "generate" => {
            let _: crate::generate::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("generate params 无效: {e}"))?;
        }
        #[cfg(feature = "scan")]
        "scan" => {
            let _: crate::scan::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("scan params 无效: {e}"))?;
        }
        #[cfg(feature = "screenshot")]
        "screenshot" => {
            let _: crate::screenshot::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("screenshot params 无效: {e}"))?;
        }
        #[cfg(feature = "morph")]
        "morph" => {
            let _: crate::morph::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("morph params 无效: {e}"))?;
        }
        #[cfg(feature = "bootstrap")]
        "bootstrap" => {
            let _: crate::bootstrap::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("bootstrap params 无效: {e}"))?;
        }
        #[cfg(feature = "exec")]
        "exec" => {
            let _: crate::exec::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("exec params 无效: {e}"))?;
        }
        #[cfg(feature = "compression")]
        "compression" => {
            let _: crate::compression::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("compression params 无效: {e}"))?;
        }
        #[cfg(feature = "codec")]
        "codec" => {
            let _: crate::codec::schema::Args = serde_json::from_value(typed)
                .map_err(|e| anyhow::anyhow!("codec params 无效: {e}"))?;
        }
        _ => bail!("未知或未启用的模块: {module}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_compression_flat() {
        let wire = WireArgs::compression(
            "compress",
            "zip",
            json!({"from": "a", "to": "b.zip"}),
        );
        let v = assemble_typed("compression", &wire).unwrap();
        assert_eq!(
            v,
            json!({"Compress": {"Zip": {"from": "a", "to": "b.zip"}}})
        );
    }

    #[test]
    fn assembles_codec_flat() {
        let wire = WireArgs::codec("hash", "md5", json!({"input": "hello"}));
        let v = assemble_typed("codec", &wire).unwrap();
        assert_eq!(v, json!({"Hash": {"Md5": {"input": "hello"}}}));
    }

    #[test]
    fn assembles_generate_path() {
        let wire = WireArgs::action("path", json!({"from": "a", "to": "b"}));
        let v = assemble_typed("generate", &wire).unwrap();
        assert_eq!(v, json!({"Path": {"from": "a", "to": "b"}}));
    }

    #[test]
    fn assembles_unit_action() {
        let wire = WireArgs::action("monitors", json!({}));
        let v = assemble_typed("screenshot", &wire).unwrap();
        assert_eq!(v, json!({"Monitors": null}));
    }

    #[test]
    fn rejects_action_on_copy() {
        let wire = WireArgs::action("run", json!({}));
        assert!(assemble_typed("copy", &wire).is_err());
    }

    #[test]
    fn rejects_nonempty_flags_on_unit_action() {
        let wire = WireArgs::action("monitors", json!({"to": "x"}));
        let err = assemble_typed("screenshot", &wire).unwrap_err().to_string();
        assert!(err.contains("无参数"));
    }

    #[test]
    fn assembles_bootstrap_env() {
        let wire = WireArgs::action("env", json!({}));
        let v = assemble_typed("bootstrap", &wire).unwrap();
        assert_eq!(v, json!({"Env": null}));
    }

    #[test]
    fn assembles_morph_render_page() {
        let wire = WireArgs::action("render-page", json!({"path": "a.pdf", "page_index": 0}));
        let v = assemble_typed("morph", &wire).unwrap();
        assert_eq!(
            v,
            json!({"RenderPage": {"path": "a.pdf", "page_index": 0}})
        );
    }

    #[test]
    fn rejects_codec_encode_with_md5() {
        let wire = WireArgs::codec("encode", "md5", json!({"input": "x"}));
        let err = assemble_typed("codec", &wire).unwrap_err().to_string();
        assert!(err.contains("base64"));
    }

    #[test]
    fn rejects_shade_step_format() {
        let wire = WireArgs {
            action: None,
            format: Some("webp".into()),
            algorithm: None,
            flags: json!({"from": "a", "to": "b"}),
        };
        let err = assemble_typed("shade", &wire).unwrap_err().to_string();
        assert!(err.contains("params.format"));
    }

    #[test]
    fn validate_wire_rejects_missing_copy_fields() {
        let wire = WireArgs::flags(json!({"from": "a"}));
        let err = validate_wire("copy", &wire).unwrap_err().to_string();
        assert!(err.contains("copy params 无效") || err.contains("missing field"));
    }

    #[test]
    fn validate_wire_skips_placeholders() {
        let wire = WireArgs::flags(json!({
            "from": "${var.src}",
            "to": "${var.dst}",
            "empty": false,
            "includes": [],
            "excludes": []
        }));
        validate_wire("copy", &wire).expect("placeholders should skip decode");
    }
}
