# 内置 Action（v5）

> **文档导航：** [文档中心](../README.md) · [指令与输入配置](../guide/指令与输入配置.md)

由 `corex-registry` 内置模块注册的 Action ID（`crates/registry/src/builtin/`）。功能门控为 `act-*`（默认 daemon/CLI 构建通过 `full` 启用）。

在 Windows 上，`windows` crate 的 feature 按**门控**启用（而非全局）：

| 捆绑 / 门控 | 引入内容 |
|---------------|--------|
| `win32-base` | Win32 消息（`act-ui` / 进程辅助） |
| `win32-process` | + ToolHelp（`act-shell` / `act-exec`） |
| `winrt-ocr` | WinRT Imaging/OCR/Storage（`act-capture`） |

工作区中 `windows` 为 `optional` + `default-features = false`；仅已启用的门控会编译对应 API。

通过 Directive YAML（`action: <id>`）或 IPC `{"type":"invoke","action":"<id>","params":{...}}` 调用。

## 目录

| Action ID | 功能门控 | 必填 / 常用参数 | 说明 |
|-----------|---------|--------------------------|--------|
| `shell.run` | `act-shell` | `command` (str)；`args?`、`cwd?`、`host?`、`allow_nonzero?` | 进程启动器（门面）；始终返回 `{stdout,stderr,exit_code,success}` |
| `http.send` | `act-http` | `url`；`method?` (GET)、`params?`/`query?`、`headers?`、`token?`、`auth?`、`body?`、`json?`、`form?`、`timeout_ms?`、`follow_redirects?` | HTTP 客户端（curl/fetch 风格） |
| `clipboard.get` | `act-clipboard` | `format?` (`text` \| `image`) | 读取剪贴板 |
| `clipboard.set` | `act-clipboard` | `format?`；`text?`；`file?` (image) | 写入剪贴板 |
| `notify.send` | `act-notify` | `summary`；`body?`、`appname?` (corex) | 桌面通知 |
| `file.read` | `act-file` | `path` | 读取文件 → 字符串 |
| `file.write` | `act-file` | `path`；`content?`；`mode?` (`overwrite` \| `replace_between` \| `regex` \| `json_set`)；`backup?` | 写入或补丁文件 |
| `file.copy` | `act-file` | `from`、`to` | 复制文件 |
| `file.delete` | `act-file` | `path` | 删除文件 |
| `template.render` | `act-template` | `template`；`context?` (map) | MiniJinja 渲染 |
| `cron.schedule` | `act-cron` | `expr`；`directive?` | 在活动的 `corex cron` 监督进程上注册 cron 任务 |
| `keyring.get` | `act-keyring` | `service`、`user` | 读取系统钥匙串 |
| `keyring.set` | `act-keyring` | `service`、`user`、`password` | 写入系统钥匙串 |
| `copy.run` | `act-copy` | `from`、`to`；`empty?`、`includes?`、`excludes?` | 目录树 / 过滤复制 |
| `scrub.run` | `act-scrub` | `source`、`target`；`recursive?` | 路径清理 / 消毒 |
| `shade.convert` | `act-shade` | `from`、`to`；`format?`、`quality?` | 图像转换 |
| `compression.compress` | `act-compression` | `from`、`to`；`format?` (zip)、`level?`、`includes?`、`excludes?` | zip / tar.gz；**`7z` 软失败**（错误：当前构建未启用） |
| `compression.decompress` | `act-compression` | `from`、`to`；`format?` | zip / tar.gz；**`7z` 同样软失败** |
| `generate.uuid` | `act-generate` | `count?`、`uppercase?` | UUID（可多个） |
| `generate.cvid` | `act-generate` | — | 紧凑 ID |
| `generate.timestamp` | `act-generate` | `format?`、`utc?` | 当前时间 `{ value, unix, iso8601 }` |
| `generate.path` | `act-generate` | `from`、`to`、`transform`；… | 路径变换 / 重命名辅助 |
| `exec.run` | `act-exec` | `script` (path)；`args?`、`cwd?`、`host?`、`allow_nonzero?` | 脚本文件运行器（与 `shell.run` 共用同一启动内核） |
| `bootstrap.env` | `act-bootstrap` | — | 面向 Windows 的环境引导（非 Windows 会报错） |
| `bootstrap.inspect` | `act-bootstrap` | — | 检查引导状态 |
| `bootstrap.force` | `act-bootstrap` | — | 强制引导（Windows） |
| `codec.base64.encode` | `act-codec` | `input?` / `file?`；`output?` | Base64 编码 |
| `codec.base64.decode` | `act-codec` | `input?` / `file?`；`output?` | Base64 解码 |
| `codec.hash.md5` | `act-codec` | `input?` / `file?`；`output?` | MD5 摘要 |
| `codec.json.parse` | `act-codec` | `text` | 解析 JSON 字符串 → 结构化 `Value` |
| `scan.os` | `act-scan` | — | OS / 环境扫描 |
| `capture.screenshot` | `act-capture` | `to`；`format?` (png)、`quality?` | 截图（Windows 后端） |
| `capture.clipboard` | — | — | **已移除** — 请使用 `clipboard.set` 并设置 `format: image` |
| `capture.ocr` | `act-capture` | `file`；`language?` | OCR（Windows Media OCR） |
| `capture.crop` | `act-capture` | `from`、`to`、`x`、`y`、`width`、`height` | 裁剪图像 |
| `capture.monitors` | `act-capture` | — | 列出显示器（Windows 后端） |
| `ui.window.list` | `act-ui` | — | 列出顶层窗口（`hwnd`/`title`/`class`/`pid`） |
| `ui.window.focus` | `act-ui` | `title_contains?`、`hwnd?`、`prefer_largest?`、`class_name?` | 聚焦窗口；更新 ui_session 作用域 |
| `ui.window.find` | `act-ui` | 同 focus | 查找顶层窗口 |
| `ui.window.wait` | `act-ui` | `title_contains?`、`timeout_ms`、`prefer_largest?` | 等待窗口出现 |
| `ui.element.list` | `act-ui` | `hwnd?`、`title_contains?`、`depth?`、`limit?` | 列出子元素（UIA） |
| `ui.element.find` | `act-ui` | `name?`、`name_contains?`、`automation_id?`、`control_type?`、`selectors?` | 查找应用内元素 |
| `ui.element.exists` | `act-ui` | 同 find | 探测 `{ found, element? }` |
| `ui.element.click` | `act-ui` | 同 find + `safe?`（默认 true） | 点击元素（safe 时等待可用） |
| `ui.element.wait` | `act-ui` | 同 find + `state?`、`timeout_ms`、`poll_interval_ms?` | 等待 `present` / `absent` / `enabled` |
| `ui.wait` | `act-ui` | `ms` | 固定休眠（回退；受 `ui_max_settle_ms` 上限约束） |
| `ui.click` | `act-ui` | `x`、`y` | 在屏幕坐标点击 |
| `ui.type` | `act-ui` | `text` | 输入文本 |
| `ui.key` | `act-ui` | `keys` | 按键组合（`Enter`、`Ctrl+F` 等） |
| `morph.meta` | `act-morph` | `path` | PDF 元数据（启用时需要 pdfium） |
| `morph.render` | `act-morph` | `path`；`offset?`、`scale?` | PDF 页面渲染 |
| `morph.export` | `act-morph` | `src`、`dest` | PDF 导出 |
| `morph.merge` | `act-morph` | `paths`、`dest` | 合并 PDF |
| `morph.split` | `act-morph` | `path`、`dir`；`limit?`、`ranges?` | 拆分 PDF |

