//! Codec actions: base64 encode/decode and md5 hash.

use crate::builtin::util::{ensure_parent, opt_str, require_map};
use crate::ActionRegistry;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use md5::{Digest, Md5};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

fn read_bytes(map: &BTreeMap<String, Value>) -> Result<Vec<u8>, ActionError> {
    let input = opt_str(map, "input");
    let file = opt_str(map, "file");
    match (input, file) {
        (Some(text), None) => Ok(text.into_bytes()),
        (None, Some(path)) => std::fs::read(&path)
            .map_err(|e| ActionError::execution(format!("读取文件失败 {path}: {e}"))),
        (Some(_), Some(_)) => Err(ActionError::InvalidParams(
            "请只指定 input 或 file 之一".into(),
        )),
        (None, None) => Err(ActionError::MissingParam("input|file".to_string())),
    }
}

fn maybe_write(map: &BTreeMap<String, Value>, bytes: &[u8]) -> Result<Option<PathBuf>, ActionError> {
    if let Some(path) = opt_str(map, "output") {
        let p = PathBuf::from(&path);
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
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let bytes = read_bytes(map)?;
        let text = STANDARD.encode(&bytes);
        let path = maybe_write(map, text.as_bytes())?;
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
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let input = match (opt_str(map, "input"), opt_str(map, "file")) {
            (Some(t), None) => t,
            (None, Some(path)) => std::fs::read_to_string(&path)
                .map_err(|e| ActionError::execution(format!("读取失败: {e}")))?,
            _ => {
                return Err(ActionError::InvalidParams(
                    "请只指定 input 或 file 之一".into(),
                ))
            }
        };
        let bytes = STANDARD
            .decode(input.trim())
            .map_err(|e| ActionError::execution(format!("base64 解码失败: {e}")))?;
        let path = maybe_write(map, &bytes)?;
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
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let bytes = read_bytes(map)?;
        let digest = Md5::digest(&bytes);
        let text: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let path = maybe_write(map, text.as_bytes())?;
        Ok(result_map(Some(text), path))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CodecBase64Encode));
    registry.register(Arc::new(CodecBase64Decode));
    registry.register(Arc::new(CodecHashMd5));
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
}
