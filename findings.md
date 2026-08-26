# Findings — UI 自动化与交互式探测

## Call graph（directive run → ui.element.* → ui_kernel）

```
corex run <name>                    bins/cli/src/main.rs::cmd_run
  └─ Pipeline::execute              crates/engine/src/pipeline.rs:48
       ├─ apply_input_defaults      crates/engine/src/inputs.rs
       └─ execute_steps → run_action_step
            └─ invoke_action        pipeline.rs:250
                 ├─ permissions.allows_action (ui → permissions.ui)
                 ├─ Resolver::resolve_value(params)
                 └─ Action::execute  (async_trait on UiElement*)
                      └─ ui_*_impl   crates/registry/src/builtin/ui.rs
                           ├─ selector_chain_from_params   ui_kernel.rs:114
                           │    └─ ElementSelector::from_map
                           ├─ window_query_from_params       ui_kernel.rs:78
                           │    └─ ExecutionContext.ui_session (scope_hwnd/title)
                           └─ spawn_blocking (Windows)
                                ├─ resolve_scope_hwnd → find_window
                                ├─ build_matcher_for → apply_selector
                                ├─ find_with_chain (selectors[] 回退)
                                ├─ wait_element_state / element_present
                                └─ elem_to_map → Value::Map

ui::register(registry)              ui.rs:276
  → ActionRegistry (builtin/mod.rs:101)

Daemon 路径：
  IPC Request::Invoke → invoke_action   bins/daemon/src/main.rs:228
    → action.execute (无 directive permissions，fresh ExecutionContext)
```

**`ui.element.find` 专线路径：**

`UiElementFind::execute` → `ui_element_find_impl` (788) → `selector_chain_from_params` → `find_with_chain` → `elem_to_map`.

**未走 ui_kernel 的 ui 动作：** `ui.window.list`、`ui.click`、`ui.type`、`ui.key`（无 selector_chain）。

## Code review（按严重度）

### High

1. **`verify_closed` 无法验证关闭** — `examples/directives/ui-smoke-notepad.yaml:217-224`：`on_error: continue` 在 find **失败**（已关闭）时静默通过；find **成功**（仍打开）时也通过。冒烟测试从不断言窗口已关。
2. **Daemon `invoke` 绕过 directive permissions** — `bins/daemon/src/main.rs:228-236`：`strict_permissions=false` 时可单次 invoke 任意 `ui.*`（含 `ui.type`/`ui.click`），无 audit directive 上下文。企业默认 `enterprise.toml` 用 `disabled_actions` 缓解，但非 strict 部署仍有风险。
3. **无 Windows UI 集成测** — `ui.rs` 全在 `#[cfg(windows)]`；仅 `ui_kernel.rs` 有单测。回归靠手工冒烟。

### Medium

4. **`elem_to_map` 缺 bounds/enabled/clickable** — `ui.rs:586-604`：对标 Auto.js `bounds()`/`clickable()` 无数据；交互探测需补 `get_bounding_rectangle`、`is_enabled` 等。
5. **`control_type` 输出为 Debug 格式** — `ui.rs:598-599`：`ControlType(50030)` 而非 `Document`，不利于人类读 list/find 结果。
6. **`resolve_scope_hwnd` 将 `name` 当作窗口标题** — `ui.rs:444-446`：与元素 `name` 参数语义混淆，YAML 误用难排查。
7. **文档路径不一致** — `docs/ui-automation.md:60` `%LOCALAPPDATA%\corex\directives\` vs 代码 `platform_data_dir` → `%AppData%\corex\data\directives`（`ipc/transport/mod.rs:44-48`）。
8. **`findings.md` 历史笔误** — 链长曾写 ≤5；现 baseline **8**（`context.rs:15`，`MAX_SELECTOR_CHAIN`）。

### Low

9. **`ui.element.list` depth 默认 3 vs selector depth 默认 12** — `ui.rs:149` vs `ui_kernel.rs:52-56`，探索浅层树可能漏元素。
10. **`element_present` 每条 chain 用完整 probe timeout** — `ui.rs:712-714`：absent 轮询可能偏慢。
11. **REPL 无 UI 命令** — `bins/cli/src/repl.rs` 仅 `run/list/actions`；无法交互 find。
12. **CLI 无 `invoke` / `ui` 子命令** — 探测只能写 YAML 或起 daemon + 自定义 IPC 客户端。

## Auto.js 对照（现状 vs 缺失）

| Auto.js | corex 现状 | 交互 find 缺口 |
|---------|------------|----------------|
| `selector().findOne(t)` | `ui.element.find` | 无独立 CLI/REPL 一键探测 |
| `exists()` | `ui.element.exists` | 同上 |
| `waitFor()` | `ui.element.wait present` | 同上 |
| `clickable()` | `enabled` + `safe` click | 结果不返回 clickable/enabled |
| `bounds()` | **无** | 需 UIA bounding rect |
| `id()` | `automation_id` 参数/输出 | 有，但 list 输出未美化 |
| `text()` / `className()` | `name` / `class` | 有 |
| 布局分析 / 控件树 | `ui.element.list` | 无 pretty-print / tree 视图 |
| 截图 + OCR | `capture.*` 独立 | 未与 find 联动 |
| selector 构建器 | `selectors[]` YAML | 无从 list 生成链的工具 |

`docs/ui-automation.md` 现有映射表仅 5 行，未覆盖 bounds/id/className/findOne 探索工作流。

## MVP 提案（最小）

### 方案 A — `corex ui` 子命令（推荐）

```text
corex ui windows
corex ui list --title "无标题" [--depth 5] [--limit 50]
corex ui find --control-type Document [--name ...] [--timeout 3000]
corex ui exists ...
```

- 实现：复用 `build_registry()` + `ExecutionContext::new`；可选 `--hwnd` 调 `set_ui_scope`。
- 输出：`serde_json` pretty（与 `cmd_run` 一致）。
- 工作量：~1 个新模块 + clap 子命令，不改 pipeline。

### 方案 B — REPL 扩展（与 A 共享 impl）

```text
corex> windows
corex> scope title=无标题
corex> find control_type=Document
corex> list depth=5
```

- 复用 A 的 `ui_probe` 函数；适合写 directive 时边试边改。

**不建议 MVP 首选 Daemon invoke**：无 session 串联、权限模型弱、需额外 IPC 客户端。

## 架构（已实现 + 目标）

```
Recipe YAML / corex ui find (目标)
  → Pipeline 或 ActionRegistry 直调
  → ui.element.* → ui_kernel (WindowQuery, ElementSelector, selectors[])
  → ExecutionContext.ui_session
  → AuditEntry (ui_phase, error_code, selector_hint)
```

## Out of scope

- 手机端 / 扫码 OCR 登录
- `wechat.send` / `ui.app.ensure`
- macOS AX / Linux AT-SPI
