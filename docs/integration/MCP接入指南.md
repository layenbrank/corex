# MCP 接入指南

把 corex 的内置 Action 与指令暴露成 **Model Context Protocol（MCP）** 工具，让
Cursor / Claude Desktop / VS Code Copilot 等 agent 客户端直接调用。

二进制：**`corex-mcp`**（`bins/mcp`）。用官方 Rust SDK [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk)，
支持 **stdio** 与 **Streamable HTTP** 两种传输。

---

## 1. 构建

```powershell
cargo build -p corex-mcp            # 或 --release
# 产物：target/<profile>/corex-mcp.exe（Windows）/ corex-mcp
```

与 CLI / daemon 共用同一份配置：`corex.toml`、`COREX_DATA_DIR`、`COREX_TOKEN` 全部生效。

---

## 2. 工具清单

`tools/list` 返回 **80 个内置 Action 各一个工具**，外加一个 meta-tool：

| 规则                                                        | 示例                                                                     |
| ----------------------------------------------------------- | ------------------------------------------------------------------------ |
| Action id `a.b.c` → tool 名 `a_b_c`                         | `file.copy` → `file_copy`、`codec.base64.encode` → `codec_base64_encode` |
| `title` = Action 中文名，`description` 直接取 Action 的说明 | `file_copy` →「文件复制」                                                |
| `inputSchema` = 与 `corex actions --json` 同一份            | 参数类型 / 必填 / 默认值 / 说明                                          |
| 破坏性动作打 `annotations.destructiveHint`                  | `file.remove` / `file.write` / `shell.run` / `ui.*` / `capture.*` …      |

外加 **`corex_run_directive`**：按名或路径跑一条指令（`name` / `path` / `input` 三个参数），
返回结果 JSON。它对应 `corex run`，让 agent 既可用「一个动作」粒度、也可用「一条指令」粒度。

---

## 3. 接入本地编辑器（stdio）

stdio 下客户端把 `corex-mcp` 当子进程拉起，**stdout 只放 MCP 消息，日志走 stderr**。

### VS Code Copilot（`.vscode/mcp.json`）

```json
{
  "servers": {
    "corex": {
      "type": "stdio",
      "command": "corex-mcp",
      "args": ["--directives", "examples/directives"]
    }
  }
}
```

### Cursor（`.cursor/mcp.json`）

```json
{
  "mcpServers": {
    "corex": {
      "command": "corex-mcp",
      "args": ["--directives", "examples/directives"]
    }
  }
}
```

### Claude Desktop（`claude_desktop_config.json`）

```json
{
  "mcpServers": {
    "corex": {
      "command": "corex-mcp",
      "args": ["--directives", "examples/directives"]
    }
  }
}
```

> 各客户端的配置键名（`servers` / `mcpServers`）以其当前版本为准，命令与参数不变。
> `--directives` 指向你的指令目录（默认 `<数据目录>/directives`，只在这一处按名查指令，
> 与 daemon 一致——想用仓库示例就显式指过去）。

---

## 4. 远程 / 多客户端（Streamable HTTP）

```powershell
corex-mcp --transport http --bind 127.0.0.1 --port 3000
```

单 `/mcp` 端点（POST + GET），支持会话（`Mcp-Session-Id`）与无状态模式。客户端 URL 形如
`http://127.0.0.1:3000/mcp`。

**安全默认**：只绑 `127.0.0.1`；设置环境变量 `COREX_TOKEN` 后，所有请求必须带
`Authorization: Bearer <token>`，否则 401。要对外提供服务时请自行加反代 / OAuth 网关
（完整 OAuth 2.1 资源服务器在规划中，见文末）。

> Windows 上某些端口（如 8123）落在系统「排除端口范围」里，绑定时会报
> `os error 10013`——换个高位端口（如 38123）即可。

---

## 5. 权限与审计（与 corex 完全一致）

MCP server **内嵌引擎**，执行路径与 `corex run` / daemon 相同，不是另写一套：

| 项                           | 行为                                                                   |
| ---------------------------- | ---------------------------------------------------------------------- |
| `strict_permissions`         | 生效；未声明 `permissions` 的指令被拒                                  |
| `[plugins].disabled_actions` | 生效；被禁用的动作**不出现**在 `tools/list` 里                         |
| `filesystem_roots`           | 生效；`file.*` / `exec.run` 的路径被沙箱约束                           |
| `destructiveHint`            | 破坏性动作打上注解，客户端据此在调用前弹确认（「人在环」）             |
| 审计 / 历史                  | `audit.jsonl`、`history.jsonl` 照常落盘；权限拒绝记 `denied: true`     |
| 结果                         | `structuredContent` 给结构化 JSON，`content[0]` 附一份文本（向后兼容） |

错误语义：工具执行失败 → `isError: true`（消息带 `[kind]` 前缀，如 `[permission_denied]`）；
未知工具 → JSON-RPC `-32602`。详见 [退出码与错误码](../reference/退出码与错误码.md)。

---

## 6. 与其它接入方式的取舍

| 需求                                       | 选哪个                                              |
| ------------------------------------------ | --------------------------------------------------- |
| agent 要**逐个调用**动作、要参数 schema    | **corex-mcp**（本页）                               |
| 应用要**批量**跑、要流式进度、要 WASM 插件 | daemon + IPC（[IPC 接入指南](./IPC接入指南.md)）    |
| 脚本 / CI                                  | CLI `corex run`（[快速开始](../guide/快速开始.md)） |
| Rust 同进程                                | [Rust 嵌入指南](./Rust嵌入指南.md)                  |

---

## 7. 已知限制（v1）

- **不含 WASM 插件**：内嵌引擎与 `corex run` 一样不加载插件；需要插件能力时请走 daemon（将来可加 `--remote` 桥接）。
- **长任务同步返回**：`tools/call` 是请求-响应；大文件复制这类长动作会一直等到完成。
  需要轮询时用 MCP 2026-07-28 的 Tasks 扩展（规划中）。
- **无渐进披露**：80 个工具一次性返回，目录约 100 KB；需要分组 / 懒加载时再议。

---

## 相关文档

- [接入总览](./接入总览.md) — CLI / Daemon / 嵌入 / MCP 全景
- [编辑器集成](./编辑器集成.md) — schema 补全、任务与机器出口
- [IPC 协议](../reference/IPC协议.md) — `list_actions` 目录与端点发现
- [运行时配置](../guide/运行时配置.md) — 权限与审计配置
