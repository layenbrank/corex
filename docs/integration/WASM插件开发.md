# WASM 插件开发

> **文档定位：** 本文只描述 **动态 WASM 插件**（`.wasm` 热加载）。  
> 若你要在 **Rust 源码里**扩展 Action（企业内建、与 corex 同版本发布），见 [Rust 嵌入指南 — 注册自定义 Action](./Rust嵌入指南.md#3-注册自定义-action)（生产环境首选）。

通过 **WebAssembly Component** 为 Corex 扩展自定义 Action。插件由 **`corex-daemon`**（或嵌入 `ActionRegistry` 的应用）在启动时发现并加载。

> **状态说明：** WIT bindgen 仍在完善。当前 discovery 会尝试加载 `.wasm`，bindgen 未就绪时会记录错误并**跳过**该文件。生产环境请优先使用 Directive + builtin Action。

---

## 1. 契约（WIT）

源文件：[`crates/plugin-sdk/wit/corex-action.wit`](../../crates/plugin-sdk/wit/corex-action.wit)

| 项目 | 值 |
|------|-----|
| Package | `corex:plugin-sdk@0.1.0` |
| World | `corex-action` |
| 导出接口 | `action` |

### 必须实现的函数

| 函数 | 说明 |
|------|------|
| `meta()` | 返回 `{ id, name, description }` |
| `validate(params: json)` | 成功返回空字符串；失败返回错误信息 |
| `execute(params, ctx)` | 返回 `{ ok, payload }`；`payload` 为 JSON 编码的 `Value` 或错误字符串 |

**JSON 类型：** WIT 中 `json` = `string`，内容为 JSON 文本，与宿主 `corex_core::Value` 互转。

### Action ID 命名

使用反向域名前缀，避免与 builtin 冲突，例如：

- `acme.echo`
- `com.example.tools.compress`

---

## 2. 目录布局

默认插件目录：`<数据目录>/plugins/`

```text
~/.local/share/corex/plugins/    # Linux 示例
  acme-echo.wasm
  vendor-tools.wasm
```

配置覆盖：`[plugins] plugin_dir = "plugins"`

---

## 3. 构建 Guest（示意）

1. 使用 **wasm32-wasip2** 或 component 工具链编译
2. 导出 `corex:plugin-sdk/action` 接口
3. 复制 `.wasm` 到插件目录
4. 重启 `corex-daemon`

```bash
# 示意（具体工具链因语言而异）
cargo component build --release
cp target/wasm32-wasip2/release/my_plugin.wasm ~/.local/share/corex/plugins/
corex-daemon
```

---

## 4. 宿主行为

| 阶段 | 行为 |
|------|------|
| 启动 | 扫描 `*.wasm` |
| 加载 | `WasmPluginHost` + wasmtime component model |
| 失败 | 日志记录，跳过该文件，不影响其他插件 |
| 注册 | 成功则 Action ID 进入 registry，可通过 `invoke` / Directive 调用 |

禁用插件：

```toml
[plugins]
disabled = ["acme-echo"]
disabled_actions = ["acme.echo"]
```

---

## 5. 从 Directive 调用

插件 Action 与 builtin **用法相同**：

```yaml
steps:
  - id: echo
    action: acme.echo
    params:
      message: "hello"
```

或通过 IPC：

```json
{"type":"invoke","action":"acme.echo","params":{"message":"hi"},"auth_token":"..."}
```

---

## 6. Rust SDK Crate

```toml
corex-plugin-sdk = { path = "../crates/plugin-sdk" }
```

当前 crate 主要导出 WIT 路径供 guest 绑定；宿主侧见 `corex-registry` 的 WASM feature。

Feature 控制：

```toml
# 启用 WASM 宿主（CLI/daemon 默认 full）
corex-registry = { features = ["wasm"] }

# 禁用
corex-registry = { default-features = false, features = ["act-file", "..."] }
```

---

## 相关文档

- [plugins/README.md](../../plugins/README.md) — 插件目录说明
- [接入总览 — 扩展 Action](./接入总览.md#扩展-action两种路径)
- [Rust 嵌入指南](./Rust嵌入指南.md) — 内置 Action（Rust trait）
- [IPC 接入指南](./IPC接入指南.md)
- [运行时配置](../guide/运行时配置.md) — `[plugins]` 段
