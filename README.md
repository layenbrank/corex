# CoreX

一个面向 H5+ 应用构建的自动化任务调度 CLI 工具，支持文件复制、目录清理、路径生成、压缩打包、Pipeline 编排和 cron 定时任务。

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
| `corex compression`                 | 将目录打包为 ZIP/WGT      |
| `corex screenshot`                  | 截图                      |
| `corex bootstrap env/inspect/force` | 环境初始化与检查          |
| `corex pipeline`                    | 执行 YAML 定义的 Pipeline |
| `corex schedule run/generate/cron`  | 任务调度器                |

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
  action: file
  params:
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

将目录中所有文件打包为 ZIP 文件（适用于 H5+ `.wgt` 构建产物）。

### 参数

| 参数     | 缩写 | 必填 | 说明               |
| -------- | ---- | ---- | ------------------ |
| `--from` | `-f` | ✓    | 源目录路径         |
| `--to`   | `-t` | ✓    | 输出压缩包文件路径 |

### 使用示例

```powershell
corex compression -f C:\app\dist -t C:\app\release\app.wgt
```

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
```

### YAML 配置格式

```yaml
# 全局变量（所有步骤中可用 ${var.name} 引用）
variables:
  src_dir: './src'
  dist_dir: './dist'

pipelines:
  - id: build-pipeline
    description: 构建流水线
    mode: sequential # sequential（顺序）| parallel（并发）
    schedule: '0 8 * * *' # 可选：cron 定时执行
    steps:
      - id: step_copy
        module: copy
        description: 复制源文件
        params:
          from: '${var.src_dir}'
          to: '${var.dist_dir}'
          empty: false
          includes: [] # 白名单（空 = 全部）
          excludes: ['*.log', 'node_modules']

      - id: step_generate
        module: generate
        action: path
        description: 生成路径列表
        params:
          from: '${step_copy.output}' # 引用前序步骤输出
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
        params:
          from: '${var.dist_dir}'
          to: './release/app.wgt'
```

### 执行模式

| 模式         | 说明                                          |
| ------------ | --------------------------------------------- |
| `sequential` | 顺序执行，步骤间可传递数据（默认）            |
| `parallel`   | 并发执行（tokio），步骤间独立，禁止跨步骤引用 |

### 变量引用语法

| 语法                      | 说明                   |
| ------------------------- | ---------------------- |
| `${var.name}`             | 引用全局变量           |
| `${step_id.output}`       | 引用前序步骤的输出路径 |
| `${step_id.metadata.key}` | 引用前序步骤的元数据   |

> **注意**：`parallel` 模式下仅支持 `${var.name}`，禁止跨步骤引用。

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
