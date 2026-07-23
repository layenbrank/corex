# Breaking Changes（v0.3+）

> **Pipeline v3** 为破坏性重构，请参阅 [v3-migration.md](./v3-migration.md) 与 [pipeline-v3.md](./pipeline-v3.md)。下文保留 IPC / 模块级历史变更。

## 线格式：小写路由 + 扁平 params（当前）

Pipeline step 与 IPC invoke **不再**在 `params`/`args` 内嵌套 PascalCase 子命令或 `scheme`。

| 字段 | 说明 |
|------|------|
| `module` | 能力域 |
| `action` | CLI 子命令（kebab-case）；单操作模块省略 |
| `format` | 仅 compression：`zip` / `tar-gz` / `7z` |
| `algorithm` | 仅 codec：`base64` / `md5` |
| `params` / `args` | 仅 flags |

**Pipeline 迁移：**

```yaml
# 旧
module: compression
params:
  Compress:
    scheme:
      Zip: { from: '...', to: '...' }

# 新
module: compression
action: compress
format: zip
params:
  from: '...'
  to: '...'
```

**IPC 迁移：**

```json
// 旧
{"type":"invoke","id":1,"module":"codec","args":{"Hash":{"scheme":{"Md5":{"file":"a.txt"}}}}}

// 新
{"type":"invoke","id":1,"module":"codec","action":"hash","algorithm":"md5","args":{"file":"a.txt"}}
```

不保留旧 PascalCase / `scheme` 双读。

## IPC 协议

| 变更 | 旧格式 | 新格式 |
|------|--------|--------|
| Invoke 信封 | `{"id":1,"module":"...","args":{}}` | `{"type":"invoke","id":1,"module":"...","args":{}}` |
| Shutdown | `{"cmd":"shutdown"}` | `{"type":"shutdown"}` |
| 响应 data | 无 | 可选 `data` 字段（codec/scan/morph 等） |
| args 形态 | PascalCase 嵌套 / `scheme` | 扁平 flags + 顶层 `action`/`format`/`algorithm` |

## screenshot 模块

| 变更 | 旧 | 新 |
|------|----|----|
| 轻量 binary | `corex-capture` | 等价 `corex screenshot capture --to` |
| CLI | `corex screenshot --to DIR` | `corex screenshot capture --to DIR` |
| IPC Capture | `{"Capture":{"to":"DIR"}}` | `action:"capture"` + `args:{"to":"DIR"}` |
| Crop 输出 | 文档误写为文件路径 | `to` 为**输出目录**（与 Capture 一致，自动生成文件名） |
| Crop 大图 IPC | `final_image_base64` | 推荐 `image_file`（避免 64KB 行限） |

## compression 模块（v2.1+ → 线格式）

| 变更 | 旧 | 新 |
|------|----|----|
| IPC / Pipeline | `{"Compress":{"scheme":{"Zip":{...}}}}` | `action:compress` + `format:zip` + 扁平 args |
| CLI | `corex compression compress zip` | 不变 |
| 格式 | Zip / TarGz / SevenZ | 线格式键：`zip` / `tar-gz` / `7z` |
| 密码 | `password` + `${env.VAR}` | 不变；tar.gz **不支持** password |

## utils 内部命名

| 变更 | 旧 | 新 |
|------|----|----|
| glob 过滤模块 | `utils/ignore.rs` | `utils/filter.rs`（`Filter::new(includes, excludes)`） |

## Pipeline watch 字段（v2.0+）

| 字段 | 说明 |
|------|------|
| `watch.paths` | 文件/目录监听路径，由 `corex watch run` 读取 |
| `watch.includes` / `watch.excludes` | 与 copy/generate 相同的 glob 过滤命名 |

详见 [pipeline-v3.md](./pipeline-v3.md#watch-字段文件监听)。

## generate 模块（v2.2+）

| 变更 | 旧 | 新 |
|------|----|----|
| CLI 子命令 | `generate path` / `generate uuid` / **`generate file`** | 仅 `path` / `uuid` |
| Pipeline params | `File: { to, template, ... }` | **已移除**；模板文件生成改用 **`exec`** |
| 依赖 | handlebars | 已移除 |

**迁移示例（version 文件生成）：**

```yaml
# 旧
- id: gen_json
  module: generate
  params:
    File:
      to: '${var.base}/version.json'
      template: '${var.templates}/version.json.hbs'

# 新
- id: gen_version
  module: exec
  action: run
  params:
    script: '${var.scripts}/generate-version.ps1'
    args: ['-ProjectRoot', '${var.base}']
    cwd: '${var.base}'
    capture: json
```

脚本须在 stdout 最后一行输出 `{"path":"...","data":{...}}`。详见 [pipeline-v3.md](./pipeline-v3.md#exec-模块外部脚本)。

## exec 模块（v2.2+，新增）

| 项 | 说明 |
|----|------|
| CLI | `corex exec run --script ...` |
| Pipeline / IPC | `action: run` + 扁平 `params` / `args` |
| 返回值 | stdout 最后一行 JSON：`path`（string）+ `data`（object） |

## 迁移示例

```json
// 旧（PascalCase 嵌套）
{"type":"invoke","id":1,"module":"screenshot","args":{"Capture":{"to":"C:/out"}}}

// 新（小写路由 + 扁平 args）
{"type":"invoke","id":1,"module":"screenshot","action":"capture","args":{"to":"C:/out"}}
```

## i-thinking 集成

Tauri 侧需同步更新 `corex_ipc.rs` 与所有 `invoke("screenshot", ...)` 调用。参考 [`examples/tauri/corex_ipc.rs`](../examples/tauri/corex_ipc.rs)。
