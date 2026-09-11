use super::window::{resolve_probe_scope_hwnd, resolve_scope_hwnd};
use super::*;

fn ancestor_map(el: &uiautomation::UIElement) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    if let Ok(n) = el.get_name()
        && !n.is_empty()
    {
        m.insert("name".into(), Value::Str(n));
    }
    if let Ok(aid) = el.get_automation_id()
        && !aid.is_empty()
    {
        m.insert("automation_id".into(), Value::Str(aid));
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

fn format_control_type(el: &uiautomation::UIElement) -> Option<String> {
    if let Ok(lct) = el.get_localized_control_type() {
        let s = lct.trim();
        if !s.is_empty() {
            return Some(s.to_ascii_lowercase());
        }
    }
    el.get_control_type().ok().map(control_type_enum_name)
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
    if let Ok(n) = el.get_name()
        && !n.is_empty()
    {
        m.insert("name".into(), Value::Str(n));
    }
    if let Ok(aid) = el.get_automation_id()
        && !aid.is_empty()
    {
        m.insert("automation_id".into(), Value::Str(aid));
    }
    if let Some(ct) = format_control_type(el) {
        m.insert("control_type".into(), Value::Str(ct));
    }
    if let Ok(cn) = el.get_classname()
        && !cn.is_empty()
    {
        m.insert("class".into(), Value::Str(cn));
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
        let ancestors: Vec<Value> = collect_ancestors(el).into_iter().map(Value::Map).collect();
        if !ancestors.is_empty() {
            m.insert("ancestors".into(), Value::Array(ancestors));
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

/// 沿祖先上溯，直到原生 HWND 与 `scope_hwnd` 相同。
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

pub(crate) fn element_map_with_selectors(el: &uiautomation::UIElement) -> BTreeMap<String, Value> {
    use crate::builtin::ui::kernel::{selector_chain_to_yaml, suggest_selectors};
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
    m.insert("selectors".into(), Value::Array(selectors));
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
    let mut matcher = auto.create_matcher().timeout(timeout_ms).depth(sel.depth);
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
    let mut matcher = auto.create_matcher().timeout(timeout_ms).depth(sel.depth);
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
    let hint = chain
        .first()
        .map(|s| s.hint())
        .unwrap_or_else(|| "selector".into());
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
    let hint = chain
        .first()
        .map(|s| s.hint())
        .unwrap_or_else(|| "selector".into());
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
    let hint = chain
        .first()
        .map(|s| s.hint())
        .unwrap_or_else(|| "selector".into());
    loop {
        match state {
            WaitState::Present | WaitState::Enabled => {
                if let Ok(el) = find_with_chain(map, ctx, chain, poll_ms.min(500))
                    && (state == WaitState::Present || element_enabled(&el))
                {
                    return Ok(el);
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

pub async fn ui_elements_impl(
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
        let matcher = auto.create_matcher().from(root).depth(depth).timeout(500);
        let found = matcher.find_all().unwrap_or_default();
        let items: Vec<Value> = found
            .iter()
            .take(limit)
            .map(|el| Value::Map(elem_to_map(el)))
            .collect();
        let mut out = BTreeMap::new();
        out.insert("elements".into(), Value::Array(items));
        Ok(Value::Map(out))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.list 失败: {e}")))?
}

pub async fn ui_elements_probe_impl(params: BTreeMap<String, Value>) -> Result<Value, ActionError> {
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
        let matcher = auto.create_matcher().from(root).depth(depth).timeout(500);
        let found = matcher.find_all().unwrap_or_default();
        let items: Vec<Value> = found
            .iter()
            .take(limit)
            .map(|el| Value::Map(elem_to_map(el)))
            .collect();
        let mut out = BTreeMap::new();
        out.insert("elements".into(), Value::Array(items));
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

pub async fn ui_element_find_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let timeout_ms = opt_i64(&map, "timeout_ms", 3000).max(0) as u64;
    let chain = selector_chain_from_params(&map, exec_ctx.ui_depth())?;
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
    let chain = selector_chain_from_params(&map, exec_ctx.ui_depth())?;
    tokio::task::spawn_blocking(move || {
        let found = find_with_chain(&map, &exec_ctx, &chain, timeout_ms);
        let mut out = BTreeMap::new();
        match found {
            Ok(el) => {
                out.insert("found".into(), Value::Bool(true));
                out.insert(
                    "element".into(),
                    Value::Map(element_map_with_selectors(&el)),
                );
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
    let chain = selector_chain_from_params(&map, exec_ctx.ui_depth())?;
    tokio::task::spawn_blocking(move || {
        let el = if safe {
            wait_element_state(
                &map,
                &exec_ctx,
                &chain,
                WaitState::Enabled,
                timeout_ms.clamp(500, 3000),
                200,
            )?
        } else {
            find_with_chain(&map, &exec_ctx, &chain, timeout_ms)?
        };
        if safe && !element_enabled(&el) {
            let hint = chain
                .first()
                .map(|s| s.hint())
                .unwrap_or_else(|| "selector".into());
            return Err(ActionError::ui_with_hint(
                "ui_not_clickable",
                &hint,
                "元素不可点击",
            ));
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
    let chain = selector_chain_from_params(&map, exec_ctx.ui_depth())?;
    tokio::task::spawn_blocking(move || {
        if state == WaitState::Absent {
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            let hint = chain
                .first()
                .map(|s| s.hint())
                .unwrap_or_else(|| "selector".into());
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

pub async fn ui_element_get_impl(
    params: Value,
    ctx: &mut ExecutionContext,
) -> Result<Value, ActionError> {
    let map = require_map(&params)?.clone();
    let exec_ctx = ctx.clone();
    let timeout_ms = opt_i64(&map, "timeout_ms", 3000).max(0) as u64;
    let chain = selector_chain_from_params(&map, exec_ctx.ui_depth())?;
    tokio::task::spawn_blocking(move || {
        let el = find_with_chain(&map, &exec_ctx, &chain, timeout_ms)?;
        let value = element_value_text(&el);
        let mut out = BTreeMap::new();
        out.insert("value".into(), Value::Str(value));
        out.insert(
            "element".into(),
            Value::Map(element_map_with_selectors(&el)),
        );
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
    let chain = selector_chain_from_params(&map, exec_ctx.ui_depth())?;
    tokio::task::spawn_blocking(move || {
        let el = find_with_chain(&map, &exec_ctx, &chain, timeout_ms)?;
        set_element_value(&el, &value)?;
        Ok(Value::Bool(true))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.set 失败: {e}")))?
}

fn element_value_text(el: &uiautomation::UIElement) -> String {
    if let Ok(pattern) = el.get_pattern::<uiautomation::patterns::UIValuePattern>()
        && let Ok(v) = pattern.get_value()
    {
        return v;
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
