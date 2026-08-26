//! `keyring.get` / `keyring.set` via the keyring crate.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
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
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "keyring.get",
            "Keyring Get",
            "从系统钥匙串读取密钥",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("service", SchemaType::Str, true),
            ParamSchema::new("user", SchemaType::Str, true),
        ])
    }

    async fn execute(
        &self, params: Value,
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
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "keyring.set",
            "Keyring Set",
            "写入系统钥匙串",
            ActionCategory::System,
        )
        .with_params(vec![
            ParamSchema::new("service", SchemaType::Str, true),
            ParamSchema::new("user", SchemaType::Str, true),
            ParamSchema::new("password", SchemaType::Str, true),
        ])
    }

    async fn execute(
        &self, params: Value,
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
