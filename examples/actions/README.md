# 单 Action 最小示例

每个文件为**单步** Directive，演示一个 Action ID 的常用参数。多步流水线见 [`../directives/`](../directives/)。

运行方式：

```powershell
corex run examples/actions/file.copy.yaml
corex validate examples/actions/file.copy.yaml
```

验证全部示例（含本目录与 directives）：

```powershell
cargo test -p corex-engine example_directives_validate
```

## 索引

| Action ID | 示例文件 | 多步参考 |
|-----------|----------|----------|
| `shell.run` | [shell.run.yaml](./shell.run.yaml) | [shell-host-demo.yaml](../directives/shell-host-demo.yaml) |
| `exec.run` | [exec.run.yaml](./exec.run.yaml) | [exec-run-demo.yaml](../directives/exec-run-demo.yaml) |
| `http.send` | [http.send.yaml](./http.send.yaml) | [http-post-json.yaml](../directives/http-post-json.yaml) |
| `clipboard.get` / `set` | [clipboard.set.yaml](./clipboard.set.yaml) | [clipboard-notify.yaml](../directives/clipboard-notify.yaml) |
| `notify.send` | [notify.send.yaml](./notify.send.yaml) | [clipboard-notify.yaml](../directives/clipboard-notify.yaml) |
| `file.read` / `write` | [file.write.yaml](./file.write.yaml) | [file-write-modes.yaml](../directives/file-write-modes.yaml) |
| `file.copy` / `delete` | [file.copy.yaml](./file.copy.yaml) | [file-ops-demo.yaml](../directives/file-ops-demo.yaml) |
| `template.render` | [template.render.yaml](./template.render.yaml) | [hello.yaml](../directives/hello.yaml) |
| `cron.schedule` | [cron.schedule.yaml](./cron.schedule.yaml) | [cron-schedule-demo.yaml](../directives/cron-schedule-demo.yaml) |
| `keyring.*` | [keyring.set.yaml](./keyring.set.yaml) | [keyring-demo.yaml](../directives/keyring-demo.yaml) |
| `copy.run` | [copy.run.yaml](./copy.run.yaml) | [copy-demo.yaml](../directives/copy-demo.yaml) |
| `scrub.run` | [scrub.run.yaml](./scrub.run.yaml) | [scrub-demo.yaml](../directives/scrub-demo.yaml) |
| `shade.convert` | [shade.convert.yaml](./shade.convert.yaml) | [shade-demo.yaml](../directives/shade-demo.yaml) |
| `compression.*` | [compression.compress.yaml](./compression.compress.yaml) | [compression-demo.yaml](../directives/compression-demo.yaml) |
| `generate.*` | [generate.uuid.yaml](./generate.uuid.yaml) | [generate-demo.yaml](../directives/generate-demo.yaml) |
| `generate.path` | [generate.path.yaml](./generate.path.yaml) | [generate-path-demo.yaml](../directives/generate-path-demo.yaml) |
| `bootstrap.*` | [bootstrap.inspect.yaml](./bootstrap.inspect.yaml) | [bootstrap-demo.yaml](../directives/bootstrap-demo.yaml) |
| `codec.*` | [codec.base64.encode.yaml](./codec.base64.encode.yaml) | [codec-pipeline.yaml](../directives/codec-pipeline.yaml) |
| `scan.os` | [scan.os.yaml](./scan.os.yaml) | [scan-env-demo.yaml](../directives/scan-env-demo.yaml) |
| `capture.*` | [capture.screenshot.yaml](./capture.screenshot.yaml) | [capture-demo.yaml](../directives/capture-demo.yaml) |
| `ui.*` | [ui.window.list.yaml](./ui.window.list.yaml) | [ui-smoke-notepad.yaml](../directives/ui-smoke-notepad.yaml) |
| `morph.*` | [morph.export.yaml](./morph.export.yaml) | [morph-demo.yaml](../directives/morph-demo.yaml) |

平台标记：`capture.*`、`bootstrap.*`、`ui.*` 主要为 **Windows**；`morph.meta` / `morph.render` 需要 **pdfium**。
