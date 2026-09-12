//! 编解码动作：base64 编解码与 md5 摘要。

use crate::ActionRegistry;
use crate::builtin::util::{
    confine_path, ensure_parent, hex, opt_bool, opt_str, require_map, require_str,
};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use md5::{Digest, Md5};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
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
pub struct CodecJsonPick;
pub struct CodecUrlEncode;
pub struct CodecUrlDecode;

/// `encodeURIComponent` 会转义的字符集：字母数字与 `- _ . ! ~ * ' ( )` 之外全转。
const COMPONENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// `encodeURI` 会转义的字符集：URL 结构（`; , / ? : @ & = + $ #`）保持原样。
const URI: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// `mode` → 转义集。
///
/// 数据代替分支：加一种语义只加一行，报错文案也从这张表拼，不会和实际支持的集合走神。
const ENCODE_SETS: &[(&str, &AsciiSet)] = &[("component", COMPONENT), ("uri", URI)];

/// 按 `mode` 取转义集；不填按 `component`（拼查询参数最常用的那种）。
fn find_set(map: &BTreeMap<String, Value>) -> Result<&'static AsciiSet, ActionError> {
    let raw = opt_str(map, "mode").unwrap_or_default();
    let mode = raw.trim().to_ascii_lowercase();
    let mode = if mode.is_empty() {
        "component"
    } else {
        mode.as_str()
    };
    ENCODE_SETS
        .iter()
        .find(|(name, _)| *name == mode)
        .map(|(_, set)| *set)
        .ok_or_else(|| {
            let names: Vec<&str> = ENCODE_SETS.iter().map(|(name, _)| *name).collect();
            ActionError::InvalidParams(format!("不支持的 mode: {mode}（{}）", names.join(" | ")))
        })
}

const MAX_JSON_PARSE_BYTES: usize = 10 * 1024 * 1024;

#[async_trait]
impl Action for CodecBase64Encode {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.base64.encode",
            "Base64 Encode",
            "Base64 编码",
            Bucket::Data,
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
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.base64.decode",
            "Base64 Decode",
            "Base64 解码",
            Bucket::Data,
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
                ));
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
                Err(_) => hex(&bytes),
            })
        };
        Ok(result_map(text, path))
    }
}

#[async_trait]
impl Action for CodecHashMd5 {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::FILESYSTEM
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("codec.hash.md5", "MD5 Hash", "计算 MD5 摘要", Bucket::Data).with_params(
            vec![
                ParamSchema::new("input", SchemaType::Str, false),
                ParamSchema::new("file", SchemaType::File, false),
                ParamSchema::new("output", SchemaType::File, false),
            ],
        )
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let bytes = read_bytes(ctx, map)?;
        let digest = Md5::digest(&bytes);
        let text = hex(digest.as_slice());
        let path = maybe_write(ctx, map, text.as_bytes())?;
        Ok(result_map(Some(text), path))
    }
}

