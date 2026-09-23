//! `text.*`：纯字符串变换（`act-text`）。
//!
//! 六个动作同形——取参数、算出一个字符串或字符串数组、返回——所以不写六份 `impl Action`，
//! 而是把每个动作压成 [`text_ops`] 表里的一行：id + 元数据 + 参数表 + 纯函数。
//! 表顺序即 `corex actions` 的展示顺序，加动作只加一行。
//!
//! 全是 `Bucket::Data` + `PermissionSet::NONE`：不读文件、不联网、不起进程，
//! 字符串变换没有值得授权的副作用。文本按 Rust 的原样处理，不做 Unicode 归一化。

use crate::ActionRegistry;
use crate::builtin::util::{opt_bool, opt_i64, opt_str, require_map, require_str};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::Arc;

/// 一条参数声明：`None` 是必填，`Some(value)` 是带默认值的可选项。
type ParamSpec = (&'static str, SchemaType, Option<Value>);

/// 一个 `text.*` 动作：`execute` 只做转交，语义全在 `run` 里。
struct TextOp {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    params: Vec<ParamSpec>,
    run: fn(Value) -> Result<Value, ActionError>,
}

#[async_trait]
impl Action for TextOp {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    fn meta(&self) -> ActionMeta {
        let params = self
            .params
            .iter()
            .map(|(name, ty, default)| match default {
                Some(value) => ParamSchema::new(*name, *ty, false).with_default(value.clone()),
                None => ParamSchema::new(*name, *ty, true),
            })
            .collect();
        ActionMeta::new(self.id, self.name, self.description, Bucket::Data).with_params(params)
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        (self.run)(params)
    }
}

/// 表驱动：一行 = 一个动作，注册顺序就是这里的顺序。
fn text_ops() -> Vec<TextOp> {
    let ops = [
        TextOp {
            id: "text.case",
            name: "大小写",
            description: "按 mode 做 upper / lower / title 变换",
            params: vec![
                ("text", SchemaType::Str, None),
                ("mode", SchemaType::Str, None),
            ],
            run: run_case,
        },
        TextOp {
            id: "text.join",
            name: "拼接",
            description: "用分隔符连接字符串数组",
            params: vec![
                ("parts", SchemaType::Array, None),
                ("sep", SchemaType::Str, Some(Value::Str(String::new()))),
            ],
            run: run_join,
        },
        TextOp {
            id: "text.split",
            name: "拆分",
            description: "按分隔符拆成字符串数组",
            params: vec![
                ("text", SchemaType::Str, None),
                ("sep", SchemaType::Str, None),
                ("limit", SchemaType::Int, Some(Value::Int(0))),
            ],
            run: run_split,
        },
        TextOp {
            id: "text.replace",
            name: "替换",
            description: "字面或正则替换（正则时 to 支持 $1 反向引用）",
            params: vec![
                ("text", SchemaType::Str, None),
                ("from", SchemaType::Str, None),
                ("to", SchemaType::Str, Some(Value::Str(String::new()))),
                ("regex", SchemaType::Bool, Some(Value::Bool(false))),
            ],
            run: run_replace,
        },
        TextOp {
            id: "text.trim",
            name: "去空白",
            description: "按 mode 去两端 / 开头 / 结尾的空白",
            params: vec![
                ("text", SchemaType::Str, None),
                (
                    "mode",
                    SchemaType::Str,
                    Some(Value::Str("both".to_string())),
                ),
            ],
            run: run_trim,
        },
        TextOp {
            id: "text.match",
            name: "正则匹配",
            description: "返回是否匹配与捕获组（不匹配 groups 为空数组）",
            params: vec![
                ("text", SchemaType::Str, None),
                ("pattern", SchemaType::Str, None),
            ],
            run: run_match,
        },
    ];
    ops.into_iter().collect()
}

/// 参数类型名：报错时比只说「不是字符串」更指得清。
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Str(_) => "str",
        Value::Array(_) => "array",
        Value::Map(_) => "map",
        Value::File(_) => "file",
        Value::Bytes(_) => "bytes",
    }
}

/// Title Case：以「非字母数字」为词边界，首字符大写、其余小写。
///
/// 不用 `split_whitespace().join(" ")`：那会把连续空格、制表符与换行压成单个空格，
/// 一个只想去改大小写的动作不该顺手重排文本。`ß` 这类展开成大写的字符用 `extend` 接。
fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut is_word_start = true;
    for ch in text.chars() {
        if !ch.is_alphanumeric() {
            is_word_start = true;
            out.push(ch);
        } else if is_word_start {
            is_word_start = false;
            out.extend(ch.to_uppercase());
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

fn run_case(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "text")?;
    let mode = require_str(map, "mode")?.to_ascii_lowercase();
    let out = match mode.as_str() {
        "upper" => text.to_uppercase(),
        "lower" => text.to_lowercase(),
        "title" => title_case(&text),
        other => {
            return Err(ActionError::InvalidParams(format!(
                "mode 只支持 upper / lower / title，收到: {other}"
            )));
        }
    };
    Ok(Value::Str(out))
}

fn run_join(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let sep = opt_str(map, "sep").unwrap_or_default();
    let parts = map
        .get("parts")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ActionError::MissingParam("parts".into()))?;
    let mut pieces = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        let piece = part.as_str().ok_or_else(|| {
            ActionError::InvalidParams(format!(
                "parts[{index}] 必须是字符串，实际是 {}",
                kind(part)
            ))
        })?;
        pieces.push(piece);
    }
    Ok(Value::Str(pieces.join(&sep)))
}

