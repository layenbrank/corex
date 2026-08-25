# Findings & Decisions — Corex 企业级架构重构

## Requirements
- 按用户提供的 forge 企业级方案重构，**项目名保持 corex**
- 交付范围 **P0–P5 全量**
- Daemon 二进制 **`corex-daemon`**（破坏性，不再使用 `corex-serve`）
- 内置 Action（shell/http/… + 原业务模块）+ Feature Gate + 运行时禁用

## Current State (post-refactor)
- Workspace: `crates/{core,engine,registry,ipc,plugin-sdk}` + `bins/{cli,daemon}` + `pdfium`
- Action trait + `crates/registry` builtins（`act-*` features）
- Shortcut YAML：`action` + `params`，占位符 `{{var}}` / `{{input.x}}`
- IPC：Unix socket（Linux/macOS）+ Windows Named Pipe（`\\.\pipe\corex`），NDJSON `Request`/`Response`
- 旧 monolith 目录已删除

## Crate Mapping (forge → corex)
| 方案 | 本仓库 |
|------|--------|
| forge-core | `corex-core` → `crates/core` |
| forge-engine | `corex-engine` → `crates/engine` |
| forge-registry | `corex-registry` → `crates/registry` |
| forge-ipc | `corex-ipc` → `crates/ipc` |
| forge-plugin-sdk | `corex-plugin-sdk` → `crates/plugin-sdk` |
| forge-cli | `corex` → `bins/cli` |
| forge-daemon | `corex-daemon` → `bins/daemon` |

## Existing → Action IDs
| 旧 module | Action id(s) |
|-----------|----------------|
| copy | `copy.run` |
| scrub | `scrub.run` |
| shade | `shade.convert` |
| compression | `compression.compress` / `compression.decompress` |
| generate | `generate.path` / `generate.uuid` / `generate.cvid` |
| exec | `shell.run` / `exec.run` |
| engine (Bing) | `suggest.bing` |
| bootstrap | `bootstrap.env` / … |
| capture | `capture.screenshot` / … |
| codec | `codec.base64.encode` / … |
| scan | `scan.os` |
| morph | `morph.*` |

## Technical Decisions
| Decision | Rationale |
|----------|-----------|
| 新树落地后删除旧 crate | 避免双轨；P4 迁移完成后清理 |
| Windows 默认 `\\.\pipe\corex` | 兼容旧 Tauri/sidecar 习惯；Unix 用 data-dir `corex.sock` |
| REPL 为 CLI 子命令非 Action | 交互层，不进入 registry |
| 版本 4.0.0 | 破坏性公共 API/二进制改名 |

## Remaining
- **Windows CI**：实机验证 Named Pipe `serve` / client `send`（Linux 仅 cfg 编译）

## Resources
- `docs/architecture.md`, `docs/breaking-changes-v4.md`
- `.agents/skills/corex-add-module/SKILL.md`（已改写为 v4 Action）
