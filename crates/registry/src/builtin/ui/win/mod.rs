//! `ui.*` 动作的 Windows UIAutomation / Win32 平台适配层。
//!
//! 按职责拆分：`window`（顶层窗口查找 / 聚焦 / 等待）、`element`（UIA 元素映射、
//! selector 链、等待谓词）、`desktop`（资源管理器的桌面图标）与
//! `input`（合成鼠标 / 键盘输入）。
//!
//! 下面的导入刻意是 `pub(super)`：子模块改用 `use super::*;`，不必各自重复一长串
//! `windows` crate 的导入列表，而再导出也永远不会被判为“未使用”。

pub(super) use crate::builtin::ui::kernel::{
    ElementSelector, WaitState, WindowQuery, poll_interval_ms, selector_chain_from_params,
    wait_state_from_params, window_query_from_params,
};
pub(super) use crate::builtin::util::{opt_bool, opt_i64, require_map, require_str};
pub(super) use corex_core::{ActionError, ExecutionContext, Value};
pub(super) use std::collections::BTreeMap;
pub(super) use std::ffi::OsString;
pub(super) use std::os::windows::ffi::OsStringExt;
pub(super) use std::time::{Duration, Instant};
pub(super) use windows::Win32::Foundation::{HWND, LPARAM, RECT};
pub(super) use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput, VIRTUAL_KEY,
};
pub(super) use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetClassNameW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, SetCursorPos, SetForegroundWindow,
};
pub(super) use windows::core::BOOL;

mod desktop;
mod element;
mod input;
mod window;

// 各个门面（`ui::window` / `ui::element` / `ui::input`）、`ui_probe` 与 `ui_inspect` 都以
// `ui::win::<name>` 的形式访问这些；拆文件时不能把它们的路径改掉。
pub(crate) use desktop::ui_desktop_icons_impl;
pub(crate) use element::{
    element_at_point, element_in_scope, element_map_with_selectors, ui_element_click_impl,
    ui_element_exists_impl, ui_element_find_impl, ui_element_find_probe_impl, ui_element_get_impl,
    ui_element_set_impl, ui_element_wait_impl, ui_elements_impl, ui_elements_probe_impl,
};
pub(crate) use input::{
    ui_click_impl, ui_drag_impl, ui_key_impl, ui_scroll_impl, ui_type_impl, ui_wait_impl,
};
pub(crate) use window::{
    ui_window_find_impl, ui_window_focus_impl, ui_window_wait_impl, ui_windows_impl,
};
