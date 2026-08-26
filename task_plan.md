# Task Plan: 逻辑审查修复 + 文档/配置完善

## Goal
修 IPC 安全/健壮性、配置真正生效、行为正确性，并重写/归档文档与配置对齐 v4。

## Next Step
父代理可审阅/提交；本轮不强制 commit

## Current Phase
Phase B+C — complete

## Phases

### Phase A — 核心逻辑修复
- [x] A1 IPC 安全与健壮性
- [x] A2 配置真正生效 + step timeout
- [x] A3 cron/shell/permissions/parallel
- [x] A4 测试补强
- **Status:** complete

### Phase B — 文档重写与归档
- [x] Rewrite `docs/ipc-protocol.md`
- [x] Create `docs/shortcut-yaml.md` / `docs/actions.md`
- [x] Fix `breaking-changes-v4.md` / `architecture.md`
- [x] Rewrite README + `tauri-integration.md`
- [x] Archive ≤v3 docs; move legacy pipelines yaml
- **Status:** complete

### Phase C — 配置与发布说明
- [x] Examples `control-flow.yaml` / `copy-demo.yaml`
- [x] Polish plugins/README + corex-add-module skill（agents + cursor）
- [x] Verify `config/default.toml` token comments
- [x] Update tauri `corex_ipc.rs` (+ capabilities / README / lib comment)
- **Status:** complete

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| 同分支继续 | cursor/corex-enterprise-arch-b0e1 |
| YAML 省略 permissions = allow-all | 不打碎 hello.yaml |
| cron stub → Err 非伪成功 | 计划明确 |
| WASM/真实 cron 不做 | 计划范围外 |
| 本轮不强制 git commit | 父代理可能统一提交 |
| 未改 `/opt/cursor/artifacts/plans/...` | 用户明确禁止 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
|       |         |            |
