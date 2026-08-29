//! Windows UIAutomation / Win32 platform adapter for ui.* actions.

use crate::builtin::ui_kernel::{
    poll_interval_ms, selector_chain_from_params, wait_state_from_params, window_query_from_params,
    ElementSelector, WaitState, WindowQuery,
};
use crate::builtin::util::{opt_bool, opt_i64, require_map, require_str};
use corex_core::{ActionError, ExecutionContext, Value};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::time::{Duration, Instant};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, KEYBD_EVENT_FLAGS, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEEVENTF_HWHEEL, MOUSEINPUT, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetClassNameW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, SetCursorPos, SetForegroundWindow,
};

fn hwnd_to_i64(hwnd: HWND) -> i64 {
    hwnd.0 as isize as i64
}

fn hwnd_from_i64(id: i64) -> HWND {
    HWND(id as isize as *mut core::ffi::c_void)
}

fn window_title(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    let read = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if read <= 0 {
        return String::new();
    }
    OsString::from_wide(&buf[..read as usize])
        .to_string_lossy()
        .into_owned()
}

fn window_class(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    OsString::from_wide(&buf[..n as usize])
        .to_string_lossy()
        .into_owned()
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

fn window_area(hwnd: HWND) -> i64 {
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect).is_err() } {
        return 0;
    }
    let w = (rect.right - rect.left).max(0) as i64;
    let h = (rect.bottom - rect.top).max(0) as i64;
    w * h
}

fn title_matches(query: &WindowQuery, title: &str) -> bool {
    let lower = title.to_lowercase();
    if let Some(needle) = &query.title_contains {
        if !lower.contains(&needle.to_lowercase()) {
            return false;
        }
    }
    for ex in &query.title_excludes {
        if lower.contains(&ex.to_lowercase()) {
            return false;
        }
    }
    true
}

fn collect_windows(query: &WindowQuery) -> Vec<HWND> {
    let out: Vec<HWND> = Vec::new();
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = lparam.0 as *mut (WindowQuery, Vec<HWND>);
        if ctx.is_null() {
            return BOOL(0);
        }
        let (query, list) = unsafe { &mut *ctx };
        if query.visible_only && !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return BOOL(1);
        }
        let title = window_title(hwnd);
        if title.is_empty() {
            return BOOL(1);
        }
        if !title_matches(query, &title) {
            return BOOL(1);
        }
        if let Some(want_class) = &query.class_name {
            if window_class(hwnd) != *want_class {
                return BOOL(1);
            }
        }
        list.push(hwnd);
        BOOL(1)
    }
    let q = query.clone();
    let mut ctx = (q, out);
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.1
}

fn find_window(query: &WindowQuery) -> Option<HWND> {
    if let Some(id) = query.hwnd {
        let hwnd = hwnd_from_i64(id);
        if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
            return None;
        }
        if unsafe { IsWindowVisible(hwnd).as_bool() } || !query.visible_only {
            return Some(hwnd);
        }
        return None;
    }
    let mut matches = collect_windows(query);
    if matches.is_empty() {
        return None;
    }
    if query.prefer_largest {
        matches.sort_by_key(|h| window_area(*h));
    }
    matches.last().copied()
}

fn hwnd_map(hwnd: HWND) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    m.insert("hwnd".into(), Value::Int(hwnd_to_i64(hwnd)));
    m.insert("title".into(), Value::Str(window_title(hwnd)));
    m.insert("class".into(), Value::Str(window_class(hwnd)));
    m.insert("pid".into(), Value::Int(window_pid(hwnd) as i64));
    m
}

fn resolve_scope_hwnd(
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
) -> Result<HWND, ActionError> {
    let mut q = window_query_from_params(map, ctx)?;
    if q.title_contains.is_none() && map.get("name").and_then(|v| v.as_str()).is_some() {
        q.title_contains = map.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
    }
    find_window(&q).ok_or_else(|| {
        ActionError::ui(
            "ui_wrong_window",
            format!(
                "未找到窗口: {}",
                q.title_contains.unwrap_or_else(|| "scope".into())
            ),
        )
    })
}

