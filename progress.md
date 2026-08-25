# Progress Log

## Session: 2026-08-25 — P3 WASM + P5 hardening

### Done
- P3: wasmtime 34 behind `wasm` feature；`WasmPluginHost`（Engine async+component model、WasiCtxBuilder、Linker）
- P3: discovery 扫描 `*.wasm` 并尝试 load；失败 log/skip；`plugins/README.md`
- P5: `ExecutionHistory` JSONL；Pipeline `with_history`；CLI/daemon 接线；`history_smoke`
- P5: `docs/architecture.md`、`docs/breaking-changes-v4.md`、README workspace/`corex-daemon`
- P5: publish-release + build-and-test → `corex` + `corex-daemon`；tauri 示例改名
- 旧 `pipelines.yaml` → `examples/shortcuts/pipelines-v3-legacy.yaml`
- 旧 monolith 目录保留（P4 仍可能引用）

### Test Results
| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| cargo build -p corex -p corex-daemon | success | success | ✅ |
| history_smoke | JSONL ok/fail | pass | ✅ |
| pipeline_smoke | Hi, corex! | pass | ✅ |
| control_flow | if/repeat/parallel | pass | ✅ |
| P4 act-morph/full | compile | 未纳入 full（另一 agent） | ⏭ |

## 5-Question Reboot Check
| Question | Answer |
|----------|--------|
| Where am I? | P3+P5 完成并待 push |
| Where am I going? | 另一 agent 完成 P4 后再删旧 crate |
| What's the goal? | corex 企业级可组合快捷指令架构 |
| What have I learned? | wasmtime-wasi 34 在 `p2`；Action future 非 Send 时不宜 JoinSet |
| What have I done? | WASM host 骨架 + 历史/文档/CI |
