//! 指令输入默认值的填充。

use crate::definition::{Directive, InputDecl};
use crate::resolver::Resolver;
use corex_core::{EngineError, ExecutionContext, Value};

/// 该可选输入值是否应被视为“未提供”。
pub fn is_input_unset(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Str(s) => s.trim().is_empty(),
        _ => false,
    }
}

/// 在步骤执行之前，把声明的输入默认值填进 `ctx.input`。
pub fn fill_input_defaults(
    directive: &Directive,
    ctx: &mut ExecutionContext,
) -> Result<(), EngineError> {
    for decl in &directive.inputs {
        fill_input_default(decl, ctx)?;
    }
    Ok(())
}

fn fill_input_default(decl: &InputDecl, ctx: &mut ExecutionContext) -> Result<(), EngineError> {
    let existing = ctx.input.get(&decl.name);
    let needs_default = match existing {
        None => true,
        Some(v) if is_input_unset(v) => true,
        Some(_) => false,
    };

    if !needs_default {
        return Ok(());
    }

    if let Some(default) = &decl.default {
        let resolved = Resolver::resolve_value(default, ctx)?;
        ctx.input.insert(decl.name.clone(), resolved);
        return Ok(());
    }

    if decl.required {
        return Err(EngineError::UndefinedVariable(format!(
            "input.{}",
            decl.name
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::InputDecl;
    use corex_core::RuntimeConfig;

    #[test]
    fn unset_detects_null_and_blank() {
        assert!(is_input_unset(&Value::Null));
        assert!(is_input_unset(&Value::Str("".into())));
        assert!(is_input_unset(&Value::Str("  ".into())));
        assert!(!is_input_unset(&Value::Str("x".into())));
        assert!(!is_input_unset(&Value::Int(0)));
    }

    #[test]
    fn applies_default_when_key_missing() {
        let mut ctx = ExecutionContext::new(RuntimeConfig::default());
        let decl = InputDecl {
            name: "path".into(),
            description: String::new(),
            required: false,
            default: Some(Value::Str("/default".into())),
        };
        fill_input_default(&decl, &mut ctx).unwrap();
        assert_eq!(
            ctx.input.get("path").and_then(|v| v.as_str()),
            Some("/default")
        );
    }

    #[test]
    fn applies_default_when_empty_string() {
        let mut ctx = ExecutionContext::new(RuntimeConfig::default());
        ctx.input.insert("path".into(), Value::Str("".into()));
        let decl = InputDecl {
            name: "path".into(),
            description: String::new(),
            required: false,
            default: Some(Value::Str("/default".into())),
        };
        fill_input_default(&decl, &mut ctx).unwrap();
        assert_eq!(
            ctx.input.get("path").and_then(|v| v.as_str()),
            Some("/default")
        );
    }

    #[test]
    fn required_missing_errors() {
        let mut ctx = ExecutionContext::new(RuntimeConfig::default());
        let decl = InputDecl {
            name: "x".into(),
            description: String::new(),
            required: true,
            default: None,
        };
        let err = fill_input_default(&decl, &mut ctx).unwrap_err();
        assert!(matches!(err, EngineError::UndefinedVariable(_)));
    }

    #[test]
    fn optional_without_default_leaves_missing() {
        let mut ctx = ExecutionContext::new(RuntimeConfig::default());
        let decl = InputDecl {
            name: "x".into(),
            description: String::new(),
            required: false,
            default: None,
        };
        fill_input_default(&decl, &mut ctx).unwrap();
        assert!(!ctx.input.contains_key("x"));
    }
}
