# 内置 Action（v10）

> **文档导航：** [文档中心](../README.md) · [指令与输入配置](../guide/指令与输入配置.md)

由 `corex-registry` 内置模块注册的 Action ID（`crates/registry/src/builtin/`）。功能门控为 `act-*`（默认 daemon/CLI 构建通过 `all-actions` 启用）。

在 Windows 上，`windows` crate 的 feature 按**门控**启用（而非全局）：

| 捆绑 / 门控     | 引入内容                                   |
| --------------- | ------------------------------------------ |
| `win32-base`    | Win32 消息（`act-ui` / 进程辅助）          |
| `win32-process` | + ToolHelp（`act-shell` / `act-exec`）     |
| `winrt-ocr`     | WinRT Imaging/OCR/Storage（`act-capture`） |

工作区中 `windows` 为 `optional` + `default-features = false`；仅已启用的门控会编译对应 API。

通过 Directive YAML（`action: <id>`）或 IPC `{"type":"invoke","action":"<id>","params":{...}}` 调用。

参数类型、默认值与要声明的权限以**机器可读目录**为准：`corex actions --json`，宿主侧走 IPC
`list_actions`（两者由 `corex_registry::catalog` 同一份实现产出）。本表是给人读的散文版，
同族动作会合并成一行，不要拿它当调用契约。

## 目录

