# Corex v3 迁移说明

> 本文档仅描述 **v3 新规范**。旧版 Pipeline YAML、TaskExecutor、`tasks` 模块已移除，不提供兼容层。

## 必须变更

1. 配置文件增加 `version: 3`
2. 删除 `mode: sequential|parallel`
3. 更新变量引用：
   - `${step_id.output}` → `${steps.step_id.artifact.path}`
   - `${step_id.metadata.key}` → `${steps.step_id.artifact.data.key}`
4. Pipeline 步骤使用 `module` + 嵌套 `params`（无 `action` 字段）

## 架构变更

| 旧 | 新 |
|----|-----|
| `tasks/mod.rs` 三重分发 | `invoke/` 统一 `invoke(module, args, ctx)` |
| `GlobalArgs` | `runtime::RuntimeOpts` |
| `runner` 顺序/并行模式 | `orchestrator` + `StageGraph`（petgraph DAG） |
| generate Path 批处理 | `pipeline/stream/path_stream` 流式 |
| 模块仅 `run()` | `run()` → `execute()` → `Output` → `Artifact` |

## execute 规范

每个 Batch/Signal 模块应提供：

```rust
pub struct Output { pub path: Option<PathBuf>, /* ... */ }
pub fn execute(args: &Args) -> Result<Output>;
pub fn run(args: &Args) -> Result<()>; // 包装 execute + CLI 输出
```

IPC（Tauri）经 `serve/dispatch` → `invoke()` → `execute()`；screenshot 使用 `InvokeContext::daemon` 传入 Monitor 缓存。

## IPC / CLI

- CLI 全局选项：`--format`、`--quiet`、`--verbose`、`--color`
- IPC 仍通过 `serve/dispatch` → `invoke()`，响应可含 `data` 字段
- 历史 IPC 信封变更见 [breaking-changes.md](./breaking-changes.md)

## 参考

- [runtime.md](./runtime.md) — 运行时与退出码
- [pipeline-v3.md](./pipeline-v3.md) — YAML schema 与变量
- [architecture.md](./architecture.md) — 分层架构
