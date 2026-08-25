# CoreX

可组合的快捷指令（Shortcut）/ Action 运行时：YAML 定义流水线，内置与 WASM 插件提供动作，CLI 与 Daemon 共用同一套引擎。

## 架构与 Tauri 集成

v4 将运行时拆为独立 crate；Tauri / 宿主通过 IPC 调用 **`corex-daemon`**，不直接链接业务库。

**Workspace：**

| 路径 | 包名 | 说明 |
|------|------|------|
| `crates/core` | `corex-core` | Value / Action / ExecutionContext |
| `crates/engine` | `corex-engine` | Shortcut YAML、Pipeline、历史 |
| `crates/registry` | `corex-registry` | 内置 Action + WASM host |
| `crates/ipc` | `corex-ipc` | Unix socket 协议 |
| `crates/plugin-sdk` | `corex-plugin-sdk` | WIT 契约 |
| `bins/cli` | `corex` | CLI |
| `bins/daemon` | `corex-daemon` | 后台 Daemon |
| `pdfium` | `pdfium` | 可选 native DLL 辅助 |

| 文档 | 说明 |
|------|------|
| [docs/architecture.md](docs/architecture.md) | **v4 架构**（crate 布局与执行模型） |
| [docs/breaking-changes-v4.md](docs/breaking-changes-v4.md) | **v4 破坏性变更**（serve→daemon、YAML、Action ID） |
| [plugins/README.md](plugins/README.md) | 第三方 WASM 插件 |
| [docs/architecture-and-tauri-integration.md](docs/architecture-and-tauri-integration.md) | 历史总览（部分仍写 corex-serve） |
| [docs/tauri-integration.md](docs/tauri-integration.md) | Tauri 接入（请改用 corex-daemon） |
| [examples/tauri/](examples/tauri/) | Tauri 示例代码 |
| [examples/shortcuts/](examples/shortcuts/) | Shortcut YAML 示例 |

### Daemon 与 IPC

```bash
# 启动 Daemon（默认 <data-dir>/corex.sock）
cargo run -p corex-daemon

# CLI 控制
corex daemon status
corex daemon stop
```

协议见 `corex-ipc`（`ping` / `run_shortcut` / `invoke` / …）。破坏性说明见 [breaking-changes-v4.md](docs/breaking-changes-v4.md)。

---

## 快速开始

```bash
# 查看命令
corex --help

# 运行 Shortcut（名称或 YAML 路径）
corex run examples/shortcuts/hello.yaml
corex run hello --input who=Corex

# 列出快捷指令 / 动作
corex list
corex actions

# 校验 / 创建
corex validate examples/shortcuts/hello.yaml
corex create my-shortcut

# Daemon
corex daemon run
```

---

## 命令一览（v4 CLI）

| 命令 | 说明 |
|------|------|
| `corex run` | 执行 Shortcut |
| `corex list` | 列出 Shortcut |
| `corex actions` | 列出已注册 Action |
| `corex create` | 创建 Shortcut 脚手架 |
| `corex validate` | 校验 YAML |
| `corex daemon` | start / stop / status / run |

> 旧版 `corex copy` / `pipeline` / `morph` 等子命令对应业务模块将在 P4 以 Action 形式迁入；下文「文件复制」等章节保留作历史参考。

### 独立 Binary

| Binary | 说明 |
|--------|------|
| `corex` | CLI |
| `corex-daemon` | Unix socket Daemon（Tauri / 宿主 sidecar） |

```bash
cargo build -p corex -p corex-daemon --release
```

### GitHub Release（Windows x64）

打 `v*` SemVer 标签（或 `workflow_dispatch`）会发布 `corex-{tag}-windows-x64.zip`，解压后同目录包含：

| 文件 | 用途 |
|------|------|
| `corex.exe` | CLI |
| `corex-daemon.exe` | sidecar Daemon |
| `pdfium.dll` | PDF 运行时（若仍捆绑） |

另附 `.zip.sha256` 与 `SHA256SUMS.txt`。

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

## CVID 生成 (generate cvid)

生成符合 GUID v4 标准的安全 CVID（32 位大写十六进制），可用于 Bing 搜索建议等客户端标识场景。无额外参数。

### 使用示例

```powershell
corex generate cvid
```