/// Probe-only scope: explicit `--hwnd` / `--title` in params; no session fallback.
fn resolve_probe_scope_hwnd(map: &BTreeMap<String, Value>) -> Result<HWND, ActionError> {
    use crate::builtin::ui_kernel::probe_scope_explicit;
    probe_scope_explicit(map)?;
    let q = WindowQuery {
        hwnd: map.get("hwnd").and_then(|v| v.as_i64()),
        title_contains: map.get("title_contains").and_then(|v| v.as_str()).map(|s| s.to_string()),
        class_name: map.get("class_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
        visible_only: map
            .get("visible_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        prefer_largest: false,
        title_excludes: Vec::new(),
    };
    find_window(&q).ok_or_else(|| {
        ActionError::ui(
            "ui_wrong_window",
            format!(
                "未找到窗口: {}",
                q.title_contains.unwrap_or_else(|| format!("hwnd={:?}", q.hwnd))
            ),
        )
    })
}

fn ancestor_map(el: &uiautomation::UIElement) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    if let Ok(n) = el.get_name() {
        if !n.is_empty() {
            m.insert("name".into(), Value::Str(n));
        }
    }
    if let Ok(aid) = el.get_automation_id() {
        if !aid.is_empty() {
            m.insert("automation_id".into(), Value::Str(aid));
        }
    }
    if let Some(ct) = format_control_type(el) {
        m.insert("control_type".into(), Value::Str(ct));
    }
    m
}

fn collect_ancestors(el: &uiautomation::UIElement) -> Vec<BTreeMap<String, Value>> {
    let auto = match uiautomation::UIAutomation::new() {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };
    let walker = match auto.get_control_view_walker() {
        Ok(w) => w,
        Err(_) => return Vec::new(),
    };
    let mut current = el.clone();
    let mut chain = Vec::new();
    for _ in 0..12 {
        match walker.get_parent(&current) {
            Ok(parent) => {
                let m = ancestor_map(&parent);
                if m.is_empty() {
                    break;
                }
                chain.push(m);
                current = parent;
            }
            Err(_) => break,
        }
    }
    chain.reverse();
    chain
}

pub async fn ui_window_list_impl() -> Result<Value, ActionError> {
    tokio::task::spawn_blocking(|| {
        let mut windows: Vec<Value> = Vec::new();
        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let list = lparam.0 as *mut Vec<Value>;
            if list.is_null() {
                return BOOL(0);
            }
            if !unsafe { IsWindowVisible(hwnd).as_bool() } {
                return BOOL(1);
            }
            let title = window_title(hwnd);
            if title.is_empty() {
                return BOOL(1);
            }
            let mut m = BTreeMap::new();
            m.insert("hwnd".into(), Value::Int(hwnd_to_i64(hwnd)));
            m.insert("title".into(), Value::Str(title));
            m.insert("class".into(), Value::Str(window_class(hwnd)));
            m.insert("pid".into(), Value::Int(window_pid(hwnd) as i64));
            unsafe {
                (*list).push(Value::Map(m));
            }
            BOOL(1)
        }
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut windows as *mut _ as isize));
        }
        let mut out = BTreeMap::new();
        out.insert("windows".into(), Value::List(windows));
        Ok(Value::Map(out))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.window.list 失败: {e}")))?
}

pub async fn ui_window_focus_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let out: BTreeMap<String, Value> = tokio::task::spawn_blocking(
        move || -> Result<BTreeMap<String, Value>, ActionError> {
            let hwnd = resolve_scope_hwnd(&map, &exec_ctx)?;
            unsafe {
                if !SetForegroundWindow(hwnd).as_bool() {
                    return Err(ActionError::execution("SetForegroundWindow 失败"));
                }
            }
            Ok(hwnd_map(hwnd))
        },
    )
    .await
    .map_err(|e| ActionError::execution(format!("ui.window.focus 失败: {e}")))?
    ?;
    if let Some(id) = out.get("hwnd").and_then(|v| v.as_i64()) {
        let title = out.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
        ctx.set_ui_scope(id, title);
    }
    Ok(Value::Map(out))
}

pub async fn ui_window_find_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let out: BTreeMap<String, Value> = tokio::task::spawn_blocking(
        move || -> Result<BTreeMap<String, Value>, ActionError> {
            let hwnd = resolve_scope_hwnd(&map, &exec_ctx)?;
            Ok(hwnd_map(hwnd))
        },
    )
    .await
    .map_err(|e| ActionError::execution(format!("ui.window.find 失败: {e}")))?
    ?;
    if let Some(id) = out.get("hwnd").and_then(|v| v.as_i64()) {
        let title = out.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
        ctx.set_ui_scope(id, title);
    }
    Ok(Value::Map(out))
}

pub async fn ui_window_wait_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let timeout_ms = opt_i64(&map, "timeout_ms", 5000).max(1) as u64;
    let poll = poll_interval_ms(&map, 200);
    let out: BTreeMap<String, Value> = tokio::task::spawn_blocking(
        move || -> Result<BTreeMap<String, Value>, ActionError> {
            let q = window_query_from_params(&map, &exec_ctx)?;
            if q.title_contains.is_none() && q.hwnd.is_none() {
                return Err(ActionError::MissingParam(
                    "title_contains|hwnd|ui_session".into(),
                ));
            }
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            loop {
                if let Some(hwnd) = find_window(&q) {
                    return Ok(hwnd_map(hwnd));
                }
                if Instant::now() >= deadline {
                    let needle = q.title_contains.unwrap_or_else(|| "window".into());
                    return Err(ActionError::ui_with_hint(
                        "ui_sync_timeout",
                        &needle,
                        "等待窗口超时",
                    ));
                }
                std::thread::sleep(Duration::from_millis(poll));
            }
        },
    )
    .await
    .map_err(|e| ActionError::execution(format!("ui.window.wait 失败: {e}")))?
    ?;
    if let Some(id) = out.get("hwnd").and_then(|v| v.as_i64()) {
        let title = out.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
        ctx.set_ui_scope(id, title);
    }
    Ok(Value::Map(out))
}

