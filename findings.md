# Findings & Decisions — Corex 企业级架构重构

## Requirements
- 按用户提供的 forge 企业级方案重构，**项目名保持 corex**
- 交付范围 **P0–P5 全量**
- Daemon 二进制 **`corex-daemon`**（破坏性，不再使用 `corex-serve`）
- 计划内置（shell/http/clipboard/notify/file/template/cron/keyring）+ **全部现有业务模块** 均为内置 Action
- Feature Gate（编译期）+ config.toml 运行时禁用

## Current State (pre-refactor)
- Workspace: `corex-core`(lib `cx`) + `corex` + `corex-serve` + `corex-capture` + `pdfium`
- 无 Action trait；`invoke/registry.rs` 静态 match
- Pipeline v3: YAML `module`+`action`+`params`，占位符 `${var.*}` / `${steps.*}`
- IPC: Windows Named Pipe only，协议 `type:invoke|shutdown`
- i-thinking 依赖 `corex-serve.exe` + `--pipe`（本重构将破坏）

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
| exec | `shell.run`（与计划 shell 合并）或 `exec.run` |
| engine (Bing) | `suggest.bing` |
| bootstrap | `bootstrap.env` / `bootstrap.inspect` / `bootstrap.force` |
| capture | `capture.screenshot` / `capture.clipboard` / `capture.crop` / … |
| codec | `codec.base64.encode` / `codec.base64.decode` / `codec.hash.md5` |
| scan | `scan.os` |
| morph | `morph.*`（保留各 PDF action） |
| schedule/watch | 引擎 triggers / CLI 子命令，非 Action |

## Plan Builtins (new)
| Action id | Feature |
|-----------|---------|
| `shell.run` | act-shell |
| `http.request` | act-http |
| `clipboard.get` / `clipboard.set` | act-clipboard |
| `notify.send` | act-notify |
| `file.read` / `file.write` / `file.copy` / `file.delete` | act-file |
| `template.render` | act-template |
| `cron.schedule` | act-cron |
| `keyring.get` / `keyring.set` | act-keyring |

## Technical Decisions
| Decision | Rationale |
|----------|-----------|
| 先建新树再删旧 crate | 破坏性允许，避免半吊子双轨长期并存 |
| exec → 可映射为 shell.run 的脚本模式 | 减少重复；保留 `exec.run` 别名注册同一实现若需要兼容 |
| 版本保持 workspace 3.x → bump 到 4.0.0 | 破坏性公共 API/二进制改名 |

## Resources
- 用户架构方案（消息正文）
- `docs/architecture.md`, `docs/pipeline-v3.md`, `docs/ipc-protocol.md`
- `.agents/skills/corex-add-module/SKILL.md`（重构后需改写）
