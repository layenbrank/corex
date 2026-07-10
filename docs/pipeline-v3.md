# Pipeline v3 规范

Corex v3 采用破坏性新格式，**不兼容** v1/v2 YAML。

## 顶层 Schema

```yaml
version: 3          # 必填，非 3 直接 reject
variables:
  base: './dist'

pipelines:
  - id: build-h5
    description: H5+ 构建
    schedule: '0/30 * * * * *'   # 可选 cron（`corex schedule cron`）
    watch:                        # 可选文件监听（`corex watch start`）
      paths: ['${var.base}/src']
      includes: []
      excludes: ['**/node_modules/**', '**/.git/**']
      debounce_ms: 300
    steps:
      - id: copy_cache
        module: copy
        params: { from: '${var.base}/node_modules', to: '${var.base}/copies' }

      - id: gen_path
        module: generate
        depends_on: [copy_cache]
        params:
          Path:
            from: '${steps.copy_cache.artifact.path}'
            to: '${var.base}/path.txt'
            transform: '...'

      - id: compress_wgt
        module: compression
        depends_on: [copy_cache]
        when: '${env.SHOULD_PACK}'
        retry: { max: 3, backoff_ms: 1000 }
        params:
          Compress:
            scheme:
              Zip:
                from: '${steps.copy_cache.artifact.path}'
                to: '${var.base}/out.wgt'
                level: 6
```

## 已移除字段

- `mode: sequential|parallel` — 由 DAG 语义统一表达
- `action` — 参数嵌套在 `params` 内（如 `Compress.scheme.Zip`）
- `${step_id.output}` — 见下方变量语法

## 变量语法 v3

| 语法 | 含义 |
|------|------|
| `${var.name}` | `variables` 中的键 |
| `${env.NAME}` | 环境变量 |
| `${steps.step_id.artifact.path}` | 前序步骤产物路径 |
| `${steps.step_id.artifact.data.key}` | 前序步骤产物 metadata |

## DAG 执行

- `depends_on` 缺省：按 `steps` 数组顺序建隐式链（等价旧 sequential）
- 显式 `depends_on`：fork-join；同层 `JoinSet` 并发
- `validate` 检测：version=3、DAG 无环、depends 存在、module 已知；`watch.paths` 非空（若配置了 watch）

## watch 字段（文件监听）

与 `schedule` 类似，`watch` 为 Pipeline 级可选字段，**不由 Pipeline step 执行**，而由 `corex watch start` 守护进程读取。

```yaml
- id: dev-rebuild
  description: 开发时自动重建
  watch:
    paths: ['${var.base}/src', '${var.base}/templates']
    includes: []                              # 空 = paths 下全部变更
    excludes: ['**/node_modules/**', '**/.git/**']
    debounce_ms: 300
  steps: [...]
```

| 字段 | 说明 |
|------|------|
| `paths` | 必填；监听路径（文件或目录），支持 `${var.*}` |
| `includes` | glob 白名单；非空时仅匹配项触发 |
| `excludes` | glob 黑名单；默认 `**/.git/**`、`**/node_modules/**` |
| `debounce_ms` | 防抖毫秒，默认 300 |

过滤逻辑复用 `utils/filter.rs`（与 copy / generate 的 `includes` / `excludes` 语义一致）。CLI 可追加 `--includes` / `--excludes` / `--debounce-ms` 覆盖。

```powershell
corex watch start
corex watch start -p dev-rebuild --run-on-start
corex watch start --debounce-ms 500 --excludes '**/*.tmp'
```

`schedule` 与 `watch` 可共存（cron 与文件变更两种触发源）。

## Stage 类型

| 类型 | 适用 module | 行为 |
|------|-------------|------|
| Batch | copy, compression, scrub, shade, morph, screenshot | 1 artifact in → 1 out |
| Stream | generate `Path` | walkdir → transform line → sink file |
| Signal | scan, codec, bootstrap | 0/1 in → metadata out |

## CLI

```bash
# 验证
corex pipeline --validate --config pipelines.yaml
corex pipeline --validate --format json

# 运行
corex pipeline --id build-h5 --config pipelines.yaml
corex pipeline --id build-h5 --format json --report-file report.json

# 覆盖变量
corex pipeline --id build-h5 -D base=D:/proj/dist
```

## RunReport（JSON）

```json
{
  "pipeline_id": "build-h5",
  "status": "success",
  "started_at": "2026-07-10T07:00:00Z",
  "duration_ms": 1234,
  "steps": [
    {
      "id": "copy_cache",
      "status": "success",
      "artifact": { "path": "..." },
      "items": 0,
      "duration_ms": 400
    }
  ]
}
```

## ValidateReport（JSON）

```json
{
  "ok": true,
  "pipeline_count": 3,
  "errors": []
}
```

失败时 `ok: false`，`errors` 包含人类可读消息列表。
