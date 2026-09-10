use super::*;

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

pub(super) fn window_class(hwnd: HWND) -> String {
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
    if let Some(needle) = &query.title_contains
        && !lower.contains(&needle.to_lowercase())
    {
        return false;
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
        let (query, windows) = unsafe { &mut *ctx };
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
        if let Some(want_class) = &query.class_name
            && window_class(hwnd) != *want_class
        {
            return BOOL(1);
        }
        windows.push(hwnd);
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

pub(super) fn resolve_scope_hwnd(
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
) -> Result<HWND, ActionError> {
    let mut q = window_query_from_params(map, ctx)?;
    if q.title_contains.is_none() && map.get("name").and_then(|v| v.as_str()).is_some() {
        q.title_contains = map
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
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

/// 仅供探测的范围：参数里显式给 `--hwnd` / `--title`；不回退到会话范围。
pub(super) fn resolve_probe_scope_hwnd(map: &BTreeMap<String, Value>) -> Result<HWND, ActionError> {
    use crate::builtin::ui::kernel::probe_scope_explicit;
    probe_scope_explicit(map)?;
    let q = WindowQuery {
        hwnd: map.get("hwnd").and_then(|v| v.as_i64()),
        title_contains: map
            .get("title_contains")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        class_name: map
            .get("class_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
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
                q.title_contains
                    .unwrap_or_else(|| format!("hwnd={:?}", q.hwnd))
            ),
        )
    })
}

pub async fn ui_windows_impl() -> Result<Value, ActionError> {
    tokio::task::spawn_blocking(|| {
        let mut windows: Vec<Value> = Vec::new();
        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let windows = lparam.0 as *mut Vec<Value>;
            if windows.is_null() {
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
                (*windows).push(Value::Map(m));
            }
            BOOL(1)
        }
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut windows as *mut _ as isize));
        }
        let mut out = BTreeMap::new();
        out.insert("windows".into(), Value::Array(windows));
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
    let out: BTreeMap<String, Value> =
        tokio::task::spawn_blocking(move || -> Result<BTreeMap<String, Value>, ActionError> {
            let hwnd = resolve_scope_hwnd(&map, &exec_ctx)?;
            unsafe {
                if !SetForegroundWindow(hwnd).as_bool() {
                    return Err(ActionError::execution("SetForegroundWindow 失败"));
                }
            }
            Ok(hwnd_map(hwnd))
        })
        .await
        .map_err(|e| ActionError::execution(format!("ui.window.focus 失败: {e}")))??;
    if let Some(id) = out.get("hwnd").and_then(|v| v.as_i64()) {
        let title = out
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
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
    let out: BTreeMap<String, Value> =
        tokio::task::spawn_blocking(move || -> Result<BTreeMap<String, Value>, ActionError> {
            let hwnd = resolve_scope_hwnd(&map, &exec_ctx)?;
            Ok(hwnd_map(hwnd))
        })
        .await
        .map_err(|e| ActionError::execution(format!("ui.window.find 失败: {e}")))??;
    if let Some(id) = out.get("hwnd").and_then(|v| v.as_i64()) {
        let title = out
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
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
    let out: BTreeMap<String, Value> =
        tokio::task::spawn_blocking(move || -> Result<BTreeMap<String, Value>, ActionError> {
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
        })
        .await
        .map_err(|e| ActionError::execution(format!("ui.window.wait 失败: {e}")))??;
    if let Some(id) = out.get("hwnd").and_then(|v| v.as_i64()) {
        let title = out
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        ctx.set_ui_scope(id, title);
    }
    Ok(Value::Map(out))
}