fn format_control_type(el: &uiautomation::UIElement) -> Option<String> {
    if let Ok(lct) = el.get_localized_control_type() {
        let s = lct.trim();
        if !s.is_empty() {
            return Some(s.to_ascii_lowercase());
        }
    }
    el.get_control_type()
        .ok()
        .map(control_type_enum_name)
}

fn control_type_enum_name(ct: uiautomation::types::ControlType) -> String {
    use uiautomation::types::ControlType;
    match ct {
        ControlType::Button => "button",
        ControlType::Edit => "edit",
        ControlType::Text => "text",
        ControlType::Window => "window",
        ControlType::Pane => "pane",
        ControlType::List => "list",
        ControlType::ListItem => "listitem",
        ControlType::Menu => "menu",
        ControlType::MenuItem => "menuitem",
        ControlType::CheckBox => "checkbox",
        ControlType::ComboBox => "combobox",
        ControlType::Tab => "tab",
        ControlType::TabItem => "tabitem",
        ControlType::Tree => "tree",
        ControlType::TreeItem => "treeitem",
        ControlType::Document => "document",
        ControlType::Hyperlink => "hyperlink",
        ControlType::ToolBar => "toolbar",
        ControlType::ToolTip => "tooltip",
        ControlType::Image => "image",
        ControlType::Group => "group",
        ControlType::TitleBar => "titlebar",
        ControlType::Custom => "custom",
        _ => "unknown",
    }
    .into()
}

pub(crate) fn elem_to_map(el: &uiautomation::UIElement) -> BTreeMap<String, Value> {
    elem_to_map_with_options(el, true)
}

pub(crate) fn elem_to_map_with_options(
    el: &uiautomation::UIElement,
    include_ancestors: bool,
) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    if let Ok(h) = el.get_native_window_handle() {
        let raw: isize = h.into();
        m.insert("hwnd".into(), Value::Int(raw as i64));
    }
    if let Ok(n) = el.get_name() {
        if !n.is_empty() {
            m.insert("name".into(), Value::Str(n));
        }
    }
    if let Ok(aid) = el.get_automation_id() {
        if !aid.is_empty() {
            m.insert("automation_id".into(), Value::Str(aid));
        }
    }
    if let Some(ct) = format_control_type(el) {
        m.insert("control_type".into(), Value::Str(ct));
    }
    if let Ok(cn) = el.get_classname() {
        if !cn.is_empty() {
            m.insert("class".into(), Value::Str(cn));
        }
    }
    if let Ok(rect) = el.get_bounding_rectangle() {
        let w = rect.get_width().max(0);
        let h = rect.get_height().max(0);
        if w > 0 || h > 0 {
            let mut bounds = BTreeMap::new();
            bounds.insert("x".into(), Value::Int(rect.get_left() as i64));
            bounds.insert("y".into(), Value::Int(rect.get_top() as i64));
            bounds.insert("width".into(), Value::Int(w as i64));
            bounds.insert("height".into(), Value::Int(h as i64));
            m.insert("bounds".into(), Value::Map(bounds));
        }
    }
    let enabled = el.is_enabled().unwrap_or(false);
    m.insert("enabled".into(), Value::Bool(enabled));
    let offscreen = el.is_offscreen().unwrap_or(true);
    m.insert("clickable".into(), Value::Bool(enabled && !offscreen));
    if include_ancestors {
        let ancestors: Vec<Value> = collect_ancestors(el)
            .into_iter()
            .map(Value::Map)
            .collect();
        if !ancestors.is_empty() {
            m.insert("ancestors".into(), Value::List(ancestors));
        }
    }
    m
}

pub(crate) fn element_at_point(x: i32, y: i32) -> Result<uiautomation::UIElement, ActionError> {
    let auto = uiautomation::UIAutomation::new()
        .map_err(|e| ActionError::execution(format!("UIAutomation 初始化失败: {e}")))?;
    let pt = uiautomation::types::Point::new(x, y);
    auto.element_from_point(pt)
        .map_err(|e| ActionError::execution(format!("ElementFromPoint ({x},{y}) 失败: {e}")))
}

/// Walk ancestors until native HWND matches `scope_hwnd`.
pub(crate) fn element_in_scope(
    el: &uiautomation::UIElement,
    scope_hwnd: i64,
) -> Result<bool, ActionError> {
    let auto = uiautomation::UIAutomation::new()
        .map_err(|e| ActionError::execution(format!("UIAutomation 初始化失败: {e}")))?;
    let walker = auto
        .get_control_view_walker()
        .map_err(|e| ActionError::execution(format!("TreeWalker 失败: {e}")))?;
    let mut current = el.clone();
    for _ in 0..48 {
        if let Ok(h) = current.get_native_window_handle() {
            let raw: isize = h.into();
            if raw as i64 == scope_hwnd {
                return Ok(true);
            }
        }
        match walker.get_parent(&current) {
            Ok(parent) => current = parent,
            Err(_) => break,
        }
    }
    Ok(false)
}

