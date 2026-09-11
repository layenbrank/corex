//! `url.open` —— 用 ShellExecute 打开 URL / 文件。

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use std::sync::Arc;

pub struct UrlOpen;

#[async_trait]
impl Action for UrlOpen {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::SHELL
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "url.open",
            "打开 URL",
            "用系统默认程序打开 URL 或文件",
            Bucket::System,
        )
        .with_params(vec![ParamSchema::new("url", SchemaType::Str, true)])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".into()))?;
        let url = map
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("url".into()))?
            .to_string();
        #[cfg(windows)]
        {
            return tokio::task::spawn_blocking(move || win::shell_open(&url))
                .await
                .map_err(|e| ActionError::execution(format!("url.open 失败: {e}")))?
                .map(|_| Value::Bool(true));
        }
        #[cfg(not(windows))]
        {
            let _ = url;
            Err(ActionError::execution("url.open 需要 Windows"))
        }
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(UrlOpen));
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::PCWSTR;

    pub fn shell_open(url: &str) -> Result<(), ActionError> {
        let wide: Vec<u16> = OsStr::new(url)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let op: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let ret = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(op.as_ptr()),
                PCWSTR(wide.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecute 成功时返回值 > 32（HINSTANCE 当作 isize）
        if ret.0 as isize <= 32 {
            return Err(ActionError::execution(format!(
                "ShellExecute 失败 code={}",
                ret.0 as isize
            )));
        }
        Ok(())
    }
}
