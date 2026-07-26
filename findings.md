# Findings & Decisions

## Requirements
- README：命令一览、generate cvid、engine、Pipeline module 表
- docs：corex.task.schema.README、pipeline-v3、breaking-changes、architecture-and-tauri-integration
- 不改 architecture.md / ipc-protocol.md

## Research Findings
- architecture.md / ipc-protocol.md 已覆盖 engine 与 generate cvid
- README 命令表与 Pipeline 表仍缺 engine / cvid

## Technical Decisions
| Decision | Rationale |
|----------|-----------|
| engine 节放在 scan 之后 | 同为 JSON data 输出模块 |
| cvid 节放在 uuid 之后 | 同属 generate 子命令 |
