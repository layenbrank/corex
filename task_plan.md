# Task Plan: 修复 Corex 企业级发布（对齐 i-thinking）

## Goal
修复 publish-release：ZIP 同时包含 corex.exe + corex-serve.exe + pdfium.dll，并用 --target 对齐产物路径，补齐 SemVer 门禁与 checksum。

## Next Step
Done — 等待用户打标签或 workflow_dispatch 触发发布

## Current Phase
complete

## Phases

### Phase 1: Planning files
- **Status:** complete

### Phase 2: Rewrite publish-release.yml
- **Status:** complete

### Phase 3: Docs
- **Status:** complete

### Phase 4: Verify
- **Status:** complete

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| ZIP 含 corex + corex-serve + pdfium | i-thinking 两者都需要 |
| 不打包 corex-capture | 常驻极速由 serve 承担 |
| 显式 --target | 与 pdfium build.rs 路径一致 |
| 不做 updater-* | 消费方钉死版本 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
|         |         |            |