> **可运行示例：** 多步流程见 [`examples/directives/`](../../examples/directives/README.md)；单 Action 存根见 [`examples/actions/`](../../examples/actions/README.md)。

## 各 Action 示例

每条包含最小 Directive 步骤、IPC invoke 行，以及可运行文件链接。平台标签：**Win** = 仅 Windows，**pdfium** = 需要原生 pdfium，**cron sup** = 需要 `corex cron run`。

### 系统与 shell

#### `shell.run`

- 示例：[`examples/actions/shell.run.yaml`](../../examples/actions/shell.run.yaml) · [`shell-host-demo.yaml`](../../examples/directives/shell-host-demo.yaml)

```yaml
- id: run
  action: shell.run
  params:
    command: echo
    args: ["hello"]
    host: none
    wait: sync
  save_to: out
```

IPC: `{"type":"invoke","action":"shell.run","params":{"command":"echo","args":["hello"]}}`

#### `exec.run`

- 示例：[`examples/actions/exec.run.yaml`](../../examples/actions/exec.run.yaml) · [`exec-run-demo.yaml`](../../examples/directives/exec-run-demo.yaml)

```yaml
- id: seed
  action: file.write
  params:
    path: "{{env.TEMP}}/demo.ps1"
    content: "Write-Output hello"
    mode: overwrite
    create_dirs: true
- id: run
  action: exec.run
  params:
    script: "{{env.TEMP}}/demo.ps1"
    host: auto
  save_to: out
```

