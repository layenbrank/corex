//! `math.*`：数值计算与统计（`act-math`）。
//!
//! 四个动作同形——取参数、算出一个数或一组统计、返回——所以沿用 `text.rs` 的表驱动：
//! 一行 = 一个动作（id + 元数据 + 参数表 + 纯函数）。加动作只加一行。
//!
//! 全是 `Bucket::Data` + `PermissionSet::NONE`：不读文件、不联网、不起进程。
//! `math.eval` 复用 MiniJinja 的表达式引擎（与 `template.render` 同一个），
//! 于是指令里只有一套表达式语法——模板里能写的，这里也能写。

use crate::ActionRegistry;
use crate::builtin::util::{opt_bool, opt_f64, opt_str, require_map, require_str};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use rand::RngExt;
use std::collections::BTreeMap;
use std::sync::Arc;

/// f64 能精确表示整数的上界（2^53）：越过它 `as i64` 就不再可逆。
const INT_EXACT_LIMIT: f64 = 9_007_199_254_740_992.0;

/// 一条参数声明：`None` 是必填，`Some(value)` 是带默认值的可选项。
type ParamSpec = (&'static str, SchemaType, Option<Value>);

/// 一个 `math.*` 动作：`execute` 只做转交，语义全在 `run` 里。
struct MathOp {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    params: Vec<ParamSpec>,
    run: fn(Value) -> Result<Value, ActionError>,
}

#[async_trait]
impl Action for MathOp {
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
fn math_ops() -> Vec<MathOp> {
    let ops = [
        MathOp {
            id: "math.eval",
            name: "表达式求值",
            description: "求值一个表达式（MiniJinja 语法，结果须是标量）",
            params: vec![("expr", SchemaType::Str, None)],
            run: run_eval,
        },
        MathOp {
            id: "math.rand",
            name: "随机数",
            description: "在 [min, max] 内取随机数（int 为真取整数，否则浮点）",
            params: vec![
                ("min", SchemaType::Float, Some(Value::Float(0.0))),
                ("max", SchemaType::Float, Some(Value::Float(100.0))),
                ("int", SchemaType::Bool, Some(Value::Bool(true))),
            ],
            run: run_rand,
        },
        MathOp {
            id: "math.round",
            name: "取整",
            description: "按 mode 取整（round / floor / ceil / trunc）",
            params: vec![
                ("n", SchemaType::Float, None),
                (
                    "mode",
                    SchemaType::Str,
                    Some(Value::Str("round".to_string())),
                ),
            ],
            run: run_round,
        },
        MathOp {
            id: "math.stats",
            name: "统计",
            description: "统计数值数组：sum / min / max / mean / count",
            params: vec![("values", SchemaType::Array, None)],
            run: run_stats,
        },
    ];
    ops.into_iter().collect()
}

/// `mode` → 取整函数：加一种模式只加一行。
const ROUND_MODES: &[(&str, Rounder)] = &[
    ("round", f64::round),
    ("floor", f64::floor),
    ("ceil", f64::ceil),
    ("trunc", f64::trunc),
];

/// 取整函数。
type Rounder = fn(f64) -> f64;

fn rounder(mode: &str) -> Result<Rounder, ActionError> {
    ROUND_MODES
        .iter()
        .find(|(name, _)| *name == mode)
        .map(|(_, rounder)| *rounder)
        .ok_or_else(|| {
            ActionError::InvalidParams(format!(
                "mode 只支持 round / floor / ceil / trunc，收到: {mode}"
            ))
        })
}

fn run_eval(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let expr = require_str(map, "expr")?;
    let mut env = minijinja::Environment::new();
    // Strict：写错的变量名要报错，而不是悄悄变成 undefined 再渲染成空串。
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    let compiled = env
        .compile_expression(&expr)
        .map_err(|err| ActionError::InvalidParams(format!("表达式语法错误: {err}")))?;
    let value = compiled
        .eval(())
        .map_err(|err| ActionError::InvalidParams(format!("表达式求值失败: {err}")))?;
    scalar_of(value)
}

/// 表达式结果 → corex 值：只接标量。
///
/// 序列与映射不转：`math.eval` 的用处是算一个数，返回结构让别的动作做，
/// 免得同一个值在两条路径上有两种形状。
fn scalar_of(value: minijinja::Value) -> Result<Value, ActionError> {
    use minijinja::value::ValueKind;
    match value.kind() {
        ValueKind::None => Ok(Value::Null),
        ValueKind::Bool => bool::try_from(value)
            .map(Value::Bool)
            .map_err(scalar_failure),
        ValueKind::Number => {
            // 整数优先：`6 / 3` 是 int，`1 / 2` 是 float，不要把前者渲染成 `2.0`。
            match i64::try_from(value.clone()) {
                Ok(int) => Ok(Value::Int(int)),
                Err(_) => f64::try_from(value)
                    .map(Value::Float)
                    .map_err(scalar_failure),
            }
        }
        // `ValueKind::String` 下 `Display` 就是字符串本身：不转义、不加引号。
        ValueKind::String => Ok(Value::Str(value.to_string())),
        other => Err(ActionError::InvalidParams(format!(
            "表达式结果是 {other}，math.eval 只接标量"
        ))),
    }
}

fn scalar_failure(err: minijinja::Error) -> ActionError {
    ActionError::InvalidParams(format!("表达式结果无法取值: {err}"))
}

fn run_rand(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let min = opt_f64(map, "min", 0.0);
    let max = opt_f64(map, "max", 100.0);
    if !min.is_finite() || !max.is_finite() {
        return Err(ActionError::InvalidParams("min / max 需要有限数值".into()));
    }
    if min > max {
        return Err(ActionError::InvalidParams(format!(
            "min ({min}) 不能大于 max ({max})"
        )));
    }
    let mut rng = rand::rng();
    if !opt_bool(map, "int", true) {
        return Ok(Value::Float(rng.random_range(min..=max)));
    }
    // 整数落在闭区间 [min, max] 内的整数上：`ceil(min)..=floor(max)`，
    // 所以 [0.5, 0.9] 没有整数可取而 [0.5, 1.5] 只有 1。
    let (lo, hi) = (min.ceil(), max.floor());
    if lo > hi {
        return Err(ActionError::InvalidParams(format!(
            "[{min}, {max}] 内没有整数"
        )));
    }
    if lo < i64::MIN as f64 || hi > i64::MAX as f64 {
        return Err(ActionError::InvalidParams("区间超出 i64 范围".into()));
    }
    Ok(Value::Int(rng.random_range(lo as i64..=hi as i64)))
}

fn run_round(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let n = map
        .get("n")
        .and_then(Value::as_f64)
        .ok_or_else(|| ActionError::MissingParam("n".into()))?;
    if !n.is_finite() {
        return Err(ActionError::InvalidParams("n 需要有限数值".into()));
    }
    let mode = opt_str(map, "mode").unwrap_or_else(|| "round".to_string());
    let rounded = rounder(&mode)?(n);
    if rounded < i64::MIN as f64 || rounded > i64::MAX as f64 {
        return Err(ActionError::InvalidParams(format!("n ({n}) 超出 i64 范围")));
    }
    Ok(Value::Int(rounded as i64))
}

fn run_stats(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let values = map
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| ActionError::MissingParam("values".into()))?;
    if values.is_empty() {
        return Err(ActionError::InvalidParams("values 至少要有一个数值".into()));
    }
    let mut numbers = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        // 不静默丢非数值项：丢一项就少一列 count，算出来的均值没人知道错在哪。
        let number = value
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or_else(|| ActionError::InvalidParams(format!("values[{index}] 不是有限数值")))?;
        numbers.push(number);
    }
    let sum: f64 = numbers.iter().sum();
    let count = numbers.len() as f64;
    let min = numbers.iter().copied().fold(f64::INFINITY, f64::min);
    let max = numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok(Value::Map(BTreeMap::from_iter([
        ("sum".to_string(), literal(sum)),
        ("min".to_string(), literal(min)),
        ("max".to_string(), literal(max)),
        ("mean".to_string(), literal(sum / count)),
        ("count".to_string(), Value::Int(count as i64)),
    ])))
}

