# CoreX

一个多功能命令行工具，目前支持文件复制、目录扫描和自定义路径格式输出等功能，持续开发中。

## 环境配置

### 初始化环境变量

将 CoreX 可执行文件路径添加到系统环境变量中：

```powershell
corex setup env
```

### 检查环境配置

验证 CoreX 是否已正确添加到环境变量：

```powershell
corex setup check
```

## 文件复制 (copy)

复制文件或目录，支持忽略特定文件和清空目标目录。

### 参数说明

| 参数     | 描述                     |
| -------- | ------------------------ |
| `from`   | 源路径                   |
| `to`     | 目标路径                 |
| `ignore` | 忽略的文件或目录模式     |
| `empty`  | 清空目标目录下的所有内容 |

### 使用示例

```powershell
# 基本用法
corex copy --from ./input --to ./output --ignore "example.js,*.git,node_modules"

# 使用简写参数
corex copy -f ./input -t ./output --ignore "example.js,*.git,node_modules"
```

## 路径生成 (generate path)

扫描目录并生成自定义格式的路径列表。

### 参数说明

| 参数        | 描述                                           |
| ----------- | ---------------------------------------------- |
| `from`      | 源路径                                         |
| `to`        | 输出文件路径                                   |
| `ignore`    | 忽略的文件或目录模式（用 `,` 分割或多次调用）  |
| `transform` | 转换规则模板                                   |
| `index`     | 起始索引                                       |
| `separator` | 路径分隔符                                     |
| `uppercase` | 将指定规则转换为大写（可多次使用或用逗号分隔） |

### 转换规则

| 规则            | 说明         |
| --------------- | ------------ |
| `{{index}}`     | 索引         |
| `{{filename}}`  | 文件名       |
| `{{extension}}` | 扩展名       |
| `{{path}}`      | 文件上级路径 |
| `{{fullpath}}`  | 完整路径     |

### 使用示例

```powershell
corex generate path --from dist --to path.txt --ignore "example.js,*.git,node_modules" --index 1 --separator "\" --uppercase "extension" --transform '<include name="IDR_ITAB_{{extension}}_{{index}}" file="{{fullpath}}" type="BINDATA" />'
```

## Node.js 集成示例

以下是在 Node.js 项目中集成 CoreX 的示例配置：

### 安装依赖

```powershell
pnpm install npm-run-all2
```

### 创建构建脚本

在 `scripts` 文件夹下创建 `post-build.js`：

```javascript
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { resolve } from "node:path";

const from = "../dist";
const toPath = resolve(homedir(), "Documents", "Vue");
const ignore = "index.html";

spawn(
  "corex",
  ["copy", "--from", from, "--to", toPath, "--ignore", ignore, "--empty"],
  {
    stdio: "inherit",
  }
);
```

### 配置 package.json

```json
{
  "name": "example",
  "private": true,
  "version": "0.0.1",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "run-s build:core build:post",
    "build-only": "vite build",
    "build:pre": "node ./scripts/pre-build.js",
    "build:post": "node ./scripts/post-build.js",
    "build:core": "run-p type-check \"build-only {@}\" --",
    "type-check": "vue-tsc --build"
  }
}
```
