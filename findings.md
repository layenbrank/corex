# Findings & Decisions

## Requirements
- i-thinking 需要 `corex.exe`（CLI）与 `corex-serve.exe`（Tauri sidecar）及 `pdfium.dll`
- Release 资产名保持：`corex-{tag}-windows-x64.zip`（prepare.ts 约定）
- 企业级：SemVer 门禁、渠道 prerelease、workflow_dispatch、SHA256、concurrency

## Research Findings
- 旧 workflow 只打包 `corex.exe` + `pdfium.dll`，缺少 `corex-serve.exe`
- 未传 `--target` 时二进制在 `target/release/`，pdfium 经 build.rs 写到 `target/{TARGET}/{profile}/`
- prepare.ts：解压后取 `corex-serve.exe` + `pdfium.dll`，断言 `--pipe`，禁止把 CLI 当 sidecar
- corex-capture：无 Daemon 的一次性截图；i-thinking 不依赖，不进 ZIP

## Technical Decisions
| Decision | Rationale |
|----------|-----------|
| 三件套同 ZIP | 同版本分发；资产名不变 |
| `--target` 构建 | 产物路径稳定 |
| 保留 `.zip.sha256` | prepare checksumKind: file |
| 另写 SHA256SUMS.txt | 企业级清单 |

## Resources
- `.github/workflows/publish-release.yml`
- i-thinking `apps/client/scripts/prepare.ts`
- i-thinking `.github/workflows/client-release.yaml`
