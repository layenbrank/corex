//! `{{...}}` variable resolver.

use corex_core::{EngineError, ExecutionContext, Value};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::json;

static VAR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\{\{\s*([^}]+?)\s*\}\}").expect("valid regex")
});

/// Resolves `{{input.x}}`, `{{directive_input}}`, `{{step.id.path}}`,
/// `{{env.NAME}}`, and `{{variables.name}}` / bare `{{name}}`.
pub struct Resolver;

impl Resolver {
    /// Resolve all placeholders inside a [`Value`] tree.
    pub fn resolve_value(value: &Value, ctx: &ExecutionContext) -> Result<Value, EngineError> {
        match value {
            Value::Str(s) => Self::resolve_string(s, ctx),
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(Self::resolve_value(item, ctx)?);
                }
                Ok(Value::List(out))
            }
            Value::Map(map) => {
                let mut out = std::collections::BTreeMap::new();
                for (k, v) in map {
                    out.insert(k.clone(), Self::resolve_value(v, ctx)?);
                }
                Ok(Value::Map(out))
            }
            other => Ok(other.clone()),
        }
    }

    /// If the entire string is a single `{{expr}}`, return the raw Value
    /// (preserving type). Otherwise interpolate to a string.
    pub fn resolve_string(input: &str, ctx: &ExecutionContext) -> Result<Value, EngineError> {
        let trimmed = input.trim();
        if let Some(caps) = VAR_RE.captures(trimmed) {
            let whole = caps.get(0).map(|m| m.as_str()).unwrap_or("");
            if whole == trimmed {
                let expr = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                return Self::lookup(expr, ctx);
            }
        }

        if !VAR_RE.is_match(input) {
            return Ok(Value::Str(input.to_string()));
        }

        let mut err: Option<EngineError> = None;
        let replaced = VAR_RE.replace_all(input, |caps: &regex::Captures| {
            let expr = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            match Self::lookup(expr, ctx) {
                Ok(v) => v.to_string(),
                Err(e) => {
                    err = Some(e);
                    String::new()
                }
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
        Ok(Value::Str(replaced.into_owned()))
    }

    /// Lookup a dotted expression against the execution context.
    pub fn lookup(expr: &str, ctx: &ExecutionContext) -> Result<Value, EngineError> {
        let expr = expr.trim();
        if expr.is_empty() {
            return Err(EngineError::ResolveError("空的变量表达式".into()));
        }

        let (head, rest) = split_first(expr);

        match head {
            "input" => {
                if rest.is_empty() {
                    let map: std::collections::BTreeMap<_, _> = ctx
                        .input
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    return Ok(Value::Map(map));
                }
                let (name, path) = split_first(rest);
                let val = ctx
                    .input
                    .get(name)
                    .cloned()
                    .ok_or_else(|| EngineError::UndefinedVariable(format!("input.{name}")))?;
                get_required_path(val, path, expr)
            }
            "directive_input" => {
                let val = ctx.directive_input.clone().unwrap_or(Value::Null);
                get_required_path(val, rest, expr)
            }
            "step" | "steps" => {
                if rest.is_empty() {
                    return Err(EngineError::ResolveError(
                        "step 引用需要 id，例如 step.foo.path".into(),
                    ));
                }
                let (id, path) = split_first(rest);
                let val = ctx
                    .step_outputs
                    .get(id)
                    .cloned()
                    .ok_or_else(|| EngineError::UndefinedVariable(format!("step.{id}")))?;
                get_required_path(val, path, expr)
            }
            "env" => {
                if rest.is_empty() {
                    let map: std::collections::BTreeMap<_, _> = ctx
                        .env
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::Str(v.clone())))
                        .collect();
                    return Ok(Value::Map(map));
                }
                let (name, path) = split_first(rest);
                let raw = ctx
                    .env
                    .get(name)
                    .cloned()
                    .ok_or_else(|| EngineError::UndefinedVariable(format!("env.{name}")))?;
                let val = Value::Str(raw);
                get_required_path(val, path, expr)
            }
            "variables" | "var" => {
                if rest.is_empty() {
                    let map: std::collections::BTreeMap<_, _> = ctx
                        .variables
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    return Ok(Value::Map(map));
                }
                let (name, path) = split_first(rest);
                let val = ctx
                    .variables
                    .get(name)
                    .cloned()
                    .ok_or_else(|| EngineError::UndefinedVariable(format!("variables.{name}")))?;
                get_required_path(val, path, expr)
            }
            // Bare name → variables, then input.
            other => {
                if let Some(v) = ctx.variables.get(other) {
                    return get_required_path(v.clone(), rest, expr);
                }
                if rest.is_empty() {
                    if let Some(v) = ctx.input.get(other) {
                        return Ok(v.clone());
                    }
                }
                // Allow JSON-ish literals in conditions: true/false/null/numbers
                if rest.is_empty() {
                    if let Ok(v) = literal(other) {
                        return Ok(v);
                    }
                }
                Err(EngineError::UndefinedVariable(expr.to_string()))
            }
        }
    }
}

