//! Browser-style element picker: hover highlight + click to capture selector YAML.

use corex_core::{ActionError, Value};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;
use uiautomation::types::Rect;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetCursorPos,
    GetMessageW, GWLP_USERDATA, HWND_TOPMOST, KillTimer, LWA_ALPHA, LoadCursorW, MSG,
    PostQuitMessage, RegisterClassW, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos,
    SetWindowTextW, ShowWindow, TranslateMessage, WM_DESTROY, WM_LBUTTONUP, WM_TIMER, WNDCLASSW,
    WNDPROC, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_VISIBLE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA,
};

const BORDER_CLASS: &str = "CorexUiPickBorder";
const OVERLAY_CLASS: &str = "CorexUiPickOverlay";
const TOOLTIP_CLASS: &str = "CorexUiPickTooltip";
const BORDER_SIZE: i32 = 3;
const PICK_TIMER_ID: usize = 1;
const POLL_MS: u32 = 16;

struct PickClasses {
    border: Vec<u16>,
    overlay: Vec<u16>,
    tooltip: Vec<u16>,
}

static PICK_CLASSES: OnceLock<PickClasses> = OnceLock::new();

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn init_pick_classes(instance: HINSTANCE) -> Result<(), ActionError> {
    PICK_CLASSES.get_or_init(|| PickClasses {
        border: wide(BORDER_CLASS),
        overlay: wide(OVERLAY_CLASS),
        tooltip: wide(TOOLTIP_CLASS),
    });

    unsafe {
        let cursor = LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_ARROW)
            .map_err(|e| ActionError::execution(format!("LoadCursorW: {e}")))?;
        let brush = CreateSolidBrush(COLORREF(0x0000_00FF));
        let classes = PICK_CLASSES.get().expect("PICK_CLASSES");

        let register = |name: &[u16], proc: WNDPROC| {
            let wc = WNDCLASSW {
                lpfnWndProc: proc,
                hInstance: instance,
                lpszClassName: windows::core::PCWSTR(name.as_ptr()),
                hCursor: cursor,
                hbrBackground: brush,
                style: CS_HREDRAW | CS_VREDRAW,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        };
        register(&classes.border, Some(static_wnd_proc));
        register(&classes.tooltip, Some(static_wnd_proc));
        register(&classes.overlay, Some(overlay_wnd_proc));
    }
    Ok(())
}

unsafe extern "system" fn static_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

struct PickUi {
    borders: [HWND; 4],
    tooltip: HWND,
    overlay: HWND,
}

impl PickUi {
    fn new() -> Self {
        Self {
            borders: [HWND::default(); 4],
            tooltip: HWND::default(),
            overlay: HWND::default(),
        }
    }

