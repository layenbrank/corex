# Corex

可组合的**指令（Directive）/ Action** 运行时：用 YAML 定义流水线，CLI 与 `corex-daemon` 共用同一引擎。

**当前版本：v7.0.0**（workspace `7.0.0`）

## 快速开始

完整说明见 → [docs/guide/快速开始.md](docs/guide/快速开始.md)

```powershell
cargo build -p corex -p corex-daemon
corex run hello                # 不给名称也行：会在终端里列出来让你选
corex run hello -i who=Corex
corex schedule
corex actions file.copy        # 某个动作的参数表 + 可直接粘的步骤片段
```

## 常用命令

| 命令                                    | 说明                                                                  |
| --------------------------------------- | --------------------------------------------------------------------- |
| `corex run [名称\|路径]`                | 执行指令；省略名称就交互挑选，`-i KEY=VALUE` 传参                     |
| `corex run <名称> --dry-run`            | 只解析与校验（含运行时权限门），打印将要执行的步骤                    |
| `corex run <名称> --json-events`        | 步骤事件按 NDJSON 输出给宿主消费                                      |
| `corex run <名称> --remote`             | 交给 `corex-daemon` 执行（插件 Action 只在它那里可用），进度按帧流回  |
| `corex run <名称> --timeout 30`         | 只本次运行覆盖单步超时（秒）；`--jobs N` 覆盖并行度                   |
| `corex history`                         | 最近的执行记录（`-n` 条数、`--failed` 只看失败、可跟指令名）          |
| `corex schedule`                        | 列出指令                                                              |
| `corex watch …` / `corex cron …`        | 文件监听 / 定时守护                                                   |
| `corex actions [id]`                    | 按 bucket 分组列出 Action（`--bucket ui` 过滤）；给 id 则打印参数表、权限与步骤片段 |
| `corex validate <path>`                 | 校验 YAML；`--watch` 存一次盘重校一次                                 |
| `corex create [名称]`                   | 指令脚手架（交互向导，或 `-t hello\|http\|file\|shell\|cron\|watch`） |
| `corex edit <名称>` / `corex repl`      | 用编辑器打开 / 交互式 REPL                                            |
| `corex schema`                          | 输出指令 YAML 的 JSON Schema，供编辑器补全与校验                      |
| `corex completions <shell>`             | 打印 shell 补全注册脚本（候选由 corex 现算）                          |
| `corex doctor`                          | 自检数据目录、配置、守护进程、动作与指令                              |
| `corex daemon start\|stop\|status\|run` | Daemon 管理                                                           |
| `corex ui ...`                          | Windows UI 探测                                                       |
| `corex update`                          | 自更新（`--check` 只检查）                                            |

## 文档

→ **[docs/README.md](docs/README.md)**（分类索引，推荐从这里找）

| 分类 | 入口                                                                                                                                                                                     |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 入门 | [快速开始](docs/guide/快速开始.md) · [指令与输入](docs/guide/指令与输入配置.md)                                                                                                          |
| 参考 | [指令 YAML](docs/reference/指令YAML.md) · [内置 Action](docs/reference/内置Action.md) · [架构](docs/reference/架构.md) · [自更新](docs/reference/自更新.md)                              |
| 接入 | [接入总览](docs/integration/接入总览.md) · [IPC](docs/integration/IPC接入指南.md) · [Tauri](docs/integration/Tauri接入指南.md)                                                           |
| 示例 | [directives](examples/directives/README.md) · [actions](examples/actions/README.md)                                                                                                      |
| 运维 | [企业部署](docs/ops/企业部署.md) · [发布与 Changelog](docs/ops/发布与Changelog.md) · [合规](docs/ops/合规说明.md)                                                                        |
| 变更 | [v7](docs/changelog/破坏性变更-v7.md) · [v6](docs/changelog/破坏性变更-v6.md) · [v5](docs/changelog/破坏性变更-v5.md) · [v4](docs/changelog/破坏性变更-v4.md) · [archive](docs/archive/) |

## Workspace

| 路径                                         | 说明                                 |
| -------------------------------------------- | ------------------------------------ |
| `crates/core`                                | Value / Action / ExecutionContext    |
| `crates/engine`                              | Directive、Pipeline、解析器          |
| `crates/registry`                            | 内置 Action、WASM host               |
| `crates/ipc`                                 | NDJSON 协议                          |
| `crates/updater`                             | 自更新：Release 发现、校验、原子替换 |
| `bins/cli` · `bins/daemon`                   | `corex` / `corex-daemon`             |
| `examples/directives/` · `examples/actions/` | 可运行 YAML                          |
| `examples/tauri/`                            | Tauri sidecar 示例                   |

## 三种集成方式

| 方式         | 场景          | 文档                                          |
| ------------ | ------------- | --------------------------------------------- |
| CLI          | 脚本、CI      | [快速开始](docs/guide/快速开始.md)            |
| Daemon + IPC | Tauri、多进程 | [IPC 接入](docs/integration/IPC接入指南.md)   |
| Rust 嵌入    | 同进程        | [Rust 嵌入](docs/integration/Rust嵌入指南.md) |

## 许可证

见仓库贡献说明与 CI（`.github/`）。
