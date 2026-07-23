---
name: corex-add-module
description: "在 corex-core 中新增业务模块的标准流程。当用户提到「新增模块」「添加命令」「迁移到 corex」「实现 invoke」「注册 CLI/IPC/Pipeline」「补全功能模块」或要在 corex 里加类似 copy/codec/scan 的能力时，务必使用本 skill。适用于从外部项目迁入可移植逻辑，或从零实现新的 CLI + Daemon IPC + Pipeline step。"
argument-hint: "<模块名> [--invoke-only|--cli-only]"
allowed-tools: ["Read", "Glob", "Grep", "Edit", "Write", "Shell"]
---

# Corex 新增模块

在 `corex-core` 中按统一契约添加业务模块，使其可被 **CLI**、**Pipeline invoke**、**corex-serve IPC** 调用（按需）。

## 开始前：确认模块类型

| 类型 | 典型例子 | 需要 invoke/registry？ | 需要 command/mod.rs？ |
|------|----------|------------------------|----------------------|
| **Invoke 业务模块** | copy, codec, scan, morph | 是 | 是 |
| **编排/调度模块** | pipeline, schedule | 否（内部调用 invoke） | 是 |
| **仅 Daemon 胶水** | serve | 否 | 否 |

绝大多数新功能属于 **Invoke 业务模块**。若用户未说明，默认按此类型实现。

## 标准目录结构

```
corex-core/src/<module>/
├── mod.rs       # pub mod schema; pub mod service; [pub mod parse;] pub use service::run;
├── schema.rs    # Args（clap::Parser + serde Serialize/Deserialize）
├── service.rs   # execute() 纯业务 + run() CLI 包装
└── parse.rs     # 可选：Pipeline 占位符 ${var.*} / ${steps.*}
```

**命名约定：**
- 模块目录：`snake_case`（如 `codec`、`scan`）
- CLI 子命令：与模块名一致的小写（`corex codec ...`）
- IPC `module` 字段：与目录名一致（`"codec"`）

## 执行流程

```
用户请求新增模块
    │
    ▼
[1] 读 docs/architecture.md 确认契约
    │
    ▼
[2] 选参考模块（见下表）并阅读其实现
    │
    ▼
[3] 创建 schema.rs + service.rs + mod.rs [+ parse.rs]
    │
    ▼
[4] 注册 Cargo feature + lib.rs + command/mod.rs
    │
    ▼
[5] 若需 Pipeline/IPC：注册 invoke/registry.rs
    │
    ▼
[6] 写测试 + cargo build/test 验证
    │
    ▼
[7] 更新文档（仅当用户要求或模块对外可见时）
```

### 参考模块速查

| 场景 | 参考模块 | 原因 |
|------|----------|------|
| 返回 JSON 数据、无文件输出 | `scan` | `into_invoke_result()` + IPC data |
| 返回文件路径 | `copy` | `Output { path }` + `path_result` |
| 路径 + 文本双输出 | `codec` | `into_invoke_result()` 组合 path/text |
| 需要 Pipeline 变量替换 | `copy` / `codec` | `parse.rs` + `parse_args` |
| 子命令嵌套（操作→算法→参数） | `codec` | `Args` enum + `Subcommand` |
| 无输出副作用 | `bootstrap` | `InvokeResult::default()` |
| Daemon 特殊状态 | `screenshot` | `ctx.cached_monitors()` |

详细模板见 [references/templates.md](references/templates.md)；完整检查清单见 [references/checklist.md](references/checklist.md)。

## 各层职责（不可混淆）

| 层 | 文件 | 职责 |
|----|------|------|
| Schema | `schema.rs` | 仅参数定义；`#[derive(Parser, Serialize, Deserialize)]` |
| Parse | `parse.rs` | 占位符解析；**不做**业务逻辑 |
| Execute | `service.rs::execute` | 纯业务；**禁止** `println!` |
| Run | `service.rs::run` | `execute` + 人类可读输出 |
| Invoke | `invoke/registry.rs` | `decode_json` → `parse_args?` → `execute` → `InvokeResult` |

### execute / run 模式

```rust
#[derive(Debug, Clone)]
pub struct Output {
    pub path: PathBuf,           // 或 Option<PathBuf>、自定义字段
}

/// CLI 入口
pub fn run(args: &Args) -> Result<()> {
    let out = execute(args)?;
    println!("✅ {}", out.path.display());
    Ok(())
}

/// Pipeline / IPC 复用
pub fn execute(args: &Args) -> Result<Output> {
    // 纯业务，无 println
}
```

## Schema 要求

1. **必须**同时 derive `clap::Parser` 与 `serde::{Serialize, Deserialize}`，以便 CLI 与 JSON IPC 共用。
2. 有子命令时用 `enum Args` + `#[command(subcommand)]`（参考 `codec`、`screenshot`）。
3. IPC args 使用**线格式**（小写路由 + 扁平 flags）：

```json
{"module":"scan","action":"os","args":{}}
{"module":"screenshot","action":"capture","args":{"to":"C:/out"}}
{"module":"codec","action":"encode","algorithm":"base64","args":{"input":"aGVsbG8="}}
```

**禁止**在 args 内嵌套 PascalCase 子命令或 `scheme`。

4. 路径参数优先用 `crate::utils::paths` 中的校验（`validate_read_file`、`validate_write_path`）。

## Invoke 注册（registry.rs）