| Action ID                             | 功能门控          | 必填 / 常用参数                                                                                                                                                                                | 说明                                                                                            |
| ------------------------------------- | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `shell.run`                           | `act-shell`       | `command` (str)；`args?`、`cwd?`、`host?`、`allow_nonzero?`、`input?`、`wait?`                                                                                                                 | 进程启动器（门面）；始终返回 `{stdout,stderr,exit_code,success}`                                |
| `http.send`                           | `act-http`        | `url`；`method?` (GET)、`params?`/`query?`、`headers?`、`token?`、`auth?`、`body?`、`json?`、`form?`、`multipart?`、`timeout_ms?`、`follow_redirects?`、`response?`、`encoding?`、`max_bytes?` | HTTP 客户端（curl/fetch 风格；multipart 可带文件部件与字节区间；`response: binary` 拿原始字节） |
| `clipboard.get`                       | `act-clipboard`   | `format?` (`text` \| `image`)                                                                                                                                                                  | 读取剪贴板                                                                                      |
| `clipboard.set`                       | `act-clipboard`   | `format?`；`text?`；`file?` (image)                                                                                                                                                            | 写入剪贴板                                                                                      |
| `notify.send`                         | `act-notify`      | `summary`；`body?`、`appname?` (corex)                                                                                                                                                         | 桌面通知                                                                                        |
| `file.read`                           | `act-file`        | `path`；`mode?` (`content` \| `lines` \| `stat` \| `exists` \| `bytes`)；`start_line?`/`end_line?`/`limit?`/`offset?`/`length?`/`max_bytes?`                                                   | 全文、行窗、二进制段或轻量元数据                                                                |
| `file.write`                          | `act-file`        | `path`；`mode?` (`overwrite` \| `append` \| `str_replace` \| `replace_lines` \| `insert_lines` \| `delete_lines` \| `splice` \| `regex` \| `json_set` \| `patch`)；`newline?`；`backup?`       | 写入 / 迷你 IDE 局部更新；`content` 也可直接接 `Bytes`（此时只支持 `overwrite` / `append`）     |
| `file.update`                         | `act-file`        | `from`、`to`；`create_dirs?`                                                                                                                                                                   | 重命名 / 移动文件                                                                               |
| `file.copy`                           | `act-file`        | `from`、`to`                                                                                                                                                                                   | 复制文件                                                                                        |
| `file.remove`                         | `act-file`        | `path`                                                                                                                                                                                         | 删除文件（目录则递归删除）                                                                      |
| `dir.write`                           | `act-file`        | `path`；`parents?`；`exist_ok?`                                                                                                                                                                | 创建目录                                                                                        |
| `dir.read`                            | `act-file`        | `path`；`mode?` (`flat` \| `tree`)；`max_depth?`；`max_entries?`                                                                                                                               | 列举目录                                                                                        |
| `dir.update`                          | `act-file`        | `from`、`to`；`create_dirs?`                                                                                                                                                                   | 重命名 / 移动目录                                                                               |
| `dir.remove`                          | `act-file`        | `path`；`recursive?`                                                                                                                                                                           | 删除目录（默认仅空目录）                                                                        |
| `template.render`                     | `act-template`    | `template`；`context?` (map)                                                                                                                                                                   | MiniJinja 渲染                                                                                  |
| `cron.schedule`                       | `act-cron`        | `expr`；`timezone?`；`directive?`                                                                                                                                                              | 在活动的 `corex cron` 监督进程上注册 cron 任务                                                  |
| `keyring.get`                         | `act-keyring`     | `service`、`user`                                                                                                                                                                              | 读取系统钥匙串                                                                                  |
| `keyring.set`                         | `act-keyring`     | `service`、`user`、`password`                                                                                                                                                                  | 写入系统钥匙串                                                                                  |
| `copy.run`                            | `act-copy`        | `from`、`to`；`empty?`、`includes?`、`excludes?`                                                                                                                                               | 目录树 / 过滤复制（按字节上报进度）                                                             |
| `scrub.run`                           | `act-scrub`       | `source`、`target`；`recursive?`                                                                                                                                                               | 路径清理 / 消毒                                                                                 |
| `shade.convert`                       | `act-shade`       | `from`、`to`；`format?`、`quality?`                                                                                                                                                            | 图像转换                                                                                        |
| `compression.compress`                | `act-compression` | `from`、`to`；`format?` (zip)、`level?`、`includes?`、`excludes?`                                                                                                                              | zip / tar.gz；**`7z` 软失败**（错误：当前构建未启用）                                           |
| `compression.decompress`              | `act-compression` | `from`、`to`；`format?`                                                                                                                                                                        | zip / tar.gz；**`7z` 同样软失败**                                                               |
| `generate.uuid`                       | `act-generate`    | `count?`、`uppercase?`                                                                                                                                                                         | UUID（可多个）                                                                                  |
| `generate.cvid`                       | `act-generate`    | —                                                                                                                                                                                              | 紧凑 ID                                                                                         |
| `generate.timestamp`                  | `act-generate`    | `format?`、`utc?`                                                                                                                                                                              | 当前时间 `{ value, unix, iso8601 }`                                                             |
| `generate.hash`                       | `act-generate`    | `algorithm?` (sha256；可为数组)、`encoding?` (`hex`)、`path?`/`text?`；`offset?`、`length?`                                                                                                    | 流式摘要（sha256 / sha512 / md5），多算法一遍算完，可只算一段                                   |
| `generate.chunks`                     | `act-generate`    | `path`、`chunk`；`index?`、`offset?`、`length?`                                                                                                                                                | 只切块：每片 `index`/`offset`/`length`（不算摘要；摘要用 `generate.hash`）                      |
| `generate.path`                       | `act-generate`    | `from`、`to`、`transform`；…                                                                                                                                                                   | 路径变换 / 重命名辅助                                                                           |
| `exec.run`                            | `act-exec`        | `script` (path)；`args?`、`cwd?`、`host?`、`allow_nonzero?`、`input?`、`wait?`                                                                                                                 | 脚本文件运行器（与 `shell.run` 共用同一启动内核）                                               |
| `bootstrap.env`                       | `act-bootstrap`   | —                                                                                                                                                                                              | 面向 Windows 的环境引导（非 Windows 会报错）                                                    |
| `bootstrap.inspect`                   | `act-bootstrap`   | —                                                                                                                                                                                              | 检查引导状态                                                                                    |
| `bootstrap.force`                     | `act-bootstrap`   | —                                                                                                                                                                                              | 强制引导（Windows）                                                                             |
| `codec.base64.encode`                 | `act-codec`       | `input?` / `file?`；`output?`                                                                                                                                                                  | Base64 编码                                                                                     |
| `codec.base64.decode`                 | `act-codec`       | `input?` / `file?`；`output?`                                                                                                                                                                  | Base64 解码                                                                                     |
| `codec.hash.md5`                      | `act-codec`       | `input?` / `file?`；`output?`                                                                                                                                                                  | MD5 摘要                                                                                        |
| `codec.json.parse`                    | `act-codec`       | `text`                                                                                                                                                                                         | 解析 JSON 字符串 → 结构化 `Value`                                                               |
| `codec.json.pick`                     | `act-codec`       | `value`、`pointer`；`default?`                                                                                                                                                                 | 从已解析的结构里按 JSON Pointer 取子节点（取不到即 `Null`）                                     |
| `codec.url.encode`                    | `act-codec`       | `input`；`mode?` (`component` \| `uri`)                                                                                                                                                        | 百分号编码（encodeURIComponent / encodeURI 语义）                                               |
| `codec.url.decode`                    | `act-codec`       | `input`；`plus_as_space?`                                                                                                                                                                      | 百分号解码；表单串把 `+` 当空格                                                                 |
| `html.select`                         | `act-html`        | `html`、`selector`；`attr?`、`all?`、`limit?`、`trim?`、`base?`、`absolute?`                                                                                                                   | CSS 选择器取元素文本或属性                                                                      |
| `html.links`                          | `act-html`        | `html`；`selector?` (`a[href]`)、`attr?`、`base?`、`absolute?`、`junk?`、`unique?`                                                                                                             | 取链接，可补绝对地址、去重、滤掉锚点/脚本                                                       |
| `html.text`                           | `act-html`        | `html`；`selector?` (`body`)、`separator?`                                                                                                                                                     | 去标签取正文（跳过 `script` / `style`）                                                         |
| `scan.os`                             | `act-scan`        | —                                                                                                                                                                                              | OS / 环境扫描                                                                                   |
| `capture.screenshot`                  | `act-capture`     | `to`；`format?` (png)、`quality?`                                                                                                                                                              | 截图（Windows 后端）                                                                            |
| `capture.clipboard`                   | —                 | —                                                                                                                                                                                              | **已移除** — 请使用 `clipboard.set` 并设置 `format: image`                                      |
| `capture.ocr`                         | `act-capture`     | `file`；`language?`                                                                                                                                                                            | OCR（Windows Media OCR）                                                                        |
| `capture.crop`                        | `act-capture`     | `from`、`to`、`x`、`y`、`width`、`height`                                                                                                                                                      | 裁剪图像                                                                                        |
| `capture.monitors`                    | `act-capture`     | —                                                                                                                                                                                              | 列出显示器（Windows 后端）                                                                      |
| `capture.find`                        | `act-capture`     | `haystack`、`needle`；`threshold?`、`step?`、区域                                                                                                                                              | 模板匹配找图（逐行上报扫描进度）                                                                |
| `ui.window.list`                      | `act-ui`          | —                                                                                                                                                                                              | 列出顶层窗口（`hwnd`/`title`/`class`/`pid`）                                                    |
| `ui.window.desktop`                   | `act-ui`          | —                                                                                                                                                                                              | 桌面图标 ListItem                                                                               |
| `ui.window.focus`                     | `act-ui`          | `title_contains?`、`hwnd?`、`prefer_largest?`、`class_name?`                                                                                                                                   | 聚焦窗口；更新 ui_session 作用域                                                                |
| `ui.window.find`                      | `act-ui`          | 同 focus                                                                                                                                                                                       | 查找顶层窗口                                                                                    |
| `ui.window.wait`                      | `act-ui`          | `title_contains?`、`timeout_ms`、`prefer_largest?`                                                                                                                                             | 等待窗口出现                                                                                    |
| `ui.element.list`                     | `act-ui`          | `hwnd?`、`title_contains?`、`depth?`、`limit?`                                                                                                                                                 | 列出子元素（UIA）                                                                               |
| `ui.element.find`                     | `act-ui`          | `name?`、`name_contains?`、`automation_id?`、`control_type?`、`selectors?`                                                                                                                     | 查找应用内元素                                                                                  |
| `ui.element.get` / `set`              | `act-ui`          | 同 find；`set` 需 `value`                                                                                                                                                                      | ValuePattern 读写                                                                               |
| `ui.element.exists`                   | `act-ui`          | 同 find                                                                                                                                                                                        | 探测 `{ found, element? }`                                                                      |
| `ui.element.click`                    | `act-ui`          | 同 find + `safe?`（默认 true）                                                                                                                                                                 | 点击元素（safe 时等待可用）                                                                     |
| `ui.element.wait`                     | `act-ui`          | 同 find + `state?`、`timeout_ms`、`poll_interval_ms?`                                                                                                                                          | 等待 `present` / `absent` / `enabled`                                                           |
| `ui.element.point`                    | `act-ui`          | `x`、`y`                                                                                                                                                                                       | 屏幕坐标命中元素                                                                                |
| `ui.element.inspect`                  | `act-ui`          | `scope_hwnd?`                                                                                                                                                                                  | 交互式 Inspect                                                                                  |
| `ui.wait`                             | `act-ui`          | `ms`                                                                                                                                                                                           | 固定休眠（回退；受 `ui_settle_limit` 上限约束）                                                 |
| `ui.click`                            | `act-ui`          | `x`、`y`；`button?`、`clicks?`                                                                                                                                                                 | 屏幕坐标点击（可双击/右键）                                                                     |
| `ui.scroll`                           | `act-ui`          | `dy?`、`dx?`；`x?`、`y?`                                                                                                                                                                       | 滚轮                                                                                            |
| `ui.drag`                             | `act-ui`          | `from_x/y`、`to_x/y`；`steps?`                                                                                                                                                                 | 拖拽                                                                                            |
| `ui.type`                             | `act-ui`          | `text`                                                                                                                                                                                         | 输入文本                                                                                        |
| `ui.key`                              | `act-ui`          | `keys`                                                                                                                                                                                         | 按键组合（`Enter`、`Ctrl+F` 等）                                                                |
| `dialog.alert` / `confirm` / `prompt` | `act-sys`         | `message`；`title?`                                                                                                                                                                            | 原生对话框                                                                                      |
| `url.open`                            | `act-sys`         | `url`                                                                                                                                                                                          | ShellExecute 打开                                                                               |
| `process.list` / `kill`               | `act-sys`         | list:`name_contains?`；kill:`pid`                                                                                                                                                              | 进程枚举/结束                                                                                   |
| `morph.meta`                          | `act-morph`       | `path`                                                                                                                                                                                         | **未实现** —— 调用即返回错误                                                                    |
| `morph.render`                        | `act-morph`       | `path`；`offset?`、`scale?`                                                                                                                                                                    | **未实现** —— 调用即返回错误                                                                    |
| `morph.export`                        | `act-morph`       | `src`、`dest`                                                                                                                                                                                  | PDF 导出（按字节上报进度）                                                                      |
| `morph.merge`                         | `act-morph`       | `paths`、`dest`                                                                                                                                                                                | 合并 PDF（按输入文件数上报进度）                                                                |
| `morph.split`                         | `act-morph`       | `path`、`dir`；`limit?`、`ranges?`                                                                                                                                                             | 拆分 PDF（按输出分段数上报进度）                                                                |

