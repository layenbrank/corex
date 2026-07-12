# CoreX

一个面向 H5+ 应用构建的自动化任务调度 CLI 工具，支持文件复制、目录清理、路径生成、压缩打包、Pipeline 编排、cron 定时任务与文件变更监听（watch）。

## 架构与 Tauri 集成

Corex 从 Tauri 项目中独立拆分，重依赖（xcap、image、tokio 等）保留在本仓库。Tauri 作为瘦客户端，通过 Named Pipe 调用 `corex-serve` Daemon，**不**链回 `corex-core` 库。

**Workspace：** `corex-core`（库 `cx`）、`corex`（完整 CLI）、`corex-serve`（Daemon）、`corex-capture`（轻量截图 capture）。

| 文档 | 说明 |
|------|------|
| [docs/architecture-and-tauri-integration.md](docs/architecture-and-tauri-integration.md) | 架构总览、四阶段改动、快速开始 |
| [docs/architecture.md](docs/architecture.md) | Feature 体系、serve 模块深度 |
| [docs/pipeline-v3.md](docs/pipeline-v3.md) | Pipeline v3 配置、watch / schedule 字段 |
| [docs/breaking-changes.md](docs/breaking-changes.md) | v0.3+ IPC / screenshot 破坏性变更 |
| [docs/ipc-protocol.md](docs/ipc-protocol.md) | Named Pipe JSON 协议 |
| [docs/tauri-integration.md](docs/tauri-integration.md) | Tauri 2 完整接入指南 |
| [examples/tauri/](examples/tauri/) | 可复制的 Tauri 示例代码 |

### Daemon 与 IPC

```powershell
# 启动 Daemon（默认 \\.\pipe\corex）
cargo run -p corex-serve

# 验证 IPC（另开终端，Daemon 运行中）
cargo run -p corex-core --example ipc --features serve -- C:\Temp\screenshots
```

协议与 args 格式见 [docs/ipc-protocol.md](docs/ipc-protocol.md)。

---

## 快速开始

```powershell
# 查看所有命令
corex --help

# 执行单个命令
corex copy --from ./src --to ./dist --excludes "node_modules,*.log"

# 执行 Pipeline
corex pipeline --config pipelines.yaml

# 交互式选择 Pipeline
corex schedule run

# 定时调度（守护进程）
corex schedule cron

# 文件变更监听（Vite 风格 dev watch）
corex watch run --immediate
```

---

## 命令一览

| 命令                                | 说明                      |
| ----------------------------------- | ------------------------- |
| `corex copy`                        | 复制文件或目录            |
| `corex scrub`                       | 清理指定名称的文件/目录   |
| `corex generate path`               | 扫描目录并生成路径列表    |
| `corex generate uuid`               | 生成 UUID                 |
| `corex generate file`               | 基于模板生成文件          |
| `corex compression compress zip`    | 压缩（Zip / tar.gz / 7z） |
| `corex compression decompress zip`  | 解压归档                  |
| `corex codec encode/decode/hash`    | Base64 编解码、MD5 摘要   |
| `corex scan os`                     | 采集 OS / CPU / 内存信息  |
| `corex morph`                       | PDF 处理（10 子命令）     |
| `corex screenshot capture/…`        | 截图（capture/monitors/windows/crop/clipboard） |
| `corex shade`                       | 图片格式转换 / 压缩       |
| `corex bootstrap env/inspect/force` | 环境初始化与检查          |
| `corex pipeline`                    | 执行 YAML 定义的 Pipeline |
| `corex schedule run/generate/cron`  | 任务调度器                |
| `corex watch run`                 | 文件变更监听，debounce 后重跑 Pipeline |

### 独立 Binary

| Binary        | 说明                                      |
| ------------- | ----------------------------------------- |
| `corex`       | 完整 CLI（`features = all`）              |
| `corex-serve` | Named Pipe Daemon，供 Tauri sidecar 使用  |
| `corex-capture` | 轻量 capture，等价 `corex screenshot capture --to` |