/// 整数值给 `Int`：`[1, 2, 3]` 的 `sum` 渲染成 `6` 而不是 `6.0`。
fn literal(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() <= INT_EXACT_LIMIT {
        Value::Int(value as i64)
    } else {
        Value::Float(value)
    }
}

pub fn register(registry: &mut ActionRegistry) {
    for op in math_ops() {
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
        let op = math_ops()
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
    async fn eval_returns_scalars() {
        let sum = run("math.eval", &[("expr", text("1 + 2 * 3"))])
            .await
            .unwrap();
        assert_eq!(sum.as_i64(), Some(7));

        let ratio = run("math.eval", &[("expr", text("1 / 2"))]).await.unwrap();
        assert_eq!(ratio.as_f64(), Some(0.5));

        let joined = run("math.eval", &[("expr", text("'a' ~ 'b'"))])
            .await
            .unwrap();
        assert_eq!(joined.as_str(), Some("ab"));

        let flag = run("math.eval", &[("expr", text("2 > 1"))]).await.unwrap();
        assert_eq!(flag.as_bool(), Some(true));
    }

    #[tokio::test]
    async fn eval_rejects_bad_expressions() {
        let syntax = run("math.eval", &[("expr", text("1 +"))]).await;
        assert!(matches!(syntax, Err(ActionError::InvalidParams(_))));

        // Strict 下未定义变量报错，而不是算成空值。
        let undefined = run("math.eval", &[("expr", text("nope + 1"))]).await;
        assert!(matches!(undefined, Err(ActionError::InvalidParams(_))));

        let sequence = run("math.eval", &[("expr", text("[1, 2]"))]).await;
        assert!(matches!(sequence, Err(ActionError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn rand_stays_inside_the_range() {
        for _ in 0..64 {
            let value = run(
                "math.rand",
                &[("min", Value::Float(3.0)), ("max", Value::Float(7.0))],
            )
            .await
            .unwrap();
            let int = value.as_i64().expect("默认 int 模式返回整数");
            assert!((3..=7).contains(&int), "{int} 越界");
        }

        let float = run(
            "math.rand",
            &[
                ("min", Value::Float(-0.5)),
                ("max", Value::Float(0.5)),
                ("int", Value::Bool(false)),
            ],
        )
        .await
        .unwrap();
        let float = float.as_f64().expect("浮点模式返回浮点");
        assert!((-0.5..=0.5).contains(&float), "{float} 越界");
    }

    #[tokio::test]
    async fn rand_rejects_empty_and_reversed_ranges() {
        let reversed = run(
            "math.rand",
            &[("min", Value::Float(5.0)), ("max", Value::Float(1.0))],
        )
        .await;
        assert!(matches!(reversed, Err(ActionError::InvalidParams(_))));

        // [0.5, 0.9] 里没有整数。
        let empty = run(
            "math.rand",
            &[("min", Value::Float(0.5)), ("max", Value::Float(0.9))],
        )
        .await;
        assert!(matches!(empty, Err(ActionError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn round_modes() {
        let cases = [
            ("round", -1.5, -2),
            ("floor", -1.5, -2),
            ("ceil", -1.5, -1),
            ("trunc", -1.5, -1),
        ];
        for (mode, n, expected) in cases {
            let value = run(
                "math.round",
                &[("n", Value::Float(n)), ("mode", text(mode))],
            )
            .await
            .unwrap();
            assert_eq!(value.as_i64(), Some(expected), "{mode}({n})");
        }

        let default = run("math.round", &[("n", Value::Float(2.6))])
            .await
            .unwrap();
        assert_eq!(default.as_i64(), Some(3));

        let bad = run(
            "math.round",
            &[("n", Value::Float(1.0)), ("mode", text("nearest"))],
        )
        .await;
        assert!(matches!(bad, Err(ActionError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn stats_summarises_numbers() {
        let value = run(
            "math.stats",
            &[(
                "values",
                Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            )],
        )
        .await
        .unwrap();
        let map = value.as_map().expect("返回 map");
        assert_eq!(map.get("sum").and_then(Value::as_i64), Some(6));
        assert_eq!(map.get("min").and_then(Value::as_i64), Some(1));
        assert_eq!(map.get("max").and_then(Value::as_i64), Some(3));
        assert_eq!(map.get("mean").and_then(Value::as_i64), Some(2));
        assert_eq!(map.get("count").and_then(Value::as_i64), Some(3));

        let mean = run(
            "math.stats",
            &[(
                "values",
                Value::Array(vec![Value::Float(0.5), Value::Float(1.0)]),
            )],
        )
        .await
        .unwrap();
        let mean = mean.as_map().unwrap().get("mean").cloned().unwrap();
        assert_eq!(mean.as_f64(), Some(0.75));
    }

    #[tokio::test]
    async fn stats_rejects_non_numbers_and_empty() {
        let mixed = run(
            "math.stats",
            &[("values", Value::Array(vec![Value::Int(1), text("2")]))],
        )
        .await;
        assert!(matches!(mixed, Err(ActionError::InvalidParams(_))));

        let empty = run("math.stats", &[("values", Value::Array(Vec::new()))]).await;
        assert!(matches!(empty, Err(ActionError::InvalidParams(_))));
    }
}