> **可运行示例：** 多步流程见 [`examples/directives/`](../../examples/directives/README.md)；单 Action 存根见 [`examples/actions/`](../../examples/actions/README.md)。

## 各 Action 示例

每条包含最小 Directive 步骤、IPC invoke 行，以及可运行文件链接。平台标签：**Win** = 仅 Windows，**cron sup** = 需要 `corex cron run`。

### 系统与 shell

#### `shell.run`

- 示例：[`examples/actions/shell.run.yaml`](../../examples/actions/shell.run.yaml) · [`shell-host-demo.yaml`](../../examples/directives/shell-host-demo.yaml) · [`shell-input-demo.yaml`](../../examples/directives/shell-input-demo.yaml)

```yaml
- id: run
  action: shell.run
  params:
    command: echo
    args: ['hello']
    host: none
    wait: sync
  save_to: out
```

IPC: `{"type":"invoke","action":"shell.run","params":{"command":"echo","args":["hello"]}}`

**交互式提问**：默认子进程继承当前终端，可以手动回答（`corepack`/`pnpm`、`Read-Host`、`set /p` 之类）。
要无人值守地**自动应答**，用 `input` 把内容写进子进程 stdin（写完即关闭，子进程会读到 EOF）：

```yaml
- id: ask
  action: shell.run
  params:
    host: powershell
    command: '$line = [Console]::In.ReadLine(); Write-Output "answered: $line"'
    input: "{{input.answer}}\n"
  save_to: out
```