完整 CLI 截图请使用 `corex screenshot capture --to`；`corex-capture` 为轻量独立 binary。

```powershell
cargo build -p corex-serve --release
cargo run -p corex-capture -- --to C:\Temp\screenshots
corex screenshot capture --to C:\Temp\screenshots
```

---

## 文件复制 (copy)

复制文件或目录。支持白名单/黑名单过滤、重命名复制、自动创建目标目录。

### 行为

- **源为文件**：直接复制文件，`--to` 可以是新的文件路径（重命名）或目录
- **源为目录**：递归复制整个目录结构，自动创建不存在的目录

### 参数

| 参数         | 缩写 | 必填 | 默认值 | 说明                                                 |
| ------------ | ---- | ---- | ------ | ---------------------------------------------------- |
| `--from`     | `-f` | ✓    | -      | 源路径（文件或目录）                                 |
| `--to`       | `-t` | ✓    | -      | 目标路径                                             |
| `--empty`    | `-e` | ✗    | `true` | 复制前是否清空目标目录（仅目录模式）                 |
| `--includes` |      | ✗    | -      | 包含模式（白名单），逗号分隔或多次使用，空则包含全部 |
| `--excludes` |      | ✗    | -      | 排除模式（黑名单），逗号分隔或多次使用               |

### 使用示例

```powershell
# 复制目录，排除 node_modules 和 .git
corex copy -f ./src -t ./dist --excludes "node_modules,*.git"

# 只复制 .js 和 .css 文件（白名单）
corex copy -f ./src -t ./dist --includes "*.js,*.css"

# 复制单个文件并重命名
corex copy -f ./build/app.js -t ./deploy/bundle.min.js

# 复制文件到目录（保持原名）
corex copy -f ./config.json -t ./dist/

# 复制目录且不清空目标
corex copy -f ./assets -t ./dist/assets --empty false
```

---

## 目录清理 (scrub)

递归删除目录中指定名称的文件或文件夹。

### 参数

| 参数          | 缩写 | 必填 | 默认值  | 说明                         |
| ------------- | ---- | ---- | ------- | ---------------------------- |
| `--source`    | `-s` | ✓    | -       | 要清理的根目录路径           |
| `--target`    | `-t` | ✓    | -       | 要删除的目标名称（不含路径） |
| `--recursive` | `-r` | ✗    | `false` | 是否递归处理子目录           |

### 使用示例

```powershell
# 递归删除所有 .turbo 文件夹
corex scrub -s C:\Projects\my-app -t .turbo -r

# 删除根目录下的 node_modules
corex scrub -s C:\Projects\my-app -t node_modules
```

---

## 路径生成 (generate path)

扫描目录并按模板生成自定义格式的路径列表文件。

### 参数

| 参数          | 必填 | 默认值  | 说明                                                 |
| ------------- | ---- | ------- | ---------------------------------------------------- |
| `--from`      | ✓    | -       | 源目录路径                                           |
| `--to`        | ✓    | -       | 输出文件路径                                         |
| `--transform` | ✓    | -       | 转换规则模板                                         |
| `--index`     | ✗    | `0`     | 起始索引                                             |
| `--separator` | ✓    | -       | 路径分隔符                                           |
| `--pad`       | ✗    | `false` | 对索引进行补零填充                                   |
| `--includes`  | ✗    | -       | 包含模式（白名单），逗号分隔或多次使用，空则包含全部 |
| `--excludes`  | ✗    | -       | 排除模式（黑名单），逗号分隔或多次使用               |
| `--uppercase` | ✗    | -       | 将指定规则转换为大写，逗号分隔或多次使用             |

### 转换规则模板变量

| 变量            | 说明                            |
| --------------- | ------------------------------- |
| `{{index}}`     | 文件序号（受 `--pad` 控制补零） |
| `{{filename}}`  | 文件名（含扩展名）              |
| `{{extension}}` | 扩展名（不含点）                |
| `{{path}}`      | 文件所在目录的相对路径          |
| `{{fullpath}}`  | 完整相对路径                    |

