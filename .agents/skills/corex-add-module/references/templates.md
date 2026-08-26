# 代码模板（v4 Action）

将 `<name>`、`<id>`、`<描述>` 替换为实际值。

## builtin/<name>.rs（单 Action）

```rust
//! `<id>` — <描述>

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;

pub struct FooBar;

#[async_trait]
impl Action for FooBar {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "<id>",
            "Foo Bar",
            "<描述>",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("input", SchemaType::Str, true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".into()))?;
        let input = map
            .get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("input".into()))?;
        Ok(Value::Str(input.to_string()))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(FooBar));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn executes() {
        let action = FooBar;
        let mut ctx = ExecutionContext::default();
        let mut map = BTreeMap::new();
        map.insert("input".into(), Value::Str("hi".into()));
        let out = action.execute(Value::Map(map), &mut ctx).await.unwrap();
        assert_eq!(out.as_str(), Some("hi"));
    }
}
```

## builtin/<name>.rs（多 Action）

```rust
pub struct FooEncode;
pub struct FooDecode;

#[async_trait]
impl Action for FooEncode {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new("foo.encode", "Foo Encode", "...", ActionCategory::Data)
    }
    async fn execute(&self, params: Value, ctx: &mut ExecutionContext) -> Result<Value, ActionError> {
        // ...
        Ok(Value::Null)
    }
}

#[async_trait]
impl Action for FooDecode {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new("foo.decode", "Foo Decode", "...", ActionCategory::Data)
    }
    async fn execute(&self, params: Value, ctx: &mut ExecutionContext) -> Result<Value, ActionError> {
        // ...
        Ok(Value::Null)
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(FooEncode));
    registry.register(Arc::new(FooDecode));
}
```

## builtin/mod.rs 片段

```rust
#[cfg(feature = "act-<name>")]
pub mod <name>;

pub fn register_all(registry: &mut ActionRegistry) {
    #[cfg(feature = "act-<name>")]
    <name>::register(registry);
}
```

## Cargo.toml 片段

```toml
[features]
full = [
  # ...
  "act-<name>",
]
act-<name> = []                 # 或 ["dep:some-crate"]

[dependencies]
# some-crate = { workspace = true, optional = true }
```

## Directive YAML

```yaml
name: demo-<name>
description: ""
inputs: []
variables: {}
steps:
  - id: main
    action: <id>
    params:
      input: "hello"
    save_to: result
```

## IPC Invoke

每条 daemon 请求须带 `auth_token`（`COREX_TOKEN` / `config.toml` / data-dir `token` 文件）。

```json
{"type":"invoke","id":1,"auth_token":"<token>","action":"<id>","params":{"input":"hello"}}
```
