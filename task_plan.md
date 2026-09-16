# Task Plan: Windows 中文输出乱码根因修复

## Goal
定位并修复 Windows 上仍出现的中文乱码（尤其是「有些内容」），针对真正根因下手，而不是重复无效的控制台代码页补丁。

## Next Step
提交并推送，创建 PR。

## Current Phase
Phase 3

## Phases

### Phase 1: 历史与根因调研
- [x] 检索乱码/UTF-8/编码相关提交
- [x] 对照 `use_utf8_console` / doctor / 文档
- [x] 定位 `shell.run`/`exec.run` 的 `from_utf8_lossy` 管道解码
- [x] 对照业界做法（lime-rs UTF-8→GBK、long-shell GetConsoleOutputCP）
- **Status:** complete

### Phase 2: 实现针对性修复
- [x] `process_launch`：UTF-8 优先，失败回退 OEM/ACP/GBK
- [x] 实时回显改为解码后的 UTF-8
- [x] `act-shell`/`act-exec` 拉上 `encoding_rs`
- [x] cmd/powershell 宿主尽量发出 UTF-8
- [x] 文档补充「子进程管道」场景
- **Status:** complete

### Phase 3: 测试与提交
- [x] 单元测试：GBK / UTF-8 / partial / code page
- [x] `cargo test -p corex-registry process_launch`
- [ ] 提交、推送、开 PR
- **Status:** in_progress

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| 不重做 SetConsoleOutputCP 为主修 | 6453571 已做；「有些内容」仍乱来自管道子进程 |
| 修 process_launch 解码 | 中文 Windows OEM(CP936/GBK) + `from_utf8_lossy` |
| UTF-8 优先再回退 OEM/GBK | 兼容已 UTF-8 的 pwsh / 系统 Beta UTF-8 |
| 用 GetOEMCP 而非 GetConsoleOutputCP | 父进程已 65001 时 Console CP 会误导管道解码 |
| 不引 codepage crate | 手写常见 CP→encoding_rs 映射 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| cargo 1.83 不支持 edition2024 | 1 | rustup install / default 1.95.0 |
