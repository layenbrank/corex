# Breaking Changes（v0.3+）

> **Pipeline v3** 为破坏性重构，请参阅 [v3-migration.md](./v3-migration.md) 与 [pipeline-v3.md](./pipeline-v3.md)。下文保留 IPC / 模块级历史变更。

## IPC 协议

| 变更 | 旧格式 | 新格式 |
|------|--------|--------|
| Invoke 信封 | `{"id":1,"module":"...","args":{}}` | `{"type":"invoke","id":1,"module":"...","args":{}}` |
| Shutdown | `{"cmd":"shutdown"}` | `{"type":"shutdown"}` |
| 响应 data | 无 | 可选 `data` 字段（codec/scan/morph 等） |

## screenshot 模块

| 变更 | 旧 | 新 |
|------|----|----|
| CLI | `corex screenshot --to DIR` | `corex screenshot capture --to DIR` |
| IPC Capture | `{"to":"DIR"}` | `{"Capture":{"to":"DIR"}}` |
| Crop 输出 | 文档误写为文件路径 | `to` 为**输出目录**（与 Capture 一致，自动生成文件名） |
| Crop 大图 IPC | `final_image_base64` | 推荐 `image_file`（避免 64KB 行限） |

## compression 模块（v2.1+）

| 变更 | 旧 | 新 |
|------|----|----|
| IPC / Pipeline | `{"Zip":{...}}` / `{"Unzip":{...}}` | `{"Compress":{"scheme":{"Zip":{...}}}}` / `Decompress` |
| Pipeline step | `action: zip` / `action: unzip` + 扁平 params | `params.Compress.scheme.Zip` 等 |
| CLI | `corex compression zip` | `corex compression compress zip` |
| 格式 | 仅 ZIP | Zip / TarGz / SevenZ |
| 密码 | 无 | `password` + `${env.VAR}`；tar.gz **不支持** password |

**迁移示例：**

```json
// 旧
{"Zip":{"from":"C:/src","to":"C:/out.wgt"}}

// 新
{"Compress":{"scheme":{"Zip":{"from":"C:/src","to":"C:/out.wgt","level":6,"encryption":"aes256","password":"${env.COREX_ARCHIVE_PASSWORD}"}}}}
```

## Pipeline step.action

`generate` / `compression` / `bootstrap` 不再支持 `step.action`。请改用 params 外层 enum（`Path` / `Compress` / `Env` 等）。

## utils 内部命名

| 变更 | 旧 | 新 |
|------|----|----|
| glob 过滤模块 | `utils/ignore.rs` | `utils/filter.rs`（`Filter::new(includes, excludes)`） |

## Pipeline watch 字段（v2.0+）

| 字段 | 说明 |
|------|------|
| `watch.paths` | 文件/目录监听路径，由 `corex watch start` 读取 |
| `watch.includes` / `watch.excludes` | 与 copy/generate 相同的 glob 过滤命名 |

详见 [pipeline-v3.md](./pipeline-v3.md#watch-字段文件监听)。

## 迁移示例

```json
// 旧
{"id":1,"module":"screenshot","args":{"to":"C:/out"}}

// 新
{"type":"invoke","id":1,"module":"screenshot","args":{"Capture":{"to":"C:/out"}}}
```

## i-thinking 集成

Tauri 侧需同步更新 `corex_ipc.rs` 与所有 `invoke("screenshot", ...)` 调用。参考 [`examples/tauri/corex_ipc.rs`](../examples/tauri/corex_ipc.rs)。
