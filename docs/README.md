# Corex 文档

可组合的**指令（Directive）/ Action** 运行时。当前版本：**v5.1.0**。

按用途选文档即可；找不到时先回本页。

---

## 我想…

| 目标 | 去哪 |
|------|------|
| 第一次跑起来 | [guide/快速开始.md](./guide/快速开始.md) |
| 写指令 / 传参 / 权限 | [guide/指令与输入配置.md](./guide/指令与输入配置.md) |
| 查 YAML 语法 / Action 参数 | [reference/指令YAML.md](./reference/指令YAML.md) · [reference/内置Action.md](./reference/内置Action.md) |
| 写 cron 表达式 | [reference/cron表达式.md](./reference/cron表达式.md) |
| 跑示例 | [examples/directives](../examples/directives/README.md) · [examples/actions](../examples/actions/README.md) |
| 接到 Tauri / 别的应用 | [integration/接入总览.md](./integration/接入总览.md) |
| 配 `corex.toml` | [guide/运行时配置.md](./guide/运行时配置.md) |
| 了解架构 / watch·cron | [reference/架构.md](./reference/架构.md) |
| 企业锁定 / 合规 | [ops/企业部署.md](./ops/企业部署.md) |
| 升级迁移 | [changelog/破坏性变更-v5.md](./changelog/破坏性变更-v5.md) |

---

## 目录（按分类）

```text
docs/
  guide/         入门与日常使用
  reference/     权威参考（YAML / Action / 架构 / IPC）
  integration/   接入与 SDK
  topics/        专题（UI、跨平台）
  ops/           企业部署与安全
  changelog/     破坏性变更
  archive/       ≤v3 历史稿（勿当现行 API）
```

### 入门 — `guide/`

| 文档 | 说明 |
|------|------|
| [快速开始](./guide/快速开始.md) | 构建、首跑、常用命令 |
| [指令与输入配置](./guide/指令与输入配置.md) | `-i`、占位符、权限 |
| [运行时配置](./guide/运行时配置.md) | `corex.toml`、UI profile |

### 参考 — `reference/`

| 文档 | 说明 |
|------|------|
| [指令 YAML](./reference/指令YAML.md) | DSL、triggers、schema |
| [Cron 表达式](./reference/cron表达式.md) | tokio-cron-scheduler → croner 3 规则 |
| [内置 Action](./reference/内置Action.md) | Action ID 与示例 |
| [架构](./reference/架构.md) | crate 布局、Supervisor |
| [IPC 协议](./reference/IPC协议.md) | NDJSON 请求/响应 |

Schema：[schemas/directive.schema.json](../schemas/directive.schema.json)

### 接入 — `integration/`

| 文档 | 说明 |
|------|------|
| [接入总览](./integration/接入总览.md) | CLI / Daemon / 嵌入怎么选 |
| [IPC 接入指南](./integration/IPC接入指南.md) | NDJSON 客户端 |
| [Rust 嵌入指南](./integration/Rust嵌入指南.md) | 同进程 Pipeline |
| [WASM 插件开发](./integration/WASM插件开发.md) | 动态扩展 Action |
| [Tauri 接入指南](./integration/Tauri接入指南.md) | sidecar `corex-daemon` |

### 专题 — `topics/`

| 文档 | 说明 |
|------|------|
| [UI 自动化](./topics/UI自动化.md) | `ui.*`、`corex ui` |
| [跨平台后端](./topics/跨平台后端.md) | 非 Windows 规划 |

### 运维与安全 — `ops/`

| 文档 | 说明 |
|------|------|
| [企业部署](./ops/企业部署.md) | 锁定配置、最小构建、CLI 边界 |
| [合规说明](./ops/合规说明.md) | 授权与控制项 |
| [威胁模型](./ops/威胁模型.md) | 高风险 Action |

### 变更 — `changelog/`

| 文档 | 说明 |
|------|------|
| [破坏性变更 v5](./changelog/破坏性变更-v5.md) | Shortcut→Directive、CLI 等 |
| [破坏性变更 v4](./changelog/破坏性变更-v4.md) | v3→v4 重构 |
| [archive/](./archive/) | ≤v3 与已 superseded 草稿 |

---

## 5 分钟上手

```powershell
cargo build -p corex -p corex-daemon
corex run hello
corex run hello -i who=Corex
corex schedule
corex actions
corex validate examples/directives/hello.yaml
```