IPC: `{"type":"invoke","action":"exec.run","params":{"script":"C:/Temp/demo.ps1","host":"auto"}}`

#### `bootstrap.env` / `bootstrap.inspect` / `bootstrap.force` (**Win**)

- 示例：[`examples/actions/bootstrap.inspect.yaml`](../../examples/actions/bootstrap.inspect.yaml) · [`bootstrap-demo.yaml`](../../examples/directives/bootstrap-demo.yaml)

```yaml
- id: inspect
  action: bootstrap.inspect
  params: {}
  save_to: out
```

IPC: `{"type":"invoke","action":"bootstrap.inspect","params":{}}`

#### `scan.os`

- 示例：[`examples/actions/scan.os.yaml`](../../examples/actions/scan.os.yaml) · [`scan-env-demo.yaml`](../../examples/directives/scan-env-demo.yaml)

```yaml
- id: scan
  action: scan.os
  params: {}
  save_to: info
```

IPC: `{"type":"invoke","action":"scan.os","params":{}}`

#### `cron.schedule` (**cron sup**)

- 示例：[`examples/actions/cron.schedule.yaml`](../../examples/actions/cron.schedule.yaml) · [`cron-schedule-demo.yaml`](../../examples/directives/cron-schedule-demo.yaml)

```yaml
- id: reg
  action: cron.schedule
  params:
    expr: "0 0 12 * * *"
    directive: hello
  save_to: job
```

IPC: `{"type":"invoke","action":"cron.schedule","params":{"expr":"0 0 12 * * *","directive":"hello"}}`

### 网络与模板

#### `http.send`

- 示例：[`examples/actions/http.send.yaml`](../../examples/actions/http.send.yaml) · [`http-post-json.yaml`](../../examples/directives/http-post-json.yaml)

```yaml
- id: get
  action: http.send
  params:
    url: "https://httpbin.org/get"
    method: GET
    timeout_ms: 15000
  save_to: resp
```

IPC: `{"type":"invoke","action":"http.send","params":{"url":"https://httpbin.org/get"}}`

#### `template.render`

- 示例：[`examples/actions/template.render.yaml`](../../examples/actions/template.render.yaml) · [`hello.yaml`](../../examples/directives/hello.yaml)

```yaml
- id: tpl
  action: template.render
  params:
    template: "Hello, {{ name }}!"
    context: { name: "{{input.who}}" }
  save_to: message
```

IPC: `{"type":"invoke","action":"template.render","params":{"template":"Hi","context":{"name":"x"}}}`

### 文件系统

#### `file.read` / `file.write` / `file.copy` / `file.delete`

- 示例：[`file.write.yaml`](../../examples/actions/file.write.yaml) · [`file.copy.yaml`](../../examples/actions/file.copy.yaml) · [`file-ops-demo.yaml`](../../examples/directives/file-ops-demo.yaml) · [`file-write-modes.yaml`](../../examples/directives/file-write-modes.yaml)

```yaml
- id: write
  action: file.write
  params:
    path: "{{env.TEMP}}/out.txt"
    content: "hello"
    mode: overwrite
    create_dirs: true
```

```yaml
- id: copy
  action: file.copy
  params:
    from: "./examples/directives/hello.yaml"
    to: "{{env.TEMP}}/hello-copy.yaml"
```

IPC: `{"type":"invoke","action":"file.copy","params":{"from":"a.txt","to":"b.txt"}}`

#### `copy.run`

- 示例：[`examples/actions/copy.run.yaml`](../../examples/actions/copy.run.yaml) · [`copy-demo.yaml`](../../examples/directives/copy-demo.yaml)

```yaml
- id: copy
  action: copy.run
  params:
    from: "./examples/directives"
    to: "{{env.TEMP}}/copy-out"
    empty: false
  save_to: result
```

#### `scrub.run`

