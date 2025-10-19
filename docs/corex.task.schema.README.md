corex.task.schema.json

此文件提供 `corex` 任务配置的 JSON Schema、示例与验证说明，便于编辑器提示与静态验证。

主要目标：

- 定义 `copy` 与 `generate.path` 两类任务的结构与必需字段。
- 提供一个可验证的示例配置 (`corex.task.example.json`)。
- 给出在 Windows PowerShell 下使用 ajv-cli 进行验证的示例命令。

文件说明：

- `corex.task.schema.json` - schema 本体，位于同一目录。
- `corex.task.example.json` - 参考示例，声明了 `copy` 和 `generate.path` 的用法。

常用字段：

- copy: 数组，项为对象（taskId -> taskOptions）。taskOptions 必须包含 `from` 与 `to`。

  - empty: boolean，可选，是否清空目标文件夹。
  - from: string，必需，源路径。
  - ignores: string[]，可选，忽略的扩展名或模式。
  - to: string，必需，目标路径。

- generate.path: 数组，项为对象（taskId -> taskOptions）。taskOptions 必须包含 `from` 与 `to`。
  - from: string，必需，源路径。
  - ignores: string[]，可选，忽略的扩展名或模式。
  - index: integer，可选，起始索引。
  - separator: string，可选，路径分隔符（例如 `\\` 表示反斜杠）。
  - to: string，必需，输出文件路径。
  - transform: string，可选，模板字符串，支持 `{{fullpath}}`, `{{extension}}`, `{{index}}` 等占位符。
  - uppercase: string[]，可选，要大写的占位符字段名列表。

在 PowerShell 上使用 ajv-cli 验证示例：

1. 安装 ajv-cli（如未安装）：

```powershell
npm install -g ajv-cli
```

2. 在 schema 所在目录运行验证（示例）：

```powershell
ajv validate -s .\corex.task.schema.json -d .\corex.task.example.json --strict=false
```

注意：

- JSON Schema 使用 Draft-07。
- 示例 `corex.task.example.json` 中 `$schema` 字段用于编辑器指向本地 schema。某些 lint/工具链可能不接受 `$schema` 并报错，此时可在工具中忽略或删除该字段用于验证。