### 使用示例

```powershell
corex generate path `
  --from dist `
  --to path.txt `
  --index 1 `
  --separator "\" `
  --pad `
  --excludes "example.js,*.git,node_modules" `
  --uppercase "extension" `
  --transform '<include name="IDR_ITAB_{{extension}}_{{index}}" file="{{fullpath}}" type="BINDATA" />'
```

---

## 文件生成 (generate file)

基于 Handlebars 模板引擎生成文件。支持模板文件或直接传入内容片段，可注入动态变量。

### 参数

| 参数           | 缩写 | 必填 | 说明                                         |
| -------------- | ---- | ---- | -------------------------------------------- |
| `--to`         | `-t` | ✓    | 输出文件路径（自动创建不存在的目录）         |
| `--template`   | `-p` | ✗    | 模板文件路径（推荐，与 `--fragment` 二选一） |
| `--fragment`   | `-f` | ✗    | 直接传入模板内容（简单模式）                 |
| `--variable`   |      | ✗    | 动态变量 `key=value`，可多次使用             |

> `--template` 和 `--fragment` 必须指定其一。

### 模板引擎

使用 Handlebars 语法，支持标准变量替换和内置 Helper：

| Helper                    | 说明                                                  |
| ------------------------- | ----------------------------------------------------- |
| `{{now "format"}}`        | 输出当前时间（`iso` / `unix` / `unix_ms` 或 strftime）|
| `{{uuid true}}`           | 生成 UUID v4，传 `true` 为大写                        |
| `{{rand 32}}`             | 生成随机字符串，参数为长度（默认 16）                  |

### 使用示例

```powershell
# 使用模板文件 + 变量生成配置
corex generate file `
  --to ./dist/config.json `
  --template ./templates/config.hbs `
  --variable name=myapp `
  --variable version=1.0.0

# 直接用 fragment 快速生成
corex generate file `
  --to ./dist/version.txt `
  --fragment "Build: {{now \"%Y-%m-%d %H:%M:%S\"}} | ID: {{uuid}}"

# 生成带随机 token 的环境文件
corex generate file `
  --to ./.env `
  --fragment "APP_SECRET={{rand 32}}" `
```

### 模板文件示例

`templates/config.hbs`：
```handlebars
{
  "name": "{{name}}",
  "version": "{{version}}",
  "build": {
    "time": "{{now "unix"}}",
    "id": "{{uuid}}"
  }
}
```

### Pipeline YAML 示例

```yaml
- id: gen_config
  module: generate
  params:
    File:
      to: '${var.dist_dir}/config.json'
      template: './templates/config.hbs'
      variable:
        - [name, '${var.project_name}']
        - [version, '2.0.0']
```

---

## UUID 生成 (generate uuid)

生成随机 UUID v4。

### 参数

| 参数          | 缩写 | 必填 | 默认值  | 说明             |
| ------------- | ---- | ---- | ------- | ---------------- |
| `--count`     | `-c` | ✗    | `1`     | 生成 UUID 的数量 |
| `--uppercase` |      | ✗    | `false` | 以大写形式输出   |

### 使用示例

```powershell
# 生成 5 个大写 UUID
corex generate uuid --count 5 --uppercase
```

---

## 压缩打包 (compression)

支持 **Zip**（含 H5+ `.wgt`）、**tar.gz**、**7z**。CLI 与 Pipeline/IPC 同构：`Compress/Decompress` + `scheme.{Zip|TarGz|SevenZ}`。

### CLI

```powershell
corex compression compress zip -f C:\app\dist -t C:\app\release\app.wgt --level 9
corex compression decompress zip -f C:\app\release\app.wgt -t C:\app\extracted
corex compression compress tar-gz -f C:\app\dist -t C:\app\release\app.tar.gz
corex compression compress 7z -f C:\app\dist -t C:\app\release\app.7z --password secret
```

### 常用参数

| 字段 | 说明 |
| ---- | ---- |
| `from` / `to` | 源目录或归档 / 输出文件或目录 |
| `level` | 压缩级别 |
| `method` | Zip：`deflated` / `stored` / `bzip2` / `zstd` |
| `encryption` | Zip：`none` / `aes128` / `aes256` |
| `password` | Zip、7z；Pipeline 用 `${env.COREX_ARCHIVE_PASSWORD}` |
| `includes` / `excludes` | 文件过滤 |
| `overwrite` | 解压是否覆盖（默认 false） |

tar.gz **不支持** password。详见 [docs/ipc-protocol.md](docs/ipc-protocol.md)。

### Pipeline 示例

```yaml
params:
  Compress:
    scheme:
      Zip:
        from: '${copy_cache.output}'
        to: '${var.base}\\app.wgt'
        level: 6
