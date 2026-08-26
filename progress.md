# Progress Log

## Session: 2026-08-26 — Phase B+C docs/config

### Done
- Rewrote `docs/ipc-protocol.md` from `protocol.rs` (NDJSON, 1MiB, auth_token, endpoints)
- Created `docs/shortcut-yaml.md`, `docs/actions.md`
- Fixed `breaking-changes-v4.md` (Named Pipe exists, actions migrated, repl, token, confine)
- Fixed `architecture.md` (parallel concurrent when max>1; config sections; new doc links)
- Rewrote README v4-first; removed fake copy/pipeline/watch chapters
- Rewrote `docs/tauri-integration.md`; updated `examples/tauri/corex_ipc.rs` (+ README, capabilities)
- Archived ≤v3 docs → `docs/archive/`; moved `pipelines-v3-legacy.yaml` → `examples/legacy/`
- Added `control-flow.yaml`, `copy-demo.yaml`
- Polished `plugins/README.md`, synced `.agents` + `.cursor` `corex-add-module` skills
- Verified `config/default.toml` token comments present
- **No commit** (parent may commit)

### Files touched
See agent final file list in reply.

## Prior sessions
- 2026-08-25: Windows Named Pipe + REPL + cleanup; P3 WASM + P5 history; Windows CI fix

## 5-Question Reboot Check
| Question | Answer |
|----------|--------|
| Where am I? | Phase B+C complete |
| Where am I going? | Parent review / commit |
| What's the goal? | Docs/config align Corex v4 |
| What have I learned? | Pipe exists; parallel concurrent; cron Err; 7z soft-fail |
| What have I done? | Full docs rewrite + archive + examples + Tauri client |
