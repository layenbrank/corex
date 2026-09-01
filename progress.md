# Progress Log

## 2026-09-01 — Engineering toolchain (template-style)

### Done
- 新增：`.pre-commit-config.yaml`、`deny.toml`、`_typos.toml`、`cliff.toml`
- 安装：`cargo-deny` 0.20.2、`typos` 1.50.0、`git-cliff` 2.13.1、`pre-commit` 4.6.2（uv tool）
- `pre-commit install` → `.git/hooks/pre-commit`
- 验证：`cargo deny check` ✅；`typos` ✅；`git-cliff -l` ✅；hooks typos/deny ✅
- 未改：业务代码、CI workflow、README（无开发环境章节）

### Known
- `pre-commit run cargo-fmt` 当前因仓库既有格式漂移失败；需单独 `cargo fmt` 后再启用严格门禁
- `chacha20@0.10.1` yanked：warn only