```

---

## 图片处理 (shade)

图片格式转换或压缩（png / jpg / webp / bmp）。

### 参数

| 参数       | 缩写 | 必填 | 默认值 | 说明                          |
| ---------- | ---- | ---- | ------ | ----------------------------- |
| `--from`   | `-f` | ✓    | -      | 输入图片或目录                |
| `--to`     | `-t` | ✓    | -      | 输出路径                      |
| `--format` | `-o` | ✗    | -      | 输出格式（留空按扩展名推断）  |
| `--quality`| `-q` | ✗    | `100`  | 质量 1–100（仅 jpg 有效）     |

### 使用示例

```powershell
corex shade -f C:\images\photo.png -t C:\out\photo.webp -o webp
```

---

## 编解码 (codec)

Base64 编解码与 MD5 文件/文本摘要。Pipeline / IPC 的 `params` / `args` 使用 serde enum JSON（与 CLI 子命令同构）。

```powershell
# Base64 编码文件
corex codec encode base64 --file ./input.bin --output ./out.b64

# Base64 解码
corex codec decode base64 --input aGVsbG8= 

# MD5 摘要
corex codec hash md5 --file ./README.md
```

IPC 示例：`{"Hash":{"scheme":{"Md5":{"file":"C:/README.md"}}}}` — 见 [docs/ipc-protocol.md](docs/ipc-protocol.md)。

---

## 系统信息 (scan)

采集操作系统、内核、主机名、CPU 与内存信息，输出 JSON。

```powershell
corex scan os
```

Pipeline / IPC：`{"Os":{}}`。结果写入 `TaskOutput.metadata["data"]` 或响应 `data` 字段。

---

## PDF 处理 (morph)

PDF 元数据、渲染、搜索、合并、拆分、导出图片/Office 等 10 个子命令。发布包内已捆绑 `pdfium.dll`，需与 `corex.exe` 同目录；开发环境请先运行 `scripts/download-pdfium.ps1`。

```powershell
corex morph meta --path ./doc.pdf
corex morph merge --paths a.pdf,b.pdf --dest ./merged.pdf
corex morph split --path ./doc.pdf --ranges 1-3,5-7 --dest-dir ./parts
```

Pipeline / IPC：`{"Meta":{"path":"D:/doc.pdf"}}` 等，路径字段支持 `${var.*}` / `${step.output}` 变量解析。

---

## 截图 (screenshot)

子命令：`capture`（全屏）、`monitors`（显示器列表）、`windows`（窗口列表）、`crop`（区域裁剪）、`clipboard`（剪贴板区域）。

| 子命令 | 关键参数 | 说明 |
| ------ | -------- | ---- |
| `capture` | `--to` | 输出目录 |
| `crop` | `--source`、`--to` | 源图路径 + 输出目录 |
| `clipboard` | `--source` | 剪贴板图片路径 |

```powershell
corex screenshot capture --to C:\Screenshots
corex screenshot crop --source C:\in.png --to C:\out --x 0 --y 0 --w 800 --h 600
```

IPC Capture：`{"Capture":{"to":"C:/Screenshots"}}`。破坏性变更见 [docs/breaking-changes.md](docs/breaking-changes.md)。

---

## 环境初始化 (bootstrap)

初始化或检查 CoreX 运行环境。

```powershell
corex bootstrap env       # 初始化环境变量
corex bootstrap inspect   # 检查环境配置
corex bootstrap force     # 强制重新初始化
```

---

## Pipeline 编排

Pipeline 允许通过 YAML 配置文件将多个命令组合成有序流水线，支持步骤间数据传递和并发执行。

### 命令

```powershell
# 执行 Pipeline（指定配置文件）
corex pipeline --config pipelines.yaml

