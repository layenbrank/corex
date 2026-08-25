# Progress Log

## Session: 2026-08-25 — Windows IPC + REPL + cleanup

### Done
- Windows Named Pipe：`NamedPipeTransport`（interprocess 2.x tokio）+ `default_endpoint` / `serve_platform`
- CLI/daemon 按平台选 Unix socket 或 `\\.\pipe\corex`（`--socket` / `--pipe`）
- `corex repl`：help / actions / list / run / quit
- 删除旧 monolith：`corex/`、`corex-core/`、`corex-serve/`、`corex-capture/`（保留 `pdfium/`）
- 重写 `.agents/skills/corex-add-module` 为 v4 Action（`builtin/<name>.rs` + `act-*`）
- 规划文件标记 P0–P5 完成；剩余：Windows CI 实机验证 Named Pipe

### Test Results
| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| cargo build -p corex -p corex-daemon | success | success | ✅ |
| cargo test --workspace | all pass | all pass | ✅ |
| Windows Named Pipe | serve/send NDJSON | 需 Windows CI（本机无 windows target） | ⏭ |

## Session: 2026-08-25 — P3 WASM + P5 hardening

### Done
- P3: wasmtime 34 behind `wasm` feature；`WasmPluginHost`
- P3: discovery 扫描 `*.wasm`；`plugins/README.md`
- P5: `ExecutionHistory` JSONL；CLI/daemon；docs；CI → `corex` + `corex-daemon`

### Test Results
| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| cargo build -p corex -p corex-daemon | success | success | ✅ |
| history_smoke / pipeline_smoke / control_flow | pass | pass | ✅ |

## 5-Question Reboot Check
| Question | Answer |
|----------|--------|
| Where am I? | P0–P5 完成；收尾 IPC/REPL/清理 |
| Where am I going? | Windows CI 验证 Named Pipe |
| What's the goal? | corex 企业级可组合快捷指令架构 |
| What have I learned? | interprocess named pipe 用 `&conn` 双半读写，无 into_split |
| What have I done? | Named Pipe + REPL + 删旧 crate + skill/docs |