    fn create(&mut self) -> Result<(), ActionError> {
        unsafe {
            let instance = GetModuleHandleW(None)
                .map_err(|e| ActionError::execution(format!("GetModuleHandleW: {e}")))?;
            init_pick_classes(HINSTANCE(instance.0))?;

            let classes = PICK_CLASSES.get().expect("PICK_CLASSES");
            let ex = WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
            let style = WS_POPUP | WS_VISIBLE;

            for i in 0..4 {
                self.borders[i] = CreateWindowExW(
                    ex,
                    windows::core::PCWSTR(classes.border.as_ptr()),
                    windows::core::PCWSTR::null(),
                    style,
                    0,
                    0,
                    BORDER_SIZE,
                    BORDER_SIZE,
                    None,
                    None,
                    HINSTANCE(instance.0),
                    None,
                )
                .map_err(|e| ActionError::execution(format!("CreateWindowExW border: {e}")))?;
            }

            self.tooltip = CreateWindowExW(
                ex,
                windows::core::PCWSTR(classes.tooltip.as_ptr()),
                windows::core::PCWSTR::null(),
                style,
                0,
                0,
                320,
                24,
                None,
                None,
                HINSTANCE(instance.0),
                None,
            )
            .map_err(|e| ActionError::execution(format!("CreateWindowExW tooltip: {e}")))?;

            let vx = windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_XVIRTUALSCREEN,
            );
            let vy = windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_YVIRTUALSCREEN,
            );
            let vw = windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_CXVIRTUALSCREEN,
            );
            let vh = windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_CYVIRTUALSCREEN,
            );

            self.overlay = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                windows::core::PCWSTR(classes.overlay.as_ptr()),
                windows::core::PCWSTR::null(),
                WS_POPUP | WS_VISIBLE,
                vx,
                vy,
                vw,
                vh,
                None,
                None,
                HINSTANCE(instance.0),
                None,
            )
            .map_err(|e| ActionError::execution(format!("CreateWindowExW overlay: {e}")))?;
            SetLayeredWindowAttributes(self.overlay, COLORREF(0), 1, LWA_ALPHA)
                .map_err(|e| ActionError::execution(format!("SetLayeredWindowAttributes: {e}")))?;
        }
        Ok(())
    }

    fn hide_highlight(&self) {
        unsafe {
            for h in &self.borders {
                if !h.is_invalid() {
                    let _ = ShowWindow(*h, SW_HIDE);
                }
            }
            if !self.tooltip.is_invalid() {
                let _ = ShowWindow(self.tooltip, SW_HIDE);
            }
        }
    }

    fn show_highlight(&mut self, rect: &Rect, label: &str) {
        unsafe {
            let left = rect.get_left();
            let top = rect.get_top();
            let width = rect.get_width().max(1);
            let height = rect.get_height().max(1);
            let borders = [
                (
                    left - BORDER_SIZE,
                    top - BORDER_SIZE,
                    BORDER_SIZE,
                    height + 2 * BORDER_SIZE,
                ),
                (
                    left - BORDER_SIZE,
                    top - BORDER_SIZE,
                    width + 2 * BORDER_SIZE,
                    BORDER_SIZE,
                ),
                (
                    left + width,
                    top - BORDER_SIZE,
                    BORDER_SIZE,
                    height + 2 * BORDER_SIZE,
                ),
                (
                    left - BORDER_SIZE,
                    top + height,
                    width + 2 * BORDER_SIZE,
                    BORDER_SIZE,
                ),
            ];
            for (i, (x, y, w, h)) in borders.iter().enumerate() {
                let _ = SetWindowPos(
                    self.borders[i],
                    HWND_TOPMOST,
                    *x,
                    *y,
                    *w,
                    *h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                let _ = ShowWindow(self.borders[i], SW_SHOWNA);
            }
            let tip_y = (top - 28).max(0);
            let _ = SetWindowPos(
                self.tooltip,
                HWND_TOPMOST,
                left,
                tip_y,
                480,
                24,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = SetWindowTextW(self.tooltip, windows::core::PCWSTR(wide(label).as_ptr()));
            let _ = ShowWindow(self.tooltip, SW_SHOWNA);
        }
    }

    fn destroy(self) {
        unsafe {
            for h in self.borders {
                if !h.is_invalid() {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(h);
                }
            }
            if !self.tooltip.is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.tooltip);
            }
            if !self.overlay.is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.overlay);
            }
        }
    }
}

struct PickSession {
    ui: PickUi,
    scope_hwnd: Option<i64>,
    done: bool,
    cancelled: bool,
    result: Option<BTreeMap<String, Value>>,
}

