//! `template.render` 收原始模板：Jinja 过滤器与 `{% %}` 必须活到 MiniJinja。

use corex_core::{EngineError, ExecutionContext, RuntimeConfig, Value};
use corex_engine::{Directive, Pipeline};
use corex_registry::ActionRegistry;
use std::sync::Arc;

fn ctx_with(inputs: &[(&str, &str)]) -> ExecutionContext {
    let mut ctx = ExecutionContext::new(RuntimeConfig::default());
    for (k, v) in inputs {
        ctx.input
            .insert((*k).to_string(), Value::Str((*v).to_string()));
    }
    ctx
}

async fn render(yaml: &str, ctx: ExecutionContext) -> Result<Value, EngineError> {
    let directive = Directive::from_yaml_str(yaml).expect("parse Directive");
    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    Pipeline::new(Arc::new(registry))
        .execute(&directive, ctx)
        .await
}

const FILTERS: &str = r#"
name: raw-filters
inputs:
  - name: qry
    required: true
  - name: cp
    required: false
steps:
  - id: url
    action: template.render
    params:
      template: "https://x/?q={{ qry | urlencode }}&n={{ qry | length }}&pt={{ pt | default('page.home') }}&cp={% if cp is defined and cp != '' %}{{ cp }}{% else %}{{ qry | length }}{% endif %}"
"#;

#[tokio::test]
async fn keeps_filters_and_conditionals() {
    let value = render(FILTERS, ctx_with(&[("qry", "rust")]))
        .await
        .expect("execute");
    assert_eq!(
        value.as_str(),
        Some("https://x/?q=rust&n=4&pt=page.home&cp=4")
    );
}

#[tokio::test]
async fn conditional_takes_else_branch_on_blank_input() {
    let value = render(FILTERS, ctx_with(&[("qry", "rust"), ("cp", "")]))
        .await
        .expect("execute");
    assert_eq!(
        value.as_str(),
        Some("https://x/?q=rust&n=4&pt=page.home&cp=4")
    );
}

#[tokio::test]
async fn conditional_takes_then_branch_on_input() {
    let value = render(FILTERS, ctx_with(&[("qry", "rust"), ("cp", "7")]))
        .await
        .expect("execute");
    assert_eq!(
        value.as_str(),
        Some("https://x/?q=rust&n=4&pt=page.home&cp=7")
    );
}

const NAMESPACES: &str = r#"
name: raw-namespaces
inputs:
  - name: who
    default: "corex"
variables:
  prefix: "Hi"
steps:
  - id: first
    action: template.render
    params:
      template: "{{ prefix }}"
    save_to: first_text
  - id: second
    action: template.render
    params:
      template: "{{ first_text }}|{{ step.first }}|{{ steps.first }}|{{ input.who }}|{{ var.prefix }}|{{ env.COREX_TEST }}"
      context:
        prefix: "{{prefix}}"
    save_to: joined
"#;

#[tokio::test]
async fn exposes_resolver_namespaces_to_jinja() {
    let mut ctx = ctx_with(&[("who", "corex")]);
    ctx.env.insert("COREX_TEST".into(), "env-val".into());
    let value = render(NAMESPACES, ctx).await.expect("execute");
    assert_eq!(value.as_str(), Some("Hi|Hi|Hi|corex|Hi|env-val"));
}

#[test]
fn undefined_variable_still_fails_loudly() {
    let yaml = r#"
name: raw-typo
steps:
  - id: boom
    action: template.render
    params:
      template: "{{ no_such_var }}"
"#;
    let err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(render(yaml, ctx_with(&[])))
        .expect_err("should fail");
    let text = err.to_string();
    assert!(text.contains("boom"), "unexpected error: {text}");
    assert!(text.contains("undefined"), "unexpected error: {text}");
    // `undefined value` 不带变量名，得靠出错那行的原文定位。
    assert!(
        text.contains("{{ no_such_var }}"),
        "unexpected error: {text}"
    );
}

#[tokio::test]
async fn failed_step_output_renders_as_none() {
    let yaml = r#"
name: raw-null
steps:
  - id: missing
    action: file.read
    on_error: continue
    params:
      path: "corex-raw-params-absent.txt"
  - id: shown
    action: template.render
    params:
      template: "plain={{ step.missing }}|lax={{ step.missing | default('none', true) }}|kept={{ step.missing | default('none') }}"
"#;
    let value = render(yaml, ctx_with(&[])).await.expect("execute");
    // on_error: continue 写的是 null（已定义），default 默认只兜未定义。
    assert_eq!(
        value.as_str(),
        Some("plain=None|lax=none|kept=None"),
        "null 应渲染成 MiniJinja 的 None，lax default 才兜得住"
    );
}

#[tokio::test]
async fn objects_render_as_json() {
    let yaml = r#"
name: raw-object
steps:
  - id: out
    action: template.render
    params:
      template: "meta={{ variables.meta }}"
"#;
    let mut ctx = ctx_with(&[]);
    ctx.variables.insert(
        "meta".into(),
        Value::Map(
            [
                ("path".to_string(), Value::Str("out.txt".into())),
                ("removed".to_string(), Value::Int(1)),
            ]
            .into_iter()
            .collect(),
        ),
    );
    let value = render(yaml, ctx).await.expect("execute");
    assert_eq!(
        value.as_str(),
        Some(r#"meta={"path": "out.txt", "removed": 1}"#),
        "对象应渲染成 JSON 形状，而不是 Display 的 {{path: out.txt}}"
    );
}

#[tokio::test]
async fn bytes_render_as_byte_count() {
    let yaml = r#"
name: raw-bytes
steps:
  - id: dumped
    action: template.render
    params:
      template: "content={{ variables.content }}"
"#;
    let mut ctx = ctx_with(&[]);
    ctx.variables
        .insert("content".into(), Value::Bytes(b"corex".to_vec()));
    let value = render(yaml, ctx).await.expect("execute");
    assert_eq!(value.as_str(), Some("content=<5 bytes>"));
}
