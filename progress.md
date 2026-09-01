# Progress Log

## 2026-09-01 — Architecture optimization

### Phase 1
- 覆盖旧 task_plan；写入 findings
- WIP：typed EngineError/ActionError → audit/history；denied；must_abort；watch CAS；cron unregister

### Phase 2–3
- `ActionError::selector_hint` + 统一 `bracket_segments` / `ui_code`
- `audit.rs` 删除 extract_* 二次解析；failure 只读 typed ActionError
- pipeline：`find_action`；on_error 无死分支；`prefer_branch_err`
- `ActionStore::find_action` / `actions`

### Phase 4 verification
- `cargo check -p corex-core -p corex-engine -p corex-registry -p corex -p corex-daemon` ✅
- `cargo test -p corex-core --lib error` ✅ (2)
- `cargo test -p corex-engine --lib` ✅ (40)
- `cargo test -p corex-engine --test parallel_partial --test permissions_and_timeout` ✅ (13)
- `cargo test -p corex-daemon --bin corex-daemon` ✅ (4)
- nextest：未安装，跳过

### Not done (intentional)
- cargo-deny / pre-commit / git-cliff / typos 引入
- pdfium / config 改动
- Value::get_path / Context::get_variable 全局改名
- JSONL writer 抽取
