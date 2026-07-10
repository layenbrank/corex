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
| 连接模式 | 服务端同连接可多行 Invoke；官方客户端每请求新建连接 |

非 Windows 平台：`serve::run` 与 Pipe 客户端均不可用（`pipe/mod.rs` 返回 bail）。

---

## 消息类型

### Invoke（执行业务模块）

```json
{"type":"invoke","id":1,"module":"screenshot","args":{"Capture":{"to":"C:/Screenshots"}}}
```

字段说明：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| type | string | 是 | 固定 `"invoke"` |
| id | u64 | 是 | 请求 ID，响应原样返回 |
| module | string | 是 | 模块名，见下表 |
| args | object | 是 | 对应模块 `Args` 的 JSON |

### Shutdown（关闭 Daemon）

```json
{"type":"shutdown"}
```

收到 Shutdown 后，Daemon **不**返回响应，处理完当前连接后退出主循环。

---

## 响应格式

所有 Invoke 请求返回单行 JSON：

```json
{"id":1,"ok":true,"path":"C:/Screenshots/screenshot-Primary-1234567890.png","ms":87}
```

带结构化数据时（如 scan、codec、morph meta）：

```json
{"id":2,"ok":true,"data":{"text":"aGVsbG8="},"ms":1}
```

失败时：

```json
{"id":1,"ok":false,"ms":12,"error":"screenshot args 解析失败: missing field `to`"}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| id | u64 | 与请求 id 一致 |
| ok | bool | 是否成功 |
| path | string? | 成功时可选输出路径（写文件类操作） |
| data | object? | 成功时可选结构化 JSON（文本、列表、元数据等） |
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
    "Capture": {
      "to": "C:/Screenshots",
      "description": "可选描述"
    }
  }
}
```

**Monitors / Windows：**

```json
{ "module": "screenshot", "args": { "Monitors": null } }
{ "module": "screenshot", "args": { "Windows": null } }
```

成功时 `data` 为 `MonitorInfo[]` 或 `WindowInfo[]`；Capture 成功时 `path` 为生成的 PNG 文件路径。

**Crop / Clipboard：**

```json
{
  "module": "screenshot",
  "args": {
    "Crop": {
      "source": "C:/in.png",
      "to": "C:/out",
      "x": 0,
      "y": 0,
      "w": 100,
      "h": 100
    }
  }
}
```

IPC 大图裁剪推荐使用 `image_file`（PNG 文件路径）而非 `final_image_base64`，避免超过 64KB 行限。`Crop.to` 与 `Capture.to` 相同，均为**输出目录**。

Crop 成功时 `path` 为输出 PNG 路径；Clipboard 无 `path`，仅 `ok: true`。

### codec

args 为三层 enum：操作 → 算法 → 参数。算法名必填，不可省略。

**Base64 编码：**

```json
{
  "module": "codec",
  "args": {
    "Encode": {
      "scheme": {
        "Base64": { "input": "hello" }
      }
    }
  }
}
```

**Base64 解码：**

```json
{
  "module": "codec",
  "args": {
    "Decode": {
      "scheme": {
        "Base64": { "input": "aGVsbG8=" }
      }
    }
  }
}
```

**MD5 摘要：**

```json
{
  "module": "codec",
  "args": {
    "Hash": {
      "scheme": {
        "Md5": { "input": "hello" }
      }
    }
  }
}
```

成功时 `data` 为 `{"text":"..."}`（Base64 字符串或小写 hex MD5）。

### scan

```json
{ "module": "scan", "args": { "Os": {} } }
```

成功时 `data` 为 `OsContext` JSON（OS、CPU、内存等），CLI 同样输出 JSON 到 stdout。

### morph

PDF 操作需系统安装 Pdfium 动态库。args 为 tagged enum，与 CLI 子命令对应。

**Meta：**

```json
{
  "module": "morph",
  "args": {
    "Meta": { "path": "C:/report.pdf" }
  }
}
```

**Merge：**

```json
{
  "module": "morph",
  "args": {
    "Merge": {
      "paths": ["C:/a.pdf", "C:/b.pdf"],
      "dest": "C:/out.pdf"
    }
  }
}
```

