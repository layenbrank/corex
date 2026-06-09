# CoreX

一个多功能命令行工具，支持文件复制、目录清理、路径生成、压缩打包、定时任务等功能。

## 环境配置 (bootstrap)

初始化或检查 CoreX 运行环境。

```powershell
# 初始化环境变量
corex bootstrap env

# 检查环境配置
corex bootstrap inspect

# 强制重新初始化
corex bootstrap force
```

## 压缩打包 (compression)

将 H5+ 构建产物目录打包为 `.wgt` 文件，并在输出目录下同步写入版本信息文件。

### 参数说明

| 参数     | 缩写 | 必填 | 描述                                |
| -------- | ---- | ---- | ----------------------------------- |
| `--from` | `-f` | ✓    | 源目录路径（必须包含 `index.html`） |
| `--to`   | `-t` | ✓    | 输出的 `.wgt` 文件路径              |

### 使用示例

```powershell
corex compression -f C:\Users\iwell\Documents\Vue2\front\master\app -t C:\Users\iwell\Documents\Vue2\front\master\H58991839.wgt
```

### 输出文件

以 `--to` 所在目录（`output`）为基准，完成后额外生成以下文件：

| 文件                                | 说明                     |
| ----------------------------------- | ------------------------ |
| `<output>/H58991839.wgt`            | 压缩后的 H5+ 应用包      |
| `<output>/version.json`             | 根目录版本文件           |
| `<output>/public/version.json`      | public 目录版本文件      |
| `<output>/src/assets/js/version.js` | 前端可直接导入的版本模块 |

`version.json` 内容示例：

```json
{
	"version": "20260514"
}
```

`src/assets/js/version.js` 内容示例：

```js
export function version() {
	return 20260514
}
```

## 文件复制 (copy)

复制文件或目录，支持忽略特定文件和清空目标目录。

### 参数说明

| 参数        | 缩写 | 必填 | 默认值 | 描述                                       |
| ----------- | ---- | ---- | ------ | ------------------------------------------ |
| `--from`    | `-f` | ✓    | -      | 源路径                                     |
| `--to`      | `-t` | ✓    | -      | 目标路径                                   |
| `--empty`   | `-e` | ✗    | `true` | 复制前清空目标目录                         |
| `--ignores` | `-i` | ✗    | -      | 忽略的文件或目录模式（逗号分隔或多次使用） |

### 使用示例

```powershell
corex copy -f ./input -t ./output -i "example.js,*.git,node_modules"
```

## 目录清理 (scrub)

递归删除目录中指定名称的文件或文件夹。

### 参数说明

| 参数          | 缩写 | 必填 | 默认值  | 描述                         |
| ------------- | ---- | ---- | ------- | ---------------------------- |
| `--source`    | `-s` | ✓    | -       | 要清理的根目录路径           |
| `--target`    | `-t` | ✓    | -       | 要删除的目标名称（不含路径） |
| `--recursive` | `-r` | ✗    | `false` | 是否递归处理子目录           |

### 使用示例

```powershell
# 递归删除所有 .turbo 文件夹
corex scrub -s C:\Projects\my-app -t .turbo -r true

# 删除根目录下的 node_modules
corex scrub -s C:\Projects\my-app -t node_modules
```

## 路径生成 (generate path)

扫描目录并按模板生成自定义格式的路径列表。

### 参数说明

| 参数          | 必填 | 默认值  | 描述                                           |
| ------------- | ---- | ------- | ---------------------------------------------- |
| `--from`      | ✓    | -       | 源路径                                         |
| `--to`        | ✓    | -       | 输出文件路径                                   |
| `--transform` | ✓    | -       | 转换规则模板                                   |
| `--index`     | ✓    | -       | 起始索引                                       |
| `--separator` | ✓    | -       | 路径分隔符                                     |
| `--pad`       | ✗    | `false` | 对索引进行补零填充                             |
| `--ignores`   | ✗    | -       | 忽略的文件或目录模式（逗号分隔或多次使用）     |
| `--uppercase` | ✗    | -       | 将指定规则转换为大写（可多次使用或用逗号分隔） |

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
corex generate path --from dist --to path.txt --ignores "example.js,*.git,node_modules" --index 1 --separator "\" --uppercase "extension" --transform '<include name="IDR_ITAB_{{extension}}_{{index}}" file="{{fullpath}}" type="BINDATA" />'
```

## 定时任务 (schedule)

从配置文件读取并批量执行 `copy` / `generate path` 任务。

```powershell
corex schedule
```

任务配置文件路径参考 [docs/corex.task.json](docs/corex.task.json)，配置格式参考 [docs/corex.task.schema.json](docs/corex.task.schema.json)。

## Node.js 集成示例

以下是在 Node.js 项目中集成 CoreX 的示例配置：

### 安装依赖

```powershell
pnpm install npm-run-all2
```

### 创建构建脚本

在 `scripts` 文件夹下创建 `post-build.js`：

```javascript
import { spawn } from 'node:child_process'
import { homedir } from 'node:os'
import { resolve } from 'node:path'

const from = '../dist'
const toPath = resolve(homedir(), 'Documents', 'Vue')
const ignore = 'index.html'

spawn('corex', ['copy', '--from', from, '--to', toPath, '--ignores', ignore, '--empty'], {
	stdio: 'inherit'
})
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
