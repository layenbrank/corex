//! `time.*`：时间解析、格式化、加减与时区换算（`act-time`）。
//!
//! 与 `math.rs` 同构的表驱动：一行 = 一个动作（id + 元数据 + 参数表 + 纯函数）。
//! 取「现在」仍是 `generate.timestamp` 的事，这里只处理**给定**的时刻。
//!
//! 全是 `Bucket::Data` + `PermissionSet::NONE`：不读文件、不联网、不起进程。

use crate::ActionRegistry;
use crate::builtin::util::{opt_bool, opt_i64, opt_str, require_map, require_str};
use async_trait::async_trait;
use chrono::{
    DateTime, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// `time.format` 不写 format 时的形状：一眼能读的本地时间。
const DEFAULT_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// 加减法的时间单位 → 秒：加一个单位只加一行。
const SHIFT_UNITS: &[(&str, i64)] = &[
    ("days", 86_400),
    ("hours", 3_600),
    ("minutes", 60),
    ("seconds", 1),
];

/// 一条参数声明：`None` 是必填，`Some(value)` 是带默认值的可选项。
type ParamSpec = (&'static str, SchemaType, Option<Value>);

/// 要处理的时刻：RFC3339、unix 秒、`YYYY-MM-DD`，或配合 `format` 的任意写法。
const AT_PARAM: ParamSpec = ("at", SchemaType::Str, None);

/// 输入不带时区时按哪里归属：`false` 是本机时区。
const ZONE_PARAM: ParamSpec = ("utc", SchemaType::Bool, Some(Value::Bool(false)));

/// 加减法共用的四个单位，默认都是 0。
const SHIFT_PARAMS: &[ParamSpec] = &[
    ("days", SchemaType::Int, Some(Value::Int(0))),
    ("hours", SchemaType::Int, Some(Value::Int(0))),
    ("minutes", SchemaType::Int, Some(Value::Int(0))),
    ("seconds", SchemaType::Int, Some(Value::Int(0))),
];

/// 一个 `time.*` 动作：`execute` 只做转交，语义全在 `run` 里。
struct TimeOp {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    params: Vec<ParamSpec>,
    run: fn(Value) -> Result<Value, ActionError>,
}

#[async_trait]
impl Action for TimeOp {
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
fn time_ops() -> Vec<TimeOp> {
    let ops = [
        TimeOp {
            id: "time.parse",
            name: "解析时间",
            description: "解析成 iso8601 与 unix 秒（RFC3339 / unix 秒 / 纯日期，或给 format）",
            params: vec![
                ("text", SchemaType::Str, None),
                // 空串 = 自动识别，与 run_parse 里 `filter(|f| !f.is_empty())` 一致。
                ("format", SchemaType::Str, Some(Value::Str(String::new()))),
                ZONE_PARAM,
            ],
            run: run_parse,
        },
        TimeOp {
            id: "time.format",
            name: "格式化时间",
            description: "按 format 输出字符串（utc 为真按 UTC，否则按本机时区）",
            params: vec![
                AT_PARAM,
                (
                    "format",
                    SchemaType::Str,
                    Some(Value::Str(DEFAULT_FORMAT.to_string())),
                ),
                ZONE_PARAM,
            ],
            run: run_format,
        },
        TimeOp {
            id: "time.add",
            name: "时间相加",
            description: "时刻加 days / hours / minutes / seconds",
            params: shift_params(),
            run: |params| shift(params, 1),
        },
        TimeOp {
            id: "time.sub",
            name: "时间相减",
            description: "时刻减 days / hours / minutes / seconds",
            params: shift_params(),
            run: |params| shift(params, -1),
        },
        TimeOp {
            id: "time.tz",
            name: "时区换算",
            description: "换算到 offset 时区（+08:00 / -05:30 / utc）",
            params: vec![AT_PARAM, ("offset", SchemaType::Str, None)],
            run: run_tz,
        },
    ];
    ops.into_iter().collect()
}

/// `time.add` / `time.sub` 的参数：要处理的时刻加四个可正可负的单位。
fn shift_params() -> Vec<ParamSpec> {
    std::iter::once(AT_PARAM)
        .chain(SHIFT_PARAMS.iter().cloned())
        .collect()
}

fn run_parse(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "text")?;
    let format = opt_str(map, "format");
    let at = parse_at(&text, format.as_deref(), opt_bool(map, "utc", false))?;
    Ok(stamp(at))
}

fn run_format(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "at")?;
    let format = opt_str(map, "format")
        .filter(|format| !format.is_empty())
        .unwrap_or_else(|| DEFAULT_FORMAT.to_string());
    let utc = opt_bool(map, "utc", false);
    let at = parse_at(&text, None, utc)?;
    let rendered = if utc {
        at.with_timezone(&Utc).format(&format)
    } else {
        at.with_timezone(&Local).format(&format)
    };
    Ok(Value::Str(rendered.to_string()))
}

/// `time.add` 与 `time.sub` 只差符号，共用一条路径。
fn shift(params: Value, sign: i64) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "at")?;
    let at = parse_at(&text, None, opt_bool(map, "utc", false))?;
    let mut seconds: i64 = 0;
    for (unit, per_unit) in SHIFT_UNITS {
        let amount = opt_i64(map, unit, 0);
        let scaled = amount
            .checked_mul(*per_unit)
            .and_then(|value| value.checked_mul(sign))
            .ok_or_else(|| ActionError::InvalidParams(format!("{unit} ({amount}) 过大")))?;
        seconds = seconds
            .checked_add(scaled)
            .ok_or_else(|| ActionError::InvalidParams("偏移过大".into()))?;
    }
    let delta = Duration::try_seconds(seconds)
        .ok_or_else(|| ActionError::InvalidParams("偏移超出时间范围".into()))?;
    let moved = at
        .checked_add_signed(delta)
        .ok_or_else(|| ActionError::InvalidParams("结果超出时间范围".into()))?;
    Ok(stamp(moved))
}