- 示例：[`examples/actions/scrub.run.yaml`](../../examples/actions/scrub.run.yaml) · [`scrub-demo.yaml`](../../examples/directives/scrub-demo.yaml)

```yaml
- id: scrub
  action: scrub.run
  params:
    source: "{{env.TEMP}}/work"
    target: "stale.tmp"
    recursive: true
```

#### `shade.convert`

- 示例：[`examples/actions/shade.convert.yaml`](../../examples/actions/shade.convert.yaml) · [`shade-demo.yaml`](../../examples/directives/shade-demo.yaml)

```yaml
- id: convert
  action: shade.convert
  params:
    from: "{{env.TEMP}}/in.png"
    to: "{{env.TEMP}}/out.jpg"
    format: jpeg
    quality: 85
```

#### `compression.compress` / `compression.decompress`

- 示例：[`examples/actions/compression.compress.yaml`](../../examples/actions/compression.compress.yaml) · [`compression-demo.yaml`](../../examples/directives/compression-demo.yaml)

```yaml
- id: zip
  action: compression.compress
  params:
    from: "./examples/directives/hello.yaml"
    to: "{{env.TEMP}}/demo.zip"
    format: zip
    level: 6
```

IPC: `{"type":"invoke","action":"compression.compress","params":{"from":"dir","to":"out.zip","format":"zip"}}`

### 生成与编解码

#### `generate.uuid` / `generate.cvid` / `generate.timestamp`

- 示例：[`examples/actions/generate.uuid.yaml`](../../examples/actions/generate.uuid.yaml) · [`generate-demo.yaml`](../../examples/directives/generate-demo.yaml)

```yaml
- id: uuids
  action: generate.uuid
  params: { count: 2, uppercase: false }
  save_to: ids
```

#### `generate.path`

- 示例：[`examples/actions/generate.path.yaml`](../../examples/actions/generate.path.yaml) · [`generate-path-demo.yaml`](../../examples/directives/generate-path-demo.yaml)

```yaml
- id: paths
  action: generate.path
  params:
    from: "./examples/directives"
    to: "{{env.TEMP}}/paths.txt"
    transform: "{{path}}"
    includes: ["*.yaml"]
```

#### `codec.base64.encode` / `decode` / `codec.hash.md5` / `codec.json.parse`

- 示例：[`examples/actions/codec.base64.encode.yaml`](../../examples/actions/codec.base64.encode.yaml) · [`codec-pipeline.yaml`](../../examples/directives/codec-pipeline.yaml)

```yaml
- id: b64
  action: codec.base64.encode
  params: { input: "corex" }
  save_to: encoded
```

IPC: `{"type":"invoke","action":"codec.json.parse","params":{"text":"{\"a\":1}"}}`

### 桌面与密钥

#### `clipboard.get` / `clipboard.set`

- 示例：[`examples/actions/clipboard.set.yaml`](../../examples/actions/clipboard.set.yaml) · [`clipboard-notify.yaml`](../../examples/directives/clipboard-notify.yaml)

```yaml
- id: clip
  action: clipboard.set
  params: { format: text, text: "hello" }
```

#### `notify.send`

- 示例：[`examples/actions/notify.send.yaml`](../../examples/actions/notify.send.yaml)

```yaml
- id: toast
  action: notify.send
  params: { summary: "Corex", body: "done" }
```

#### `keyring.get` / `keyring.set`

- 示例：[`examples/actions/keyring.set.yaml`](../../examples/actions/keyring.set.yaml) · [`keyring-demo.yaml`](../../examples/directives/keyring-demo.yaml)

```yaml
- id: store
  action: keyring.set
  params:
    service: my-app
    user: demo
    password: "CHANGE-ME"
```

声明权限时需要 `permissions.secret: true`。

### 捕获（截图 / 显示器 / OCR 为 **Win**）

#### `capture.screenshot` / `capture.crop` / `capture.monitors` / `capture.ocr`

- 示例：[`examples/actions/capture.screenshot.yaml`](../../examples/actions/capture.screenshot.yaml) · [`capture-demo.yaml`](../../examples/directives/capture-demo.yaml)

```yaml
- id: shot
  action: capture.screenshot
  params:
    to: "{{env.TEMP}}/shot.png"
    format: png
  save_to: path
```