> `input` 只在 `wait: sync` 下生效（`detach` 不能写 stdin）。从 daemon 运行时既无终端也无 `input`，
> 子进程的提问会一直等下去，只受 `step_timeout` 兜底——交互式命令务必显式给 `input`。
> 另一个更省事的办法是直接消灭提问：如 `COREPACK_ENABLE_DOWNLOAD_PROMPT=0`。

#### `exec.run`

- 示例：[`examples/actions/exec.run.yaml`](../../examples/actions/exec.run.yaml) · [`exec-run-demo.yaml`](../../examples/directives/exec-run-demo.yaml)

```yaml
- id: seed
  action: file.write
  params:
    path: '{{env.TEMP}}/demo.ps1'
    content: 'Write-Output hello'
    mode: overwrite
    create_dirs: true
- id: run
  action: exec.run
  params:
    script: '{{env.TEMP}}/demo.ps1'
    host: auto
  save_to: out
```

IPC: `{"type":"invoke","action":"exec.run","params":{"script":"C:/Temp/demo.ps1","host":"auto"}}`

与 `shell.run` 一样支持 `input`（写入子进程 stdin）来自动应答脚本里的交互式提问。

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
    expr: '0 0 12 * * *'
    timezone: local
    directive: hello
  save_to: job
```

IPC: `{"type":"invoke","action":"cron.schedule","params":{"expr":"0 0 12 * * *","timezone":"local","directive":"hello"}}`

### 网络与模板

#### `http.send`

- 示例：[`examples/actions/http.send.yaml`](../../examples/actions/http.send.yaml) · [`http-post-json.yaml`](../../examples/directives/http-post-json.yaml) · [`html-crawl.yaml`](../../examples/directives/html-crawl.yaml)
- 进度：响应体逐块读取，`Content-Length` 作分母；分块传输（或压缩后）没有总量时只报已下载字节。
- 上传大文件用 `multipart`：字段值为标量就是文本字段，值里带 `path` 就是文件部件，
  可再给 `offset`/`length` 只发这一段字节（分片上传因此不必先落盘切片）。
  它与 `json` / `form` / `body` 四选一。
- **请求体支持 `Bytes`**：`body` 直接接 `file.read` 的 `mode: bytes` 输出。
- **响应体默认是字符串**；下载图片 / 压缩包用 `response: binary`，`body` 就是 `Bytes`，
  可直接给 `file.write` 的 `content` 落盘。`.body` 是 `Bytes` 时不要再当文本引用；
  此时 `encoding` 无意义，给了会当场报错（字节不解码）。
- 字符集：不填 `encoding` 时按 `Content-Type` → BOM → 前 4 KiB 里的 `<meta charset=…>` 猜，
  最后回退 UTF-8；中文站的 GBK 页面因此不会整页乱码，猜错就用 `encoding: gbk` 显式指定。
- 缓冲上限 `max_bytes`（默认 256 MiB）：超出即报错，避免把 10 GB 一次读进内存。
  （这是 v8 以来的新上限：以前文本响应不封顶。）
- **状态码不会让步骤失败**：4xx / 5xx 也照样返回，结论都在 `status`（数字）与 `ok`（布尔）
  里；只有连不上、超时、超 `max_bytes` 这类才真报错。所以收到响应要判 `{{resp.ok}}`——
  否则「请求被服务端拒了」会静默地跑到后面某一步才以别的错冒出来
  （分片上传的 `PATCH /upload/hash` 就是这样：400，流水线却继续到 finalize 才报「未绑定哈希」）。

> **二进制过 IPC 会变形**：`Bytes` 序列化成整数数组后，反序列化只能还原成数组。
> 两个动作都认这两种形态，所以 `http.send(response: binary)` → `file.write` 跨 daemon
> 也能跑通；但自己看响应时别再当字符串处理。

```yaml
- id: get
  action: http.send
  params:
    url: 'https://httpbin.org/get'
    method: GET
    timeout_ms: 15000
  save_to: resp