fn run_split(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "text")?;
    let sep = require_str(map, "sep")?;
    if sep.is_empty() {
        // `str::split("")` 会在首尾各留一个空串；与其悄悄换个语义，不如报错让调用方写清楚。
        return Err(ActionError::InvalidParams(
            "sep 不能为空字符串（要逐字符拆请用 text.match 按字符正则取）".into(),
        ));
    }
    let limit = opt_i64(map, "limit", 0);
    if limit < 0 {
        return Err(ActionError::InvalidParams(format!(
            "limit 不能为负（0 表示不限段数），收到: {limit}"
        )));
    }
    let pieces: Vec<Value> = if limit == 0 {
        text.split(sep.as_str())
            .map(|piece| Value::Str(piece.to_string()))
            .collect()
    } else {
        text.splitn(limit as usize, sep.as_str())
            .map(|piece| Value::Str(piece.to_string()))
            .collect()
    };
    Ok(Value::Array(pieces))
}

fn run_replace(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "text")?;
    let from = require_str(map, "from")?;
    let to = opt_str(map, "to").unwrap_or_default();
    if !opt_bool(map, "regex", false) {
        return Ok(Value::Str(text.replace(&from, &to)));
    }
    let pattern = Regex::new(&from)
        .map_err(|e| ActionError::InvalidParams(format!("from 不是合法正则: {e}")))?;
    Ok(Value::Str(
        pattern.replace_all(&text, to.as_str()).into_owned(),
    ))
}

fn run_trim(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "text")?;
    let mode = opt_str(map, "mode").unwrap_or_else(|| "both".into());
    let out = match mode.to_ascii_lowercase().as_str() {
        "both" => text.trim(),
        "start" => text.trim_start(),
        "end" => text.trim_end(),
        other => {
            return Err(ActionError::InvalidParams(format!(
                "mode 只支持 both / start / end，收到: {other}"
            )));
        }
    };
    Ok(Value::Str(out.to_string()))
}

fn run_match(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "text")?;
    let pattern = require_str(map, "pattern")?;
    let regex = Regex::new(&pattern)
        .map_err(|e| ActionError::InvalidParams(format!("pattern 不是合法正则: {e}")))?;

    // 没参与匹配的可选组填 `null`：`groups[i]` 才是第 i 组（`groups[0]` 是整个匹配），
    // 否则 `(a)?(b)` 在只匹配到 b 时会把 b 挤到下标 0 上。
    let (matched, groups) = match regex.captures(&text) {
        Some(caps) => (
            Value::Bool(true),
            (0..caps.len())
                .map(|i| {
                    caps.get(i)
                        .map_or(Value::Null, |m| Value::Str(m.as_str().to_string()))
                })
                .collect(),
        ),
        None => (Value::Bool(false), Vec::new()),
    };

    let mut out = BTreeMap::new();
    out.insert("matched".into(), matched);
    out.insert("groups".into(), Value::Array(groups));
    Ok(Value::Map(out))
}

