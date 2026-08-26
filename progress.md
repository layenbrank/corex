# Progress Log

## Session: UI Inspector 企业级实现

### Done
- [x] 嵌套 CLI：`corex ui window list/desktop`、`element tree/get/point/pick`
- [x] scope 硬约束 + desktop 独立 probe
- [x] ancestors、tree format、class selector 回退
- [x] pick 全局鼠标检测（删 overlay 点击路径）
- [x] disabled_actions + audit
- [x] Tauri inspector 骨架 + IPC helpers
- [x] docs + 单测 91 passed

### Validate
```bash
cargo test --workspace
```

### Windows 实机（运维）
- [ ] element tree 必须 scope
- [ ] window desktop vs element tree 不混淆
- [ ] pick 非终端区域可点
- [ ] enterprise.toml 禁用 ui.* 时 CLI probe 拒绝
