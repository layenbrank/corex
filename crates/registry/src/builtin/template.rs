//! `template.render` —— MiniJinja 模板渲染。

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use std::sync::Arc;

pub struct TemplateRender;

#[async_trait]
impl Action for TemplateRender {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NONE
    }

    /// 参数原样交给 MiniJinja：引擎的 `{{...}}` 预解析会先吃掉过滤器和 `{% %}`。
    fn is_raw_params(&self) -> bool {
        true
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "template.render",
            "模板渲染",
            "使用 MiniJinja 渲染模板字符串",
            Bucket::Data,
        )
        .with_params(vec![
            ParamSchema::new("template", SchemaType::Str, true),
            ParamSchema::new("context", SchemaType::Map, false)
                .with_default(Value::Map(Default::default())),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".to_string()))?;
        let template = map
            .get("template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("template".to_string()))?;

        let mut env = minijinja::Environment::new();
        // 变量拼错时直接报错，而不是静默渲染成空串。
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.add_template("tpl", template)
            .map_err(|e| render_failure("模板语法错误", &e, template))?;
        let tmpl = env
            .get_template("tpl")
            .map_err(|e| ActionError::execution(format!("加载模板失败: {e}")))?;

        let mut scope = template_scope(ctx);
        if let Some(Value::Map(extra)) = map.get("context") {
            for (k, v) in extra {
                scope.insert(k.clone(), render_context_value(&env, &scope, v)?);
            }
        }

        let rendered = tmpl
            .render(serde_json::Value::Object(scope))
            .map_err(|e| render_failure("渲染失败", &e, template))?;
        Ok(Value::Str(rendered))
    }
}

/// 组装 MiniJinja 上下文。裸名取值与 `Resolver` 一致（先查 variables 再查
/// input），并补齐 `input` / `step(s)` / `env` / `var(iables)` 命名空间。
fn template_scope(ctx: &ExecutionContext) -> serde_json::Map<String, serde_json::Value> {
    let mut scope: serde_json::Map<String, serde_json::Value> = ctx
        .input
        .iter()
        .map(|(k, v)| (k.clone(), to_jinja(v)))
        .collect();
    for (k, v) in &ctx.variables {
        scope.insert(k.clone(), to_jinja(v));
    }

    let steps = values_to_json(ctx.step_outputs.iter());
    scope.insert("input".into(), values_to_json(ctx.input.iter()));
    scope.insert("variables".into(), values_to_json(ctx.variables.iter()));
    scope.insert("var".into(), values_to_json(ctx.variables.iter()));
    scope.insert("step".into(), steps.clone());
    scope.insert("steps".into(), steps);
    scope.insert(
        "env".into(),
        serde_json::Value::Object(
            ctx.env
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect(),
        ),
    );
    scope.insert(
        "directive_input".into(),
        ctx.directive_input
            .as_ref()
            .map(to_jinja)
            .unwrap_or(serde_json::Value::Null),
    );
    scope
}

/// 显式 `context` 的值：字符串按模板渲染一次，与预解析时代的语义一致。
fn render_context_value(
    env: &minijinja::Environment<'_>,
    scope: &serde_json::Map<String, serde_json::Value>,
    value: &Value,
) -> Result<serde_json::Value, ActionError> {
    match value {
        Value::Str(text) => env
            .render_str(text, scope)
            .map(serde_json::Value::String)
            .map_err(|e| render_failure("context 渲染失败", &e, text)),
        other => Ok(to_jinja(other)),
    }
}

/// `Value` map → Jinja 对象。
fn values_to_json<'a>(entries: impl Iterator<Item = (&'a String, &'a Value)>) -> serde_json::Value {
    serde_json::Value::Object(entries.map(|(k, v)| (k.clone(), to_jinja(v))).collect())
}

/// `Value` → Jinja 上下文值。
///
/// 二进制渲染成 `<N bytes>`（同 `Value::Display`），否则会变成一串数字。
fn to_jinja(value: &Value) -> serde_json::Value {
    match value {
        Value::Bytes(bytes) => serde_json::Value::String(format!("<{} bytes>", bytes.len())),
        Value::Array(items) => serde_json::Value::Array(items.iter().map(to_jinja).collect()),
        Value::Map(entries) => values_to_json(entries.iter()),
        other => other.to_json(),
    }
}

/// 渲染失败：MiniJinja 的 `undefined value` 不带变量名，拼上出错那行原文才定位得到。
fn render_failure(prefix: &str, err: &minijinja::Error, source: &str) -> ActionError {
    let at = err
        .line()
        .and_then(|n| source.lines().nth(n - 1))
        .map(|line| format!(" | 模板: {line}"))
        .unwrap_or_default();
    ActionError::execution(format!("{prefix}: {err}{at}"))
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(TemplateRender));
}
