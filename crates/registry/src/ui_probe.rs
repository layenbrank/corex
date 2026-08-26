//! Interactive UI probe API for `corex ui` CLI (Windows UIAutomation).

use corex_core::{ActionError, ExecutionContext, PluginConfig, RuntimeConfig, Value};
use crate::builtin::ui_kernel::{probe_action_denied, probe_scope_explicit};
#[cfg(windows)]
use crate::builtin::ui_kernel::elements_flat_to_tree;
use std::collections::BTreeMap;

#[cfg(not(windows))]
fn unavailable() -> ActionError {
    ActionError::execution("ui probe 在当前平台不可用（需要 Windows）")
}

/// Returns error if action is listed in `[plugins].disabled_actions`.
pub fn check_probe_allowed(plugins: &PluginConfig, action_id: &str) -> Result<(), ActionError> {
    if probe_action_denied(plugins, action_id) {
        return Err(ActionError::execution(format!(
            "probe_denied: 动作 {action_id} 已被 disabled_actions 禁用"
        )));
    }
    Ok(())
}

/// List visible top-level windows.
pub async fn probe_windows() -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        crate::builtin::ui::win::ui_window_list_impl().await
    }
    #[cfg(not(windows))]
    {
        Err(unavailable())
    }
}

/// Desktop shell icons (ListItem on Progman).
pub async fn probe_desktop_icons() -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        crate::builtin::ui::win::ui_desktop_icons_impl().await
    }
    #[cfg(not(windows))]
    {
        Err(unavailable())
    }
}

/// List UIA elements under a scope window (requires explicit `--hwnd` or `--title`).
pub async fn probe_element_tree(
    ctx: &ExecutionContext,
    params: BTreeMap<String, Value>,
    format: TreeFormat,
) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        probe_scope_explicit(&params)?;
        let v = crate::builtin::ui::win::ui_element_list_probe_impl(params).await?;
        match format {
            TreeFormat::Flat => Ok(v),
            TreeFormat::Tree => {
                if let Value::Map(mut m) = v {
                    if let Some(Value::List(list)) = m.remove("elements") {
                        let maps: Vec<BTreeMap<String, Value>> = list
                            .into_iter()
                            .filter_map(|v| v.as_map().cloned())
                            .collect();
                        let mut out = BTreeMap::new();
                        out.insert("tree".into(), elements_flat_to_tree(&maps));
                        return Ok(Value::Map(out));
                    }
                }
                Ok(v)
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (ctx, params, format);
        Err(unavailable())
    }
}

/// Find element by selector chain (requires explicit window scope).
pub async fn probe_element_get(
    ctx: &ExecutionContext,
    params: BTreeMap<String, Value>,
) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        probe_scope_explicit(&params)?;
        crate::builtin::ui::win::ui_element_find_probe_impl(params, ctx.ui_max_selector_chain())
            .await
    }
    #[cfg(not(windows))]
    {
        let _ = (ctx, params);
        Err(unavailable())
    }
}

/// Hit-test at screen coordinates; returns element map + suggested selectors.
pub async fn probe_element_point(x: i64, y: i64) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || {
            let el = crate::builtin::ui::win::element_at_point(x as i32, y as i32)?;
            Ok(Value::Map(
                crate::builtin::ui::win::element_map_with_selectors(&el),
            ))
        })
        .await
        .map_err(|e| ActionError::execution(format!("ui element point 失败: {e}")))?
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y);
        Err(unavailable())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeFormat {
    Flat,
    Tree,
}

impl TreeFormat {
    pub fn parse(s: &str) -> Result<Self, ActionError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "flat" => Ok(TreeFormat::Flat),
            "tree" => Ok(TreeFormat::Tree),
            other => Err(ActionError::InvalidParams(format!(
                "未知 format: {other}（flat|tree）"
            ))),
        }
    }
}

/// Build a default execution context for probe commands.
pub fn probe_context(config: RuntimeConfig) -> ExecutionContext {
    ExecutionContext::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::PluginConfig;

    #[test]
    fn probe_denied_when_action_disabled() {
        let plugins = PluginConfig {
            disabled_actions: vec!["ui.element.list".into()],
            ..Default::default()
        };
        assert!(check_probe_allowed(&plugins, "ui.element.list").is_err());
        assert!(check_probe_allowed(&plugins, "ui.window.list").is_ok());
    }
}
