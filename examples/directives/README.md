# Directive 示例索引

本目录为 `corex run <name>` 可直接运行的 YAML 指令示例。文件名（不含扩展名）即指令名。

**编写与配置说明：** [docs/guide/指令与输入配置.md](../../docs/guide/指令与输入配置.md)  
**完整文档入口：** [docs/README.md](../../docs/README.md)
同步到用户数据目录（可选）：

```text
%LOCALAPPDATA%\corex\directives\
```

验证全部示例能否解析且 action 已注册：

```powershell
cargo test -p corex-engine example_directives_validate
corex validate examples/directives/<file>.yaml
```

## 入门

| 文件 | 说明 | 运行示例 |
|------|------|----------|
| [hello.yaml](./hello.yaml) | 最小指令：`inputs` / `variables` / `save_to` / `file.write` | `corex run hello` |
| [resolver-demo.yaml](./resolver-demo.yaml) | 占位符：`input` / `env` / `variables` / `step.*` | `corex run resolver-demo` |
| [control-flow.yaml](./control-flow.yaml) | `if` / `else` / `repeat.count` | `corex run control-flow -i mode=prod` |
| [control-flow-advanced.yaml](./control-flow-advanced.yaml) | `repeat.each` / `parallel` / `when` / `on_error` | `corex run control-flow-advanced` |

## HTTP 与数据

| 文件 | 说明 | 运行示例 |
|------|------|----------|
| [http-save-body.yaml](./http-save-body.yaml) | GET + query/headers/token → 写文件 | `corex run http-save-body -i url=https://httpbin.org/get -i q=corex` |
| [http-post-json.yaml](./http-post-json.yaml) | POST JSON → 解析 → 模板输出 | `corex run http-post-json` |
| [http-extract-patch.yaml](./http-extract-patch.yaml) | 登录 API → JSON 提取 → `file.write` regex | `corex run http-extract-patch` |
| [bing-suggest.yaml](./bing-suggest.yaml) | 模板拼 URL + `http.send` + `codec.json.parse` | `corex run bing-suggest -i qry=rust` |
| [codec-pipeline.yaml](./codec-pipeline.yaml) | Base64 / MD5 / JSON 解析链 | `corex run codec-pipeline` |

## 文件与生成

| 文件 | 说明 | 运行示例 |
|------|------|----------|
| [file-write-modes.yaml](./file-write-modes.yaml) | `overwrite` / `replace_between` / `json_set` / `regex` | `corex run file-write-modes` |
| [copy-demo.yaml](./copy-demo.yaml) | `copy.run` 目录/文件复制 | `corex run copy-demo` |
| [inject-build-time.yaml](./inject-build-time.yaml) | 时间戳注入 JSON / JS marker | `corex run inject-build-time -i json_path=./package.json` |
| [generate-demo.yaml](./generate-demo.yaml) | UUID / 时间戳 / CVID | `corex run generate-demo` |

## 系统与桌面

| 文件 | 说明 | 运行示例 |
|------|------|----------|
| [shell-host-demo.yaml](./shell-host-demo.yaml) | `shell.run`：`host` / `wait` / `allow_nonzero` | `corex run shell-host-demo` |
| [clipboard-notify.yaml](./clipboard-notify.yaml) | 剪贴板读写 + 桌面通知 | `corex run clipboard-notify -i text=你好` |
| [scan-env-demo.yaml](./scan-env-demo.yaml) | `scan.os` 系统信息摘要 | `corex run scan-env-demo` |

## UI 自动化（Windows）

| 文件 | 说明 | 运行示例 |
|------|------|----------|
| [ui-smoke-notepad.yaml](./ui-smoke-notepad.yaml) | 13 个 `ui.*` 动作冒烟（Win11 记事本） | `corex run ui-smoke-notepad` |
| [wechat-send-message.yaml](./wechat-send-message.yaml) | 微信发消息（实验性，需授权） | `corex run wechat-send-message -i contact=文件传输助手 -i message=你好` |

## 声明式触发器

| 文件 | 说明 |
|------|------|
| [triggers-declared.yaml](./triggers-declared.yaml) | `triggers.cron` / `triggers.watch` 声明；配合 `corex cron start` / `corex watch start` |

手动执行始终用 `corex run <name>`，无需在 YAML 中声明。

## 常见占位符

| 写法 | 含义 |
|------|------|
| `{{input.name}}` | 指令输入 |
| `{{env.TEMP}}` | 环境变量 |
| `{{variables.x}}` / `{{x}}` | 顶层 `variables` 或 `save_to` |
| `{{step.fetch.status}}` | 某步输出字段 |
| `{{message}}` | 等价于 `variables.message`（`save_to` 写入） |

## 权限

未声明 `permissions` 时默认 **allow-all**。一旦声明任一 `permissions.*: true`，则仅允许对应类别，其余 action 会被拒绝。企业环境可设 `[runtime].strict_permissions = true` 拒绝未声明权限的指令。

详见 [docs/directive-yaml.md](../../docs/directive-yaml.md) 与 [docs/guide/指令与输入配置.md](../../docs/guide/指令与输入配置.md#8-权限)。
