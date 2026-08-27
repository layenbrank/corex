# Progress Log

## Session: Code Review 修复

### Done
- [x] 门禁：`plugins.disabled` + `strict_permissions` + 独立 action_id
- [x] Win11 desktop WorkerW / SHELLDLL_DefView fallback
- [x] pick scope 外 stderr 提示；`element get --class`；tree bounds；redact automation_id
- [x] enterprise.toml / docs / Windows scope 测试去 ignore
- [x] 单测扩展

### Validate
```bash
cargo test --workspace
```

### Deferred
- pick USERDATA 生命周期重构
- Windows 实机：desktop icons / pick 非终端区域