pub fn register(registry: &mut ActionRegistry) {
    for op in text_ops() {
        registry.register(Arc::new(op));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, Value)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        )
    }

    /// 走 trait 入口执行，顺带证明 `meta` 里的参数名与 `run` 读的一致。
    async fn run(id: &str, pairs: &[(&str, Value)]) -> Result<Value, ActionError> {
        let op = text_ops()
            .into_iter()
            .find(|op| op.id == id)
            .expect("动作在表里");
        let mut ctx = ExecutionContext::default();
        op.execute(params(pairs), &mut ctx).await
    }

    fn text(value: &str) -> Value {
        Value::Str(value.to_string())
    }

    #[tokio::test]
    async fn case_modes() {
        let upper = run(
            "text.case",
            &[("text", text("Hi there")), ("mode", text("UPPER"))],
        )
        .await
        .unwrap();
        assert_eq!(upper.as_str(), Some("HI THERE"));

        let lower = run(
            "text.case",
            &[("text", text("Hi")), ("mode", text("lower"))],
        )
        .await
        .unwrap();
        assert_eq!(lower.as_str(), Some("hi"));

        // title 只改大小写，不动原有空白。
        let title = run(
            "text.case",
            &[("text", text("hello  wORLD")), ("mode", text("title"))],
        )
        .await
        .unwrap();
        assert_eq!(title.as_str(), Some("Hello  World"));

        let bad = run("text.case", &[("text", text("x")), ("mode", text("wide"))]).await;
        assert!(matches!(bad, Err(ActionError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn join_defaults_to_no_separator() {
        let parts = Value::Array(vec![text("a"), text("b"), text("c")]);
        let joined = run("text.join", &[("parts", parts)]).await.unwrap();
        assert_eq!(joined.as_str(), Some("abc"));

        let with_sep = run(
            "text.join",
            &[
                ("parts", Value::Array(vec![text("a"), text("b")])),
                ("sep", text("-")),
            ],
        )
        .await
        .unwrap();
        assert_eq!(with_sep.as_str(), Some("a-b"));
    }

    #[tokio::test]
    async fn join_rejects_non_string_parts() {
        let parts = Value::Array(vec![text("a"), Value::Int(1)]);
        let err = run("text.join", &[("parts", parts)]).await.unwrap_err();
        assert!(err.to_string().contains("parts[1]"), "错误信息: {err}");
    }

    #[tokio::test]
    async fn split_unlimited_and_limited() {
        let all = run("text.split", &[("text", text("a,b,c")), ("sep", text(","))])
            .await
            .unwrap();
        assert_eq!(all.as_array().map(|p| p.len()), Some(3));

        // limit 是「最多几段」：剩下的整段收进最后一段。
        let limited = run(
            "text.split",
            &[
                ("text", text("a,b,c")),
                ("sep", text(",")),
                ("limit", Value::Int(2)),
            ],
        )
        .await
        .unwrap();
        let pieces: Vec<_> = limited
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap_or_default())
            .collect();
        assert_eq!(pieces, vec!["a", "b,c"]);
    }

    #[tokio::test]
    async fn split_rejects_empty_separator_and_negative_limit() {
        let empty = run("text.split", &[("text", text("ab")), ("sep", text(""))]).await;
        assert!(matches!(empty, Err(ActionError::InvalidParams(_))));

        let negative = run(
            "text.split",
            &[
                ("text", text("ab")),
                ("sep", text(",")),
                ("limit", Value::Int(-1)),
            ],
        )
        .await;
        assert!(matches!(negative, Err(ActionError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn replace_literal_and_regex() {
        let literal = run(
            "text.replace",
            &[
                ("text", text("a.b")),
                ("from", text(".")),
                ("to", text("-")),
            ],
        )
        .await
        .unwrap();
        assert_eq!(literal.as_str(), Some("a-b"));

        let regex = run(
            "text.replace",
            &[
                ("text", text("2026-09-21")),
                ("from", text(r"(\d{4})-(\d{2})-(\d{2})")),
                ("to", text("$3/$2/$1")),
                ("regex", Value::Bool(true)),
            ],
        )
        .await
        .unwrap();
        assert_eq!(regex.as_str(), Some("21/09/2026"));

        let bad = run(
            "text.replace",
            &[
                ("text", text("x")),
                ("from", text("(")),
                ("regex", Value::Bool(true)),
            ],
        )
        .await;
        assert!(matches!(bad, Err(ActionError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn trim_modes() {
        let input = &[("text", text("  hi  "))];
        let both = run("text.trim", input).await.unwrap();
        assert_eq!(both.as_str(), Some("hi"));

        let start = run(
            "text.trim",
            &[("text", text("  hi  ")), ("mode", text("start"))],
        )
        .await
        .unwrap();
        assert_eq!(start.as_str(), Some("hi  "));

        let end = run(
            "text.trim",
            &[("text", text("  hi  ")), ("mode", text("end"))],
        )
        .await
        .unwrap();
        assert_eq!(end.as_str(), Some("  hi"));
    }

    #[tokio::test]
    async fn match_keeps_group_positions() {
        let hit = run(
            "text.match",
            &[("text", text("b")), ("pattern", text("(a)?b"))],
        )
        .await
        .unwrap();
        let groups = hit
            .as_map()
            .and_then(|m| m.get("groups"))
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(groups[0].as_str(), Some("b"));
        assert_eq!(groups[1], Value::Null);

        let miss = run("text.match", &[("text", text("b")), ("pattern", text("z"))])
            .await
            .unwrap();
        let map = miss.as_map().unwrap();
        assert_eq!(map.get("matched"), Some(&Value::Bool(false)));
        assert_eq!(map.get("groups"), Some(&Value::Array(Vec::new())));
    }

    #[test]
    fn every_op_is_registered() {
        let mut registry = ActionRegistry::new();
        registry.register_builtins();
        for op in text_ops() {
            assert!(registry.contains(op.id), "{} 没有注册", op.id);
        }
    }

    #[test]
    fn optional_params_declare_defaults() {
        let op = text_ops()
            .into_iter()
            .find(|op| op.id == "text.trim")
            .unwrap();
        let params = op.meta().params;
        let mode = params.iter().find(|p| p.name == "mode").unwrap();
        assert!(!mode.required);
        assert_eq!(mode.default.as_ref().and_then(|v| v.as_str()), Some("both"));
    }
}
