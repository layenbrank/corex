# Corex WASM 插件目录

本目录用于存放 **WebAssembly Component** 插件（`.wasm`）。完整开发指南见 [docs/integration/WASM插件开发.md](../docs/integration/WASM插件开发.md)。

> **文档定位：** 此处为 **动态 WASM 插件**（热加载 `.wasm`）。  
> 若在 **Rust 源码**中扩展 Action（企业内建），见 [Rust 嵌入指南](../docs/integration/Rust嵌入指南.md#3-注册自定义-action-内置--rust)（生产环境首选）。

第三方 Action 以 WASM **组件**形式发布，需导出 WIT 接口 `corex:plugin-sdk/action`，定义见 [`crates/plugin-sdk/wit/corex-action.wit`](../crates/plugin-sdk/wit/corex-action.wit)。

---

## 契约摘要

| 项目 | 值 |
|------|-----|
| Package | `corex:plugin-sdk@0.1.0` |
| World | `corex-action` |
| 导出 | `action`（`meta`、`validate`、`execute`） |
| 值类型 | JSON 字符串（WIT 类型 `json`） |

| 函数 | 行为 |
|------|------|
| `meta()` | 返回 `{ id, name, description }`；ID 建议反向域名，如 `acme.echo` |
| `validate(params)` | 成功返回空字符串，失败返回错误信息 |
| `execute(params, ctx)` | 返回 `{ ok, payload }`；`payload` 为 JSON 编码的 `Value` 或错误字符串 |

---

## 目录布局

编译产物放在配置的插件目录（默认 **`<数据目录>/plugins/`**，非本仓库 `plugins/` 源码目录）：

```text
~/.local/share/corex/plugins/     # Linux 示例
  acme-echo.wasm
  vendor-tools.wasm
```

`config/corex.toml` 可覆盖：

```toml
[plugins]
plugin_dir = "plugins"
```

---

## 发现与加载

**`corex-daemon` 启动时**扫描 `*.wasm`（调用 `corex_registry::discovery::discover` 的流程相同）：

1. 通过 `WasmPluginHost` + wasmtime component model 加载
2. 成功 → 注册到 ActionRegistry，可在 Directive / IPC `invoke` 中使用
3. 失败 → 记录日志并跳过，不影响其他插件

> **状态：** WIT **bindgen 接线尚未完成**。bindgen 未就绪时 `load_plugin` 会在解析后报错，discovery 跳过该文件。第三方 WASM 插件目前视为**实验能力**。

---

## 构建 Guest（示意）

1. 实现并导出 `corex:plugin-sdk/action`（见 WIT 文件）
2. 目标 `wasm32-wasip2`，或用 `wasm-tools component new` 生成组件
3. 复制 `.wasm` 到 `<数据目录>/plugins/`
4. 重启 `corex-daemon`（或重新 discovery）

```bash
# 示意（语言/工具链因项目而异）
cargo component build --release
cp target/wasm32-wasip2/release/my_plugin.wasm ~/.local/share/corex/plugins/
corex-daemon
```

---

## 宿主实现

`corex-registry`（feature `wasm`，`full` 默认启用）使用 wasmtime **async** + **component model**，准备 `WasiCtxBuilder` 并解析组件字节。完整 `bindgen!` 接入完成后即可在不改结构的前提下启用 execute。

禁用插件：

```toml
[plugins]
disabled = ["acme-echo"]
disabled_actions = ["acme.echo"]
```

---

## Feature 开关

```toml
# 启用（full 默认）
corex-registry = { features = ["wasm"] }

# 禁用 WASM 宿主
corex-registry = { default-features = false, features = ["act-file", "..."] }
```

---

## 相关文档

- [WASM 插件开发（完整）](../docs/integration/WASM插件开发.md)
- [接入总览 — 扩展 Action](../docs/integration/接入总览.md#扩展-action两种路径)
- [运行时配置 — `[plugins]` 段](../docs/guide/运行时配置.md)
