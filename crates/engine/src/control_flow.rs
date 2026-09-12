//! 条件求值辅助函数。

use crate::definition::Condition;
use crate::resolver::Resolver;
use corex_core::{EngineError, ExecutionContext, Value};

/// 在当前上下文里对 [`Condition`] 求值。
pub fn evaluate_condition(
    condition: &Condition,
    ctx: &ExecutionContext,
) -> Result<bool, EngineError> {
    match condition {
        Condition::Expr(expr) => {
            let v = Resolver::resolve_string(expr, ctx)?;
            Ok(v.is_truthy())
        }
        Condition::Eq { eq } => {
            let a = Resolver::resolve_value(&eq[0], ctx)?;
            let b = Resolver::resolve_value(&eq[1], ctx)?;
            Ok(values_equal(&a, &b))
        }
        Condition::Ne { ne } => {
            let a = Resolver::resolve_value(&ne[0], ctx)?;
            let b = Resolver::resolve_value(&ne[1], ctx)?;
            Ok(!values_equal(&a, &b))
        }
        Condition::Gt { gt } => {
            let a = Resolver::resolve_value(&gt[0], ctx)?;
            let b = Resolver::resolve_value(&gt[1], ctx)?;
            cmp_num(&a, &b)
                .map(|o| o.is_gt())
                .ok_or_else(|| EngineError::ConditionError("gt 需要数值比较".into()))
        }
        Condition::Lt { lt } => {
            let a = Resolver::resolve_value(&lt[0], ctx)?;
            let b = Resolver::resolve_value(&lt[1], ctx)?;
            cmp_num(&a, &b)
                .map(|o| o.is_lt())
                .ok_or_else(|| EngineError::ConditionError("lt 需要数值比较".into()))
        }
        Condition::Contains { contains } => {
            let haystack = Resolver::resolve_value(&contains[0], ctx)?;
            let needle = Resolver::resolve_value(&contains[1], ctx)?;
            Ok(contains_value(&haystack, &needle))
        }
        Condition::And { and } => {
            for c in and {
                if !evaluate_condition(c, ctx)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Condition::Or { or } => {
            for c in or {
                if evaluate_condition(c, ctx)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Condition::Not { not } => Ok(!evaluate_condition(not, ctx)?),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Float(y)) => (*x as f64) == *y,
        (Value::Float(x), Value::Int(y)) => *x == (*y as f64),
        _ => a == b,
    }
}

/// `contains` 的判定：数组含元素、字符串含子串、map 含键。
///
/// 数组里是对象时，`needle` 按**子集**匹配：某一项含有 `needle` 的全部键值就算命中。
/// 服务端的对象常带一堆我们没写的字段（`{index, hash, size, updatedAt}`），
/// 要求逐字段写全才相等是不现实的。
fn contains_value(haystack: &Value, needle: &Value) -> bool {
    match haystack {
        Value::Array(items) => items.iter().any(|item| matches_needle(item, needle)),
        Value::Str(s) => scalar_text(needle).is_some_and(|n| s.contains(&n)),
        Value::Map(map) => scalar_text(needle).is_some_and(|k| map.contains_key(&k)),
        _ => false,
    }
}

/// 一个元素算不算命中 `needle`。
///
/// 空对象不作子集：`contains: [arr, {}]` 不该命中任何对象（空集是任何集合的子集，
/// 但那不是调用方想要的意思），仍按完全相等处理。
fn matches_needle(item: &Value, needle: &Value) -> bool {
    match (item, needle) {
        (Value::Map(item), Value::Map(needle)) if !needle.is_empty() => needle
            .iter()
            .all(|(key, want)| item.get(key).is_some_and(|got| loose_equal(got, want))),
        _ => loose_equal(item, needle),
    }
}

/// 宽松相等：结构化值仍用 `values_equal`，标量则再比一次文本形式。
///
/// 后者是「同一个序号」的两种写法：`index` 是 `Int(3)`，而服务端回的分片表
/// 很可能是 `["3", "4"]`。
fn loose_equal(a: &Value, b: &Value) -> bool {
    values_equal(a, b) || matches!((scalar_text(a), scalar_text(b)), (Some(x), Some(y)) if x == y)
}

/// 标量的文本形式；数组 / map / bytes 没有「一个」文本，返回 `None`。
///
/// `Null` 也在 `None` 里：把空值当字面量 `"null"` 只会让 `contains` 变出
/// 「字符串里有 null 字样」这种没人想耍的命中。
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::Str(s) => Some(s.clone()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::File(p) => Some(p.to_string_lossy().into_owned()),
        _ => None,
    }
}

fn cmp_num(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    let af = a.as_f64()?;
    let bf = b.as_f64()?;
    af.partial_cmp(&bf)
}
