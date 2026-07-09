# Corex IPC 协议参考

本文档定义 `corex-serve` Daemon 与客户端（Tauri `corex_ipc.rs`、库 `serve::request`）之间的通信契约。

总览请参阅 [architecture-and-tauri-integration.md](./architecture-and-tauri-integration.md)。

---

## 传输层

| 项 | 值 |
|----|-----|
| 平台 | Windows（当前唯一支持） |
| 机制 | Named Pipe |
| 默认路径 | `\\.\pipe\corex` |
| 编码 | UTF-8 JSON |
| 帧格式 | 单行 JSON + `\n`（LF）换行 |
| 请求行上限 | 64 KB（`MAX_LINE_BYTES`） |
| 连接模式 | 单连接单次请求-响应，处理后断开 |

非 Windows 平台：`serve::run` 与 Pipe 客户端均不可用。

---

## 消息类型

### Invoke（执行业务模块）

**Typed 格式（推荐）：**

```json
{"type":"invoke","id":1,"module":"screenshot","args":{"to":"C:/Screenshots"}}
```

**Legacy 简写（兼容）：**

```json
{"id":1,"module":"screenshot","args":{"to":"C:/Screenshots"}}
```

字段说明：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| type | string | typed 时必填 | 固定 `"invoke"` |
| id | u64 | 是 | 请求 ID，响应原样返回 |
| module | string | 是 | 模块名，见下表 |
| args | object | 是 | 对应模块 `Args` 的 JSON |

### Shutdown（关闭 Daemon）

**Typed 格式：**

```json
{"type":"shutdown"}
```

**Legacy 格式：**

```json
{"cmd":"shutdown"}
```

收到 Shutdown 后，Daemon 处理完当前连接即退出主循环。

---

## 响应格式

所有 Invoke 请求返回单行 JSON：

```json
{"id":1,"ok":true,"path":"C:/Screenshots/screenshot-Primary-1234567890.png","ms":87}
```

失败时：

```json
{"id":1,"ok":false,"ms":12,"error":"screenshot args 解析失败: missing field `to`"}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| id | u64 | 与请求 id 一致 |
| ok | bool | 是否成功 |
| path | string? | 成功时可选输出路径 |
| ms | u64 | 处理耗时（毫秒） |
| error | string? | 失败时错误信息 |

Rust 类型定义见 `corex-core/src/serve/protocol.rs` 的 `Response` 结构体。

---

## 支持的 module 与 args

args 字段与各模块 `schema::Args` 的 serde 结构一致，可直接对照 CLI 参数。

### screenshot

```json
{
  "module": "screenshot",
  "args": {
    "to": "C:/Screenshots",
    "description": "可选描述"
  }
}
```

成功时 `path` 为生成的 PNG 文件路径。

### copy

```json
{
  "module": "copy",
  "args": {
    "from": "C:/src",
    "to": "C:/dist",
    "empty": true,
    "includes": [],
    "excludes": ["node_modules", "*.log"]
  }
}
```

成功时 `path` 为 `args.to`。

### scrub

```json
{
  "module": "scrub",
  "args": {
    "source": "C:/project",
    "target": "node_modules",
    "recursive": true
  }
}
```

成功时 `path` 为 `args.target`（名称，非完整路径）。

### shade

```json
{
  "module": "shade",
  "args": {
    "from": "C:/images",
    "to": "C:/output",
    "format": "webp"
  }
}
```

字段以 `shade/schema.rs` 为准。

### compression

args 为 tagged enum，与 CLI 子命令对应：

**Zip：**

```json
{
  "module": "compression",
  "args": {
    "Zip": {
      "from": "C:/project",
      "to": "C:/out/project.zip"
    }
  }
}
```

**Unzip：**

```json
{
  "module": "compression",
  "args": {
    "Unzip": {
      "from": "C:/in/project.zip",
      "to": "C:/out"
    }
  }
}
```

### generate

**Path：**

```json
{
  "module": "generate",
  "args": {
    "Path": {
      "from": "C:/scan",
      "to": "C:/paths.txt"
    }
  }
}
```

**Uuid：**

```json
{
  "module": "generate",
  "args": {
    "Uuid": {
      "count": 5,
      "uppercase": false
    }
  }
}
```

**File：**

```json
{
  "module": "generate",
  "args": {
    "File": {
      "from": "C:/template.hbs",
      "to": "C:/out/file.txt"
    }
  }
}
```

Uuid 成功时 `path` 为 null。

### bootstrap

```json
{
  "module": "bootstrap",
  "args": {
    "Env": {}
  }
}
```

子命令结构见 `bootstrap/schema.rs`（Env / Inspect / Force）。

---

## 并发与错误语义

- Daemon **串行**处理连接：一个 Pipe 连接处理完再接受下一个
- 单连接内只处理**第一行** JSON 请求，然后返回响应并断开
- 空行或非法 JSON：返回错误响应或跳过（见 `handle_client` 实现）
- 未知 module：返回 `ok: false`，error 含 `"未知或未启用的模块"`
- args 解析失败：返回 `ok: false`，error 含 serde 上下文

---

## 客户端实现

### 库 API（corex-core）

```rust
use cx::serve;

// 调用模块
let resp = serve::request(
    r"\\.\pipe\corex",
    "screenshot",
    serde_json::json!({ "to": "C:/out" }),
)?;

// 关闭 Daemon
serve::shutdown(r"\\.\pipe\corex")?;
```

### 最小验证示例

`corex-core/examples/ipc.rs`：

```powershell
# 终端 1
cargo run -p corex-serve

# 终端 2
cargo run -p corex-core --example ipc --features serve -- C:\Temp\screenshots
```

### Tauri 侧（独立实现）

不依赖 `corex-core`，见 `examples/tauri/corex_ipc.rs`：

- `invoke(module, args)` — 通用调用
- `screenshot(to)` — 截图快捷方法
- `is_ready()` — 探测 Pipe 是否可连接
- `shutdown()` — 发送 shutdown

完整 Tauri 接入见 [tauri-integration.md](./tauri-integration.md)。

---

## 协议演进

| 版本 | 变更 |
|------|------|
| 当前 | typed + legacy 双格式；64KB 行限；Windows Named Pipe |

未来可能扩展：

- Unix Domain Socket（非 Windows）
- 长连接复用（当前每请求新连接）
- pipeline/schedule 模块（当前仅 CLI）

---

## 调试技巧

### 手动发送请求（PowerShell）

Named Pipe 不适合直接用 echo，建议使用 example 或 Tauri 客户端。

### 查看 Daemon 日志

Daemon 将业务日志写入 stderr，例如：

```
corex-serve: 已缓存 2 个显示器
corex-serve: 监听 Named Pipe \\.\pipe\corex
screenshot saved: C:\out\screenshot-....png (87ms)
```

### 单元测试

```powershell
cargo test -p corex-core --features serve -- protocol::
```

覆盖 `parse_request` 的 typed/legacy/empty/invalid 场景。