#[async_trait]
impl Action for CodecJsonParse {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.json.parse",
            "JSON 解析",
            "将 JSON 字符串解析为结构化 Value",
            Bucket::Data,
        )
        .with_params(vec![ParamSchema::new("text", SchemaType::Str, true)])
    }

    async fn execute(
        &self,
        params: Value,
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

#[async_trait]
impl Action for CodecJsonPick {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.json.pick",
            "JSON 取子节点",
            "从已解析的结构里按 JSON Pointer 取一个子节点；取不到就是 Null",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("value", SchemaType::Any, true)
                .with_description("已解析的结构，如 `{{steps.session.data}}`"),
            ParamSchema::new("pointer", SchemaType::Str, true)
                .with_description("RFC 6901 写法，如 `/data/uploaded`"),
            ParamSchema::new("default", SchemaType::Any, false)
                .with_description("取不到时改用它（默认 Null）"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let value = map
            .get("value")
            .ok_or_else(|| ActionError::MissingParam("value".into()))?;
        let pointer = require_str(map, "pointer")?;
        if !pointer.is_empty() && !pointer.starts_with('/') {
            return Err(ActionError::InvalidParams(format!(
                "pointer 需要是 JSON Pointer（以 / 开头，如 /data/uploaded），收到 {pointer}"
            )));
        }
        // 取不到不算错误：可选字段缺席是常态（服务端的 `uploaded` 就常常整个不写）。
        Ok(pick(value, &pointer)
            .cloned()
            .or_else(|| map.get("default").cloned())
            .unwrap_or(Value::Null))
    }
}

/// 按 JSON Pointer（RFC 6901）在结构上走一段路。
///
/// 不借道 `serde_json`：那会把整棵已解析的结构再克隆一遍，大响应上不值得。
fn pick<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(root);
    }
    let mut current = root;
    for segment in pointer.trim_start_matches('/').split('/') {
        // RFC 6901 的转义：`~1` 是 `/`，`~0` 是 `~`（顺序不能反）。
        let key = segment.replace("~1", "/").replace("~0", "~");
        current = match current {
            Value::Map(map) => map.get(&key)?,
            Value::Array(items) => items.get(key.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

#[async_trait]
impl Action for CodecUrlEncode {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.url.encode",
            "URL 编码",
            "百分号编码：拼查询参数用 component，编整条 URL 用 uri",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("input", SchemaType::Str, true),
            ParamSchema::new("mode", SchemaType::Str, false)
                .with_default("component")
                .with_description(
                    "component（encodeURIComponent）| uri（encodeURI，保留 URL 结构）",
                ),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let input = require_str(map, "input")?;
        let set = find_set(map)?;
        let text = utf8_percent_encode(&input, set).to_string();
        Ok(result_map(Some(text), None))
    }
}

#[async_trait]
impl Action for CodecUrlDecode {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "codec.url.decode",
            "URL 解码",
            "百分号解码；表单串（`+` 代表空格）时开 plus_as_space",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("input", SchemaType::Str, true),
            ParamSchema::new("plus_as_space", SchemaType::Bool, false)
                .with_default(false)
                .with_description("按 application/x-www-form-urlencoded 把 `+` 当空格"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let input = require_str(map, "input")?;
        // `+` 要先换掉再解码：否则真正的加号（`%2B`）会被一起吃掉。
        let prepared = if opt_bool(map, "plus_as_space", false) {
            input.replace('+', " ")
        } else {
            input
        };
        let text = percent_decode_str(&prepared)
            .decode_utf8_lossy()
            .into_owned();
        Ok(result_map(Some(text), None))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CodecBase64Encode));
    registry.register(Arc::new(CodecBase64Decode));
    registry.register(Arc::new(CodecHashMd5));
    registry.register(Arc::new(CodecJsonParse));
    registry.register(Arc::new(CodecJsonPick));
    registry.register(Arc::new(CodecUrlEncode));
    registry.register(Arc::new(CodecUrlDecode));
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
        assert_eq!(out.find_path("a").and_then(|v| v.as_i64()), Some(1));
    }

    /// `codec.json.pick`：从已解析的结构里取子节点。
    ///
    /// 缺席不是错误（可选字段的常态），所以「取不到」直接给 `Null`，`default` 可以改。
    #[tokio::test]
    async fn json_pick_takes_a_subnode() {
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert(
            "text".into(),
            Value::Str(r#"{"data":{"uploaded":[{"index":0,"hash":"aa"}]}}"#.into()),
        );
        let parsed = CodecJsonParse
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let pick = |pointer: &str| {
            let mut params = BTreeMap::new();
            params.insert("value".into(), parsed.clone());
            params.insert("pointer".into(), Value::Str(pointer.into()));
            params
        };

        // 命中的子节点
        let picked = CodecJsonPick
            .execute(Value::Map(pick("/data/uploaded")), &mut ctx)
            .await
            .unwrap();
        assert_eq!(picked.as_array().unwrap().len(), 1);

        // 下钻到数组元素
        let hash = CodecJsonPick
            .execute(Value::Map(pick("/data/uploaded/0/hash")), &mut ctx)
            .await
            .unwrap();
        assert_eq!(hash.as_str(), Some("aa"));

        // 缺席 → Null，而不是报错
        let missing = CodecJsonPick
            .execute(Value::Map(pick("/data/missing")), &mut ctx)
            .await
            .unwrap();
        assert_eq!(missing, Value::Null);

        // `default` 可改这一支
        let mut params = pick("/data/missing");
        params.insert("default".into(), Value::Array(vec![]));
        let fallback = CodecJsonPick
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert_eq!(fallback, Value::Array(vec![]));

        // 写成点路径要当场报，而不是静默落到「取不到」
        let err = CodecJsonPick
            .execute(Value::Map(pick("data.uploaded")), &mut ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("JSON Pointer"), "{err}");
    }

    #[test]
    fn json_parse_rejects_oversized() {
        use proptest::prelude::*;
        proptest!(|(n in 1usize..64)| {
            // 让生成的负载保持很小，但用直接的大小检查来覆盖拒绝路径。
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
        assert!(
            err.to_string().contains("上限")
                || err.to_string().contains("limit")
                || err.to_string().contains("过大")
                || err.to_string().contains("10")
        );
    }

    async fn url_case(action: &dyn Action, input: &str, extra: &[(&str, Value)]) -> String {
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("input".into(), Value::Str(input.into()));
        for (k, v) in extra {
            params.insert((*k).to_string(), v.clone());
        }
        let out = action.execute(Value::Map(params), &mut ctx).await.unwrap();
        out.as_map()
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
    }

    /// `component` 模式对付查询参数：`&` 与 `/` 都得转，不然拼出来的 URL 会散架。
    #[tokio::test]
    async fn url_encode_component() {
        let out = url_case(&CodecUrlEncode, "a b&c/d", &[]).await;
        assert_eq!(out, "a%20b%26c%2Fd");
        // 未保留字符不该被多转义。
        assert_eq!(
            url_case(&CodecUrlEncode, "-_.!~*'()", &[]).await,
            "-_.!~*'()"
        );
    }

    /// `uri` 模式保留 URL 结构，只把空格、引号这类「非法」字符转掉。
    #[tokio::test]
    async fn url_encode_uri() {
        let out = url_case(
            &CodecUrlEncode,
            "https://x/a b?q=1&r=2#frag",
            &[("mode", Value::Str("uri".into()))],
        )
        .await;
        assert_eq!(out, "https://x/a%20b?q=1&r=2#frag");
    }

    #[tokio::test]
    async fn url_decode_roundtrip() {
        assert_eq!(
            url_case(&CodecUrlDecode, "a%20b%26c%2Fd", &[]).await,
            "a b&c/d"
        );
        // 中文按 UTF-8 编码后能还原。
        let encoded = url_case(&CodecUrlEncode, "分片上传", &[]).await;
        assert_eq!(url_case(&CodecUrlDecode, &encoded, &[]).await, "分片上传");
    }

    /// 表单串里 `+` 是空格，但 `%2B` 必须是加号本身。
    #[tokio::test]
    async fn url_decode_form_plus() {
        assert_eq!(
            url_case(
                &CodecUrlDecode,
                "a+b%2Bc",
                &[("plus_as_space", Value::Bool(true))],
            )
            .await,
            "a b+c"
        );
        assert_eq!(url_case(&CodecUrlDecode, "a+b", &[]).await, "a+b");
    }
}