impl PickSession {
    fn label_for_map(m: &BTreeMap<String, Value>) -> String {
        let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let aid = m
            .get("automation_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let ct = m.get("control_type").and_then(|v| v.as_str()).unwrap_or("");
        if !name.is_empty() {
            format!("{ct} \"{name}\"")
        } else if !aid.is_empty() {
            format!("{ct} id={aid}")
        } else {
            ct.to_string()
        }
    }

    fn element_at_cursor(&self) -> Result<uiautomation::UIElement, ActionError> {
        let mut pt = windows::Win32::Foundation::POINT::default();
        unsafe {
            GetCursorPos(&mut pt)
                .map_err(|e| ActionError::execution(format!("GetCursorPos: {e}")))?;
        }
        let el = crate::builtin::ui::win::element_at_point(pt.x, pt.y)?;
        if let Some(scope) = self.scope_hwnd {
            if !crate::builtin::ui::win::element_in_scope(&el, scope)? {
                return Err(ActionError::execution("不在 scope 窗口内"));
            }
        }
        Ok(el)
    }

    fn on_tick(&mut self) -> Result<(), ActionError> {
        if unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000 != 0 } {
            self.cancelled = true;
            self.done = true;
            unsafe {
                PostQuitMessage(0);
            }
            return Ok(());
        }
        if self.done {
            return Ok(());
        }

        let el = match self.element_at_cursor() {
            Ok(el) => el,
            Err(_) => {
                self.ui.hide_highlight();
                return Ok(());
            }
        };
        let rect = el
            .get_bounding_rectangle()
            .map_err(|e| ActionError::execution(format!("get_bounding_rectangle: {e}")))?;
        if rect.get_width() <= 0 && rect.get_height() <= 0 {
            self.ui.hide_highlight();
            return Ok(());
        }
        let map = crate::builtin::ui::win::element_map_with_selectors(&el);
        let label = Self::label_for_map(&map);
        self.ui.show_highlight(&rect, &label);
        Ok(())
    }

    fn select_at_cursor(&mut self) -> Result<(), ActionError> {
        let el = self.element_at_cursor()?;
        self.result = Some(crate::builtin::ui::win::element_map_with_selectors(&el));
        self.done = true;
        Ok(())
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW;

    let session_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PickSession };
    match msg {
        WM_LBUTTONUP => {
            if !session_ptr.is_null() {
                if unsafe { (*session_ptr).select_at_cursor() }.is_ok() {
                    unsafe { PostQuitMessage(0) };
                }
            }
        }
        WM_TIMER => {
            if wparam.0 == PICK_TIMER_ID && !session_ptr.is_null() {
                let _ = unsafe { (*session_ptr).on_tick() };
            }
        }
        WM_DESTROY => unsafe { PostQuitMessage(0) },
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn run_pick_blocking(scope_hwnd: Option<i64>) -> Result<BTreeMap<String, Value>, ActionError> {
    let mut ui = PickUi::new();
    ui.create()?;
    let mut session = PickSession {
        ui,
        scope_hwnd,
        done: false,
        cancelled: false,
        result: None,
    };

    unsafe {
        SetWindowLongPtrW(
            session.ui.overlay,
            GWLP_USERDATA,
            &mut session as *mut _ as isize,
        );
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
            session.ui.overlay,
            PICK_TIMER_ID,
            POLL_MS,
            None,
        );
    }
    session.on_tick()?;

    eprintln!("corex ui pick: 移动鼠标高亮元素，左键选中，Esc 取消");

    let mut msg = MSG::default();
    loop {
        let ok = unsafe { GetMessageW(&mut msg, None, 0, 0).0 > 0 };
        if !ok || session.done {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    unsafe {
        let _ = KillTimer(session.ui.overlay, PICK_TIMER_ID);
        SetWindowLongPtrW(session.ui.overlay, GWLP_USERDATA, 0);
    }
    session.ui.hide_highlight();
    let ui = session.ui;
    ui.destroy();

    if session.cancelled {
        return Err(ActionError::execution("已取消"));
    }
    session
        .result
        .ok_or_else(|| ActionError::execution("未选中元素"))
}

/// Interactive pick: hover to highlight, click to capture selector YAML.
pub async fn probe_pick(scope_hwnd: Option<i64>) -> Result<Value, ActionError> {
    tokio::task::spawn_blocking(move || {
        let map = run_pick_blocking(scope_hwnd)?;
        Ok(Value::Map(map))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui pick 失败: {e}")))?
}
