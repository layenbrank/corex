# Findings: corex 架构优化调研

## Workspace 职责
| 路径 | 职责 |
|------|------|
| `crates/core` | Value / Action / Context / Error / Permission |
| `crates/engine` | Pipeline / audit / history / watch / cron / supervisor |
| `crates/registry` | Builtin actions + ActionStore 实现 |
| `crates/ipc` | CLI↔daemon 协议与传输 |
| `crates/plugin-sdk` | 插件 SDK |
| `bins/cli` | CLI（含 ui probe） |
| `bins/daemon` | 守护进程 IPC 服务 |
| `pdfium` | 构建期复制 pdfium.dll（无问题） |
| `config/` | TOML 样例配置（无架构债） |
| `tests/` | 集成测试 |

## 进行中改动（WIP）主线 — 正确方向
已把 audit/history 从**字符串启发式分类**迁到 **typed `EngineError`/`ActionError`**：
- `ActionError::kind()` / `EngineError::kind()` / `is_permission_denied()` / `action_source()`
- `AuditEntry::from_engine` / `from_action`；字段 `denied`（true = 权限拒绝）统一替代 `permission_denied` / `allowed`
- 删除 `classify_error` / `classify_history_error` / `execute_with_resilience` 别名
- pipeline：`must_abort_step`；parallel 优先保留 permission_denied
- watch：`compare_exchange` 消除 is_running 竞态
- cron supervisor 退出时 `unregister`（与 watch 对齐）
- cli/daemon 调用方已切到 `from_action`

## 仍存问题（按优先级）

### 已处理（本会话）
1. ~~UI 错误码 audit 二次字符串解析~~ → 并入 `ActionError::{ui_code,selector_hint}`
2. ~~pipeline Abort 死分支~~ → `is_permission_denied` + 穷尽 match
3. ~~from_engine/from_action 重复~~ → 共用 `failure(..., Option<&ActionError>)`
4. ~~`get_action` / `list_actions`~~ → `find_action` / `actions`

### 刻意保留 / 后续
5. `check_probe_allowed` / `check_invoke_allowed`：域别名 + anyhow 转换，可接受
6. `is_sensitive_action`：敏感清单工具函数，生产暂不强制调用
7. `get_variable` / `get_path`：API 面大，另开任务
8. 模板工具链（deny/pre-commit/nextest）：可选工程化，非本任务

## 模板对照结论
tyr-rust-bootcamp/template 主要是工具链脚手架。本次不引入；可用 nextest 作后续验证增强。
