# Progress Log

## Session: windows feature 拆分收尾

### Done
- [x] `windows` workspace：`default-features = false`，按 act-* 启用
- [x] 共享 bundle：`win32-base` / `win32-process` / `winrt-ocr`
- [x] OCR 保持 `spawn_blocking` + `.join()`（WinRT !Send）
- [x] daemon：`Write` 仅 unix 导入（修 CI warning）
- [x] 交叉编译验证：`act-ui` / `act-capture` / `act-shell+act-exec`

### Skip
- windows-sys 双轨
- OCR 跨线程 await
- forepaw / windows-registry（无现成消费方）
