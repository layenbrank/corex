# Findings & Decisions — Corex 企业级架构重构

## Requirements
- 按 forge 方案重构，**项目名保持 corex**；Daemon **`corex-daemon`**
- Phase B+C：文档/配置/示例对齐 v4（本轮）

## Current State (docs round)
- IPC：NDJSON，`MAX_LINE_BYTES = 1MiB`；`Request`/`Response` + `auth_token`
- Transport：Unix `<data-dir>/corex.sock`；Windows `\\.\pipe\corex`
- Token：`COREX_TOKEN` → config `daemon.token` → `<data-dir>/token`
- Parallel：`max_concurrency` / `runtime.max_parallel` >1 时 `buffer_unordered` 并发
- Permissions：YAML 省略 = allow-all；任一 flag true 后未声明类别拒绝
- `cron.schedule`：注册但 `execute` 返回未实现错误
- compression `7z`：返回 execution error（未在此构建启用）
- Path confinement：daemon `run_shortcut` / `list_shortcuts` 路径限制在 shortcuts 根下

## Crate Mapping
| 方案 | 本仓库 |
|------|--------|
| forge-core | `corex-core` → `crates/core` |
| forge-engine | `corex-engine` → `crates/engine` |
| forge-registry | `corex-registry` → `crates/registry` |
| forge-ipc | `corex-ipc` → `crates/ipc` |
| forge-plugin-sdk | `corex-plugin-sdk` → `crates/plugin-sdk` |
| forge-cli | `corex` → `bins/cli` |
| forge-daemon | `corex-daemon` → `bins/daemon` |

## Docs inventory (done)
| Doc | Role |
|-----|------|
| `docs/ipc-protocol.md` | v4 NDJSON protocol |
| `docs/shortcut-yaml.md` | Shortcut DSL |
| `docs/actions.md` | Builtin Action ID table |
| `docs/architecture.md` | v4 layout + links |
| `docs/breaking-changes-v4.md` | v4 breaking (pipe exists, actions migrated) |
| `docs/tauri-integration.md` | corex-daemon + token + pipe |
| `docs/archive/*` | historical ≤v3 |

## Remaining
- 父代理统一 commit（本轮未提交）
- Windows 实机 Named Pipe 冒烟（CI 已过编译）

## Resources
- `crates/ipc/src/protocol.rs`, `crates/engine/src/definition.rs`
- `crates/registry/src/builtin/*`
- `.agents/skills/corex-add-module/SKILL.md`
