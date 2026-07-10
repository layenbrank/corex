# Task Plan: 文档与 Rust 代码同步

## Goal

对照实际 Rust 源码修正 `docs/` 与 `README.md` 中的不准确描述，使文档与 `serve/pipe`、`schema`、CLI 子命令一致。

## Current Phase

Phase 4 — Verification

## Phases

### Phase 1: 代码审计
- [x] 对照 pipe/windows.rs、各 schema.rs、command/mod.rs
- [x] 记录差异到 findings.md
- **Status:** complete

### Phase 2: IPC / 架构文档修正
- [x] docs/ipc-protocol.md
- [x] docs/architecture.md
- [x] docs/architecture-and-tauri-integration.md
- **Status:** complete

### Phase 3: README 修正
- [x] shade、compression 子命令、binary 表
- **Status:** complete

### Phase 4: 验证
- [x] cargo test / check
- **Status:** complete

## Key Corrections

1. `handle_client` 同连接可多行 Invoke（非单次即断）
2. generate Path/File、bootstrap unit enum args 示例
3. README compression 改为 zip/unzip 子命令

## Errors Encountered

| Error | Attempt | Resolution |
|-------|---------|------------|
| （无） | — | — |