# 执行指定 ID 的 Pipeline
corex pipeline --id build-h5

# 仅验证配置不执行
corex pipeline --validate

# Dry-run 预览
corex pipeline --dry-run

# 强制单次执行（忽略 yaml 中的 watch / schedule）
corex pipeline --id build-h5 --once
```

配置了 `watch` 或 `schedule` 时，`corex pipeline` 会自动进入对应守护模式；加 `--once` 则只跑一遍。详见 [docs/pipeline-v3.md](docs/pipeline-v3.md#cli)。

### 支持的 module

| module | params 说明 |
| ------ | ----------- |
| `copy` | `from` / `to` |
| `scrub` | `source` / `target` |
| `shade` | `from` / `to` |
| `compression` | `Compress` / `Decompress` + `scheme.{Zip\|TarGz\|SevenZ}` |
| `generate` | `Path` / `Uuid` / `File` |
| `bootstrap` | `Env` / `Inspect` / `Force` |
| `screenshot` | `Capture` / `Crop` / …（enum JSON） |
| `codec` | `Encode` / `Decode` / `Hash` |
| `scan` | `Os: {}` |
| `morph` | `Meta` / `Merge` / …（捆绑 `pdfium.dll`） |

步骤仅含 `module` + `params`（= IPC args）。变量：`${var.*}`、`${step.output}`、`${env.*}`（密码）。

```yaml
- id: capture_screen
  module: screenshot
  params:
    Capture:
      to: '${var.base}\\screenshots'
```

完整 smoke test 见根目录 [`pipelines.yaml`](pipelines.yaml) 中的 `dev-tools` pipeline（含 `watch` 配置示例）。

### YAML 配置格式

```yaml
# version 必须为 3
version: 3

variables:
  src_dir: './src'
  dist_dir: './dist'

pipelines:
  - id: build-pipeline
    description: 构建流水线
    schedule: '0 8 * * *'   # 可选：cron 定时（见 schedule 章节）
    watch:                  # 可选：文件监听（见 watch 章节）
      paths: ['${var.src_dir}']
      debounce_ms: 300
    steps:
      - id: step_copy
        module: copy
        description: 复制源文件
        params:
          from: '${var.src_dir}'
          to: '${var.dist_dir}'
          empty: false
          includes: []
          excludes: ['*.log', 'node_modules']

      - id: step_generate
        module: generate
        description: 生成路径列表
        depends_on: [step_copy]
        params:
          Path:
            from: '${steps.step_copy.artifact.path}'
            to: './output/path.txt'
            transform: '{{fullpath}}'
            index: 1
            separator: '/'
            pad: false
            includes: ['*.js', '*.css']
            excludes: []
            uppercase: []

      - id: step_compress
        module: compression
        description: 打包
        depends_on: [step_copy]
        params:
          Compress:
            scheme:
              Zip:
                from: '${var.dist_dir}'
                to: './release/app.wgt'
                level: 6
