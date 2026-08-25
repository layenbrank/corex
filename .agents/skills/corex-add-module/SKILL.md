---
name: corex-add-module
description: "在 corex v4 中新增内置 Action 的标准流程。当用户提到「新增模块」「添加命令」「迁移到 corex」「实现 Action」「注册 builtin」「补全功能模块」或要在 corex 里加类似 copy/codec/scan 的能力时，务必使用本 skill。适用于从外部项目迁入可移植逻辑，或从零实现新的 Action（CLI run / Daemon IPC / Shortcut YAML 共用）。"
argument-hint: "<action-name> [--feature-only]"
allowed-tools: ["Read", "Glob", "Grep", "Edit", "Write", "Shell"]
---

# Corex 新增 Action（v4）

在 **`crates/registry`** 中按统一契约添加内置 Action，使其可被：

- **CLI**：`corex run` / `corex actions` / `corex repl`
- **Daemon IPC**：`Request::Invoke` / `RunShortcut`
- **Shortcut YAML**：`steps[].action: foo.bar`

> 旧架构（`corex-core/src/<module>/` + `invoke/registry.rs` match + `command/mod.rs` clap 树）**已删除**，不要再往那套路径加代码。

## 开始前：确认类型

| 类型 | 典型例子 | 做法 |
|------|----------|------|
| **内置 Action** | copy, codec, scan, template | `crates/registry/src/builtin/<name>.rs` + feature |
| **WASM 插件** | 第三方 `.wasm` | `plugins/` + WIT（见 `corex-plugin-sdk`） |
| **仅引擎控制流** | if / repeat / parallel | 改 `crates/engine`，不是 Action |

绝大多数新功能属于 **内置 Action**。

## 标准文件布局

```
crates/registry/
├── Cargo.toml                 # act-<name> feature + deps
└── src/
    └── builtin/
        ├── mod.rs             # pub mod + register_all 调用
        └── <name>.rs          # Action impl + register()
```

**命名约定：**

- 源文件：`snake_case`（如 `codec.rs`、`template.rs`）
- Cargo feature：`act-<name>`（如 `act-codec`、`act-template`）
- Action ID：点分命名 `domain.verb`（如 `template.render`、`codec.base64.encode`）

## 执行流程

```
用户请求新增 Action
    │
    ▼
[1] 读 docs/architecture.md 确认契约与现有 ID
    │
    ▼
[2] 选参考实现（见下表）并阅读
    │
    ▼
[3] 新建 crates/registry/src/builtin/<name>.rs
    │
    ▼
[4] 在 builtin/mod.rs 注册 mod + register_all
    │
    ▼
[5] 在 Cargo.toml 加 act-<name>，并按需加入 full
    │
    ▼
[6] 写测试 + cargo test / build 验证
    │
    ▼
[7] 更新文档（仅当用户要求或对外可见时）
```

### 参考实现速查

| 场景 | 参考 | 原因 |
|------|------|------|
| 简单字符串/数据变换 | `template` | 单 Action、`ParamSchema`、MiniJinja |
| 文件路径读写 | `file` | 多 Action（read/write/copy/delete） |
| HTTP / 外部 IO | `http` / `shell` | 可选依赖 + async |
| 从旧 monolith 迁入 | `copy` / `codec` / `scan` | 已迁移的业务模块形态 |
| 平台相关（剪贴板等） | `clipboard` / `capture` | feature + 可选 crate |

详细模板见 [references/templates.md](references/templates.md)；检查清单见 [references/checklist.md](references/checklist.md)。

## Action 实现契约

```rust
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
            "foo.bar",           // Action ID
            "Foo Bar",
            "一句话说明",
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
        // 纯业务；禁止 println!
        Ok(Value::Str(input.to_string()))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(FooBar));
}
```

### 要点

| 项 | 要求 |
|----|------|
| ID | 全局唯一点分 ID，如 `foo.bar` |
| `execute` | 无 `println!` / `eprintln!`；返回 `Value` |
| 参数 | 用 `Value` map；校验用 `ActionError::*` |
| 多动作 | 同一文件多个 struct，或多个 `register` 调用 |
| 占位符 | Shortcut YAML 的 `{{var}}` 由 **engine** 在调用前解析，Action 收到已展开值 |