fn run_tz(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "at")?;
    let offset = parse_offset(&require_str(map, "offset")?)?;
    let at = parse_at(&text, None, opt_bool(map, "utc", false))?;
    Ok(stamp(at.with_timezone(&offset)))
}

/// 解析一个时刻：给了 `format` 就只按它解析，否则依次试 RFC3339、unix 秒、纯日期。
///
/// `utc` 只决定**输入没带时区**时按哪个时区归属；带 `+08:00` 这类偏移的输入自带答案。
fn parse_at(
    text: &str,
    format: Option<&str>,
    utc: bool,
) -> Result<DateTime<FixedOffset>, ActionError> {
    let text = text.trim();
    if let Some(format) = format.filter(|format| !format.is_empty()) {
        let naive = parse_with_format(text, format)?;
        return naive_in_zone(naive, utc);
    }
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Ok(at);
    }
    if let Ok(seconds) = text.parse::<i64>() {
        return Utc
            .timestamp_opt(seconds, 0)
            .single()
            .map(|at| at.fixed_offset())
            .ok_or_else(|| ActionError::InvalidParams(format!("unix 秒超出范围: {text}")));
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return naive_in_zone(date.and_time(NaiveTime::MIN), utc);
    }
    Err(ActionError::InvalidParams(format!(
        "无法解析时间: {text}（用 RFC3339、unix 秒、YYYY-MM-DD，或给 format）"
    )))
}

fn parse_with_format(text: &str, format: &str) -> Result<NaiveDateTime, ActionError> {
    NaiveDateTime::parse_from_str(text, format)
        .or_else(|_| {
            NaiveDate::parse_from_str(text, format).map(|date| date.and_time(NaiveTime::MIN))
        })
        .map_err(|err| ActionError::InvalidParams(format!("text 与 format ({format}) 不符: {err}")))
}

