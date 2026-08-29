//! Macro helpers for ui Action facades (imported via `#[macro_use]`).

#![allow(unused_macros)]

macro_rules! ui_unavailable {
    () => {
        Err(corex_core::ActionError::execution(
            "ui.* 在当前平台不可用（需要 Windows UI 后端）",
        ))
    };
}

macro_rules! impl_ui_action_ctx {
    ($ty:ty, $id:expr, $title:expr, $desc:expr, $params:expr, $call:ident) => {
        #[async_trait::async_trait]
        impl corex_core::Action for $ty {
            fn meta(&self) -> corex_core::ActionMeta {
                corex_core::ActionMeta::new($id, $title, $desc, corex_core::ActionCategory::Ui)
                    .with_params($params)
            }
            async fn execute(
                &self,
                params: corex_core::Value,
                ctx: &mut corex_core::ExecutionContext,
            ) -> Result<corex_core::Value, corex_core::ActionError> {
                $call(params, ctx).await
            }
        }
    };
}

macro_rules! impl_ui_action {
    ($ty:ty, $id:expr, $title:expr, $desc:expr, $params:expr, $call:ident) => {
        #[async_trait::async_trait]
        impl corex_core::Action for $ty {
            fn meta(&self) -> corex_core::ActionMeta {
                corex_core::ActionMeta::new($id, $title, $desc, corex_core::ActionCategory::Ui)
                    .with_params($params)
            }
            async fn execute(
                &self,
                params: corex_core::Value,
                _ctx: &mut corex_core::ExecutionContext,
            ) -> Result<corex_core::Value, corex_core::ActionError> {
                $call(params).await
            }
        }
    };
}