## 注册：`builtin/mod.rs`

```rust
#[cfg(feature = "act-foo")]
pub mod foo;

pub fn register_all(registry: &mut ActionRegistry) {
    #[cfg(feature = "act-foo")]
    foo::register(registry);
}
```

## Cargo feature：`crates/registry/Cargo.toml`

```toml
[features]
full = [
  # ...
  "act-foo",
]
act-foo = ["dep:some-crate"]   # 无新依赖则 act-foo = []

[dependencies]
some-crate = { workspace = true, optional = true }
```

- 新依赖先写入 **workspace** `Cargo.toml` 的 `[workspace.dependencies]`，再在 registry 中 `optional = true`。
- 未稳定前可不进 `full`，用 `cargo test -p corex-registry --features act-foo` 验证。

## 不再使用的旧路径（禁止）

| 旧做法 | 状态 |
|--------|------|
| `corex-core/src/<module>/{schema,service,parse}.rs` | 已删除 |
| `invoke/registry.rs` 静态 match / `known_modules()` | 已删除 |
| `command/mod.rs` clap 子命令树 | 已删除（CLI 用 Shortcut / Action，非每模块子命令） |
| `corex-serve` / Named Pipe 旧协议 `module`+`action` | 已删除；现用 `corex-daemon` + `corex-ipc` |

## Shortcut YAML 用法

```yaml
name: demo-foo
steps:
  - id: step1
    action: foo.bar
    params:
      input: "{{input.text}}"
    save_to: out
```

```bash
corex run demo-foo --input text=hello
corex actions   # 应列出 foo.bar
```

## IPC（Daemon）

```json
{"type":"invoke","id":1,"action":"foo.bar","params":{"input":"hello"}}
```

## 测试

至少覆盖：

1. **单元测试** — `builtin/<name>.rs` 内 `#[cfg(test)]` 或 `crates/registry` tests
2. **Registry 可见** — `register_builtins` 后 `contains("foo.bar")`
3. **构建**

```bash
cargo test -p corex-registry --features act-foo
cargo build -p corex -p corex-daemon
cargo test --workspace
```

## 从外部 / 旧 monolith 迁移

**应迁移：** 纯 Rust 业务逻辑、无 UI/DB 依赖的工具函数。

**不迁移：** Tauri command 胶水、SeaORM/CRUD、窗口/托盘。

步骤：

1. 将业务逻辑迁入 `builtin/<name>.rs` 的 `Action::execute`
2. 参数从 clap `Args` 改为 `Value` map + `ParamSchema`
3. 注册 `act-<name>` + `builtin/mod.rs`
4. Tauri / 调用方改为 Shortcut YAML 或 `Request::Invoke { action, params }`

## 常见错误

| 症状 | 原因 | 修复 |
|------|------|------|
| `动作未注册` | feature 未开或未 `register` | 查 `act-*` + `register_all` |
| CLI 有、daemon 无 | daemon 未开对应 feature | daemon 依赖 `corex-registry` 的 `full` |
| 编译失败（可选依赖） | feature 未声明 `dep:` | 修正 `Cargo.toml` |
| 仍改 `invoke/registry` | 用了旧 skill | 改走 `builtin/<name>.rs` |

## 输出给用户

```markdown
## 新增 Action：`<id>`

**Feature：** `act-<name>`
**文件：** `crates/registry/src/builtin/<name>.rs`

### Shortcut
- action: <id>
  params: ...

### IPC
{"type":"invoke","id":1,"action":"<id>","params":{...}}

### 验证
cargo test -p corex-registry --features act-<name>
cargo build -p corex -p corex-daemon
```

## 相关文档

- [docs/architecture.md](../../../docs/architecture.md) — v4 workspace 与执行模型
- [docs/breaking-changes-v4.md](../../../docs/breaking-changes-v4.md) — 破坏性变更
- [plugins/README.md](../../../plugins/README.md) — WASM 插件（非内置 Action）

## 相关 Skill

| 场景 | Skill |
|------|-------|
| 函数调用关系分析 | rust-call-graph |
| 安全重构 | rust-refactor-helper |
| 复杂多步任务规划 | planning-with-files |