IPC: `{"type":"invoke","action":"capture.screenshot","params":{"to":"C:/Temp/shot.png"}}`

### UI 自动化（**Win**）

全部 `ui.*` Action：[`ui-smoke-notepad.yaml`](../../examples/directives/ui-smoke-notepad.yaml)（13 个 Action） · 存根：[`ui.window.list.yaml`](../../examples/actions/ui.window.list.yaml)

```yaml
- id: wins
  action: ui.window.list
  params: {}
  save_to: windows
```

```yaml
- id: click
  action: ui.element.click
  params:
    name_contains: "编辑"
    safe: true
  save_to: clicked
```

声明权限时需要 `permissions.ui: true`。

### PDF / morph

#### `morph.export` / `morph.merge` / `morph.split`

- 示例：[`examples/actions/morph.export.yaml`](../../examples/actions/morph.export.yaml) · [`morph-demo.yaml`](../../examples/directives/morph-demo.yaml)

```yaml
- id: export
  action: morph.export
  params:
    src: "{{input.pdf_path}}"
    dest: "{{env.TEMP}}/copy.pdf"
```

#### `morph.meta` / `morph.render` (**pdfium**)

- 示例：[`examples/actions/morph.meta.yaml`](../../examples/actions/morph.meta.yaml)

```yaml
- id: meta
  action: morph.meta
  params: { path: "{{input.pdf_path}}" }
```

当前构建未捆绑 pdfium 时，这些 Action 会返回错误。

## 进程启动（`shell.run` / `exec.run`）

两个门面共用同一套**进程启动内核**（`process_launch`）。二者仅在产品意图上不同：

| 门面 | 必填参数 | 目标类型 | 典型用途 |
|--------|----------------|-------------|-------------|
| `shell.run` | `command` | 命令 / 二进制 | `npm`、`fnm`、绝对路径 `.exe` |
| `exec.run` | `script` | 脚本文件（必须存在） | 你指定的 `.bat` / `.ps1` / `.sh` 路径 |

**`host`**（可选，默认 `auto`）：

| 取值 | 行为 |
|-------|----------|
| `none` | 直接 `Command::new(program)` + args |
| `cmd` | Windows `cmd /C` …；Unix `sh -c` / 脚本路径 |
| `powershell` | Windows PowerShell 5.x `-File` / `-Command` |
| `pwsh` | PowerShell 7+ |
| `auto` | 命令 → `none`；脚本按扩展名（`.ps1`→pwsh/powershell，`.bat`/`.cmd`→cmd，`.sh`→sh） |

**GUI / 单实例**（可选）：

| 参数 | 取值 | 默认 |
|-------|--------|---------|
| `wait` | `sync` \| `detach` | `sync` |
| `if_running` | `launch` \| `skip` \| `fail` | `launch` |
| `if_running_window` | `{ title_contains, title_excludes?, prefer_largest? }` | — |

适用时返回可含 `detached`、`skipped`、`reason`、`pid`。

参见 [ui-automation.md](../topics/UI自动化.md)。

企业场景：为可审计性显式设置 `host`；可通过 `disabled_actions` 禁用任一 Action，同时保留 `permissions.shell`。

## 重要注意

1. **`cron.schedule`** — 需要活动的 `corex cron run` 监督进程；在共享的 `CronEngine` 上注册任务。
2. **`compression.*` + `7z`** — 格式可识别，但当前构建**未启用**；请使用 `zip` 或 `tar.gz`。
3. **平台相关 Action** — `capture.*`（截图/显示器）、`bootstrap.*`（env/force）以及部分 `morph.*` 路径，在原生后端 / pdfium 不可用时可能报错。
4. **运行时禁用** — 配置中的 `[plugins].disabled` / `disabled_actions` 可在注册后隐藏 Action。
5. **权限** — 当 Directive 声明任一权限标志时，未声明的类别会被拒绝（见 [directive-yaml.md](指令YAML.md)）。

## WASM 插件

第三方 `*.wasm` 组件可通过发现机制注册额外 ID。宿主 bindgen 尚不完整 — 加载失败会被跳过。参见 [plugins/README.md](../../plugins/README.md)。

## 相关文档

- [directive-yaml.md](指令YAML.md)
- [ui-automation.md](../topics/UI自动化.md)
- [ipc-protocol.md](IPC协议.md)
- [architecture.md](架构.md)