fn naive_in_zone(naive: NaiveDateTime, utc: bool) -> Result<DateTime<FixedOffset>, ActionError> {
    if utc {
        return Ok(Utc.from_utc_datetime(&naive).fixed_offset());
    }
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|at| at.fixed_offset())
        .ok_or_else(|| {
            ActionError::InvalidParams(format!("本地时区下 {naive} 不存在或有歧义（夏令时切换）"))
        })
}

/// `+08:00` / `-0530` / `+08`，也认 `utc` 与 `z`。
fn parse_offset(text: &str) -> Result<FixedOffset, ActionError> {
    let text = text.trim();
    let (sign, offset) = if text.eq_ignore_ascii_case("utc") || text.eq_ignore_ascii_case("z") {
        (1, "0")
    } else {
        match text.as_bytes().first() {
            Some(b'+') => (1, &text[1..]),
            Some(b'-') => (-1, &text[1..]),
            _ => return Err(invalid_offset(text)),
        }
    };
    let digits = offset.replace(':', "");
    if offset.matches(':').count() > 1
        || digits.is_empty()
        || digits.len() > 4
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_offset(text));
    }
    let (hours, minutes) = match digits.len() {
        1 | 2 => (&digits[..], "0"),
        3 => (&digits[..1], &digits[1..]),
        _ => (&digits[..2], &digits[2..]),
    };
    let (hours, minutes) = (
        hours.parse::<i32>().map_err(|_| invalid_offset(text))?,
        minutes.parse::<i32>().map_err(|_| invalid_offset(text))?,
    );
    if minutes > 59 {
        return Err(invalid_offset(text));
    }
    FixedOffset::east_opt(sign * (hours * 3_600 + minutes * 60))
        .ok_or_else(|| ActionError::InvalidParams(format!("offset 超出 ±24 小时: {text}")))
}

fn invalid_offset(text: &str) -> ActionError {
    ActionError::InvalidParams(format!(
        "offset 形如 +08:00 / -0530 / +08 / utc，收到: {text}"
    ))
}

/// 时刻的对外形状：`iso8601` 带偏移，`unix` 是秒。
fn stamp(at: DateTime<FixedOffset>) -> Value {
    Value::Map(BTreeMap::from_iter([
        ("iso8601".to_string(), Value::Str(at.to_rfc3339())),
        ("unix".to_string(), Value::Int(at.timestamp())),
    ]))
}