```

```yaml
- id: download
  action: http.send
  params:
    url: '{{input.image}}'
    response: binary
  save_to: got

- id: store
  action: file.write
  params:
    path: '{{env.TEMP}}/pic.png'
    content: '{{got.body}}' # 整个值就是一个 {{ }} → 保留 Bytes，不经过字符串
```

IPC: `{"type":"invoke","action":"http.send","params":{"url":"https://httpbin.org/get"}}`

#### `template.render`

- 示例：[`examples/actions/template.render.yaml`](../../examples/actions/template.render.yaml) · [`hello.yaml`](../../examples/directives/hello.yaml)

```yaml
- id: tpl
  action: template.render
  params:
    template: 'Hello, {{ name }}!'
    context: { name: '{{input.who}}' }
  save_to: message
```

**参数不经过[占位符解析器](指令YAML.md#占位符解析器)**：`template` 与 `context` 原样交给
MiniJinja，所以过滤器、`{% if %}` / `{% for %}`、`is defined` 都是完整可用的。

```yaml
- id: url
  action: template.render
  params:
    template: >-
      https://example.com/s?q={{ input.q | urlencode }}&n={{ items | length }}
      {% if input.cp is defined and input.cp | length %} &cp={{ input.cp }}{% endif %}
```

上下文里的名字与占位符解析器**同一套**，裸名同样是「先变量后输入」：

- 指令输入：`input.x`，或裸名 `x`
- 变量：`variables.x` / `var.x`（`save_to` 写进去的）
- 先前步骤的输出：`step.id.path` / `steps.id.path`
- 环境变量：`env.NAME`
- 整份文档 Directive 输入：`directive_input`
- `context` 显式传入的键覆盖上面的同名项

其余语义：

- `context` 的**字符串值会先按模板渲染一次**（`context: { name: '{{input.who}}' }` 得到的是
  替换后的值），非字符串值原样传下去。
- **变量未定义即报错**（`UndefinedBehavior::Strict`），不会静默渲染成空串。想给默认值就写
  `{{ x | default('y') }}`，想判存在就写 `{% if x is defined %}`。注意 `default` 默认只兜
  未定义；`on_error: continue` 之类写进去的是 `null`（已定义），要兜住得写
  `{{ x | default('y', true) }}`。
- 渲染对象得到 **JSON 形状**（`{"path": "out.txt", "removed": 1}`，分隔符是 MiniJinja 自己
  的 `", "`），不是解析器的 Rust 风格 `{path: out.txt}`；`null` 渲染成 MiniJinja 的
  `None`；Bytes 渲染成 `<N bytes>`。
- 过滤器按 MiniJinja 默认特性集提供（含 `urlencode`）；它与 `codec.url.encode` 的转义集
  略有差别，要精确控制编码用后者。

IPC: `{"type":"invoke","action":"template.render","params":{"template":"Hi","context":{"name":"x"}}}`

### 文件系统

#### `file.read` / `file.write` / `file.update` / `file.copy` / `file.remove`

- 示例：[`file.write.yaml`](../../examples/actions/file.write.yaml) · [`file.update.yaml`](../../examples/actions/file.update.yaml) · [`file.copy.yaml`](../../examples/actions/file.copy.yaml) · [`file.remove.yaml`](../../examples/actions/file.remove.yaml) · [`file-ops-demo.yaml`](../../examples/directives/file-ops-demo.yaml) · [`file-write-modes.yaml`](../../examples/directives/file-write-modes.yaml)

```yaml
- id: write
  action: file.write
  params:
    path: '{{env.TEMP}}/out.txt'
    content: 'hello'
    mode: overwrite
    create_dirs: true
```

```yaml
- id: edit
  action: file.write
  params:
    path: '{{env.TEMP}}/out.txt'
    mode: str_replace
    old: 'hello'
    new: 'world'
```

```yaml
- id: splice_block
  action: file.write
  params:
    path: '{{env.TEMP}}/out.txt'
    mode: splice
    start: '/* START */'
    end: '/* END */'
    content: 'NEW'
```

```yaml
- id: copy
  action: file.copy
  params:
    from: './examples/directives/hello.yaml'
    to: '{{env.TEMP}}/hello-copy.yaml'
```

IPC: `{"type":"invoke","action":"file.copy","params":{"from":"a.txt","to":"b.txt"}}`

`mode: bytes` 读二进制：给出 `offset`/`length` 就只读这一段，返回 `Bytes`（可直接当 `http.send` 的 `body`）。
上限是 `max_bytes`（默认 32 MiB）—— 大块数据应当用 `http.send` 的 multipart 文件部件直发，不必先进内存里的值。

`file.write` 的 `content` 也接受 `Bytes`：此时没有行窗、正则、换行归一那套文本处理，
只支持 `overwrite`（原子替换）与 `append`。下载二进制再落盘就是这两步：

IPC 往返后 `Bytes` 会变成整数数组，`content` 同样认得，所以这条链跨 daemon 也不断：

```yaml
- id: download
  action: http.send
  params: { url: '{{input.url}}', response: binary }
  save_to: got

- id: store
  action: file.write
  params:
    path: '{{env.TEMP}}/blob.bin'
    content: '{{got.body}}'
