//! 供 `corex ui` CLI 使用的交互式 UI 探测 API（Windows UIAutomation）。

#[cfg(windows)]
use crate::builtin::ui::kernel::{elements_flat_to_tree, probe_scope_explicit};
use corex_core::{ActionError, ExecutionContext, RuntimeConfig, Value};
use std::collections::BTreeMap;

#[cfg(not(windows))]
fn unavailable() -> ActionError {
    ActionError::execution("ui probe 在当前平台不可用（需要 Windows）")
}

/// 像 registry + daemon Invoke 那样给 CLI/`ui_probe` 做门禁：
/// `plugins.disabled`、`plugins.disabled_actions` 与 `strict_permissions`。
pub fn check_probe_allowed(
    config: &RuntimeConfig,
    store: &dyn corex_core::ActionStore,
    action_id: &str,
) -> Result<(), ActionError> {
    corex_core::check_runtime_allowed(config, store, action_id)
}

/// 列出可见的顶层窗口。
pub async fn probe_windows() -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        crate::builtin::ui::win::ui_windows_impl().await
    }
    #[cfg(not(windows))]
    {
        Err(unavailable())
    }
}

/// 桌面图标（Progman 上的 ListItem）。
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

/// 列出指定窗口下的 UIA 元素（必须显式给 `--hwnd` 或 `--title`）。
pub async fn probe_element_tree(
    ctx: &ExecutionContext,
    params: BTreeMap<String, Value>,
    format: TreeFormat,
) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        let _ = ctx;
        probe_scope_explicit(&params)?;
        let mut v = crate::builtin::ui::win::ui_elements_probe_impl(params).await?;
        match format {
            TreeFormat::Flat => Ok(v),
            TreeFormat::Tree => {
                if let Value::Map(ref mut m) = v
                    && let Some(Value::Array(elements)) = m.remove("elements")
                {
                    let maps: Vec<BTreeMap<String, Value>> = elements
                        .into_iter()
                        .filter_map(|v| v.as_map().cloned())
                        .collect();
                    let mut out = BTreeMap::new();
                    out.insert("tree".into(), elements_flat_to_tree(&maps));
                    return Ok(Value::Map(out));
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

/// 按 selector 链查找元素（必须显式指定窗口范围）。
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

/// 按屏幕坐标做命中测试；返回元素 map 与建议的 selector。
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

/// 为探测命令构建默认执行上下文。
pub fn probe_context(config: RuntimeConfig) -> ExecutionContext {
    ExecutionContext::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::PluginConfig;

    /// 用真实的内置声明，使写错的权限要求不会悄悄溜过。
    fn store() -> crate::ActionRegistry {
        let mut registry = crate::ActionRegistry::new();
        registry.register_builtins();
        registry
    }

    #[test]
    fn probe_denied_when_action_disabled() {
        let cfg = RuntimeConfig {
            plugins: PluginConfig {
                disabled_actions: vec!["ui.element.list".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(check_probe_allowed(&cfg, &store(), "ui.element.list").is_err());
        assert!(check_probe_allowed(&cfg, &store(), "ui.window.list").is_ok());
    }

    #[test]
    fn probe_denied_when_plugin_disabled() {
        let cfg = RuntimeConfig {
            plugins: PluginConfig {
                disabled: vec!["ui".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(check_probe_allowed(&cfg, &store(), "ui.window.desktop").is_err());
        assert!(check_probe_allowed(&cfg, &store(), "ui.element.pick").is_err());
    }

    #[test]
    fn probe_denied_under_strict_permissions() {
        let cfg = RuntimeConfig {
            strict_permissions: true,
            ..Default::default()
        };
        assert!(check_probe_allowed(&cfg, &store(), "ui.element.point").is_err());
        assert!(check_probe_allowed(&cfg, &store(), "ui.window.list").is_err());
    }
}
