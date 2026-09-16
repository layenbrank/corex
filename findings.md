# Findings: Windows 中文乱码

## 相关历史提交

| Commit | 说明 | 效果边界 |
|--------|------|----------|
| `6453571` | CLI 启动 `SetConsoleOutputCP(65001)`；文档写 PowerShell 管道要设 OutputEncoding | 只覆盖 **corex 自己**写到直连控制台的文本；不管管道子进程 |
| `4534a72` | `corex doctor` 报 `GetConsoleOutputCP` | 诊断用；65001 仍可能「有些内容」乱码 |
| `7c554f9` | 文档补 doctor 编码行 | 同上 |
| `bbc884e` 等 | `http.send` 的 GBK/`encoding` | 网页响应，不是控制台/shell |

## 根因（Windows）

`crates/registry/src/builtin/process_launch.rs` 的 `pump_process_stream`：

1. `shell.run` / `exec.run` **把子进程 stdout/stderr 接到管道**（不是控制台）。
2. 中文 Windows 上 `cmd`、传统控制台程序往管道写的是 **OEM 代码页**（常见 CP936 = GBK），不是 UTF-8。
3. 收集结果时用 `String::from_utf8_lossy` → 非法 UTF-8 变 `` 或损坏。
4. 实时回显 `write_all(chunk)` 把 **GBK 原始字节**写进父进程 stdout：Rust 在真控制台上走 `WriteConsoleW`，要求合法 UTF-8，失败被 `let _ =` 吃掉 → 现场也乱/缺字。

先前 `SetConsoleOutputCP`「没效果」符合预期：它不改变管道字节编码，且 Rust 自有输出本来就不靠代码页。

## 业界对照

- **lime-rs**：先严格 UTF-8，失败再 `encoding_rs::GBK`；并对 cmd/`powershell` 包一层逼 UTF-8。
- **long-shell / shell-engine**：`GetConsoleOutputCP` + `codepage`；流式用有状态 decoder。
- 对本仓库：父进程已设 65001，**不能用 GetConsoleOutputCP 解子进程管道**；应用 **GetOEMCP**（必要时再 ACP）。

## 次要路径（已有文档，非本次主修）

PowerShell cmdlet 管道（`| Out-File`）由读方解码 → 文档已有 `[Console]::OutputEncoding = UTF8`。
