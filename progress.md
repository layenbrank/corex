# Progress Log — 企业架构加固（P0–P3）

## Session
- Phase 0–4 完成
- `cargo test --workspace --locked` 全部通过
- 变更摘要：
  - enterprise.toml / enterprise-deploy 对齐并补禁 monitors/keyring/scan
  - exec.run script+cwd、shell.run cwd → confine_path
  - `corex_core::check_runtime_allowed`；daemon + ui_probe 共用
  - history 错误分类/路径打码/截断；最小构建与 CLI 信任边界文档
