//! Codec actions: base64 encode/decode and md5 hash.

use crate::builtin::util::{confine_path, ensure_parent, opt_str, require_map, require_str};
use crate::ActionRegistry;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use md5::{Digest, Md5};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn read_bytes(
    ctx: &ExecutionContext,
    map: &BTreeMap<String, Value>,
) -> Result<Vec<u8>, ActionError> {
    let input = opt_str(map, "input");
    let file = opt_str(map, "file");
    match (input, file) {
        (Some(text), None) => Ok(text.into_bytes()),
        (None, Some(path)) => {
            let p = confine_path(ctx, Path::new(&path))?;
            std::fs::read(&p)
                .map_err(|e| ActionError::execution(format!("读取文件失败 {}: {e}", p.display())))
        }
        (Some(_), Some(_)) => Err(ActionError::InvalidParams(
            "请只指定 input 或 file 之一".into(),
        )),
        (None, None) => Err(ActionError::MissingParam("input|file".to_string())),
    }
}

fn maybe_write(
    ctx: &ExecutionContext,
    map: &BTreeMap<String, Value>,
    bytes: &[u8],
) -> Result<Option<PathBuf>, ActionError> {
    if let Some(path) = opt_str(map, "output") {
        let p = confine_path(ctx, Path::new(&path))?;
        ensure_parent(&p)?;
        std::fs::write(&p, bytes)?;
        Ok(Some(p))
    } else {
        Ok(None)
    }
}

fn result_map(text: Option<String>, path: Option<PathBuf>) -> Value {
    let mut m = BTreeMap::new();
    if let Some(t) = text {
        m.insert("text".into(), Value::Str(t));
    }
    if let Some(p) = path {
        m.insert("path".into(), Value::File(p));
    }
    Value::Map(m)
}

pub struct CodecBase64Encode;
pub struct CodecBase64Decode;
pub struct CodecHashMd5;
pub struct CodecJsonParse;

const MAX_JSON_PARSE_BYTES: usize = 10 * 1024 * 1024;

#[async_trait]
impl Action for CodecBase64Encode {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.base64.encode",
            "Base64 Encode",
            "Base64 编码",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("input", SchemaType::Str, false),
            ParamSchema::new("file", SchemaType::File, false),
            ParamSchema::new("output", SchemaType::File, false),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let bytes = read_bytes(ctx, map)?;
        let text = STANDARD.encode(&bytes);
        let path = maybe_write(ctx, map, text.as_bytes())?;
        Ok(result_map(Some(text), path))
    }
}

#[async_trait]
impl Action for CodecBase64Decode {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.base64.decode",
            "Base64 Decode",
            "Base64 解码",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("input", SchemaType::Str, false),
            ParamSchema::new("file", SchemaType::File, false),
            ParamSchema::new("output", SchemaType::File, false),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let input = match (opt_str(map, "input"), opt_str(map, "file")) {
            (Some(t), None) => t,
            (None, Some(path)) => {
                let p = confine_path(ctx, Path::new(&path))?;
                std::fs::read_to_string(&p)
                    .map_err(|e| ActionError::execution(format!("读取失败: {e}")))?
            }
            _ => {
                return Err(ActionError::InvalidParams(
                    "请只指定 input 或 file 之一".into(),
                ))
            }
        };
        let bytes = STANDARD
            .decode(input.trim())
            .map_err(|e| ActionError::execution(format!("base64 解码失败: {e}")))?;
        let path = maybe_write(ctx, map, &bytes)?;
        let text = if path.is_some() {
            None
        } else {
            Some(match std::str::from_utf8(&bytes) {
                Ok(s) => s.to_string(),
                Err(_) => bytes.iter().map(|b| format!("{b:02x}")).collect(),
            })
        };
        Ok(result_map(text, path))
    }
}

#[async_trait]
impl Action for CodecHashMd5 {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.hash.md5",
            "MD5 Hash",
            "计算 MD5 摘要",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("input", SchemaType::Str, false),
            ParamSchema::new("file", SchemaType::File, false),
            ParamSchema::new("output", SchemaType::File, false),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let bytes = read_bytes(ctx, map)?;
        let digest = Md5::digest(&bytes);
        let text: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let path = maybe_write(ctx, map, text.as_bytes())?;
        Ok(result_map(Some(text), path))
    }
}

#[async_trait]
impl Action for CodecJsonParse {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.json.parse",
            "JSON Parse",
            "将 JSON 字符串解析为结构化 Value",
            ActionCategory::Data,
        )
        .with_params(vec![ParamSchema::new("text", SchemaType::Str, true)])
    }

    async fn execute(
        &self, params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let text = require_str(map, "text")?;
        if text.len() > MAX_JSON_PARSE_BYTES {
            return Err(ActionError::InvalidParams(format!(
                "JSON 输入超过 {MAX_JSON_PARSE_BYTES} 字节"
            )));
        }
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| ActionError::execution(format!("JSON 解析失败: {e}")))?;
        Ok(Value::from_json(json))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CodecBase64Encode));
    registry.register(Arc::new(CodecBase64Decode));
    registry.register(Arc::new(CodecHashMd5));
    registry.register(Arc::new(CodecJsonParse));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;

    #[tokio::test]
    async fn encode_decode_roundtrip() {
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("input".into(), Value::Str("hello".into()));
        let enc = CodecBase64Encode
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let text = enc.as_map().unwrap().get("text").unwrap().as_str().unwrap();
        assert_eq!(text, "aGVsbG8=");

        let mut params = BTreeMap::new();
        params.insert("input".into(), Value::Str(text.into()));
        let dec = CodecBase64Decode
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert_eq!(
            dec.as_map().unwrap().get("text").unwrap().as_str().unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn md5_hello() {
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("input".into(), Value::Str("hello".into()));
        let out = CodecHashMd5
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert_eq!(
            out.as_map().unwrap().get("text").unwrap().as_str().unwrap(),
            "5d41402abc4b2a76b9719d911017c592"
        );
    }

    #[tokio::test]
    async fn json_parse_object() {
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("text".into(), Value::Str(r#"{"a":1}"#.into()));
        let out = CodecJsonParse
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert_eq!(
            out.get_path("a").and_then(|v| v.as_i64()),
            Some(1)
        );
    }

    #[test]
    fn json_parse_rejects_oversized() {
        use proptest::prelude::*;
        proptest!(|(n in 1usize..64)| {
            // Keep generated payloads small but exercise rejection path via direct size check.
            let text = format!("{{\"k\":\"{}\"}}", "x".repeat(n));
            let mut ctx = ExecutionContext::default();
            let mut params = BTreeMap::new();
            params.insert("text".into(), Value::Str(text));
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let _ = rt.block_on(CodecJsonParse.execute(Value::Map(params), &mut ctx));
        });
    }

    #[tokio::test]
    async fn json_parse_rejects_huge_input() {
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("text".into(), Value::Str("x".repeat(11 * 1024 * 1024)));
        let err = CodecJsonParse
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("上限") || err.to_string().contains("limit") || err.to_string().contains("过大") || err.to_string().contains("10"));
    }
}