在 `invoke()` match 和 `known_modules()` 各加一处，并用 `#[cfg(feature = "<module>")]` 包裹：

```rust
#[cfg(feature = "my_module")]
"my_module" => invoke_my_module(args, ctx),

fn invoke_my_module(args: Value, ctx: &InvokeContext<'_>) -> Result<InvokeResult> {
    let raw: crate::my_module::schema::Args = decode_json(args, "my_module")?;
    let args = crate::my_module::parse_args(raw, ctx); // 无占位符可省略
    let output = crate::my_module::service::execute(&args)?;
    Ok(output.into_invoke_result()) // 或 path_result / path_str_result
}
```

### InvokeResult 映射

| 输出类型 | registry 写法 |
|----------|---------------|
| 文件路径 `PathBuf` | `path_result(output.path)` |
| `Option<PathBuf>` | `optional_path_result(output.path)` |
| `Option<String>` 路径 | `path_str_result(output.path)` |
| 纯 JSON 数据 | `output.into_invoke_result()` |
| 路径 + 侧车 data | `path_str_result(...).with_ipc_data(output.data)` |
| 无返回值 | `Ok(InvokeResult::default())` |

`known_modules()` 用于 Pipeline 配置校验（`pipeline/config.rs`），漏注册会导致 pipeline step 校验失败。

## Cargo Feature 注册

编辑 `corex-core/Cargo.toml`：

1. **模块 feature**（声明外部依赖）：
   ```toml
   my_module = ["dep:some-crate"]
   ```

2. **加入聚合 feature**（按需要）：
   - `command` — 完整 CLI
   - `invoke` — Pipeline / 统一调用层
   - `daemon` — corex-serve（不含 pipeline/schedule）

3. 在 `[dependencies]` 添加 `some-crate = { ... }`（若为新依赖）。

4. 若模块产出文件且会被 pipeline 下游消费，确认是否需加入 `invoke` feature 列表。

## lib.rs 与 command/mod.rs

**lib.rs：**
```rust
#[cfg(feature = "my_module")]
pub mod my_module;
```

**command/mod.rs：**
- `use crate::my_module;`
- `Commands` enum 加 variant（带 `#[cfg(feature = "my_module")]`）
- `dispatch()` match 加分支 → `my_module::run(&a)`

## parse.rs（Pipeline 占位符）

仅当 Args 含路径/字符串且需在 Pipeline 中引用 `${var.x}` / `${steps.prev.path}` 时添加：

```rust
pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    Args {
        path: ctx.parse(&parsed.path),
        // 其余字段原样透传
        ..parsed
    }
}
```

`scan`、`bootstrap` 等无路径占位需求的模块可省略 `parse.rs`，registry 直接 `execute(&raw)`。

## 测试

至少覆盖：

1. **单元测试** — `service.rs` 内 `#[cfg(test)]` 或 `corex-core/tests/`
2. **invoke 集成** — `tests/invoke_modules.rs` 加一条 `invoke("my_module", json!(...))` 测试
3. **CLI smoke** — `cargo run -p corex -- my_module --help`

```powershell
cargo build -p corex-core --features my_module
cargo test -p corex-core --features invoke
cargo build --workspace
```

## 从外部项目迁移

参考 [补全 corex 功能模块](docs/architecture.md) 与历史迁移（codec/scan/morph）：

**应迁移：** 纯 Rust 业务逻辑、无 UI/DB 依赖的工具函数。

**不迁移：**
- Tauri `#[tauri::command]` 胶水
- SeaORM / 产品 CRUD
- 窗口、托盘、emit 相关代码

迁移步骤：
1. 将 `utils/xxx.rs` 逻辑迁入 `service.rs::execute`
2. clap 参数迁入 `schema.rs`
3. Tauri 侧改为 `corex_ipc::invoke("module", args)` 薄调用
4. 删除 Tauri 内重复业务代码

## 常见错误

| 症状 | 原因 | 修复 |
|------|------|------|
| `未知或未启用的模块` | registry 未注册或 feature 未启用 | 检查 registry + Cargo.toml |
| Pipeline 校验失败 | `known_modules()` 缺项 | 补 registry |
| IPC 参数解析失败 | JSON 结构与 Args 不一致 | 用 typed variant 格式 |
| CLI 有输出、Pipeline 无输出 | 逻辑写在 `run` 而非 `execute` | 下沉到 `execute` |
| daemon 编译失败 | 新模块未加入 `daemon` feature | 更新 Cargo.toml |

## 输出给用户

完成实现后，用以下格式汇报：

```markdown
## 新增模块：`<name>`

**类型：** Invoke 业务模块
**文件：** `corex-core/src/<name>/`（列出新建/修改文件）

### CLI
corex <name> ...

### IPC
{"type":"invoke","id":1,"module":"<name>","args":{...}}

### Pipeline step
- module: <name>
- args: ...

### 验证命令
cargo test -p corex-core --features invoke
```

## 相关文档

- [docs/architecture.md](../../../docs/architecture.md) — 模块契约与 feature 树
- [docs/ipc-protocol.md](../../../docs/ipc-protocol.md) — IPC typed 协议
- [docs/pipeline-v3.md](../../../docs/pipeline-v3.md) — Pipeline step 配置

## 相关 Skill

| 场景 | Skill |
|------|-------|
| 函数调用关系分析 | rust-call-graph |
| 安全重构 | rust-refactor-helper |
| 复杂多步任务规划 | planning-with-files |
