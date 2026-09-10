use super::element::elem_to_map_with_options;
use super::window::window_class;
use super::*;

fn hosts_shell_defview(parent: HWND) -> bool {
    unsafe {
        FindWindowExW(
            Some(parent),
            None,
            windows::core::w!("SHELLDLL_DefView"),
            None,
        )
        .is_ok()
    }
}

/// 解析桌面 ListItem 图标（Progman 或 WorkerW + DefView）的 UIA 根。
fn find_desktop_hwnd() -> Option<HWND> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, SMTO_NORMAL, SendMessageTimeoutW,
    };
    use windows::core::BOOL;

    struct EnumState {
        found: Option<HWND>,
    }

    unsafe extern "system" fn enum_workerw(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let state = unsafe { &mut *(lparam.0 as *mut EnumState) };
        if window_class(hwnd) != "WorkerW" || !hosts_shell_defview(hwnd) {
            return BOOL(1);
        }
        state.found = Some(hwnd);
        BOOL(0)
    }

    unsafe {
        let progman = FindWindowW(windows::core::w!("Progman"), None).ok()?;
        if !IsWindow(Some(progman)).as_bool() {
            return None;
        }
        if hosts_shell_defview(progman) {
            return Some(progman);
        }
        // Win10/11：派生一个承载 SHELLDLL_DefView 的 WorkerW 兄弟窗口。
        let _ = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0),
            LPARAM(0),
            SMTO_NORMAL,
            1000,
            None,
        );
        let mut state = EnumState { found: None };
        let _ = EnumWindows(
            Some(enum_workerw),
            LPARAM(&mut state as *mut EnumState as isize),
        );
        state.found.or(Some(progman))
    }
}

pub async fn ui_desktop_icons_impl() -> Result<Value, ActionError> {
    tokio::task::spawn_blocking(move || {
        let auto = uiautomation::UIAutomation::new()
            .map_err(|e| ActionError::execution(format!("UIAutomation 初始化失败: {e}")))?;
        let hwnd = find_desktop_hwnd()
            .ok_or_else(|| ActionError::ui("ui_desktop_not_found", "未找到桌面 Shell 窗口"))?;
        let handle = uiautomation::types::Handle::from(hwnd.0 as isize);
        let root = auto
            .element_from_handle(handle)
            .map_err(|e| ActionError::execution(format!("ElementFromHandle 失败: {e}")))?;
        let matcher = auto
            .create_matcher()
            .from(root)
            .control_type(uiautomation::types::ControlType::ListItem)
            .depth(4)
            .timeout(1000);
        let found = matcher.find_all().unwrap_or_default();
        let icons: Vec<Value> = found
            .iter()
            .map(|el| Value::Map(elem_to_map_with_options(el, false)))
            .collect();
        let mut out = BTreeMap::new();
        out.insert("icons".into(), Value::Array(icons));
        Ok(Value::Map(out))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.window.desktop 失败: {e}")))?
}
