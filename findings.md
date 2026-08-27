# Findings — UI Inspector + Code Review 修复

## pick 根因（已修）
- 全屏 layered overlay + `WM_LBUTTONUP` → CLI 从终端启动只能点控制台
- 修复：FlaUI 四边框 + timer + 全局 LButton 上升沿；scope 外点击 stderr 提示

## list → 桌面图标（已修）
- 无 scope 时 `resolve_scope_hwnd` 静默匹配 Shell/桌面
- 修复：`element tree/get` 强制 scope；`window desktop` 独立
- Win11：`find_desktop_hwnd` = Progman+DefView → WorkerW+DefView（0x052C）→ Progman 兜底

## 企业门禁（审查高优，已修）
- `plugins.disabled`（插件名 `ui` 或完整 id）
- `disabled_actions`（desktop/point/pick 独立 id）
- `strict_permissions`（CLI 与 daemon Invoke 一致拒绝 ui.*）
- audit action_id：`ui.window.desktop` / `ui.element.point` / `ui.element.pick`

## 输出 / CLI
- `node_key` 含 bounds，同名兄弟不合并
- `element get --class`
- `--redact` 打码 `name` + `automation_id`

## 未改（审查低优 / 跟进）
- pick `GWLP_USERDATA` 栈指针模式：加了注释，未做大 refactor
- `window desktop` 仍需实机验证 icons 非空
