//! Resolver undefined-variable and nested-path fail-closed behavior.

use corex_core::{EngineError, ExecutionContext, RuntimeConfig, Value};
use corex_engine::Resolver;
use std::collections::BTreeMap;

#[test]
fn undefined_bare_variable_errors() {
    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let err = Resolver::resolve_string("{{missing_var}}", &ctx).unwrap_err();
    assert!(
        matches!(err, EngineError::UndefinedVariable(ref s) if s.contains("missing_var")),
        "got: {err}"
    );
}

#[test]
fn undefined_input_errors() {
    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let err = Resolver::resolve_string("{{input.nope}}", &ctx).unwrap_err();
    assert!(
        matches!(err, EngineError::UndefinedVariable(ref s) if s.contains("input.nope")),
        "got: {err}"
    );
}

#[test]
fn undefined_step_errors() {
    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let err = Resolver::resolve_string("{{step.gone.value}}", &ctx).unwrap_err();
    assert!(
        matches!(err, EngineError::UndefinedVariable(ref s) if s.contains("step.gone")),
        "got: {err}"
    );
}

#[test]
fn nested_missing_path_fail_closed() {
    let mut ctx = ExecutionContext::new(RuntimeConfig::default());
    let mut map = BTreeMap::new();
    map.insert("a".into(), Value::Int(1));
    ctx.set_variable("obj", Value::Map(map));

    let err = Resolver::resolve_string("{{obj.missing}}", &ctx).unwrap_err();
    assert!(
        matches!(err, EngineError::UndefinedVariable(ref s) if s.contains("obj.missing")),
        "nested missing must fail closed, got: {err}"
    );
}

#[test]
fn nested_existing_path_ok() {
    let mut ctx = ExecutionContext::new(RuntimeConfig::default());
    let mut map = BTreeMap::new();
    map.insert("a".into(), Value::Int(1));
    ctx.set_variable("obj", Value::Map(map));

    let v = Resolver::resolve_string("{{obj.a}}", &ctx).unwrap();
    assert_eq!(v, Value::Int(1));
}

#[test]
fn interpolation_undefined_errors() {
    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let err = Resolver::resolve_string("hello {{nowhere}}!", &ctx).unwrap_err();
    assert!(
        matches!(err, EngineError::UndefinedVariable(_)),
        "got: {err}"
    );
}
