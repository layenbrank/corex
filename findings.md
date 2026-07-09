# Findings: Corex 全阶段文档调研

## Workspace 结构

- **4 crate**：`corex-core`（库 `cx`）、`corex`、`corex-serve`、`corex-shot`
- **3 binary**：完整 CLI、Daemon、轻量截图

## Feature 体系（corex-core）

```
default → all → command → [cli + 全部业务模块 + pipeline + schedule]
serve → daemon → [cli + 业务模块，无 pipeline/schedule]
```

- `corex`：`features = ["all"]`
- `corex-serve`：`features = ["serve"]`
- `corex-shot`：`features = ["screenshot"]`

## serve 模块流程

1. `serve::run` → `DaemonState::init()`（缓存 Monitor）
2. `pipe::run_server` → 循环 CreateNamedPipe → handle_client
3. `parse_request` → invoke / shutdown
4. `dispatch` → 各模块 `run` 或 `screenshot::capture`

## IPC 协议要点

- 传输：Windows Named Pipe `\\.\pipe\corex`
- 格式：单行 JSON + `\n`
- 请求上限：64KB
- Legacy 兼容：`{"id", "module", "args"}` 与 `{"cmd":"shutdown"}`

## 阶段状态

| 阶段 | 状态 |
|------|------|
| 1 统一 run | 完成 |
| 2 Feature 拆分 | 完成 |
| 3 Daemon IPC | 完成 |
| 4 Tauri 客户端 | 示例就绪（examples/tauri/） |
| 5 基准测试 | 未开始 |

## 文档缺口（已修复）

- `corex-core/examples/ipc.rs` 缺失 → 已恢复
- README 无架构/Daemon 索引 → 已补充

## 现有文档

- `README.md` — CLI 命令详解
- `docs/cron.md` — cron 格式
- `docs/corex.task.schema.README.md` — JSON Schema
- `docs/worktree.md` — Git worktree
- `examples/tauri/README.md` — Tauri 集成速查