```

```yaml
- id: head
  action: file.read
  params:
    path: '{{env.TEMP}}/blob.bin'
    mode: bytes
    offset: 0
    length: 4096
```

`mode: stat` 返回 `{ path, name, kind, size, readonly, modified? }`。

#### `dir.write` / `dir.read` / `dir.update` / `dir.remove`

- 示例：[`dir.write.yaml`](../../examples/actions/dir.write.yaml) · [`dir.read.yaml`](../../examples/actions/dir.read.yaml) · [`dir-ops-demo.yaml`](../../examples/directives/dir-ops-demo.yaml)

```yaml
- id: list
  action: dir.read
  params:
    path: '{{env.TEMP}}/workdir'
    mode: flat # 或 tree
```

IPC: `{"type":"invoke","action":"dir.read","params":{"path":".","mode":"tree"}}`

#### `copy.run`

- 示例：[`examples/actions/copy.run.yaml`](../../examples/actions/copy.run.yaml) · [`copy-demo.yaml`](../../examples/directives/copy-demo.yaml)
- 进度：先量一遍目录树，再逐文件拷，进度报在**整棵树的字节量**上；被过滤掉的文件不计入分母。

```yaml
- id: copy
  action: copy.run
  params:
    from: './examples/directives'
    to: '{{env.TEMP}}/copy-out'
    empty: false
  save_to: result
```

#### `scrub.run`

- 示例：[`examples/actions/scrub.run.yaml`](../../examples/actions/scrub.run.yaml) · [`scrub-demo.yaml`](../../examples/directives/scrub-demo.yaml)

```yaml
- id: scrub
  action: scrub.run
  params:
    source: '{{env.TEMP}}/work'
    target: 'stale.tmp'
    recursive: true
```

#### `shade.convert`

- 示例：[`examples/actions/shade.convert.yaml`](../../examples/actions/shade.convert.yaml) · [`shade-demo.yaml`](../../examples/directives/shade-demo.yaml)

```yaml
- id: convert
  action: shade.convert
  params:
    from: '{{env.TEMP}}/in.png'
    to: '{{env.TEMP}}/out.jpg'
    format: jpeg
    quality: 85
```

#### `compression.compress` / `compression.decompress`

- 示例：[`examples/actions/compression.compress.yaml`](../../examples/actions/compression.compress.yaml) · [`compression-demo.yaml`](../../examples/directives/compression-demo.yaml)

```yaml
- id: zip
  action: compression.compress
  params:
    from: './examples/directives/hello.yaml'
    to: '{{env.TEMP}}/demo.zip'
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
    from: './examples/directives'
    to: '{{env.TEMP}}/paths.txt'
    transform: '{{path}}'
    includes: ['*.yaml']
```

#### `codec.base64.encode` / `decode` / `codec.hash.md5` / `codec.json.parse`

- 示例：[`examples/actions/codec.base64.encode.yaml`](../../examples/actions/codec.base64.encode.yaml) · [`codec-pipeline.yaml`](../../examples/directives/codec-pipeline.yaml)

```yaml
- id: b64
  action: codec.base64.encode
  params: { input: 'corex' }
  save_to: encoded
```

IPC: `{"type":"invoke","action":"codec.json.parse","params":{"text":"{\"a\":1}"}}`

`codec.json.pick` 在**已解析**的结构上按 JSON Pointer（RFC 6901，与 `file.write mode=json_set` 同一套写法）取一个子节点。
取不到不是错误——可选字段缺席是常态，直接给 `Null`（`default?` 可改）；写错格式（如点路径）会当场报。

```yaml
- id: uploaded
  action: codec.json.pick
  params:
    value: '{{steps.session.data}}'
    pointer: /uploaded # 服务端没回这个字段就是 Null
```

#### `codec.url.encode` / `codec.url.decode`

- 示例：[`codec.url.encode.yaml`](../../examples/actions/codec.url.encode.yaml) · [`codec.url.decode.yaml`](../../examples/actions/codec.url.decode.yaml)
- `mode: component`（默认，encodeURIComponent 语义）适合拼查询参数：`&`、`/` 都会被转义。
- `mode: uri`（encodeURI 语义）保留 URL 结构（`; , / ? : @ & = + $ #`），适合编整条地址。
- 解码时 `plus_as_space` 按 `application/x-www-form-urlencoded` 把 `+` 当空格（`%2B` 仍是加号）。

```yaml
- id: q
  action: codec.url.encode
  params: { input: '分片 上传', mode: component }
  # → %E5%88%86%E7%89%87%20%E4%B8%8A%E4%BC%A0
```

#### `generate.hash` / `generate.chunks`

- 示例：[`upload-chunked.yaml`](../../examples/directives/upload-chunked.yaml)（分片上传的完整用法）
- `generate.hash`：`path`（可带 `offset`/`length`）或 `text` → `{ algorithm, algorithms, encoding, hex, hashes, size }`，
  流式读取，10 GB 的文件也只占一个缓冲区；`md5` 是 `codec.hash.md5` 的通用形式。
- **`algorithm` 可以是数组**：整文件报 `sha256`、每片按服务端要求算 `md5` 是常见组合，
  传数组就只读一遍盘，结果在 `hashes` 里按算法名取值。
