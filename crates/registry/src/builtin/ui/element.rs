//! `ui.element.*` Action facades.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;

pub struct UiElementList;
pub struct UiElementFind;
pub struct UiElementClick;
pub struct UiElementWait;
pub struct UiElementExists;
pub struct UiElementPoint;
pub struct UiElementPick;
pub struct UiElementGet;
pub struct UiElementSet;

impl_ui_action_ctx!(
    UiElementList,
    "ui.element.list",
    "List Elements",
    "枚举窗口下子元素（UIAutomation）",
    vec![
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("depth", SchemaType::Int, false).with_default(3),
        ParamSchema::new("limit", SchemaType::Int, false).with_default(50),
    ],
    ui_element_list_impl
);

impl_ui_action_ctx!(
    UiElementFind,
    "ui.element.find",
    "Find Element",
    "按 name / automation_id / control_type 查找元素",
    vec![
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("name", SchemaType::Str, false),
        ParamSchema::new("name_contains", SchemaType::Str, false),
        ParamSchema::new("automation_id", SchemaType::Str, false),
        ParamSchema::new("control_type", SchemaType::Str, false),
        ParamSchema::new("selectors", SchemaType::List, false),
        ParamSchema::new("timeout_ms", SchemaType::Int, false).with_default(3000),
    ],
    ui_element_find_impl
);

impl_ui_action_ctx!(
    UiElementClick,
    "ui.element.click",
    "Click Element",
    "点击元素（按 selector）",
    vec![
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("name", SchemaType::Str, false),
        ParamSchema::new("automation_id", SchemaType::Str, false),
        ParamSchema::new("control_type", SchemaType::Str, false),
        ParamSchema::new("selectors", SchemaType::List, false),
        ParamSchema::new("timeout_ms", SchemaType::Int, false).with_default(3000),
        ParamSchema::new("safe", SchemaType::Bool, false).with_default(true),
    ],
    ui_element_click_impl
);

impl_ui_action_ctx!(
    UiElementWait,
    "ui.element.wait",
    "Wait Element",
    "等待元素 present/absent/enabled",
    vec![
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("name", SchemaType::Str, false),
        ParamSchema::new("automation_id", SchemaType::Str, false),
        ParamSchema::new("control_type", SchemaType::Str, false),
        ParamSchema::new("selectors", SchemaType::List, false),
        ParamSchema::new("state", SchemaType::Str, false).with_default("present"),
        ParamSchema::new("timeout_ms", SchemaType::Int, true),
        ParamSchema::new("poll_interval_ms", SchemaType::Int, false).with_default(200),
    ],
    ui_element_wait_impl
);

impl_ui_action_ctx!(
    UiElementExists,
    "ui.element.exists",
    "Element Exists",
    "探测元素是否存在（非阻塞语义）",
    vec![
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("name", SchemaType::Str, false),
        ParamSchema::new("automation_id", SchemaType::Str, false),
        ParamSchema::new("control_type", SchemaType::Str, false),
        ParamSchema::new("selectors", SchemaType::List, false),
        ParamSchema::new("timeout_ms", SchemaType::Int, false).with_default(2000),
    ],
    ui_element_exists_impl
);

impl_ui_action_ctx!(
    UiElementGet,
    "ui.element.get",
    "Get Element Value",
    "读取元素 ValuePattern / Name",
    vec![
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("name", SchemaType::Str, false),
        ParamSchema::new("automation_id", SchemaType::Str, false),
        ParamSchema::new("control_type", SchemaType::Str, false),
        ParamSchema::new("selectors", SchemaType::List, false),
        ParamSchema::new("timeout_ms", SchemaType::Int, false).with_default(3000),
    ],
    ui_element_get_impl
);

impl_ui_action_ctx!(
    UiElementSet,
    "ui.element.set",
    "Set Element Value",
    "写入元素 ValuePattern",
    vec![
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("name", SchemaType::Str, false),
        ParamSchema::new("automation_id", SchemaType::Str, false),
        ParamSchema::new("control_type", SchemaType::Str, false),
        ParamSchema::new("selectors", SchemaType::List, false),
        ParamSchema::new("value", SchemaType::Str, true),
        ParamSchema::new("timeout_ms", SchemaType::Int, false).with_default(3000),
    ],
    ui_element_set_impl
);

#[async_trait]
impl Action for UiElementPoint {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "ui.element.point",
            "Element At Point",
            "按屏幕坐标取 UIA 元素",
            ActionCategory::Ui,
        )
        .with_params(vec![
            ParamSchema::new("x", SchemaType::Int, true),
            ParamSchema::new("y", SchemaType::Int, true),
        ])
    }
    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        ui_element_point_impl(params).await
    }
}

#[async_trait]
impl Action for UiElementPick {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "ui.element.pick",
            "Pick Element",
            "交互式点选 UI 元素（需桌面会话）",
            ActionCategory::Ui,
        )
        .with_params(vec![ParamSchema::new("scope_hwnd", SchemaType::Int, false)])
    }
    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        ui_element_pick_impl(params).await
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(UiElementList));
    registry.register(Arc::new(UiElementFind));
    registry.register(Arc::new(UiElementClick));
    registry.register(Arc::new(UiElementWait));
    registry.register(Arc::new(UiElementExists));
    registry.register(Arc::new(UiElementGet));
    registry.register(Arc::new(UiElementSet));
    registry.register(Arc::new(UiElementPoint));
    registry.register(Arc::new(UiElementPick));
}

#[cfg(windows)]
use super::win::{
    ui_element_click_impl, ui_element_exists_impl, ui_element_find_impl, ui_element_get_impl,
    ui_element_list_impl, ui_element_set_impl, ui_element_wait_impl,
};

#[cfg(windows)]
async fn ui_element_point_impl(params: Value) -> Result<Value, ActionError> {
    use crate::builtin::util::require_map;
    let map = require_map(&params)?;
    let x = map
        .get("x")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("x".into()))? as i32;
    let y = map
        .get("y")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ActionError::MissingParam("y".into()))? as i32;
    tokio::task::spawn_blocking(move || {
        let el = super::win::element_at_point(x, y)?;
        Ok(Value::Map(super::win::element_map_with_selectors(&el)))
    })
    .await
    .map_err(|e| ActionError::execution(format!("ui.element.point 失败: {e}")))?
}

#[cfg(windows)]
async fn ui_element_pick_impl(params: Value) -> Result<Value, ActionError> {
    use crate::builtin::util::require_map;
    let map = require_map(&params)?;
    let scope = map.get("scope_hwnd").and_then(|v| v.as_i64());
    crate::ui_pick::probe_pick(scope).await
}

#[cfg(not(windows))]
async fn ui_element_list_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_element_find_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_element_exists_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_element_get_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_element_set_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_element_click_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_element_wait_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_element_point_impl(_: Value) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_element_pick_impl(_: Value) -> Result<Value, ActionError> {
    ui_unavailable!()
}
