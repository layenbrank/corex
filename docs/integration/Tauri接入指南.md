# Tauri 接入指南

在 **Tauri 2** 应用中通过 **sidecar `corex-daemon`** 调用 Corex，避免在 WebView 进程链接 OCR、UIAutomation 等 native 依赖。

示例代码目录：[examples/tauri/](../../examples/tauri/)

---

## 1. 架构

```text
Tauri 应用进程                    corex-daemon（sidecar）
     │                                    │
     │  spawn sidecar                     │
     ├──────────────────────────────────►│ 监听 pipe/socket
     │                                    │ 注册 ActionRegistry
     │  NDJSON + auth_token               │
     ├──────────────────────────────────►│ run_directive / invoke
     │◄──────────────────────────────────┤ Response ok/error
     │  shutdown（应用退出时）              │
     └──────────────────────────────────►│
```

**原则：** Tauri **不要**直接依赖 `corex-engine` / `corex-registry`；只通过 IPC 发 NDJSON。

---

## 2. 前置步骤

```powershell
cargo build -p corex-daemon --release
```

将产物复制为 Tauri sidecar 约定名称，见 `examples/tauri/scripts/copy-corex-daemon.mjs`。

Token：与 daemon 共享 `COREX_TOKEN` 或 `<数据目录>/token`。

---

## 3. 需要复制的文件

| 示例文件 | 目标 |
|----------|------|
| `examples/tauri/corex_ipc.rs` | `src-tauri/src/corex_ipc.rs` |
| `examples/tauri/lib.rs` | 合并到 `src-tauri/src/lib.rs` |
| `examples/tauri/tauri.conf.json` | 合并 `bundle.externalBin` 等 |
| `examples/tauri/capabilities/default.json` | sidecar 权限 |
| `examples/tauri/Cargo.toml.snippet` | 依赖片段 |
| `examples/tauri/scripts/copy-corex-daemon.mjs` | 构建脚本 |

---

## 4. 配置要点

### `tauri.conf.json`

```json
{
  "bundle": {
    "externalBin": ["binaries/corex-daemon"]
  }
}
```

### Sidecar 启动参数（Windows 示例）

```json
{
  "name": "binaries/corex-daemon",
  "sidecar": true,
  "args": ["--socket", "\\\\.\\pipe\\corex"]
}
```

Unix 使用实际 socket 路径（与 daemon 配置一致）。

### Capabilities

允许 spawn sidecar；见 `examples/tauri/capabilities/default.json`。

---

## 5. Rust 侧调用

`corex_ipc.rs` 封装了连接、鉴权、请求/响应解析。

```rust
// 概念示例（见 examples/tauri/lib.rs）
corex_ipc::invoke_action("capture.screenshot", params)?;
corex_ipc::run_directive("hello", input_map)?;
```

协议字段见 [IPC 接入指南](./IPC接入指南.md)。

---

## 6. 常用 Action（桌面场景）

| Action | 用途 |
|--------|------|
| `capture.screenshot` | 截图 |
| `capture.ocr` | OCR |
| `ui.element.*` | Windows UI 自动化 |
| `notify.send` | 系统通知 |
| `run_directive` | 执行完整 YAML 流水线 |

完整列表：[actions.md](../actions.md)

---

## 7. 故障排查

| 问题 | 检查 |
|------|------|
| Sidecar 未启动 | Tauri shell 权限、`externalBin` 路径 |
| 401 | Tauri 与 daemon 是否同一 `token` |
| Pipe 连接失败 | Windows `\\.\pipe\corex` 是否被占用；`corex daemon status` |
| Action 不存在 | daemon 是否 `full` features 构建 |

---

## 8. 可选：UI Inspector

`examples/tauri/inspector/` 提供 WebView 调试 UI 树的原型，配合 `corex ui element tree` 使用。详见 [ui-automation.md](../ui-automation.md)。

---

## 相关文档

- [接入总览](./接入总览.md)
- [IPC 接入指南](./IPC接入指南.md)
- [ipc-protocol.md](../ipc-protocol.md)（英文完整协议）
- [examples/tauri/README.md](../../examples/tauri/README.md)
