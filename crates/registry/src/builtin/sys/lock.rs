//! `sys.lock` —— 锁定工作站（等价 Win+L）。

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{Action, ActionError, ActionMeta, Bucket, ExecutionContext, PermissionSet, Value};
use std::sync::Arc;

pub struct SysLock;

#[async_trait]
impl Action for SysLock {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::SHELL
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "sys.lock",
            "锁定工作站",
            "锁定当前 Windows 会话（等价 Win+L）；解锁要重新登录，指令无法代劳",
            Bucket::System,
        )
    }

    async fn execute(
        &self,
        _params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        #[cfg(windows)]
        {
            unsafe { windows::Win32::System::Shutdown::LockWorkStation() }
                .map_err(|e| ActionError::execution(format!("锁屏失败: {e}")))?;
            Ok(Value::Bool(true))
        }
        #[cfg(not(windows))]
        {
            Err(ActionError::execution("sys.lock 当前仅支持 Windows"))
        }
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(SysLock));
}
