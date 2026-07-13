# Progress Log

### 验证命令
```powershell
cargo test -p corex-core --features invoke,pipeline
```

## 2026-07-13 — Code Review 修复（exec + generate File 移除）

### 完成项
- P1：`generate-version.ps1` 改用 UTF-8 无 BOM 写入
- P2：移除 corex-core `handlebars` 依赖；`template.rs` / `tasks/mod.rs` 已不在仓库
- 文档：breaking-changes、ipc-protocol（exec + 去重 Path）、architecture IPC 表
- 测试：`data` 非 object、`.bat` smoke、orchestrator exec E2E

## 2026-07-13 — exec 模块

### 完成项
- 新增 `exec` Invoke 模块（Run 子命令、JSON stdout → Artifact）
- 注册 feature / CLI / registry / known_modules
- 测试：service 单元测试、exec_smoke、invoke_modules
- 文档：pipeline-v3.md、architecture.md
- H5+ 迁移：`.corex/pipelines.yaml` + `generate-version.ps1`

### 验证命令
```powershell
cargo test -p corex-core --features invoke
cargo test -p corex-core --test exec_smoke --test invoke_modules
cargo build -p corex-core --features exec
```

## 2026-07-11 — 触发模块命名与文档同步

### 完成项
- 逻辑：`serve_dual` cron 错误可见；`schedule::serve` 严格 `check_cron`
- 命名：guard/trigger/watch/schedule/report/runtime 全量收敛
- 去重：`check` 解析结果复用于 Watch / Dual
- 测试：guard release、check 坏路径、schedule check_cron（共 32 单元 + 9 CLI）
- 文档：architecture.md、pipeline-v3.md、README.md、pipelines.yaml

### 验证命令
```powershell
cargo build --workspace
cargo test -p corex-core -p corex
```

## 2026-07-10 — watch 模块

### 完成项
- `utils/ignore.rs` 重命名为 `utils/filter.rs`，统一 includes/excludes 命名
- 新增 `watch/` 模块（`corex watch run`）
- `PipelineConfig.watch: Option<WatchConfig>`
- 依赖：`notify-fs`、`notify-debouncer-full`
- 测试：config 解析、Filter 单元测试、watch_smoke 集成测试
- 文档：architecture.md、pipelines.yaml 示例

### 验证命令
```powershell
cargo test -p corex-core --features watch
cargo test -p corex --test cli_contract
corex pipeline --validate --config pipelines.yaml
corex watch run --help
```

## 2026-07-10 — 模块架构重构启动

### Phase 0–6
- 全部阶段代码与测试完成

### 验证命令
```bash
cargo test --workspace
cargo build --workspace --release
```
