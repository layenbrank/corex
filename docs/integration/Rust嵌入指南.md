# Rust 嵌入指南

在 **同一 Rust 进程**内加载 Corex 引擎，执行 Directive YAML 或调用 Action。适用于定制 CLI、服务端、测试 harness。

---

## 1. 依赖

```toml
[dependencies]
corex-core = { path = "../crates/core" }       # 或 crates.io 发布后版本
corex-engine = { path = "../crates/engine" }
corex-registry = { path = "../crates/registry" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

按需启用 registry features（与 CLI 默认 `full` 对齐）：

```toml
corex-registry = { path = "../crates/registry", features = ["full"] }
```

---

## 2. 最小示例：执行 YAML

```rust
use corex_core::{ExecutionContext, RuntimeConfig, Value};
use corex_engine::{Directive, Pipeline};
use corex_registry::ActionRegistry;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 加载指令
    let directive = Directive::from_yaml_file("examples/directives/hello.yaml".as_ref())?;

    // 2. 准备输入
    let mut input = HashMap::new();
    input.insert("who".into(), Value::from("Rust"));

    // 3. 注册表 + 上下文
    let mut reg = ActionRegistry::new();
    reg.register_builtins();

    let ctx = ExecutionContext::new(RuntimeConfig::default()).with_input(input);
    let registry = Arc::new(reg);
    let mut pipeline = Pipeline::new(registry);

    // 4. 执行
    let result = pipeline.execute(&directive, ctx).await?;
    println!("{}", serde_json::to_string_pretty(&result.to_json())?);
    Ok(())
}
```

仓库内参考：

- `bins/cli/src/main.rs` — `cmd_run`、`build_registry`
- `crates/engine/tests/*.rs` — 各类集成测试

---

## 3. 注册自定义 Action（内置 / Rust）

在 **同一 Rust 进程**内实现 `corex_core::Action` 并 `register`——这是 **生产环境扩展 Action 的主路径**（与 `register_builtins()` 里的 copy、http 等相同机制）。编译后 CLI、`corex-daemon`、嵌入应用均可调用。

若需要 **不重新编译宿主** 的第三方扩展，见 [WASM 插件开发](./WASM插件开发.md)（当前 bindgen 仍在完善）。

```rust
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, Value,
};
use std::sync::Arc;

struct EchoAction;

#[async_trait]
impl Action for EchoAction {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new("acme.echo", "Echo", "回显参数", ActionCategory::Data)
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        Ok(params)
    }
}

let mut reg = ActionRegistry::new();
reg.register_builtins();
reg.register(Arc::new(EchoAction));
```

在 YAML 中使用：`action: acme.echo`

内置 Action 注册入口：`corex_registry::ActionRegistry::register_builtins()`

---

## 4. 运行时配置

与 CLI 相同，从 `config/corex.toml` 或 `<数据目录>/config.toml` 加载 `RuntimeConfig`，传给 `ExecutionContext::new(config)`。

```rust
reg.apply_runtime_config(&config);  // 应用 disabled_actions 等
```

见 [运行时配置](../guide/运行时配置.md)。

---

## 5. 审计与历史

```rust
use corex_engine::{ExecutionAudit, ExecutionHistory};

let history = ExecutionHistory::open(path)?;
pipeline = pipeline.with_history(history);

let audit = ExecutionAudit::open(audit_path)?;
pipeline = pipeline.with_audit(audit);
```

CLI 参考：`bins/cli/src/main.rs` 中 `cmd_run`。

---

## 6. Crate 职责

| Crate | 用途 |
|-------|------|
| `corex-core` | `Value`、`Action` trait、`ExecutionContext`、权限 |
| `corex-engine` | `Directive`、`Pipeline`、解析器、控制流 |
| `corex-registry` | 内置 Action、`register_builtins`、WASM host |
| `corex-ipc` | Daemon 协议与传输（独立进程时用） |
| `corex-plugin-sdk` | WASM 插件 WIT 契约 |

架构图：[architecture.md](../architecture.md)

---

## 7. 与 IPC 模式的选择

| 嵌入 Rust | Daemon IPC |
|-----------|------------|
| 低延迟、同进程 | 进程隔离、sidecar 升级独立 |
| 需链接 native 依赖（OCR/UI） | 重依赖仅在 daemon 内 |
| 适合服务端/测试 | 适合 Tauri/多语言客户端 |

---

## 相关文档

- [接入总览](./接入总览.md)
- [IPC 接入指南](./IPC接入指南.md)
- [directive-yaml.md](../directive-yaml.md)