| 子命令 | IPC variant | 主要输出 |
|--------|-------------|----------|
| meta | `Meta` | `data`（PDF 元数据） |
| render-page | `RenderPage` | `data`（base64 PNG） |
| render-thumbnails | `RenderThumbnails` | `data`（base64 PNG 数组） |
| search | `Search` | `data`（匹配列表） |
| export / merge / to-office | `Export` / `Merge` / `ToOffice` | `path` |
| split / split-by-count / to-images | `Split` / `SplitByCount` / `ToImages` | `data` |

完整字段见 `morph/schema.rs`。

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
    "format": "webp",
    "quality": 100
  }
}
```

`quality` 默认 100，仅对 jpg 有效。完整字段见 `shade/schema.rs`。

### compression

args 为 `Compress` / `Decompress` + `scheme.{Zip|TarGz|SevenZ}`，与 CLI 同构。

**Zip 压缩（wgt = Zip + `.wgt` 扩展名）：**

```json
{
  "module": "compression",
  "args": {
    "Compress": {
      "scheme": {
        "Zip": {
          "from": "C:/project",
          "to": "C:/out/app.wgt",
          "level": 6,
          "method": "deflated",
          "encryption": "aes256",
          "password": "secret",
          "excludes": ["*.map"]
        }
      }
    }
  }
}
```

**Zip 解压：**

```json
{
  "module": "compression",
  "args": {
    "Decompress": {
      "scheme": {
        "Zip": {
          "from": "C:/in/project.zip",
          "to": "C:/out",
          "overwrite": false
        }
      }
    }
  }
}
```

**TarGz / SevenZ：** 同上结构，scheme 键改为 `TarGz` 或 `SevenZ`。TarGz 不支持 `password`。

Pipeline 密码推荐 `${env.COREX_ARCHIVE_PASSWORD}`，勿在 YAML 写明文。

### generate

**Path：**

```json
{
  "module": "generate",
  "args": {
    "Path": {
      "from": "C:/scan",
      "to": "C:/paths.txt",
      "transform": "{path}",
      "separator": "/",
      "index": 0,
      "pad": false,
      "includes": [],
      "excludes": []
    }
  }
}
```

`transform` 与 `separator` 为必填字段。

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
      "to": "C:/out/file.txt",
      "template": "C:/template.hbs",
      "fragment": null,
      "variable": []
    }
  }
}
```

使用 `template`（模板文件）或 `fragment`（直接内容）之一，无 `from` 字段。

Uuid 成功时 `path` 为 null。

### bootstrap

```json
{
  "module": "bootstrap",
  "args": { "Env": null }
}
```

可选值：`Env`、`Inspect`、`Force`（unit variant，见 `bootstrap/schema.rs`）。

---

## 并发与错误语义

- Daemon **串行**接受连接：`run_server` 循环中一次处理一个 Pipe 连接
- **同连接多请求**：`handle_client` 内 loop 持续读行；每行 Invoke 写一行响应后**继续读**，直到 Shutdown、EOF 或读错误
- **官方客户端**：`send_request` / `corex_ipc` 每次新建连接、发送一行、读一行响应后关闭（推荐用法）
- 请求行超过 64KB：读失败，连接断开
- 空行或非法 JSON：返回 `{"id":0,"ok":false,...}` 错误响应（id 固定为 0）
- 未知 module：返回 `ok: false`，error 含 `"未知或未启用的模块"`
- args 解析失败：返回 `ok: false`，error 含 serde 上下文
- Shutdown：Daemon **不**写响应，直接退出

---

## 客户端实现

### 库 API（corex-core）

```rust
use cx::serve;

// 调用模块
let resp = serve::request(
    r"\\.\pipe\corex",
    "screenshot",
    serde_json::json!({ "Capture": { "to": "C:/out" } }),
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
| 当前 | typed 格式（`type: invoke/shutdown`）；64KB 行限；Windows Named Pipe |

未来可能扩展：

- Unix Domain Socket（非 Windows）
- 客户端长连接复用（服务端已支持同连接多请求）
- pipeline/schedule/watch 模块（当前仅 CLI）

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

覆盖 `parse_request` 的 typed/empty/invalid 场景。
