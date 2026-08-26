//! Shared UI automation selector / sync primitives (platform-agnostic).

use corex_core::{ActionError, ExecutionContext, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitState {
    Present,
    Absent,
    Enabled,
}

impl WaitState {
    pub fn parse(s: &str) -> Result<Self, ActionError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "present" | "" => Ok(WaitState::Present),
            "absent" => Ok(WaitState::Absent),
            "enabled" => Ok(WaitState::Enabled),
            other => Err(ActionError::InvalidParams(format!(
                "未知 state: {other}（present|absent|enabled）"
            ))),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WindowQuery {
    pub title_contains: Option<String>,
    pub title_excludes: Vec<String>,
    pub class_name: Option<String>,
    pub visible_only: bool,
    pub prefer_largest: bool,
    pub hwnd: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ElementSelector {
    pub name: Option<String>,
    pub name_contains: Option<String>,
    pub automation_id: Option<String>,
    pub control_type: Option<String>,
    pub depth: u32,
}

impl ElementSelector {
    pub fn from_map(map: &BTreeMap<String, Value>) -> Result<Self, ActionError> {
        Ok(Self {
            name: opt_str(map, "name"),
            name_contains: opt_str(map, "name_contains"),
            automation_id: opt_str(map, "automation_id"),
            control_type: opt_str(map, "control_type"),
            depth: map
                .get("depth")
                .and_then(|v| v.as_i64())
                .unwrap_or(12)
                .clamp(1, 20) as u32,
        })
    }

    pub fn hint(&self) -> String {
        if let Some(aid) = &self.automation_id {
            return format!("automation_id={aid}");
        }
        if let Some(n) = &self.name {
            return format!("name={n}");
        }
        if let Some(n) = &self.name_contains {
            return format!("name_contains={n}");
        }
        if let Some(ct) = &self.control_type {
            return format!("control_type={ct}");
        }
        "selector".into()
    }
}

/// Parse window query + optional explicit hwnd from params; falls back to ui_session scope.
pub fn window_query_from_params(
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
) -> Result<WindowQuery, ActionError> {
    let mut q = WindowQuery {
        title_contains: opt_str(map, "title_contains"),
        class_name: opt_str(map, "class_name"),
        visible_only: map
            .get("visible_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        prefer_largest: map
            .get("prefer_largest")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        ..Default::default()
    };
    if let Some(list) = map.get("title_excludes").and_then(|v| v.as_list()) {
        q.title_excludes = list
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    q.hwnd = map.get("hwnd").and_then(|v| v.as_i64());
    if q.hwnd.is_none() && !q.prefer_largest {
        q.hwnd = ctx.ui_session.scope_hwnd;
    }
    if q.title_contains.is_none() && q.hwnd.is_none() {
        if let Some(title) = &ctx.ui_session.scope_title {
            q.title_contains = Some(title.clone());
        }
    }
    Ok(q)
}

/// Flat params or `selectors: [{...}, ...]` fallback chain (length capped by runtime).
pub fn selector_chain_from_params(
    map: &BTreeMap<String, Value>,
    max_chain: usize,
) -> Result<Vec<ElementSelector>, ActionError> {
    if let Some(list) = map.get("selectors").and_then(|v| v.as_list()) {
        if list.is_empty() {
            return Err(ActionError::InvalidParams("selectors 不能为空".into()));
        }
        if list.len() > max_chain {
            return Err(ActionError::InvalidParams(format!(
                "selectors 最多 {max_chain} 条（可在 [runtime] 调整 ui_max_selector_chain / ui_profile）"
            )));
        }
        let mut out = Vec::with_capacity(list.len());
        for item in list {
            let m = item.as_map().ok_or_else(|| {
                ActionError::InvalidParams("selectors[] 每项必须为 map".into())
            })?;
            out.push(ElementSelector::from_map(m)?);
        }
        return Ok(out);
    }
    let sel = ElementSelector::from_map(map)?;
    if sel.name.is_none()
        && sel.name_contains.is_none()
        && sel.automation_id.is_none()
        && sel.control_type.is_none()
    {
        return Err(ActionError::MissingParam(
            "name|name_contains|automation_id|control_type|selectors".into(),
        ));
    }
    Ok(vec![sel])
}

pub fn wait_state_from_params(map: &BTreeMap<String, Value>) -> Result<WaitState, ActionError> {
    match map.get("state").and_then(|v| v.as_str()) {
        None => Ok(WaitState::Present),
        Some(s) => WaitState::parse(s),
    }
}

pub fn poll_interval_ms(map: &BTreeMap<String, Value>, default: u64) -> u64 {
    map.get("poll_interval_ms")
        .and_then(|v| v.as_i64())
        .unwrap_or(default as i64)
        .max(50) as u64
}

pub fn opt_bool(map: &BTreeMap<String, Value>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

fn opt_str(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Suggested selector fallback chain (AutomationId → name+type → name → control_type).
pub fn suggest_selectors(
    automation_id: Option<&str>,
    name: Option<&str>,
    _class: Option<&str>,
    control_type: Option<&str>,
) -> Vec<ElementSelector> {
    let mut out = Vec::new();
    let ct = control_type.map(|s| s.to_string());
    if let Some(aid) = automation_id.filter(|s| !s.is_empty()) {
        out.push(ElementSelector {
            automation_id: Some(aid.to_string()),
            control_type: ct.clone(),
            depth: 12,
            ..Default::default()
        });
    }
    if let Some(n) = name.filter(|s| !s.is_empty()) {
        if let Some(ref ct_val) = ct {
            out.push(ElementSelector {
                name: Some(n.to_string()),
                control_type: Some(ct_val.clone()),
                depth: 12,
                ..Default::default()
            });
        }
        out.push(ElementSelector {
            name: Some(n.to_string()),
            depth: 12,
            ..Default::default()
        });
    }
    if out.is_empty() {
        if let Some(ref ct_val) = ct {
            out.push(ElementSelector {
                control_type: Some(ct_val.clone()),
                depth: 12,
                ..Default::default()
            });
        }
    }
    out
}

/// YAML snippet for directive `selectors:` block.
pub fn selector_chain_to_yaml(chain: &[ElementSelector]) -> String {
    if chain.is_empty() {
        return "selectors: []".into();
    }
    let mut lines = vec!["selectors:".into()];
    for sel in chain {
        lines.push("  -".into());
        if let Some(aid) = &sel.automation_id {
            lines.push(format!("      automation_id: {aid:?}"));
        }
        if let Some(n) = &sel.name {
            lines.push(format!("      name: {n:?}"));
        }
        if let Some(n) = &sel.name_contains {
            lines.push(format!("      name_contains: {n:?}"));
        }
        if let Some(ct) = &sel.control_type {
            lines.push(format!("      control_type: {ct:?}"));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod suggest_tests {
    use super::*;

    #[test]
    fn suggest_prefers_automation_id() {
        let chain = suggest_selectors(Some("btnOk"), Some("OK"), None, Some("button"));
        assert!(!chain.is_empty());
        assert_eq!(chain[0].automation_id.as_deref(), Some("btnOk"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::MAX_SELECTOR_CHAIN;

    #[test]
    fn selector_chain_from_flat_params() {
        let mut m = BTreeMap::new();
        m.insert("name".into(), Value::Str("进入微信".into()));
        m.insert("control_type".into(), Value::Str("Button".into()));
        let chain = selector_chain_from_params(&m, MAX_SELECTOR_CHAIN).unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].name.as_deref(), Some("进入微信"));
    }

    #[test]
    fn selector_chain_rejects_empty() {
        let mut m = BTreeMap::new();
        m.insert("selectors".into(), Value::List(vec![]));
        assert!(selector_chain_from_params(&m, MAX_SELECTOR_CHAIN).is_err());
    }

    #[test]
    fn selector_chain_respects_runtime_cap() {
        let mut m = BTreeMap::new();
        m.insert(
            "selectors".into(),
            Value::List(vec![
                Value::Map(BTreeMap::from([("name".into(), Value::Str("a".into()))])),
                Value::Map(BTreeMap::from([("name".into(), Value::Str("b".into()))])),
                Value::Map(BTreeMap::from([("name".into(), Value::Str("c".into()))])),
            ]),
        );
        assert!(selector_chain_from_params(&m, 2).is_err());
        assert_eq!(selector_chain_from_params(&m, 3).unwrap().len(), 3);
    }

    #[test]
    fn prefer_largest_ignores_session_hwnd() {
        use corex_core::ExecutionContext;
        let mut ctx = ExecutionContext::default();
        ctx.set_ui_scope(12345, Some("微信".into()));
        let mut m = BTreeMap::new();
        m.insert("prefer_largest".into(), Value::Bool(true));
        let q = window_query_from_params(&m, &ctx).unwrap();
        assert!(q.hwnd.is_none());
        assert_eq!(q.title_contains.as_deref(), Some("微信"));
        assert!(q.prefer_largest);
    }
}
