# Progress Log

## Session: 2026-08-05

### All phases complete
- 重写 `.github/workflows/publish-release.yml`（SemVer、--target、三件套、checksum、workflow_dispatch）
- README + ipc-protocol 写明 ZIP 含 corex + corex-serve + pdfium
- 本地校验：资产名 `corex-v2.1.0-windows-x64.zip` 与 prepare.ts 一致；SemVer 正则与 workflow 关键词齐全

## Test Results
| Test | Expected | Actual | Status |
|------|----------|--------|--------|
| zip 名 vs prepare.ts | corex-v2.1.0-windows-x64.zip | match | pass |
| SemVer 样例 | ok/bad 集合正确 | pass | pass |
| workflow 含三件套/--pipe/--target | present | present | pass |

## 5-Question Reboot Check
| Question | Answer |
|----------|--------|
| Where am I? | Done |
| Where am I going? | N/A（可选：打标签重发） |
| What's the goal? | 企业级发布对齐 i-thinking 三件套 |
| What have I learned? | See findings.md |
| What have I done? | See above |
