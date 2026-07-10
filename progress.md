# Progress Log

## 2026-07-10 — watch 模块

### 完成项
- `utils/ignore.rs` 重命名为 `utils/filter.rs`，统一 includes/excludes 命名
- 新增 `watch/` 模块（`corex watch start`）
- `PipelineConfig.watch: Option<WatchConfig>`
- 依赖：`notify-fs`、`notify-debouncer-full`
- 测试：config 解析、Filter 单元测试、watch_smoke 集成测试
- 文档：architecture.md、pipelines.yaml 示例

### 验证命令
```powershell
cargo test -p corex-core --features watch
cargo test -p corex --test cli_contract
corex pipeline --validate --config pipelines.yaml
corex watch start --help
```

## 2026-07-10 — 模块架构重构启动

### Phase 0–6
- 全部阶段代码与测试完成

### 验证命令
```bash
cargo test --workspace
cargo build --workspace --release
```
