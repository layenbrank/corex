//! 浏览器风格的 Inspect：悬停高亮 + 点击采集 selector YAML。

use corex_core::{ActionError, Value};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::OnceLock;
use uiautomation::types::Rect;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush,
    DEFAULT_CHARSET, DEFAULT_PITCH, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FF_DONTCARE, FW_NORMAL, FillRect, HBRUSH, HFONT,
    HGDIOBJ, InvalidateRect, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA,
    GetClientRect, GetCursorPos, GetMessageW, GetWindowLongPtrW, GetWindowTextW, HWND_BOTTOM,
    HWND_TOPMOST, KillTimer, LoadCursorW, MSG, PostQuitMessage, RegisterClassW, SW_HIDE, SW_SHOWNA,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos,
    SetWindowTextW, ShowWindow, TranslateMessage, WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WM_TIMER,
    WNDCLASSW, WNDPROC, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WindowFromPoint,
};

const BORDER_CLASS: &str = "CorexUiInspectBorder";
const LABEL_CLASS: &str = "CorexUiInspectLabel";
const MSG_CLASS: &str = "CorexUiInspectMsg";
const BORDER_SIZE: i32 = 3;
const INSPECT_TIMER_ID: usize = 1;
const POLL_MS: u32 = 16;

/// 高亮框的颜色（3px 实心条，靠类画刷填充）。
const HIGHLIGHT: COLORREF = COLORREF(0x0000_00FF);
/// 标签条：深灰底 + 白字，尺寸跟着元素宽窄在 `LABEL_MIN_W..=LABEL_MAX_W` 之间走。
const LABEL_BG: COLORREF = COLORREF(0x0020_2020);
const LABEL_FG: COLORREF = COLORREF(0x00FF_FFFF);
// 下面的尺寸与字高都按 96 DPI 写，实际像素由 `Metrics::px` 折算。
const LABEL_H: i32 = 24;
const LABEL_GAP: i32 = 4;
const LABEL_PAD: i32 = 8;
const LABEL_MIN_W: i32 = 160;
const LABEL_MAX_W: i32 = 480;
const LABEL_FONT_PX: i32 = 14;
const LABEL_TEXT_MAX: usize = 512;

struct InspectClasses {
    border: Vec<u16>,
    label: Vec<u16>,
    msg: Vec<u16>,
}

static INSPECT_CLASSES: OnceLock<InspectClasses> = OnceLock::new();

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// 96 DPI 基准尺寸到元素所在显示器像素的折算。
/// 多屏缩放不同时不能拿主屏 DC 的 DPI 充数：那在 150% 屏上 overlay 只有 2/3 大。
struct Metrics {
    dpi: i32,
}

impl Metrics {
    fn at(x: i32, y: i32) -> Self {
        let hwnd = unsafe { WindowFromPoint(POINT { x, y }) };
        Self {
            dpi: if hwnd.is_invalid() { 96 } else { dpi_of(hwnd) },
        }
    }

    fn of(hwnd: HWND) -> Self {
        Self { dpi: dpi_of(hwnd) }
    }

    fn px(&self, base: i32) -> i32 {
        base * self.dpi / 96
    }
}

/// `GetDpiForWindow` 对无效窗口返回 0，当作 96 处理。
fn dpi_of(hwnd: HWND) -> i32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi > 0 { dpi as i32 } else { 96 }
}

/// 标签字体：显式建 TrueType + `CLEARTYPE_QUALITY`。DC 默认字体是 System 点阵字体，
/// 既不抗锯齿、被系统拉伸后更糊；中文交给字体链接回退到系统中文，不用另挑字型。
/// 句柄由标签窗口自己的 `GWLP_USERDATA` 持有，随窗口销毁。
fn label_font(px: i32) -> HFONT {
    let face = wide("Segoe UI");
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            u32::from(DEFAULT_PITCH.0 | FF_DONTCARE.0),
            windows::core::PCWSTR(face.as_ptr()),
        )
    }
}

fn init_inspect_classes(instance: HINSTANCE) -> Result<(), ActionError> {
    INSPECT_CLASSES.get_or_init(|| InspectClasses {
        border: wide(BORDER_CLASS),
        label: wide(LABEL_CLASS),
        msg: wide(MSG_CLASS),
    });

    unsafe {
        let cursor = LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_ARROW)
            .map_err(|e| ActionError::execution(format!("LoadCursorW: {e}")))?;
        let classes = INSPECT_CLASSES.get().expect("INSPECT_CLASSES");

        let register = |name: &[u16], proc: WNDPROC, brush: HBRUSH| {
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
        // 高亮框只填色不画字，默认窗口过程够用。
        register(
            &classes.border,
            Some(default_wnd_proc),
            CreateSolidBrush(HIGHLIGHT),
        );
        // 标签条的文字必须自己画：`WS_POPUP` 没有 `WS_CAPTION`，默认窗口过程不会画窗口
        // 文字，若再给它类画刷，屏幕上就只剩一条纯色空条。
        register(&classes.label, Some(label_wnd_proc), HBRUSH::default());
        // 消息宿主只用来挂定时器，从不显示。
        register(&classes.msg, Some(msg_wnd_proc), HBRUSH::default());
    }
    Ok(())
}

