//! `ui.window.*` Action facades.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;

pub struct UiWindowList;
pub struct UiWindowFocus;
pub struct UiWindowFind;
pub struct UiWindowWait;
pub struct UiWindowDesktop;

#[async_trait]
impl Action for UiWindowList {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "ui.window.list",
            "List Windows",
            "枚举顶层窗口（hwnd/title/class/pid）",
            ActionCategory::Ui,
        )
    }
    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        ui_window_list_impl().await
    }
}

#[async_trait]
impl Action for UiWindowDesktop {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "ui.window.desktop",
            "Desktop Icons",
            "枚举桌面图标（Shell ListItem）",
            ActionCategory::Ui,
        )
    }
    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        ui_desktop_icons_impl().await
    }
}

impl_ui_action_ctx!(
    UiWindowFocus,
    "ui.window.focus",
    "Focus Window",
    "按标题或 hwnd 聚焦顶层窗口",
    vec![
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("prefer_largest", SchemaType::Bool, false),
        ParamSchema::new("class_name", SchemaType::Str, false),
    ],
    ui_window_focus_impl
);

impl_ui_action_ctx!(
    UiWindowFind,
    "ui.window.find",
    "Find Window",
    "按标题查找顶层窗口（非应用内元素）",
    vec![
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("name", SchemaType::Str, false),
        ParamSchema::new("hwnd", SchemaType::Int, false),
        ParamSchema::new("prefer_largest", SchemaType::Bool, false),
        ParamSchema::new("class_name", SchemaType::Str, false),
    ],
    ui_window_find_impl
);

impl_ui_action_ctx!(
    UiWindowWait,
    "ui.window.wait",
    "Wait Window",
    "等待顶层窗口标题出现",
    vec![
        ParamSchema::new("title_contains", SchemaType::Str, false),
        ParamSchema::new("timeout_ms", SchemaType::Int, false).with_default(5000),
        ParamSchema::new("prefer_largest", SchemaType::Bool, false),
        ParamSchema::new("class_name", SchemaType::Str, false),
    ],
    ui_window_wait_impl
);

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(UiWindowList));
    registry.register(Arc::new(UiWindowFocus));
    registry.register(Arc::new(UiWindowFind));
    registry.register(Arc::new(UiWindowWait));
    registry.register(Arc::new(UiWindowDesktop));
}

#[cfg(windows)]
use super::win::{
    ui_desktop_icons_impl, ui_window_find_impl, ui_window_focus_impl, ui_window_list_impl,
    ui_window_wait_impl,
};

#[cfg(not(windows))]
async fn ui_window_list_impl() -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_desktop_icons_impl() -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_window_focus_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_window_find_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
#[cfg(not(windows))]
async fn ui_window_wait_impl(_: Value, _: &mut ExecutionContext) -> Result<Value, ActionError> {
    ui_unavailable!()
}