pub(crate) fn element_map_with_selectors(
    el: &uiautomation::UIElement,
) -> BTreeMap<String, Value> {
    use crate::builtin::ui_kernel::{selector_chain_to_yaml, suggest_selectors};
    let mut m = elem_to_map(el);
    let aid = m.get("automation_id").and_then(|v| v.as_str());
    let name = m.get("name").and_then(|v| v.as_str());
    let class = m.get("class").and_then(|v| v.as_str());
    let ct = m.get("control_type").and_then(|v| v.as_str());
    let chain = suggest_selectors(aid, name, class, ct);
    let selectors: Vec<Value> = chain
        .iter()
        .map(|sel| {
            let mut sm = BTreeMap::new();
            if let Some(a) = &sel.automation_id {
                sm.insert("automation_id".into(), Value::Str(a.clone()));
            }
            if let Some(n) = &sel.name {
                sm.insert("name".into(), Value::Str(n.clone()));
            }
            if let Some(n) = &sel.name_contains {
                sm.insert("name_contains".into(), Value::Str(n.clone()));
            }
            if let Some(c) = &sel.control_type {
                sm.insert("control_type".into(), Value::Str(c.clone()));
            }
            if let Some(c) = &sel.class {
                sm.insert("class".into(), Value::Str(c.clone()));
            }
            Value::Map(sm)
        })
        .collect();
    m.insert("selectors".into(), Value::List(selectors));
    m.insert(
        "selectors_yaml".into(),
        Value::Str(selector_chain_to_yaml(&chain)),
    );
    m
}

fn parse_control_type(s: &str) -> Option<uiautomation::types::ControlType> {
    use uiautomation::types::ControlType;
    match s.to_ascii_lowercase().as_str() {
        "button" => Some(ControlType::Button),
        "edit" => Some(ControlType::Edit),
        "text" => Some(ControlType::Text),
        "window" => Some(ControlType::Window),
        "pane" => Some(ControlType::Pane),
        "list" => Some(ControlType::List),
        "listitem" => Some(ControlType::ListItem),
        "menu" => Some(ControlType::Menu),
        "menuitem" => Some(ControlType::MenuItem),
        "checkbox" => Some(ControlType::CheckBox),
        "combobox" => Some(ControlType::ComboBox),
        "tab" => Some(ControlType::Tab),
        "tabitem" => Some(ControlType::TabItem),
        "tree" => Some(ControlType::Tree),
        "treeitem" => Some(ControlType::TreeItem),
        "document" => Some(ControlType::Document),
        "hyperlink" => Some(ControlType::Hyperlink),
        _ => None,
    }
}

fn apply_selector(
    mut matcher: uiautomation::UIMatcher,
    sel: &ElementSelector,
) -> Result<uiautomation::UIMatcher, ActionError> {
    if let Some(name) = &sel.name {
        matcher = matcher.filter_fn({
            let want = name.clone();
            Box::new(move |el: &uiautomation::UIElement| {
                Ok(el.get_name().map(|n| n == want).unwrap_or(false))
            })
        });
    } else if let Some(part) = &sel.name_contains {
        matcher = matcher.contains_name(part.clone());
    }
    if let Some(ct) = &sel.control_type {
        if let Some(t) = parse_control_type(ct) {
            matcher = matcher.control_type(t);
        } else {
            return Err(ActionError::InvalidParams(format!(
                "未知 control_type: {ct}"
            )));
        }
    }
    if let Some(aid) = &sel.automation_id {
        let want = aid.clone();
        matcher = matcher.filter_fn(Box::new(move |el: &uiautomation::UIElement| {
            Ok(el.get_automation_id().map(|id| id == want).unwrap_or(false))
        }));
    }
    if let Some(class) = &sel.class {
        let want = class.clone();
        matcher = matcher.filter_fn(Box::new(move |el: &uiautomation::UIElement| {
            Ok(el.get_classname().map(|c| c == want).unwrap_or(false))
        }));
    }
    Ok(matcher)
}

fn build_matcher_for(
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
    sel: &ElementSelector,
    timeout_ms: u64,
) -> Result<uiautomation::UIMatcher, ActionError> {
    let auto = uiautomation::UIAutomation::new()
        .map_err(|e| ActionError::execution(format!("UIAutomation 初始化失败: {e}")))?;
    let mut matcher = auto
        .create_matcher()
        .timeout(timeout_ms)
        .depth(sel.depth);
    let hwnd = resolve_scope_hwnd(map, ctx)?;
    let handle = uiautomation::types::Handle::from(hwnd.0 as isize);
    let root = auto
        .element_from_handle(handle)
        .map_err(|e| ActionError::execution(format!("ElementFromHandle 失败: {e}")))?;
    matcher = matcher.from(root);
    apply_selector(matcher, sel)
}

