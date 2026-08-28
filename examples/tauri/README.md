# Tauri × corex 集成示例（v5）

将重依赖隔离在 **`corex-daemon`** 中，Tauri 仅通过 NDJSON IPC 调用。

> **中文接入指南：** [docs/integration/Tauri接入指南.md](../../docs/integration/Tauri接入指南.md)  
> **文档中心：** [docs/README.md](../../docs/README.md)

## 文件清单

| 文件 | 复制目标 |
|------|----------|
| `corex_ipc.rs` | `src-tauri/src/corex_ipc.rs` |
| `lib.rs` | `src-tauri/src/lib.rs`（或合并） |
| `tauri.conf.json` | 合并 `bundle.externalBin` 等 |
| `capabilities/default.json` | 合并 `permissions` |
| `Cargo.toml.snippet` | 合并到 `src-tauri/Cargo.toml` |
| `scripts/copy-corex-daemon.mjs` | 项目根 `scripts/` |
| `inspector/index.html` | Inspector WebView（可选） |
| `inspector.rs` | Inspector 集成说明 |

## 构建 sidecar

```bash
cargo build -p corex-daemon --release
```

Windows 产物：`target/release/corex-daemon.exe`。按 Tauri sidecar 约定复制为 `binaries/corex-daemon-<triple>.exe`（见 `scripts/copy-corex-daemon.mjs`）。

## 协议要点（v4）

- Endpoint：Windows **`\\.\pipe\corex`**；Unix 为 socket 路径（可用 `--socket` / `COREX_SOCKET`）。
- 每条请求带 **`auth_token`**（`COREX_TOKEN` 或与 daemon 共享的 `<data-dir>/token`）。
- Invoke：`{"type":"invoke","action":"capture.screenshot","params":{"to":"..."},"auth_token":"..."}`。
- 响应：`{"type":"ok"|"error"|...}`（见 `corex_ipc.rs` 中的 `Response`）。

```rust
corex_ipc::screenshot("C:/Screenshots/a.png")?;
// → invoke_action("capture.screenshot", ...)
```

## tauri.conf / capabilities

`externalBin`: `["binaries/corex-daemon"]`。capabilities 中允许 spawn sidecar；Windows 参数示例：

```json
{ "name": "binaries/corex-daemon", "sidecar": true, "args": ["--socket", "\\\\.\\pipe\\corex"] }
```

Unix 则传入实际 socket 路径。确保应用进程能读到与 daemon 相同的 token。

## 更多

托盘 + 全局快捷键 wiring 见 `lib.rs`。故障排查见 [Tauri 接入指南](../../docs/integration/Tauri接入指南.md)。
