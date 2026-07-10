# 代码模板

将 `<module>`、`<Module>`、`<描述>` 替换为实际值。

## mod.rs

```rust
pub mod schema;
pub mod service;
// pub mod parse;  // 需要 Pipeline 占位符时取消注释

pub use service::run;
// pub use parse::parse_args;
```

## schema.rs（单子命令）

```rust
use clap::Parser;
use serde::{Deserialize, Serialize};

/// <描述>
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct Args {
    /// 输入路径
    #[arg(value_parser = crate::utils::verifier::path)]
    pub input: String,
}
```

## schema.rs（多子命令）

```rust
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 子命令 A
    ActionA(ActionAArgs),
    /// 子命令 B
    ActionB(ActionBArgs),
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize, Default)]
pub struct ActionAArgs {
    pub input: Option<String>,
}
```

## service.rs（返回路径）

```rust
use std::path::PathBuf;

use anyhow::Result;

use crate::<module>::schema::Args;

#[derive(Debug, Clone)]
pub struct Output {
    pub path: PathBuf,
}

pub fn run(args: &Args) -> Result<()> {
    let output = execute(args)?;
    println!("✅ {}", output.path.display());
    Ok(())
}

pub fn execute(args: &Args) -> Result<Output> {
    // TODO: 业务逻辑
    todo!()
}
```

## service.rs（返回 JSON 数据）

```rust
use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};

use crate::<module>::schema::Args;
use crate::invoke::{Artifact, InvokeResult};

#[derive(Debug, Serialize)]
pub struct Data {
    pub field: String,
}

pub fn run(args: &Args) -> Result<()> {
    let data = execute(args)?;
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

pub fn execute(args: &Args) -> Result<Data> {
    todo!()
}

impl Data {
    pub fn into_invoke_result(self) -> InvokeResult {
        InvokeResult::from_artifact(Artifact::default().with_data("data", json!(self)))
    }
}
```

## parse.rs

```rust
use crate::<module>::schema::Args;
use crate::invoke::InvokeContext;

pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    Args {
        input: ctx.parse(&parsed.input),
    }
}
```

## invoke/registry.rs 片段

```rust
#[cfg(feature = "<module>")]
"<module>" => invoke_<module>(args, ctx),

#[cfg(feature = "<module>")]
fn invoke_<module>(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::<module>::schema::Args = decode_json(args, "<module>")?;
    let args = crate::<module>::parse_args(raw, ctx); // 无 parse 时用 &raw
    let output = crate::<module>::service::execute(&args)?;
    Ok(output.into_invoke_result())
}
```

## Cargo.toml 片段

```toml
# features 段
<module> = ["dep:some-crate"]

command = [
    # ...existing...
    "<module>",
]

invoke = [
    # ...existing...
    "<module>",
]

daemon = [
    # ...existing...
    "<module>",
]

# dependencies 段
some-crate = { version = "1", ... }
```

## invoke 集成测试

```rust
#[test]
fn invoke_<module>_smoke() {
    let ctx = cx::invoke::InvokeContext::empty();
    let result = cx::invoke::invoke(
        "<module>",
        serde_json::json!({ /* typed Args */ }),
        &ctx,
    )
    .expect("invoke <module>");
    // assert path or data
    assert!(result.data.is_some() || result.path_string().is_some());
}
```

## IPC 请求示例

```json
{
  "type": "invoke",
  "id": 1,
  "module": "<module>",
  "args": {
    "ActionA": {
      "input": "hello"
    }
  }
}
```