fn build_matcher_for_probe(
    map: &BTreeMap<String, Value>,
    sel: &ElementSelector,
    timeout_ms: u64,
) -> Result<uiautomation::UIMatcher, ActionError> {
    let auto = uiautomation::UIAutomation::new()
        .map_err(|e| ActionError::execution(format!("UIAutomation 初始化失败: {e}")))?;
    let mut matcher = auto
        .create_matcher()
        .timeout(timeout_ms)
        .depth(sel.depth);
    let hwnd = resolve_probe_scope_hwnd(map)?;
    let handle = uiautomation::types::Handle::from(hwnd.0 as isize);
    let root = auto
        .element_from_handle(handle)
        .map_err(|e| ActionError::execution(format!("ElementFromHandle 失败: {e}")))?;
    matcher = matcher.from(root);
    apply_selector(matcher, sel)
}

fn find_with_chain_probe(
    map: &BTreeMap<String, Value>,
    chain: &[ElementSelector],
    timeout_ms: u64,
) -> Result<uiautomation::UIElement, ActionError> {
    let mut last_err = String::new();
    for sel in chain {
        match build_matcher_for_probe(map, sel, timeout_ms) {
            Ok(matcher) => match matcher.find_first() {
                Ok(el) => return Ok(el),
                Err(e) => last_err = e.to_string(),
            },
            Err(e) => last_err = e.to_string(),
        }
    }
    let hint = chain.first().map(|s| s.hint()).unwrap_or_else(|| "selector".into());
    Err(ActionError::ui_with_hint(
        "ui_selector_not_found",
        &hint,
        format!("未找到元素: {last_err}"),
    ))
}

fn find_with_chain(
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
    chain: &[ElementSelector],
    timeout_ms: u64,
) -> Result<uiautomation::UIElement, ActionError> {
    let mut last_err = String::new();
    for sel in chain {
        match build_matcher_for(map, ctx, sel, timeout_ms) {
            Ok(matcher) => match matcher.find_first() {
                Ok(el) => return Ok(el),
                Err(e) => last_err = e.to_string(),
            },
            Err(e) => last_err = e.to_string(),
        }
    }
    let hint = chain.first().map(|s| s.hint()).unwrap_or_else(|| "selector".into());
    Err(ActionError::ui_with_hint(
        "ui_selector_not_found",
        &hint,
        format!("未找到元素: {last_err}"),
    ))
}

fn element_present(
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
    chain: &[ElementSelector],
    probe_ms: u64,
) -> bool {
    find_with_chain(map, ctx, chain, probe_ms).is_ok()
}

fn element_enabled(el: &uiautomation::UIElement) -> bool {
    el.is_enabled().unwrap_or(false) && !el.is_offscreen().unwrap_or(true)
}

fn wait_element_state(
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
    chain: &[ElementSelector],
    state: WaitState,
    timeout_ms: u64,
    poll_ms: u64,
) -> Result<uiautomation::UIElement, ActionError> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let hint = chain.first().map(|s| s.hint()).unwrap_or_else(|| "selector".into());
    loop {
        match state {
            WaitState::Present | WaitState::Enabled => {
                if let Ok(el) = find_with_chain(map, ctx, chain, poll_ms.min(500)) {
                    if state == WaitState::Present || element_enabled(&el) {
                        return Ok(el);
                    }
                }
            }
            WaitState::Absent => unreachable!("absent handled in ui_element_wait_impl"),
        }
        if Instant::now() >= deadline {
            return Err(ActionError::ui_with_hint(
                "ui_sync_timeout",
                &hint,
                "等待元素超时",
            ));
        }
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
}

