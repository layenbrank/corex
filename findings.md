# Findings — UI Inspector

## pick 根因（已修）
- 全屏 layered overlay + `WM_LBUTTONUP` → CLI 从终端启动只能点控制台
- 修复：FlaUI 四边框 + timer + 全局 LButton 上升沿

## list → 桌面图标（已修）
- 无 scope 时 `resolve_scope_hwnd` 静默匹配 Shell/桌面
- 修复：`element tree/get` 强制 scope；`window desktop` 独立

## 竞品对齐
| 参考 | corex |
|------|-------|
| FlaUI 四边框 | ui_pick 保留 |
| Playwright 全局输入 | GetAsyncKeyState |
| winappCli ancestors | elem_to_map ancestors[] |
| DH.Window.Analyst class 回退 | suggest_selectors |

## 企业
- CLI probe 现尊重 disabled_actions + audit
- Tauri Inspector 走 daemon IPC（同 action id）
