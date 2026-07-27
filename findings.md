# Findings & Decisions

## Requirements
- 修复 build-and-test：`probe_after_flood` 失败
- 修复 publish-release：`cargo build --release --locked` 因锁文件过期失败
- 不改 workflow；不去掉 `--locked`

## Research Findings
- `notify_flood_probe.rs` 硬编码 `C:\Users\iwell\Documents\Vue2\front\master\app`，CI 上不存在
- 标签 `v2.1.0` → `0480587`：只改 Cargo.toml 版本到 2.1.0，Cargo.lock 中 corex 仍为 2.0.7
- 提交 `55dfa9a` 已同步 Cargo.lock（含 2.1.0），但不在标签上

## Technical Decisions
| Decision | Rationale |
|----------|-----------|
| 删除 `notify_flood_probe.rs` | 临时本地探测 |
| 移动 `v2.1.0` 到修复后提交 | 重跑 publish-release |

## Resources
- `.github/workflows/build-and-test.yml`
- `.github/workflows/publish-release.yml`
- `corex-core/tests/notify_flood_probe.rs`
- `corex-core/tests/watch_smoke.rs`
