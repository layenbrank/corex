# Task Plan: corex 工程化落地（template 风格）

## Goal
引入 tyr-rust-bootcamp/template 风格工具链：pre-commit、cargo-deny、typos、git-cliff；按 corex workspace 适配，不改业务逻辑。

## Next Step
中文汇报交付。

## Current Phase
Phase 4

## Phases

### Phase 1: 调研现有 CI/hooks
- **Status:** complete

### Phase 2: 拉取模板并适配配置
- [x] `.pre-commit-config.yaml`
- [x] `deny.toml`
- [x] `_typos.toml`
- [x] `cliff.toml`
- [x] README 无「开发环境」→ 不扩写
- **Status:** complete

### Phase 3: 安装工具并验证
- [x] cargo-deny / typos / git-cliff / pre-commit 已装
- [x] `cargo deny check` 通过
- [x] typos / cliff 通过
- [x] pre-commit install；typos+deny hooks 通过；fmt 既有失败保留
- **Status:** complete

### Phase 4: 中文汇报
- [x] 文件清单、启用方式、验证结果
- **Status:** complete

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| 不加 black | 非 Python |
| system language hooks | Windows 无 bash |
| deny 仅 Windows target | 避开 Linux-only quick-xml CVE 噪音 |
| ignore serde_yml advisory | 迁栈非本任务 |
| 不批量 cargo fmt | 避免无关大 diff |
| 不改 CI / README | 任务范围 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| pip externally-managed | 1 | `uv tool install pre-commit` |
| BSL-1.0 / display-info | 1 | allow + clarify |
| quick-xml advisories | 1 | graph 限 Windows |
| BOM in `.cursor/hooks` | 1 | exclude `.cursor/` |
| cargo fmt drift | 1 | 保留 hook，不强制重排 |
