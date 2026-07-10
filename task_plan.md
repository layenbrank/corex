# Task Plan: Corex v3 模块架构重构 + watch 模块

## Goal
统一 run→execute→Artifact 契约；修复 orchestrator/IPC 缺陷；补齐测试与文档；新增 watch 文件监听。

## Phases

| Phase | Task | Status |
|-------|------|--------|
| 0 | planning 文件同步 | complete |
| 1 | orchestrator 失败报告、并行 abort、IPC monitors、删 tasks/ | complete |
| 2 | copy/scrub/shade execute 层 | complete |
| 3 | generate 统一 execute + path_stream | complete |
| 4 | resolve 下沉、screenshot 完整 Artifact | complete |
| 5 | ExitStatus 精细映射 | complete |
| 6 | 契约测试 + 文档 | complete |
| 7 | watch 模块 + utils/filter 重命名 | complete |
| 8 | 触发集成：命名优化、guard 锁、文档同步 | complete |

## 模块契约
- `execute()` → `Output` → `Artifact`
- `run()` → execute + CLI 输出
- `invoke()` → resolve → execute → InvokeResult

## watch 模块（Phase 7）
- `utils/ignore.rs` → `utils/filter.rs`（includes/excludes）
- `PipelineConfig.watch: Option<WatchConfig>`
- `corex watch run` — debounce 后重跑 Pipeline

## 触发集成（Phase 8）
- `pipeline/trigger.rs` — `run_mode` / `check` / `serve_dual`
- `pipeline/guard.rs` — `new_set` / `spawn` / `run_sync` 共享 `RunningSet`
- `watch::resolve` / `run_loop`；`schedule::check_cron` / `loop_for`
- `report::into_err` / `message`；`runtime::parse_fail`
- `corex pipeline --once` 强制单次；yaml 有 watch/schedule 时自动守护
