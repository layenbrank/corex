# Corex 文档中心

Corex 是可组合的**指令（Directive）/ Action** 运行时：用 YAML 定义流水线，通过 CLI 或 Daemon IPC 执行。

当前主线版本：**v5**（workspace `5.0.0`）。

---

## 文档分层

按阅读顺序与角色选择入口：

| 层级 | 读者 | 文档 |
|------|------|------|
| **1. 入门** | 第一次使用 | [快速开始](./guide/快速开始.md) |
| **2. 使用指南** | 编写/运行指令 | [指令与输入配置](./guide/指令与输入配置.md) · [Directive YAML 参考](./directive-yaml.md) · [内置动作目录](./actions.md) · [示例指令索引](../examples/directives/README.md) |
| **3. 接入与 SDK** | 集成到其他应用 | [接入总览](./integration/接入总览.md) · [IPC 接入](./integration/IPC接入指南.md) · [Rust 嵌入](./integration/Rust嵌入指南.md) · [WASM 插件](./integration/WASM插件开发.md) · [Tauri 接入](./integration/Tauri接入指南.md) |
| **4. 运行时** | 部署与调优 | [运行时配置](./guide/运行时配置.md) · [架构说明](./architecture.md) |
| **5. 专题** | 特定能力 | [UI 自动化](./ui-automation.md) · [跨平台后端](./cross-platform-backends.md) |
| **6. 运维与安全** | 企业/生产 | [企业部署](./enterprise-deploy.md) · [合规说明](./compliance.md) · [威胁模型](./threat-model.md) |
| **7. 变更与归档** | 升级迁移 | [v5 破坏性变更](./breaking-changes-v5.md) · [v4 破坏性变更](./breaking-changes-v4.md) · [archive/](./archive/) |

---

## 常见路径（5 分钟上手）

```powershell
# 1. 构建
cargo build -p corex -p corex-daemon

# 2. 运行示例指令
corex run hello
corex run hello -i who=Corex

# 3. 查看可用指令与动作
corex schedule
corex actions

# 4. 校验 YAML
corex validate examples/directives/hello.yaml
```

---

## 三种使用方式

| 方式 | 适用场景 | 文档 |
|------|----------|------|
| **CLI 直接运行** | 脚本、CI、本地自动化 | [快速开始](./guide/快速开始.md) |
| **Daemon + IPC** | 桌面宿主、Tauri、长期驻留 | [IPC 接入指南](./integration/IPC接入指南.md) |
| **Rust 库嵌入** | 同一进程内集成引擎 | [Rust 嵌入指南](./integration/Rust嵌入指南.md) |

---

## 目录结构（仓库）

```text
docs/                 ← 本文档根目录
  guide/              ← 中文使用指南（新手优先）
  integration/        ← SDK / 接入（第三方开发者）
  archive/            ← v3 及更早历史文档
examples/directives/  ← 可运行的 YAML 示例
examples/tauri/       ← Tauri sidecar 示例代码
config/corex.toml     ← 运行时配置模板
crates/               ← Rust 库源码
```

---

## 获取帮助

- 指令 YAML 语法： [directive-yaml.md](./directive-yaml.md)
- 某个 Action 的参数： [actions.md](./actions.md) 或 `corex actions`
- 示例跑不通：先 `corex validate <file.yaml>`，再对照 [示例索引](../examples/directives/README.md)
- 集成问题：从 [接入总览](./integration/接入总览.md) 选择路径
