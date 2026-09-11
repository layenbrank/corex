//! 经 keyring crate 实现的 `keyring.get` / `keyring.set`。

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use std::sync::Arc;

fn entry(params: &Value) -> Result<keyring::Entry, ActionError> {
    let map = params
        .as_map()
        .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".to_string()))?;
    let service = map
        .get("service")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionError::MissingParam("service".to_string()))?;
    let user = map
        .get("user")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ActionError::MissingParam("user".to_string()))?;
    keyring::Entry::new(service, user)
        .map_err(|e| ActionError::execution(format!("创建 keyring entry 失败: {e}")))
}

pub struct KeyringGet;
pub struct KeyringSet;

#[async_trait]
impl Action for KeyringGet {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::SECRET
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "keyring.get",
            "读取凭据",
            "从系统钥匙串读取密钥",
            Bucket::System,
        )
        .with_params(vec![
            ParamSchema::new("service", SchemaType::Str, true),
            ParamSchema::new("user", SchemaType::Str, true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let entry = entry(&params)?;
        let secret = entry
            .get_password()
            .map_err(|e| ActionError::execution(format!("读取密钥失败: {e}")))?;
        Ok(Value::Str(secret))
    }
}

#[async_trait]
impl Action for KeyringSet {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::SECRET
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new("keyring.set", "写入凭据", "写入系统钥匙串", Bucket::System).with_params(
            vec![
                ParamSchema::new("service", SchemaType::Str, true),
                ParamSchema::new("user", SchemaType::Str, true),
                ParamSchema::new("password", SchemaType::Secret, true),
            ],
        )
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let password = params
            .as_map()
            .and_then(|m| m.get("password"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("password".to_string()))?;
        let entry = entry(&params)?;
        entry
            .set_password(password)
            .map_err(|e| ActionError::execution(format!("写入密钥失败: {e}")))?;
        Ok(Value::Bool(true))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(KeyringGet));
    registry.register(Arc::new(KeyringSet));
}
