# Progress Log

## 2026-09-16 — Windows 中文乱码根因

### Done
- 历史：`6453571` SetConsoleOutputCP、`4534a72` doctor —— 只修 CLI 直连控制台
- 根因：`process_launch::pump_process_stream` 对管道 OEM/GBK 用 `from_utf8_lossy`，且实时回显写原始字节
- 修复：UTF-8→OEM/GBK 回退解码；回显写 UTF-8；cmd `chcp 65001`；PS OutputEncoding；CLI 同时 SetConsoleCP
- 单测：`process_launch` 13 通过；文档已补

### Next
- 提交、推送、开 PR