- `encoding: base64` 只影响 `hashes`；`hex` 永远是十六进制，老写法不受影响。
- `hashes` 只在它比 `hex` 多说一点时才出现（多算法，或 `encoding: base64`）——
  单算法 + hex 下它就是 `hex` 的副本，整段与每片都不输出。
- `generate.chunks`：只切块，给出每片的 `index` / `offset` / `length`，
  **不读内容、不算摘要**（只 `stat` 一次拿文件大小）。分片边界因此不用在指令里算
  （`{{ }}` 只做取值，没有算术）。
- **两件事各归各的动作**：摘要全归 `generate.hash`。分片计划里塞摘要会逼着只想切块的人
  多读一遍盘，也会让人以为「文件摘要 = 各片摘要的某种聚合」——而那并不是同一个值。
  某一片的摘要就是 `generate.hash` 带上这一片的 `offset` / `length`。
- **断点续传**：`offset` + `index` 从上次断的地方接着规划，multipart 的 `index` 才不会错位；
  已传过的分片用 `contains` 条件跳过。

```yaml
- id: plan
  action: generate.chunks # 只切块：stat + 除法，不读内容
  params:
    path: '{{input.file}}'
    chunk: 10485760 # 每片 10 MiB
```

```text
plan.size    26214400      plan.chunk 10485760      plan.total 3
plan.chunks  [{index:0, offset:0, length:10485760}, {index:1, offset:10485760, …}, …]
```

摘要按需要，整文件要一次、每片各要一次：

```yaml
- id: whole
  action: generate.hash # 整文件：不给 offset/length
  params:
    path: '{{input.file}}'

- id: piece
  action: generate.hash # 某一片：只读这一段
  params:
    path: '{{input.file}}'
    offset: '{{part.offset}}'
    length: '{{part.length}}'
```

```yaml
# 断点续传：从第 8 片、偏移 80 MiB 处接着规划
- id: resume
  action: generate.chunks
  params:
    path: '{{input.file}}'
    chunk: 10485760
    offset: '{{steps.session.done_bytes}}'
    index: 8
    algorithm: [sha256, md5]
    encoding: hex
```

### HTML 与爬取

#### `html.select` / `html.links` / `html.text`

- 功能门控 `act-html`（解析器 `scraper`，按 HTML5 规则补全畸形标签，容错与浏览器一致）
- 示例：[`html.select.yaml`](../../examples/actions/html.select.yaml) · [`html.links.yaml`](../../examples/actions/html.links.yaml) · [`html.text.yaml`](../../examples/actions/html.text.yaml) · [`html-crawl.yaml`](../../examples/directives/html-crawl.yaml)
- 三个动作都是纯字符串处理（`PermissionSet::NONE`）：输入是 HTML 文本，输出直接喂给下一个动作。
  `codec.json.parse` 管 JSON，它们是 HTML 的对位。

| Action        | 输出                      | 用途                                                                           |
| ------------- | ------------------------- | ------------------------------------------------------------------------------ |
| `html.select` | `{ items, count, value }` | CSS 选择器取元素文本（`attr` 给了就取属性）                                    |
| `html.links`  | `{ items, count, value }` | 取链接；默认 `a[href]`，自动滤掉 `#`/`javascript:`/`data:`，可按 `unique` 去重 |
| `html.text`   | `string`                  | 去标签取正文（元素之间用 `separator` 连接）                                    |

- **`base` / `absolute`**（两个动作同一套规则）：给了 `base` 就默认把相对链接补成绝对地址，
  `absolute: false` 可以关掉；显式写 `absolute: true` 却不给 `base` 会报错。不给 `base`
  就保留原样，相对链接也是合法结果。`base` 只解析一次（整篇文档共用）。
- **正文不含内联代码**：`script` / `style` / `noscript` / `template` 的文本被跳掉。
  直接用 `ElementRef::text()` 会把整段 JS 当正文，`html.text` 与不带 `attr` 的
  `html.select` 都走这条提取。
- `trim`（默认开）同时裁文本与属性值的首尾空白。

```yaml
- id: get
  action: http.send
  params: { url: '{{input.url}}' }
  save_to: page

- id: title
  action: html.select
  params:
    html: '{{page.body}}'
    selector: 'title'
    all: false # 只要第一个
  save_to: title

- id: links
  action: html.links
  params:
    html: '{{page.body}}'
    base: '{{page.url}}' # 相对链接补成绝对链接（默认执行，需要 base）
  save_to: links

- id: body_text
  action: html.text
  params:
    html: '{{page.body}}'
    selector: 'article'
  save_to: text
```

IPC: `{"type":"invoke","action":"html.select","params":{"html":"<h1>Hi</h1>","selector":"h1"}}`

### 桌面与密钥

#### `clipboard.get` / `clipboard.set`

- 示例：[`examples/actions/clipboard.set.yaml`](../../examples/actions/clipboard.set.yaml) · [`clipboard-notify.yaml`](../../examples/directives/clipboard-notify.yaml)

```yaml
- id: clip
  action: clipboard.set
  params: { format: text, text: 'hello' }
```

#### `notify.send`

- 示例：[`examples/actions/notify.send.yaml`](../../examples/actions/notify.send.yaml)

