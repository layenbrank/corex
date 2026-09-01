# Findings: 工程化落地

## 本仓库现状（调研）
| 项 | 状态 |
|----|------|
| `.pre-commit-config.yaml` | 无 → 已新增 |
| `deny.toml` / `_typos.toml` / `cliff.toml` | 无 → 已新增 |
| Git hooks | 仅 sample → `pre-commit install` 已写入 `.git/hooks/pre-commit` |
| CI | `build-and-test.yml`、`publish-release.yml`；未改 CI |
| README「开发环境」 | **无** → 未扩写文档 |
| origin | `https://github.com/layenbrank/corex` |

## 模板差异（已适配）
| 模板项 | corex 决策 |
|--------|------------|
| `psf/black` | 跳过 |
| `bash -c` | Windows 用 `language: system` + 直接 entry |
| nextest / 重 clippy+test | 不进默认 commit；`cargo check` 放 `pre-push` |
| deny graph | 仅 `x86_64-pc-windows-msvc`（避开 screenshots→wayland→quick-xml 的 Linux-only advisory） |
| `BSL-1.0` | clipboard-win / error-code 需要，已 allow |
| `display-info` | clarify Apache-2.0 + LICENSE hash `0xa6d4ed6` |
| `RUSTSEC-2025-0068` (serde_yml) | ignore + reason（迁栈属业务变更） |
| cliff `$REPO` | `https://github.com/layenbrank/corex` |

## 验证结果
| 命令 | 结果 |
|------|------|
| `cargo deny check` | ✅ advisories/bans/licenses/sources ok（yanked chacha20 为 warn） |
| `typos` | ✅ |
| `git-cliff -l` | ✅ 可生成 unreleased/latest |
| `pre-commit run typos/cargo-deny` | ✅ |
| `pre-commit run cargo-fmt` | ❌ 仓库既有 rustfmt 漂移（未批量 fmt，避免大 diff） |
| BOM on `.cursor/hooks/*.ps1` | 已用顶层 `exclude` 忽略 |

## 工具安装（本机）
- `cargo install cargo-deny typos-cli git-cliff --locked`
- `uv tool install pre-commit`（pip 因 externally-managed 失败）