```

### 执行模式（v3 DAG）

- 无 `depends_on`：按 `steps` 数组顺序建隐式链（顺序执行）
- 有 `depends_on`：fork-join；同层步骤并发（`JoinSet`）
- 支持 `when` 条件跳过、`retry` 重试

变量语法见 [docs/pipeline-v3.md](docs/pipeline-v3.md#变量语法-v3)。

### 变量引用语法（v3）

| 语法 | 说明 |
|------|------|
| `${var.name}` | 引用全局变量 |
| `${steps.step_id.artifact.path}` | 引用前序步骤产物路径 |
| `${env.NAME}` | 环境变量 |

> v3 已移除 `mode: sequential|parallel` 与 `${step_id.output}` 语法。

---

## 文件监听 (watch)

Vite 风格开发监听：当 `pipelines.yaml` 中 Pipeline 配置了 `watch` 字段时，`corex watch run` 会在文件变更（debounce 后）**重跑整条 Pipeline**。

```powershell
# 监听所有带 watch 的 Pipeline
corex watch run

# 仅监听 dev-tools
corex watch run -p dev-tools

# 启动先执行一遍，再进入监听
corex watch run --immediate

# CLI 覆盖 debounce / 追加过滤规则
corex watch run --debounce-ms 500 --excludes '**/*.tmp'
```

```yaml
# pipelines.yaml 示例（与 copy/generate 相同的 includes / excludes 命名）
- id: dev-tools
  watch:
    paths: ['${var.base}/src']
    includes: []
    excludes: ['**/node_modules/**', '**/.git/**']
    debounce_ms: 300
  steps: [...]
```

同一 Pipeline 正在执行时会跳过新触发。详见 [docs/pipeline-v3.md](docs/pipeline-v3.md#watch-字段文件监听)。

---

## 定时调度 (schedule)

### 交互式执行

```powershell
# 交互式选择 Pipeline
corex schedule run

# 生成配置模板（交互式向导）
corex schedule generate
```

### Cron 定时执行

以守护进程模式运行，按 Pipeline 配置中的 `schedule` cron 表达式定时触发执行。

```powershell
# 启动定时调度（使用默认配置文件）
corex schedule cron

# 指定配置文件
corex schedule cron --config ./pipelines.yaml
```

启动后会显示已加载的定时 Pipeline 及其下次执行时间，持续运行直到 `Ctrl+C` 中断。

```yaml
# 示例：每天 8:00 执行
- id: daily-build
  schedule: "0 8 * * *"
  mode: sequential
  steps:
    ...

# 示例：每 5 分钟执行
- id: sync-files
  schedule: "*/5 * * * *"
  ...
```

### Cron 表达式格式

```
分 时 日 月 周
```

| 字段 | 取值范围   | 特殊字符        |
| ---- | ---------- | --------------- |
| 分钟 | 0–59       | `*` `/` `,` `-` |
| 小时 | 0–23       | `*` `/` `,` `-` |
| 日   | 1–31       | `*` `/` `,` `-` |
| 月   | 1–12       | `*` `/` `,` `-` |
| 星期 | 0–6 (0=日) | `*` `/` `,` `-` |

> **对比 watch：** `schedule` 按 cron 定时触发；`watch` 按文件变更 debounce 触发。二者可配置在同一 Pipeline 上。

---

## Node.js 集成示例

```javascript
import { spawn } from 'node:child_process'
import { resolve } from 'node:path'

// 复制构建产物
spawn(
  'corex',
  [
    'copy',
    '--from',
    './dist',
    '--to',
    resolve('C:', 'deploy', 'app'),
    '--excludes',
    'node_modules,*.log,*.map',
    '--empty'
  ],
  { stdio: 'inherit' }
)

// 生成资源路径清单
spawn(
  'corex',
  [
    'generate',
    'path',
    '--from',
    './dist',
    '--to',
    './dist/manifest.txt',
    '--transform',
    '{{fullpath}}',
    '--index',
    '1',
    '--separator',
    '/',
    '--excludes',
    '*.map,node_modules'
  ],
  { stdio: 'inherit' }
)
```

### package.json 配置

```json
{
  "scripts": {
    "build": "run-s build:core build:post",
    "build:core": "vite build",
    "build:post": "node ./scripts/post-build.js",
    "deploy": "corex pipeline --config pipelines.yaml"
  }
}
```