```yaml
- id: toast
  action: notify.send
  params: { summary: 'Corex', body: 'done' }
```

#### `keyring.get` / `keyring.set`

- 示例：[`examples/actions/keyring.set.yaml`](../../examples/actions/keyring.set.yaml) · [`keyring-demo.yaml`](../../examples/directives/keyring-demo.yaml)

```yaml
- id: store
  action: keyring.set
  params:
    service: my-app
    user: demo
    password: 'CHANGE-ME'
```

声明权限时需要 `permissions.secret: true`。

### 捕获（截图 / 显示器 / OCR 为 **Win**）

#### `capture.screenshot` / `capture.crop` / `capture.monitors` / `capture.ocr`

- 示例：[`examples/actions/capture.screenshot.yaml`](../../examples/actions/capture.screenshot.yaml) · [`capture-demo.yaml`](../../examples/directives/capture-demo.yaml)
- `capture.ocr` 是一次 WinRT 调用，中间没有可拆的块，因此它不上报分块进度（要看进度的是 `capture.find`）。

```yaml
- id: shot
  action: capture.screenshot
  params:
    to: '{{env.TEMP}}/shot.png'
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
    name_contains: '编辑'
    safe: true
  save_to: clicked
```

声明权限时需要 `permissions.ui: true`。

### PDF / morph

#### `morph.export` / `morph.merge` / `morph.split`

- 示例：[`examples/actions/morph.export.yaml`](../../examples/actions/morph.export.yaml) · [`morph-demo.yaml`](../../examples/directives/morph-demo.yaml)
- 进度：`export` 按字节（与 `file.copy` 同一条链路）；`merge` 按输入文件数、`split` 按输出分段数（条目）。

```yaml
- id: export
  action: morph.export
  params:
    src: '{{input.pdf_path}}'
    dest: '{{env.TEMP}}/copy.pdf'
```

#### `morph.meta` / `morph.render`（未实现）

这两个动作注册了参数与元数据，但**没有实现**：调用会直接返回「当前构建未启用」的错误。仓库里 `pdfium/` 只是构建助手，**没有任何代码加载 pdfium**；要实现它们需引入 `pdfium-render` 并把 `pdfium` 加回 workspace 成员（`Cargo.toml` 有注释）。下面保留其预期的调用形式。

- 示例：[`examples/actions/morph.meta.yaml`](../../examples/actions/morph.meta.yaml)

```yaml
- id: meta
  action: morph.meta
  params: { path: '{{input.pdf_path}}' }
```

当前构建未捆绑 pdfium 时，这些 Action 会返回错误。

## 进程启动（`shell.run` / `exec.run`）

两个门面共用同一套**进程启动内核**（`process_launch`）。二者仅在产品意图上不同：

| 门面        | 必填参数  | 目标类型             | 典型用途                              |
| ----------- | --------- | -------------------- | ------------------------------------- |
| `shell.run` | `command` | 命令 / 二进制        | `npm`、`fnm`、绝对路径 `.exe`         |
| `exec.run`  | `script`  | 脚本文件（必须存在） | 你指定的 `.bat` / `.ps1` / `.sh` 路径 |

**`host`**（可选，默认 `auto`）：

| 取值         | 行为                                                                               |
| ------------ | ---------------------------------------------------------------------------------- |
| `none`       | 直接 `Command::new(program)` + args                                                |
| `cmd`        | Windows `cmd /C`（先 `chcp 65001`）；Unix `sh -c` / 脚本路径                       |
| `powershell` | Windows PowerShell 5.x（`-Command`，启动时把 OutputEncoding 设为 UTF-8）           |
| `pwsh`       | PowerShell 7+（同上）                                                              |
| `auto`       | 命令 → `none`；脚本按扩展名（`.ps1`→pwsh/powershell，`.bat`/`.cmd`→cmd，`.sh`→sh） |

**输出编码（Windows）**：子进程 stdout/stderr 走管道时，老程序常按 OEM（中文 CP936/GBK）
写字节。内核先按 UTF-8 解，失败再回退 OEM/GBK，实时回显也转成 UTF-8——这与 CLI 的
`SetConsoleOutputCP` 是两层问题。

**输出去向**：子进程的输出边读边交，交到哪取决于有没有**上报口**。有（`corex run`、
经 daemon 的请求）就作为**文本输出事件**交出去——本地执行仍落在原来那个流上，远程执行变成
`step_output` 帧回给调用方（见 [IPC 协议](../reference/IPC协议.md#进度帧-stream-true)），
两边共用一份输出。没有（`--quiet`）就直接写自己的 stdout / stderr，与 v11 逐字节一致。
不看帧就看不到 daemon 那侧的输出：它落在 daemon 自己的控制台上，不在返回值里——返回值里的
`stdout` / `stderr` 是**跑完之后**的完整副本，适合做事后判断，不适合做实时回显。

**GUI / 单实例**（可选）：

| 参数                | 取值                                                   | 默认     |
| ------------------- | ------------------------------------------------------ | -------- |
| `wait`              | `sync` \| `detach`                                     | `sync`   |
| `if_running`        | `launch` \| `skip` \| `fail`                           | `launch` |
| `if_running_window` | `{ title_contains, title_excludes?, prefer_largest? }` | —        |

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