pub fn register(registry: &mut ActionRegistry) {
    for op in time_ops() {
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
        let op = time_ops()
            .into_iter()
            .find(|op| op.id == id)
            .expect("动作在表里");
        let mut ctx = ExecutionContext::default();
        op.execute(params(pairs), &mut ctx).await
    }

    fn text(value: &str) -> Value {
        Value::Str(value.to_string())
    }

    fn field(value: &Value, key: &str) -> Value {
        value
            .as_map()
            .and_then(|map| map.get(key))
            .cloned()
            .unwrap_or_else(|| panic!("缺少 {key}"))
    }

    #[tokio::test]
    async fn parse_reads_the_common_shapes() {
        let rfc = run("time.parse", &[("text", text("2024-03-04T05:06:07Z"))])
            .await
            .unwrap();
        assert_eq!(field(&rfc, "unix").as_i64(), Some(1_709_528_767));
        assert_eq!(
            field(&rfc, "iso8601").as_str(),
            Some("2024-03-04T05:06:07+00:00")
        );

        let unix = run("time.parse", &[("text", text("1709528767"))])
            .await
            .unwrap();
        assert_eq!(field(&unix, "unix").as_i64(), Some(1_709_528_767));

        let date = run(
            "time.parse",
            &[("text", text("2024-03-04")), ("utc", Value::Bool(true))],
        )
        .await
        .unwrap();
        assert_eq!(
            field(&date, "iso8601").as_str(),
            Some("2024-03-04T00:00:00+00:00")
        );

        let formatted = run(
            "time.parse",
            &[
                ("text", text("04/03/2024 05:06")),
                ("format", text("%d/%m/%Y %H:%M")),
                ("utc", Value::Bool(true)),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            field(&formatted, "iso8601").as_str(),
            Some("2024-03-04T05:06:00+00:00")
        );
    }

    #[tokio::test]
    async fn parse_rejects_garbage() {
        let bad = run("time.parse", &[("text", text("tomorrow"))]).await;
        assert!(matches!(bad, Err(ActionError::InvalidParams(_))));

        let mismatched = run(
            "time.parse",
            &[("text", text("2024-03-04")), ("format", text("%d/%m/%Y"))],
        )
        .await;
        assert!(matches!(mismatched, Err(ActionError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn format_uses_the_requested_zone() {
        let utc = run(
            "time.format",
            &[
                ("at", text("2024-03-04T05:06:07Z")),
                ("format", text("%Y-%m-%d %H:%M")),
                ("utc", Value::Bool(true)),
            ],
        )
        .await
        .unwrap();
        assert_eq!(utc.as_str(), Some("2024-03-04 05:06"));

        // 带偏移的输入自带答案，utc 只影响格式化使用的时区。
        let local = run("time.format", &[("at", text("2024-03-04T05:06:07+08:00"))])
            .await
            .unwrap();
        let local = local.as_str().expect("返回字符串").to_string();
        // 任何时区下这个瞬间都落在 3 月 3 日或 4 日，且默认格式固定 19 个字符。
        assert_eq!(local.len(), 19, "{local}");
        assert!(local.starts_with("2024-03-0"), "{local}");
    }

    #[tokio::test]
    async fn add_and_sub_move_the_clock() {
        let added = run(
            "time.add",
            &[
                ("at", text("2024-03-04T05:06:07Z")),
                ("days", Value::Int(1)),
                ("hours", Value::Int(2)),
                ("minutes", Value::Int(3)),
                ("seconds", Value::Int(-7)),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            field(&added, "iso8601").as_str(),
            Some("2024-03-05T07:09:00+00:00")
        );

        let subbed = run(
            "time.sub",
            &[
                ("at", text("2024-03-04T05:06:07Z")),
                ("days", Value::Int(1)),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            field(&subbed, "iso8601").as_str(),
            Some("2024-03-03T05:06:07+00:00")
        );

        // 没有任何单位时是恒等变换。
        let same = run("time.add", &[("at", text("2024-03-04T05:06:07Z"))])
            .await
            .unwrap();
        assert_eq!(field(&same, "unix").as_i64(), Some(1_709_528_767));
    }

    #[tokio::test]
    async fn tz_rewrites_the_offset() {
        let shifted = run(
            "time.tz",
            &[
                ("at", text("2024-03-04T05:06:07Z")),
                ("offset", text("+08:00")),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            field(&shifted, "iso8601").as_str(),
            Some("2024-03-04T13:06:07+08:00")
        );
        // 同一瞬间：换算不改变 unix 秒。
        assert_eq!(field(&shifted, "unix").as_i64(), Some(1_709_528_767));

        let compact = run(
            "time.tz",
            &[
                ("at", text("2024-03-04T05:06:07Z")),
                ("offset", text("-0530")),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            field(&compact, "iso8601").as_str(),
            Some("2024-03-03T23:36:07-05:30")
        );

        let utc = run(
            "time.tz",
            &[
                ("at", text("2024-03-04T05:06:07+08:00")),
                ("offset", text("utc")),
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            field(&utc, "iso8601").as_str(),
            Some("2024-03-03T21:06:07+00:00")
        );
    }

    #[tokio::test]
    async fn tz_rejects_bad_offsets() {
        for offset in ["08:00", "+08:70", "+080", "+1:2:3", ""] {
            let bad = run(
                "time.tz",
                &[
                    ("at", text("2024-03-04T05:06:07Z")),
                    ("offset", text(offset)),
                ],
            )
            .await;
            assert!(
                matches!(bad, Err(ActionError::InvalidParams(_))),
                "{offset}"
            );
        }
    }
}
