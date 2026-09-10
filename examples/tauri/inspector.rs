//! Tauri 的 UI 检查器骨架（类 FlaUInspect 的控件树 + 属性面板）。
//!
//! 接入 `lib.rs`：
//!
//! ```ignore
//! mod inspector;
//!
//! #[tauri::command]
//! fn inspector_windows() -> Result<serde_json::Value, String> {
//!     corex_ipc::ui_windows()
//! }
//!
//! #[tauri::command]
//! fn inspector_elements(hwnd: i64, depth: i64) -> Result<serde_json::Value, String> {
//!     corex_ipc::ui_elements(Some(hwnd), None, depth, 100)
//! }
//! ```
//!
//! 前端：见 `inspector/index.html` —— 绑定窗口列表、控件树、属性面板。

/// Inspector MVP 建议注册的 Tauri 命令。
pub const INSPECTOR_COMMANDS: &[&str] = &[
    "inspector_windows",
    "inspector_elements",
    "inspector_find_element",
];