pub async fn ui_element_list_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let depth = opt_i64(&map, "depth", 3).clamp(1, 10) as u32;
    let limit = opt_i64(&map, "limit", 50).clamp(1, 500) as usize;
    tokio::task::spawn_blocking(move || {
        let auto = uiautomation::UIAutomation::new()
            .map_err(|e| ActionError::execution(format!("UIAutomation 初始化失败: {e}")))?;
        let hwnd = resolve_scope_hwnd(&map, &exec_ctx)?;
        let handle = uiautomation::types::Handle::from(hwnd.0 as isize);
        let root = auto
            .element_from_handle(handle)
            .map_err(|e| ActionError::execution(format!("ElementFromHandle 失败: {e}")))?;
        let matcher = auto
            .create_matcher()
            .from(root)
            .depth(depth)
            .timeout(500);
        let found = matcher.find_all().unwrap_or_default();
        let list: Vec<Value> = found
            .iter()
            .take(limit)
            .map(|el| Value::Map(elem_to_map(el)))
            .collect();
        let mut out = BTreeMap::new();
        out.insert("elements".into(), Value::List(list));
        Ok(Value::Map(out))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.list 失败: {e}")))?
}

pub async fn ui_element_list_probe_impl(
    params: BTreeMap<String, Value>,
) -> Result<Value, ActionError> {
    let map = params;
    let depth = opt_i64(&map, "depth", 3).clamp(1, 10) as u32;
    let limit = opt_i64(&map, "limit", 50).clamp(1, 500) as usize;
    tokio::task::spawn_blocking(move || {
        let auto = uiautomation::UIAutomation::new()
            .map_err(|e| ActionError::execution(format!("UIAutomation 初始化失败: {e}")))?;
        let hwnd = resolve_probe_scope_hwnd(&map)?;
        let handle = uiautomation::types::Handle::from(hwnd.0 as isize);
        let root = auto
            .element_from_handle(handle)
            .map_err(|e| ActionError::execution(format!("ElementFromHandle 失败: {e}")))?;
        let matcher = auto
            .create_matcher()
            .from(root)
            .depth(depth)
            .timeout(500);
        let found = matcher.find_all().unwrap_or_default();
        let list: Vec<Value> = found
            .iter()
            .take(limit)
            .map(|el| Value::Map(elem_to_map(el)))
            .collect();
        let mut out = BTreeMap::new();
        out.insert("elements".into(), Value::List(list));
        Ok(Value::Map(out))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.list 失败: {e}")))?
}

pub async fn ui_element_find_probe_impl(
    params: BTreeMap<String, Value>,
    max_chain: usize,
) -> Result<Value, ActionError> {
    let map = params;
    let timeout_ms = opt_i64(&map, "timeout_ms", 3000).max(0) as u64;
    let chain = selector_chain_from_params(&map, max_chain)?;
    tokio::task::spawn_blocking(move || {
        let el = find_with_chain_probe(&map, &chain, timeout_ms)?;
        Ok(Value::Map(element_map_with_selectors(&el)))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.find 失败: {e}")))?
}

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

/// Resolve the UIA root for desktop ListItem icons (Progman or WorkerW + DefView).
fn find_desktop_hwnd() -> Option<HWND> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, SendMessageTimeoutW, SMTO_NORMAL,
    };

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
        // Win10/11: spawn WorkerW sibling that hosts SHELLDLL_DefView.
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
        let hwnd = find_desktop_hwnd().ok_or_else(|| {
            ActionError::ui("ui_desktop_not_found", "未找到桌面 Shell 窗口")
        })?;
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
        out.insert("icons".into(), Value::List(icons));
        Ok(Value::Map(out))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.window.desktop 失败: {e}")))?
}

pub async fn ui_element_find_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let timeout_ms = opt_i64(&map, "timeout_ms", 3000).max(0) as u64;
    let chain = selector_chain_from_params(&map, exec_ctx.ui_max_selector_chain())?;
    tokio::task::spawn_blocking(move || {
        let el = find_with_chain(&map, &exec_ctx, &chain, timeout_ms)?;
        Ok(Value::Map(element_map_with_selectors(&el)))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.find 失败: {e}")))?
}

pub async fn ui_element_exists_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let timeout_ms = opt_i64(&map, "timeout_ms", 2000).max(0) as u64;
    let chain = selector_chain_from_params(&map, exec_ctx.ui_max_selector_chain())?;
    tokio::task::spawn_blocking(move || {
        let found = find_with_chain(&map, &exec_ctx, &chain, timeout_ms);
        let mut out = BTreeMap::new();
        match found {
            Ok(el) => {
                out.insert("found".into(), Value::Bool(true));
                out.insert("element".into(), Value::Map(element_map_with_selectors(&el)));
            }
            Err(_) => {
                out.insert("found".into(), Value::Bool(false));
            }
        }
        Ok(Value::Map(out))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.exists 失败: {e}")))?
}

pub async fn ui_element_click_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let timeout_ms = opt_i64(&map, "timeout_ms", 3000).max(0) as u64;
    let safe = opt_bool(&map, "safe", true);
    let chain = selector_chain_from_params(&map, exec_ctx.ui_max_selector_chain())?;
    tokio::task::spawn_blocking(move || {
        let el = if safe {
            wait_element_state(
                &map,
                &exec_ctx,
                &chain,
                WaitState::Enabled,
                timeout_ms.min(3000).max(500),
                200,
            )?
        } else {
            find_with_chain(&map, &exec_ctx, &chain, timeout_ms)?
        };
        if safe && !element_enabled(&el) {
            let hint = chain.first().map(|s| s.hint()).unwrap_or_else(|| "selector".into());
            return Err(ActionError::ui_with_hint("ui_not_clickable", &hint, "元素不可点击"));
        }
        el.click()
            .map_err(|e| ActionError::execution(format!("元素点击失败: {e}")))?;
        Ok(Value::Map(elem_to_map(&el)))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.click 失败: {e}")))?
}

pub async fn ui_element_wait_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let timeout_ms = map
        .get("timeout_ms")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("timeout_ms".into()))?
        .max(1) as u64;
    let state = wait_state_from_params(&map)?;
    let poll = poll_interval_ms(&map, 200);
    let chain = selector_chain_from_params(&map, exec_ctx.ui_max_selector_chain())?;
    tokio::task::spawn_blocking(move || {
        if state == WaitState::Absent {
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            let hint = chain.first().map(|s| s.hint()).unwrap_or_else(|| "selector".into());
            loop {
                if !element_present(&map, &exec_ctx, &chain, poll.min(500)) {
                    let mut m = BTreeMap::new();
                    m.insert("absent".into(), Value::Bool(true));
                    m.insert("selector_hint".into(), Value::Str(hint));
                    return Ok(Value::Map(m));
                }
                if Instant::now() >= deadline {
                    return Err(ActionError::ui_with_hint(
                        "ui_login_pending",
                        &hint,
                        "等待元素消失超时",
                    ));
                }
                std::thread::sleep(Duration::from_millis(poll));
            }
        }
        let el = wait_element_state(&map, &exec_ctx, &chain, state, timeout_ms, poll)?;
        Ok(Value::Map(elem_to_map(&el)))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.wait 失败: {e}")))?
}

pub async fn ui_wait_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let ms = map
        .get("ms")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("ms".into()))?
        .max(0) as u64;
    ctx.add_ui_settle_ms(ms)
        .map_err(ActionError::execution)?;
    tokio::time::sleep(Duration::from_millis(ms)).await;
    Ok(Value::Bool(true))
}

fn mouse_button_flags(button: &str) -> Result<(u32, u32), ActionError> {
    match button.trim().to_ascii_lowercase().as_str() {
        "left" | "" => Ok((MOUSEEVENTF_LEFTDOWN.0, MOUSEEVENTF_LEFTUP.0)),
        "right" => Ok((MOUSEEVENTF_RIGHTDOWN.0, MOUSEEVENTF_RIGHTUP.0)),
        "middle" => Ok((MOUSEEVENTF_MIDDLEDOWN.0, MOUSEEVENTF_MIDDLEUP.0)),
        other => Err(ActionError::InvalidParams(format!(
            "不支持的 button: {other}（left|right|middle）"
        ))),
    }
}

fn mouse_click_at(x: i32, y: i32, button: &str, clicks: i64) -> Result<(), ActionError> {
    let (down_f, up_f) = mouse_button_flags(button)?;
    let clicks = clicks.clamp(1, 10) as usize;
    unsafe {
        SetCursorPos(x, y).map_err(|e| ActionError::execution(format!("SetCursorPos 失败: {e}")))?;
    }
    for _ in 0..clicks {
        let down = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(down_f),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(up_f),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let _ = SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
        }
    }
    Ok(())
}

pub async fn ui_click_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let x = map
        .get("x")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("x".into()))? as i32;
    let y = map
        .get("y")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("y".into()))? as i32;
    let button = opt_str_map(map, "button").unwrap_or_else(|| "left".into());
    let clicks = opt_i64(map, "clicks", 1);
    tokio::task::spawn_blocking(move || {
        mouse_click_at(x, y, &button, clicks)?;
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.click 失败: {e}")))?
}

fn opt_str_map(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str().map(|s| s.to_string()))
}

pub async fn ui_scroll_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let dy = opt_i64(map, "dy", 0);
    let dx = opt_i64(map, "dx", 0);
    if dy == 0 && dx == 0 {
        return Err(ActionError::InvalidParams("需要 dy 或 dx".into()));
    }
    let x = map.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
    let y = map.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);
    tokio::task::spawn_blocking(move || {
        if let (Some(x), Some(y)) = (x, y) {
            unsafe {
                SetCursorPos(x, y)
                    .map_err(|e| ActionError::execution(format!("SetCursorPos 失败: {e}")))?;
            }
        }
        let mut inputs = Vec::new();
        if dy != 0 {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (dy as i16 as u16) as u32,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        if dx != 0 {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (dx as i16 as u16) as u32,
                        dwFlags: MOUSEEVENTF_HWHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        unsafe {
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.scroll 失败: {e}")))?
}

pub async fn ui_drag_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let from_x = map
        .get("from_x")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("from_x".into()))? as i32;
    let from_y = map
        .get("from_y")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("from_y".into()))? as i32;
    let to_x = map
        .get("to_x")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("to_x".into()))? as i32;
    let to_y = map
        .get("to_y")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("to_y".into()))? as i32;
    let steps = opt_i64(map, "steps", 12).clamp(1, 100) as i32;
    let button = opt_str_map(map, "button").unwrap_or_else(|| "left".into());
    let (down_f, up_f) = mouse_button_flags(&button)?;
    tokio::task::spawn_blocking(move || {
        unsafe {
            SetCursorPos(from_x, from_y)
                .map_err(|e| ActionError::execution(format!("SetCursorPos 失败: {e}")))?;
        }
        let down = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(down_f),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let _ = SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
        }
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let x = from_x as f64 + (to_x - from_x) as f64 * t;
            let y = from_y as f64 + (to_y - from_y) as f64 * t;
            unsafe {
                let _ = SetCursorPos(x as i32, y as i32);
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        let up = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(up_f),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            let _ = SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
        }
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.drag 失败: {e}")))?
}

