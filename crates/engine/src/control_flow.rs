//! Condition evaluation helpers.

use crate::definition::Condition;
use crate::resolver::Resolver;
use corex_core::{EngineError, ExecutionContext, Value};

/// Evaluate a [`Condition`] against the current context.
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

fn cmp_num(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    let af = a.as_f64()?;
    let bf = b.as_f64()?;
    af.partial_cmp(&bf)
}
