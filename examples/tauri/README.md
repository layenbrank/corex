# Tauri × corex 集成示例（阶段 4）

将 corex 重依赖隔离在 `corex-serve` Daemon 中，Tauri 仅通过 Named Pipe 发送 JSON 请求。

> **完整文档：** [docs/tauri-integration.md](../../docs/tauri-integration.md)  
> **架构总览：** [docs/architecture-and-tauri-integration.md](../../docs/architecture-and-tauri-integration.md)  
> **IPC 协议：** [docs/ipc-protocol.md](../../docs/ipc-protocol.md)

## 文件清单

| 文件                           | 复制目标                               |
| ------------------------------ | -------------------------------------- |
| `corex_ipc.rs`                 | `src-tauri/src/corex_ipc.rs`           |
| `lib.rs`                       | `src-tauri/src/lib.rs`（或合并）       |
| `tauri.conf.json`              | 合并 `bundle.externalBin` 等到现有配置 |
| `capabilities/default.json`    | 合并 `permissions` 到现有 capability   |
| `Cargo.toml.snippet`           | 合并到 `src-tauri/Cargo.toml`          |
| `scripts/copy-corex-serve.mjs` | 项目根 `scripts/`                      |

## 1. 构建 corex-serve

在 corex 仓库：

```powershell
cargo build -p corex-serve --release
# 产物: target/release/corex-serve.exe
```

## 2. Sidecar 目录结构

Tauri 要求 sidecar 按 **target triple** 命名：

```
src-tauri/
  binaries/
    corex-serve-x86_64-pc-windows-msvc.exe   # Windows
```

自动复制（在 Tauri 项目根目录）：

```powershell
# 设置 corex-serve 路径（可选）
$env:COREX_SERVE = "C:\path\to\corex\target\release\corex-serve.exe"
node scripts/copy-corex-serve.mjs
```

或在 `tauri.conf.json` 的 `beforeBuildCommand` / 开发前手动执行上述脚本。

## 3. tauri.conf.json 关键片段

```json
{
	"build": {
		"beforeBuildCommand": "node scripts/copy-corex-serve.mjs"
	},
	"bundle": {
		"externalBin": ["binaries/corex-serve"]
	}
}
```

> `externalBin` 填的是**逻辑名**（不含 triple），Tauri 运行时自动解析为 `corex-serve-{target-triple}.exe`。

## 4. capabilities 权限

必须允许 sidecar spawn/kill，并注册全局快捷键：

```json
{
	"permissions": [
		"global-shortcut:allow-register",
		{
			"identifier": "shell:allow-spawn",
			"allow": [
				{
					"name": "binaries/corex-serve",
					"sidecar": true,
					"args": ["--pipe", "\\\\.\\pipe\\corex"]
				}
			]
		}
	]
}
```

完整示例见 `capabilities/default.json`。

## 5. 运行流程

```
应用启动
  └─ setup: spawn sidecar (corex-serve --pipe \\.\pipe\corex)
  └─ 等待 Pipe 就绪
  └─ 创建托盘 + 注册 Ctrl+Shift+S

用户按快捷键 / 点托盘「截图」
  └─ corex_ipc::screenshot(dir)
  └─ Named Pipe JSON → corex-serve → 返回 path
  └─ emit("screenshot-done", path) 给前端

应用退出
  └─ corex_ipc::shutdown()  →  {"type":"shutdown"}
```

## 6. 前端监听（可选）

```typescript
import { listen } from '@tauri-apps/api/event'

await listen<string>('screenshot-done', (e) => {
	console.log('saved:', e.payload)
})

await listen<string>('screenshot-error', (e) => {
	console.error(e.payload)
})
```

## 7. 开发调试

不打包 sidecar 时，可手动先启动 Daemon：

```powershell
# 终端 1
cargo run -p corex-serve

# 终端 2 — 验证 IPC
cargo run -p corex-core --example ipc --features serve -- C:\Temp\screenshots

# 终端 3 — Tauri dev（此时 lib.rs 中 spawn 会失败，但 Pipe 已存在仍可截图）
pnpm tauri dev
```

## 8. 自定义

- **截图目录**：修改 `lib.rs` 中 `SCREENSHOT_DIR`，或通过 `set_screenshot_dir` command 持久化到 `tauri-plugin-store`
- **快捷键**：修改 `register_hotkeys` 中的 `"Ctrl+Shift+S"`
- **仅托盘无窗口**：`tauri.conf.json` 中 `"visible": false`，关闭窗口时 `prevent_close` 改为 hide（见 Tauri 文档）

## 9. 通用 IPC 调用（codec / scan / morph / screenshot）

`corex_ipc::invoke` 发送与 CLI 同构的 JSON args；结构化结果在响应 `data` 字段，文件路径在 `path` 字段。

```rust
use crate::corex_ipc;

// 系统信息
let resp = corex_ipc::invoke("scan", serde_json::json!({ "Os": {} }))?;
let os_ctx = resp.data.unwrap();

// Base64 编码
let resp = corex_ipc::invoke(
    "codec",
    serde_json::json!({
        "Encode": {
            "scheme": { "Base64": { "input": "hello" } }
        }
    }),
)?;
let text = resp.data.unwrap()["text"].as_str().unwrap();

// PDF 合并
let resp = corex_ipc::invoke(
    "morph",
    serde_json::json!({
        "Merge": {
            "paths": ["C:/a.pdf", "C:/b.pdf"],
            "dest": "C:/out.pdf"
        }
    }),
)?;
let merged = resp.path.unwrap();

// 枚举窗口
let resp = corex_ipc::invoke("screenshot", serde_json::json!({ "Windows": null }))?;
let windows = resp.data.unwrap();
```

> `morph` 依赖与 sidecar 同目录的 `pdfium.dll`（`copy-corex-serve.mjs` 会自动复制）。详见 [ipc-protocol.md](../../docs/ipc-protocol.md)。