pub async fn ui_element_get_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let timeout_ms = opt_i64(&map, "timeout_ms", 3000).max(0) as u64;
    let chain = selector_chain_from_params(&map, exec_ctx.ui_max_selector_chain())?;
    tokio::task::spawn_blocking(move || {
        let el = find_with_chain(&map, &exec_ctx, &chain, timeout_ms)?;
        let value = element_value_text(&el);
        let mut out = BTreeMap::new();
        out.insert("value".into(), Value::Str(value));
        out.insert("element".into(), Value::Map(element_map_with_selectors(&el)));
        Ok(Value::Map(out))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.get 失败: {e}")))?
}

pub async fn ui_element_set_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let value = map
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionError::MissingParam("value".into()))?
        .to_string();
    let exec_ctx = ctx.clone();
    let timeout_ms = opt_i64(&map, "timeout_ms", 3000).max(0) as u64;
    let chain = selector_chain_from_params(&map, exec_ctx.ui_max_selector_chain())?;
    tokio::task::spawn_blocking(move || {
        let el = find_with_chain(&map, &exec_ctx, &chain, timeout_ms)?;
        set_element_value(&el, &value)?;
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.set 失败: {e}")))?
}

fn element_value_text(el: &uiautomation::UIElement) -> String {
    if let Ok(pattern) = el.get_pattern::<uiautomation::patterns::UIValuePattern>() {
        if let Ok(v) = pattern.get_value() {
            return v;
        }
    }
    el.get_name().unwrap_or_default()
}

fn set_element_value(el: &uiautomation::UIElement, value: &str) -> Result<(), ActionError> {
    let pattern = el
        .get_pattern::<uiautomation::patterns::UIValuePattern>()
        .map_err(|e| ActionError::execution(format!("无 ValuePattern: {e}")))?;
    pattern
        .set_value(value)
        .map_err(|e| ActionError::execution(format!("设置 Value 失败: {e}")))?;
    Ok(())
}

fn send_unicode_char(ch: char) {
    let code = ch as u16;
    let down = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: code,
                dwFlags: KEYEVENTF_UNICODE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: code,
                dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        let _ = SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
    }
}