fn split_first(expr: &str) -> (&str, &str) {
    match expr.split_once('.') {
        Some((a, b)) => (a, b),
        None => (expr, ""),
    }
}

/// Fail-closed nested path lookup: missing keys/indices → `UndefinedVariable`.
fn get_required_path(val: Value, path: &str, expr: &str) -> Result<Value, EngineError> {
    if path.is_empty() {
        Ok(val)
    } else {
        val.get_path(path)
            .cloned()
            .ok_or_else(|| EngineError::UndefinedVariable(expr.to_string()))
    }
}

fn literal(s: &str) -> Result<Value, ()> {
    match s {
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        "null" => Ok(Value::Null),
        _ => {
            if let Ok(i) = s.parse::<i64>() {
                return Ok(Value::Int(i));
            }
            if let Ok(f) = s.parse::<f64>() {
                return Ok(Value::Float(f));
            }
            // quoted string literal
            if (s.starts_with('"') && s.ends_with('"'))
                || (s.starts_with('\'') && s.ends_with('\''))
            {
                return Ok(Value::Str(s[1..s.len() - 1].to_string()));
            }
            let _ = json!(null);
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::RuntimeConfig;
    use std::collections::HashMap;

    fn ctx() -> ExecutionContext {
        let mut c = ExecutionContext::new(RuntimeConfig::default());
        c.set_variable("name", Value::Str("corex".into()));
        c.input.insert("x".into(), Value::Int(7));
        c.env.insert("HOME".into(), "/tmp".into());
        let mut step_map = std::collections::BTreeMap::new();
        step_map.insert("text".into(), Value::Str("hello".into()));
        c.set_step_output("greet", Value::Map(step_map));
        c
    }

    #[test]
    fn resolve_variables_and_input() {
        let c = ctx();
        assert_eq!(
            Resolver::resolve_string("{{name}}", &c).unwrap(),
            Value::Str("corex".into())
        );
        assert_eq!(
            Resolver::resolve_string("{{input.x}}", &c).unwrap(),
            Value::Int(7)
        );
        assert_eq!(
            Resolver::resolve_string("hi {{name}}!", &c).unwrap(),
            Value::Str("hi corex!".into())
        );
    }

    #[test]
    fn resolve_step_and_env() {
        let c = ctx();
        assert_eq!(
            Resolver::resolve_string("{{step.greet.text}}", &c)
                .unwrap()
                .as_str(),
            Some("hello")
        );
        assert_eq!(
            Resolver::resolve_string("{{env.HOME}}", &c)
                .unwrap()
                .as_str(),
            Some("/tmp")
        );
    }

    #[test]
    fn resolve_map_params() {
        let c = ctx();
        let mut m = std::collections::BTreeMap::new();
        m.insert("msg".into(), Value::Str("{{name}}".into()));
        let resolved = Resolver::resolve_value(&Value::Map(m), &c).unwrap();
        assert_eq!(resolved.get_path("msg").and_then(|v| v.as_str()), Some("corex"));
        let _ = HashMap::<String, Value>::new();
    }

    #[test]
    fn nested_missing_is_undefined() {
        let c = ctx();
        let err = Resolver::resolve_string("{{step.greet.nope}}", &c).unwrap_err();
        assert!(matches!(err, EngineError::UndefinedVariable(_)));
    }
}
