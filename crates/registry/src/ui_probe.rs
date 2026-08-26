//! Interactive UI probe API for `corex ui` CLI (Windows UIAutomation).

use corex_core::{ActionError, ExecutionContext, RuntimeConfig, Value};
use std::collections::BTreeMap;

#[cfg(not(windows))]
fn unavailable() -> ActionError {
    ActionError::execution("ui probe 在当前平台不可用（需要 Windows）")
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

/// List UIA children under a scope window.
pub async fn probe_list(
    ctx: &ExecutionContext,
    params: BTreeMap<String, Value>,
) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        let mut c = ctx.clone();
        crate::builtin::ui::win::ui_element_list_impl(Value::Map(params), &mut c).await
    }
    #[cfg(not(windows))]
    {
        let _ = (ctx, params);
        Err(unavailable())
    }
}

/// Find element by selector chain.
pub async fn probe_find(
    ctx: &ExecutionContext,
    params: BTreeMap<String, Value>,
) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        let mut c = ctx.clone();
        crate::builtin::ui::win::ui_element_find_impl(Value::Map(params), &mut c).await
    }
    #[cfg(not(windows))]
    {
        let _ = (ctx, params);
        Err(unavailable())
    }
}

/// Hit-test at screen coordinates; returns element map + suggested selectors.
pub async fn probe_at(x: i64, y: i64) -> Result<Value, ActionError> {
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || {
            let el = crate::builtin::ui::win::element_at_point(x as i32, y as i32)?;
            Ok(Value::Map(crate::builtin::ui::win::element_map_with_selectors(&el)))
        })
        .await
        .map_err(|e| ActionError::execution(format!("ui at 失败: {e}")))?
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y);
        Err(unavailable())
    }
}

/// Build a default execution context for probe commands.
pub fn probe_context(config: RuntimeConfig) -> ExecutionContext {
    ExecutionContext::new(config)
}
