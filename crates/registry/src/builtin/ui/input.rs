//! `ui.click` / `ui.type` / `ui.key` / `ui.wait` 动作门面。

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use std::sync::Arc;

pub struct UiWait;
pub struct UiClick;
pub struct UiType;
pub struct UiKey;
pub struct UiScroll;
pub struct UiDrag;

#[async_trait]
impl Action for UiWait {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::UI
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "ui.wait",
            "等待",
            "固定毫秒等待（兜底；优先使用 ui.element.wait）",
            Bucket::Ui,
        )
        .with_params(vec![ParamSchema::new("ms", SchemaType::Int, true)])
    }
    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        ui_wait_impl(params, ctx).await
    }
}

impl_ui_action!(
    UiClick,
    "ui.click",
    "点击",
    "在屏幕坐标点击",
    vec![
        ParamSchema::new("x", SchemaType::Int, true),
        ParamSchema::new("y", SchemaType::Int, true),
        ParamSchema::new("button", SchemaType::Str, false)
            .with_default("left")
            .with_description("left | right | middle"),
        ParamSchema::new("clicks", SchemaType::Int, false).with_default(1),
    ],
    ui_click_impl
);

impl_ui_action!(
    UiType,
    "ui.type",
    "输入文本",
    "键盘输入文本",
    vec![ParamSchema::new("text", SchemaType::Str, true)],
    ui_type_impl
);

impl_ui_action!(
    UiKey,
    "ui.key",
    "发送按键",
    "发送按键或组合键（如 Enter、Ctrl+F）",
    vec![ParamSchema::new("keys", SchemaType::Str, true)],
    ui_key_impl
);

impl_ui_action!(
    UiScroll,
    "ui.scroll",
    "滚动",
    "鼠标滚轮（dy/dx，单位与 Win32 WHEEL_DELTA 一致）",
    vec![
        ParamSchema::new("dy", SchemaType::Int, false),
        ParamSchema::new("dx", SchemaType::Int, false),
        ParamSchema::new("x", SchemaType::Int, false),
        ParamSchema::new("y", SchemaType::Int, false),
    ],
    ui_scroll_impl
);

impl_ui_action!(
    UiDrag,
    "ui.drag",
    "拖拽",
    "屏幕坐标拖拽",
    vec![
        ParamSchema::new("from_x", SchemaType::Int, true),
        ParamSchema::new("from_y", SchemaType::Int, true),
        ParamSchema::new("to_x", SchemaType::Int, true),
        ParamSchema::new("to_y", SchemaType::Int, true),
        ParamSchema::new("steps", SchemaType::Int, false).with_default(12),
        ParamSchema::new("button", SchemaType::Str, false).with_default("left"),
    ],
    ui_drag_impl
);

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(UiWait));
    registry.register(Arc::new(UiClick));
    registry.register(Arc::new(UiType));
    registry.register(Arc::new(UiKey));
    registry.register(Arc::new(UiScroll));
    registry.register(Arc::new(UiDrag));
}

#[cfg(windows)]
use super::win::{
    ui_click_impl, ui_drag_impl, ui_key_impl, ui_scroll_impl, ui_type_impl, ui_wait_impl,
};

#[cfg(not(windows))]
async fn ui_wait_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_click_impl(_: Value) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_type_impl(_: Value) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_key_impl(_: Value) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_scroll_impl(_: Value) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_drag_impl(_: Value) -> Result<Value, ActionError> {
    ui_unavailable!()
}
