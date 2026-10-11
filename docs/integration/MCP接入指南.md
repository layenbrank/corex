# MCP 接入指南

把 corex 的动作发现与指令编排暴露成 **Model Context Protocol（MCP）** 工具，供
Cursor / Claude Desktop / VS Code Copilot / goose ACP 等 Agent 调用。

二进制：**`corex-mcp`**（`bins/mcp`）。SDK：[`rmcp`](https://github.com/modelcontextprotocol/rust-sdk)。
传输：**stdio**（本地子进程）与 **Streamable HTTP**（`/mcp`）。

> **破坏性（v14）**：已删除全部摊平 action tools（如 `file_copy`）与旧名
> `corex_run_directive`。`tools/list` **仅**返回下表 7 个 meta-tool。详见
> [破坏性变更 v14](../changelog/破坏性变更-v14.md)。

---

## 1. 构建

```powershell
cargo build -p corex-mcp            # 或 --release
# 产物：target/<profile>/corex-mcp.exe（Windows）/ corex-mcp
```

与 CLI / daemon 共用：`corex.toml`、`COREX_DATA_DIR`；HTTP 另认 `COREX_TOKEN`。

---

## 2. 工具面（唯一）

| 工具               | 职责                                            |
| ------------------ | ----------------------------------------------- |
| `corex_actions`    | 动作目录摘要（`bucket` / `ids` / `limit`）      |
| `corex_action`     | 单条：完整 `input_schema` + permissions + risk  |
| `corex_invoke`     | 执行单个动作：`id` + `params`                   |
| `corex_directives` | 指令库摘要（`names` / `limit`）                 |
| `corex_directive`  | 单条：`yaml` + `definition`                     |
| `corex_validate`   | 校验 definition（`strict?`）                    |
| `corex_run`        | 跑指令：`name` **或** `path`（oneOf）+ `input?` |

### 推荐编排

动作：`corex_actions` → `corex_action` → `corex_invoke`

指令：`corex_directives` → `corex_directive` → `corex_validate` → `corex_run`

Instructions 已声明：**同步阻塞**直至完成（Tasks / Cancel 为后续能力）。

---

## 3. 配置 `[mcp]`

```toml
[mcp]
# 空 = 不额外白名单（仍尊重 disabled_actions / strict_permissions）
allow_actions = []
allow_directives = []
# true：空白名单即拒 invoke / 按名 run；并禁止 corex_run 的 path 旁路
is_deny_default = false
```

| 约束                                      | 行为                                       |
| ----------------------------------------- | ------------------------------------------ |
| `allow_actions` / `allow_directives`      | 非空时 list 与 call 两侧强制               |
| `is_deny_default`                         | 空 allow = 拒执行；`path` 禁用             |
| `filesystem_roots`                        | 非 deny-default 时，`path` 须落在 roots 内 |
| `strict_permissions` / `disabled_actions` | 与 CLI 相同；目录亦不暴露                  |

`risk` / MCP `destructiveHint` **只是提示**，不是门禁。

---

## 4. 错误契约

工具失败（`isError: true`）：

```json
{
  "structuredContent": {
    "code": "policy_denied",
    "message": "…",
    "retryable": false
  },
  "content": [{ "type": "text", "text": "…" }]
}
```

`content[0].text` 与 `structuredContent.message` **同文**。`code` 为稳定子集：
`unknown_tool` / `invalid_params` / `policy_denied` / `permission_denied` /
`not_found` / `timeout` / `disabled` / `not_registered` / `parse` / `resolve` /
`control_flow` / `config` / `usage` / `conflict` / `io` / `execution`。

未知工具 → JSON-RPC `-32602`（附 `code: unknown_tool`）。

审计 `audit.jsonl` 额外字段：`tool`、`action_or_directive`、`session`
（可用环境变量 `COREX_MCP_SESSION` 标注 Agent 会话）。

---

## 5. 部署矩阵：stdio vs HTTP

|          | **stdio**                                               | **HTTP**（`--transport http`）                   |
| -------- | ------------------------------------------------------- | ------------------------------------------------ |
| 典型     | 编辑器 / ACP 子进程                                     | 本机多客户端、网关后                             |
| 信任边界 | **OS 用户 / 进程**；能起进程即等价本机权限              | 默认绑 `127.0.0.1`；`COREX_TOKEN` → Bearer       |
| Token    | **不**走 HTTP 鉴权；设 `COREX_TOKEN` **不会**限制 stdio | 未设 token = 仅环回信任；生产务必设 token + 反代 |
| 配置     | `[mcp]` / `strict_permissions` / roots 仍生效           | 同左                                             |

禁止误读：「设了 `COREX_TOKEN` 就等于 stdio 也受限」——不会。stdio 的门禁是
`[mcp]` 与 runtime 权限，不是 Bearer。

```powershell
# HTTP 示例
$env:COREX_TOKEN = "…"
corex-mcp --transport http --bind 127.0.0.1 --port 3000
# 客户端：http://127.0.0.1:3000/mcp  + Authorization: Bearer …
```

> Windows 部分端口在系统排除范围会报 `os error 10013`，换高位端口即可。

---

## 6. 接入本地编辑器（stdio）

### Cursor（`.cursor/mcp.json`）

```json
{
  "mcpServers": {
    "corex": {
      "command": "corex-mcp",
      "args": ["--import", "examples/directives"],
      "env": {
        "COREX_DATA_DIR": "C:/path/to/data",
        "COREX_MCP_SESSION": "cursor"
      }
    }
  }
}
```

### VS Code Copilot（`.vscode/mcp.json`）

```json
{
  "servers": {
    "corex": {
      "type": "stdio",
      "command": "corex-mcp",
      "args": ["--import", "examples/directives"]
    }
  }
}
```

`--import`：启动时把目录/文件 YAML **导入一次**指令库（等价 `corex directive import`），
不是换指令根。旧别名 `--directives` 仍可用。

---

## 7. 与其它接入方式

| 需求                       | 选哪个                |
| -------------------------- | --------------------- |
| Agent 渐进披露 + 编排执行  | **corex-mcp**（本页） |
| 应用批量跑、流式进度、WASM | daemon + IPC          |
| 脚本 / CI                  | `corex run`           |
| Rust 同进程                | Rust 嵌入指南         |

---

## 8. 已知限制

- **无 WASM 插件**（与 `corex run` 内嵌引擎一致）。
- **长任务同步**：无 Tasks / Cancel（规划中）。
- **无 Resources / Prompts / listChanged 推送**。

---

## 相关文档

- [接入总览](./接入总览.md)
- [破坏性变更 v14](../changelog/破坏性变更-v14.md)
- [运行时配置](../guide/运行时配置.md)
- [退出码与错误码](../reference/退出码与错误码.md)
