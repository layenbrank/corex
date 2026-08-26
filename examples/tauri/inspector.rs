//! UI Inspector skeleton for Tauri (FlaUInspect-style tree + properties).
//!
//! Wire into `lib.rs`:
//!
//! ```ignore
//! mod inspector;
//!
//! #[tauri::command]
//! fn inspector_list_windows() -> Result<serde_json::Value, String> {
//!     corex_ipc::ui_window_list()
//! }
//!
//! #[tauri::command]
//! fn inspector_list_elements(hwnd: i64, depth: i64) -> Result<serde_json::Value, String> {
//!     corex_ipc::ui_element_list(Some(hwnd), None, depth, 100)
//! }
//! ```
//!
//! Frontend: see `inspector/index.html` — bind window list, element tree, property panel.

/// Recommended Tauri commands for Inspector MVP.
pub const INSPECTOR_COMMANDS: &[&str] = &[
    "inspector_list_windows",
    "inspector_list_elements",
    "inspector_find_element",
];