Pipeline / IPC：`action: cvid` + `args: {}`；成功时 `path` 为 null，`data` 为 `{"value":"..."}`。详见 [docs/ipc-protocol.md](docs/ipc-protocol.md#generate)。

---

## 压缩打包 (compression)

支持 **Zip**（含 H5+ `.wgt`）、**tar.gz**、**7z**。Pipeline/IPC：`action` + `format`（`zip` / `tar-gz` / `7z`）+ 扁平 `params`/`args`。

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
module: compression
action: compress
format: zip
params:
  from: '${steps.copy_cache.artifact.path}'
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

IPC 示例：`{"type":"invoke","module":"codec","action":"hash","algorithm":"md5","args":{"file":"C:/README.md"}}` — 见 [docs/ipc-protocol.md](docs/ipc-protocol.md)。

---

## 系统信息 (scan)

采集操作系统、内核、主机名、CPU 与内存信息，输出 JSON。

```powershell
corex scan os
```

Pipeline / IPC：`action: os` + `params: {}` / `args: {}`。结果写入响应 `data` 字段。

---

## 搜索建议 (engine)

代理 Bing Autosuggest（`cn.bing.com/AS/Suggestions`），返回建议列表 JSON。建议项类型码 `t` 为任意字符串（非固定枚举）；省略 `--cvid` 时自动调用 `generate_secure_cvid`。

### 参数

| 参数           | 必填 | 默认值 | 说明 |
| -------------- | ---- | ------ | ---- |
| `--pt`         | ✓    | -      | 页面类型（如 `page.home`） |
| `--qry`        | ✓    | -      | 查询关键词 |
| `--cp`         | ✓    | -      | 光标位置（通常为 `qry` 长度） |
| `--csr`        | ✗    | `1`    | csr 标志 |
| `--pths`       | ✗    | `1`    | pths 标志 |
| `--cvid`       | ✗    | 自动生成 | 客户端 CVID |
| `--user-agent` | ✗    | Chrome UA | 出站 User-Agent |

### 使用示例

```powershell
corex engine suggestion --pt page.home --qry rust --cp 4
```

Pipeline / IPC：`action: suggestion` + 扁平 `params`/`args`。结果写入响应 `data`。详见 [docs/ipc-protocol.md](docs/ipc-protocol.md#engine)。

---

## PDF 处理 (morph)

PDF 元数据、渲染、搜索、合并、拆分、页整理、导出图片/Office 等子命令。发布包内已捆绑 `pdfium.dll`，需与 `corex.exe` / `corex-serve.exe` 同目录；开发环境请先运行 `scripts/download-pdfium.ps1`。

```powershell
corex morph meta --path ./doc.pdf
corex morph merge --paths a.pdf,b.pdf --dest ./merged.pdf
corex morph split --path ./doc.pdf --ranges 1-3,5-7 --dir ./parts
corex morph split --path ./doc.pdf --limit 10 --dir ./parts
corex morph reorder --path ./doc.pdf --order 2,0,1 --dest ./reordered.pdf
corex morph extract --path ./doc.pdf --pages 0,2,4 --dest ./part.pdf
```

Pipeline / IPC：`action: meta` + `args: { "path": "D:/doc.pdf" }` 等；路径字段支持 `${var.*}` / `${steps.*.artifact.path}`。

---

## 捕获 (capture)

子命令：`screenshot`（全屏截图）、`tape`（录屏，预留未实现）、`monitors`（显示器列表）、`windows`（窗口列表）、`crop`（区域裁剪）、`clipboard`（剪贴板区域）。

| 子命令 | 关键参数 | 说明 |
| ------ | -------- | ---- |
| `screenshot` | `--to` | 输出目录 |
| `tape` | （无） | 录屏占位，尚未实现 |
| `crop` | `--source`、`--to` | 源图路径 + 输出目录 |
| `clipboard` | `--source` | 剪贴板图片路径 |

```powershell
corex capture screenshot --to C:\Screenshots
corex capture crop --source C:\in.png --to C:\out --x 0 --y 0 --w 800 --h 600
```

IPC 截图：`module:"capture"` + `action:"screenshot"` + `args:{"to":"C:/Screenshots"}`。破坏性变更见 [docs/breaking-changes.md](docs/breaking-changes.md)。

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
| `shade` | （无 action）扁平 `from` / `to` |
| `compression` | `action` + `format` + 扁平 params |
| `generate` | `action: path\|uuid\|cvid` + 扁平 params |
| `exec` | `action: run` + 扁平 params |
| `engine` | `action: suggestion` + 扁平 params |
| `bootstrap` | `action: env\|inspect\|force` |
| `capture` | `action: screenshot\|tape\|crop\|…` |
| `codec` | `action` + `algorithm` + 扁平 params |
| `scan` | `action: os` |
| `morph` | `action: meta\|merge\|…` |

步骤：`module` + 可选路由字段 + `params`（= IPC args，仅 flags）。变量：`${var.*}`、`${steps.*.artifact.*}`、`${env.*}`。

```yaml
- id: capture_screen
  module: capture
  action: screenshot
  params:
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
        action: path
        description: 生成路径列表
        depends_on: [step_copy]
        params:
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
        action: compress
        format: zip
        description: 打包
        depends_on: [step_copy]
        params:
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