/// `DefWindowProcW` 在 windows-rs 里是 Rust ABI，不能直接当 `WNDPROC`（`extern "system"`）用。
unsafe extern "system" fn default_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// 标签条窗口过程：把元素描述画出来。
unsafe extern "system" fn label_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // 底色与文字在一次 `WM_PAINT` 里画完，跳过系统擦除以免闪烁。
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            unsafe { paint_label(hwnd) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// 自绘标签：铺底色 → 取回窗口文本（`SetWindowTextW` 是唯一来源）→ 单行居中、超宽省略号。
unsafe fn paint_label(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    if hdc.is_invalid() {
        return;
    }
    let m = Metrics::of(hwnd);
    let pad = m.px(LABEL_PAD);
    unsafe {
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_ok() {
            let brush = CreateSolidBrush(LABEL_BG);
            if !brush.is_invalid() {
                FillRect(hdc, &rect, brush);
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
            let font = HFONT(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut std::ffi::c_void);
            let previous = if font.is_invalid() {
                HGDIOBJ::default()
            } else {
                SelectObject(hdc, HGDIOBJ(font.0))
            };
            let mut text = [0u16; LABEL_TEXT_MAX];
            let len = GetWindowTextW(hwnd, &mut text).max(0) as usize;
            let mut inner = RECT {
                left: rect.left + pad,
                top: rect.top,
                right: (rect.right - pad).max(rect.left + pad),
                bottom: rect.bottom,
            };
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, LABEL_FG);
            let _ = DrawTextW(
                hdc,
                &mut text[..len],
                &mut inner,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            if !previous.is_invalid() {
                let _ = SelectObject(hdc, previous);
            }
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

struct InspectUi {
    borders: [HWND; 4],
    label: HWND,
    msg_hwnd: HWND,
    font: HFONT,
    font_dpi: i32,
}

impl InspectUi {
    fn new() -> Self {
        Self {
            borders: [HWND::default(); 4],
            label: HWND::default(),
            msg_hwnd: HWND::default(),
            font: HFONT::default(),
            font_dpi: 0,
        }
    }

    /// 标签条跨显示器时得换字号，否则在 150% 屏上沿用 96 DPI 的字就只有 2/3 大。
    fn sync_font(&mut self, m: &Metrics) {
        if self.font_dpi == m.dpi && !self.font.is_invalid() {
            return;
        }
        let font = label_font(m.px(LABEL_FONT_PX));
        if font.is_invalid() {
            return;
        }
        unsafe {
            if !self.font.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
            SetWindowLongPtrW(self.label, GWLP_USERDATA, font.0 as isize);
        }
        self.font = font;
        self.font_dpi = m.dpi;
    }

    fn create(&mut self) -> Result<(), ActionError> {
        unsafe {
            let instance = GetModuleHandleW(None)
                .map_err(|e| ActionError::execution(format!("GetModuleHandleW: {e}")))?;
            init_inspect_classes(HINSTANCE(instance.0))?;

            let classes = INSPECT_CLASSES.get().expect("INSPECT_CLASSES");
            let ex = WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
            let style = WS_POPUP;

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
                    Some(HINSTANCE(instance.0)),
                    None,
                )
                .map_err(|e| ActionError::execution(format!("CreateWindowExW border: {e}")))?;
            }

            self.label = CreateWindowExW(
                ex,
                windows::core::PCWSTR(classes.label.as_ptr()),
                windows::core::PCWSTR::null(),
                style,
                0,
                0,
                LABEL_MAX_W,
                LABEL_H,
                None,
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
            .map_err(|e| ActionError::execution(format!("CreateWindowExW label: {e}")))?;

            self.msg_hwnd = CreateWindowExW(
                ex,
                windows::core::PCWSTR(classes.msg.as_ptr()),
                windows::core::PCWSTR::null(),
                style,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(HINSTANCE(instance.0)),
                None,
            )
            .map_err(|e| ActionError::execution(format!("CreateWindowExW msg: {e}")))?;
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
            if !self.label.is_invalid() {
                let _ = ShowWindow(self.label, SW_HIDE);
            }
        }
    }

    fn show_highlight(&mut self, rect: &Rect, text: &str) {
        let m = Metrics::at(rect.get_left(), rect.get_top());
        let border = m.px(BORDER_SIZE);
        let label_h = m.px(LABEL_H);
        self.sync_font(&m);
        unsafe {
            let left = rect.get_left();
            let top = rect.get_top();
            let width = rect.get_width().max(1);
            let height = rect.get_height().max(1);
            let borders = [
                (left - border, top - border, border, height + 2 * border),
                (left - border, top - border, width + 2 * border, border),
                (left + width, top - border, border, height + 2 * border),
                (left - border, top + height, width + 2 * border, border),
            ];
            for (i, (x, y, w, h)) in borders.iter().enumerate() {
                let _ = SetWindowPos(
                    self.borders[i],
                    Some(HWND_TOPMOST),
                    *x,
                    *y,
                    *w,
                    *h,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                let _ = ShowWindow(self.borders[i], SW_SHOWNA);
            }
            let tip_y = (top - label_h - m.px(LABEL_GAP)).max(0);
            // 描述为空就别顶一条空色块出来。
            if text.is_empty() {
                let _ = ShowWindow(self.label, SW_HIDE);
                return;
            }
            // 宽度跟着元素走：小控件不至于被一条大黑条盖住。
            let label_w = width.clamp(m.px(LABEL_MIN_W), m.px(LABEL_MAX_W));
            let _ = SetWindowPos(
                self.label,
                Some(HWND_TOPMOST),
                left,
                tip_y,
                label_w,
                label_h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = SetWindowTextW(self.label, windows::core::PCWSTR(wide(text).as_ptr()));
            let _ = InvalidateRect(Some(self.label), None, true);
            let _ = ShowWindow(self.label, SW_SHOWNA);
        }
    }

    fn destroy(self) {
        unsafe {
            for h in self.borders {
                if !h.is_invalid() {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(h);
                }
            }
            if !self.label.is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.label);
            }
            if !self.msg_hwnd.is_invalid() {
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.msg_hwnd);
            }
            if !self.font.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
        }
    }
}

struct InspectSession {
    ui: InspectUi,
    scope_hwnd: Option<i64>,
    done: bool,
    cancelled: bool,
    prev_lbutton_down: bool,
    result: Option<BTreeMap<String, Value>>,
}

impl InspectSession {
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
        if let Some(scope) = self.scope_hwnd
            && !crate::builtin::ui::win::element_in_scope(&el, scope)?
        {
            return Err(ActionError::execution("不在 scope 窗口内"));
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

        let lbutton_down = unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000 != 0 };
        if lbutton_down && !self.prev_lbutton_down && !self.done {
            match self.select_at_cursor() {
                Ok(()) => unsafe {
                    PostQuitMessage(0);
                },
                Err(e) => {
                    eprintln!(
                        "corex ui element inspect: 未选中（{e}）— 请在目标窗口内点击，或按 Esc 取消"
                    );
                }
            }
        }
        self.prev_lbutton_down = lbutton_down;

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

unsafe extern "system" fn msg_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let session_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut InspectSession };
    if msg == WM_TIMER && wparam.0 == INSPECT_TIMER_ID && !session_ptr.is_null() {
        let _ = unsafe { (*session_ptr).on_tick() };
    }
    if msg == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn push_console_to_back() {
    unsafe {
        let console = GetConsoleWindow();
        if !console.is_invalid() {
            let _ = SetWindowPos(
                console,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

fn run_inspect_blocking(scope_hwnd: Option<i64>) -> Result<BTreeMap<String, Value>, ActionError> {
    push_console_to_back();
    let mut ui = InspectUi::new();
    ui.create()?;
    let mut session = InspectSession {
        ui,
        scope_hwnd,
        done: false,
        cancelled: false,
        prev_lbutton_down: false,
        result: None,
    };

    // USERDATA 只在定时器生命周期内持有 InspectSession；销毁前会清空。
    unsafe {
        SetWindowLongPtrW(
            session.ui.msg_hwnd,
            GWLP_USERDATA,
            &mut session as *mut _ as isize,
        );
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(
            Some(session.ui.msg_hwnd),
            INSPECT_TIMER_ID,
            POLL_MS,
            None,
        );
    }
    session.on_tick()?;

    eprintln!("corex ui element inspect: 移动鼠标高亮元素，左键选中，Esc 取消");

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
        let _ = KillTimer(Some(session.ui.msg_hwnd), INSPECT_TIMER_ID);
        SetWindowLongPtrW(session.ui.msg_hwnd, GWLP_USERDATA, 0);
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

/// 交互式 Inspect：悬停高亮，点击采集 selector YAML。
pub async fn probe_inspect(scope_hwnd: Option<i64>) -> Result<Value, ActionError> {
    tokio::task::spawn_blocking(move || {
        let map = run_inspect_blocking(scope_hwnd)?;
        Ok(Value::Map(map))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui inspect 失败: {e}")))?
}