fn send_vk(vk: u16, key_up: bool) {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

fn vk_from_token(tok: &str) -> Option<u16> {
    match tok.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(0x11),
        "alt" => Some(0x12),
        "shift" => Some(0x10),
        "win" | "meta" => Some(0x5B),
        "enter" | "return" => Some(0x0D),
        "tab" => Some(0x09),
        "esc" | "escape" => Some(0x1B),
        "space" => Some(0x20),
        "backspace" => Some(0x08),
        "delete" | "del" => Some(0x2E),
        "up" => Some(0x26),
        "down" => Some(0x28),
        "left" => Some(0x25),
        "right" => Some(0x27),
        "home" => Some(0x24),
        "end" => Some(0x23),
        "pageup" | "pgup" => Some(0x21),
        "pagedown" | "pgdn" => Some(0x22),
        "insert" | "ins" => Some(0x2D),
        "f1" => Some(0x70),
        "f2" => Some(0x71),
        "f3" => Some(0x72),
        "f4" => Some(0x73),
        "f5" => Some(0x74),
        s if s.len() == 1 => {
            let c = s.chars().next()?.to_ascii_uppercase();
            if c.is_ascii_alphanumeric() {
                Some(c as u16)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub async fn ui_type_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let text = require_str(map, "text")?;
    tokio::task::spawn_blocking(move || {
        for ch in text.chars() {
            send_unicode_char(ch);
        }
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.type 失败: {e}")))?
}

pub async fn ui_key_impl(params: Value) -> Result<Value, ActionError> {
    let map = require_map(&params)?;
    let keys = require_str(map, "keys")?;
    tokio::task::spawn_blocking(move || {
        let parts: Vec<&str> = keys.split('+').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Err(ActionError::InvalidParams("keys 为空".into()));
        }
        let mut vks = Vec::new();
        for p in &parts {
            let vk = vk_from_token(p).ok_or_else(|| {
                ActionError::InvalidParams(format!("不支持的 keys 片段: {p}"))
            })?;
            vks.push(vk);
        }
        for vk in &vks {
            send_vk(*vk, false);
        }
        for vk in vks.iter().rev() {
            send_vk(*vk, true);
        }
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.key 失败: {e}")))?
}
