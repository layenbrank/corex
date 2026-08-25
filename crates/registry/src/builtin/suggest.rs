//! `suggest.bing` — Bing AS/Suggestions via reqwest.

use crate::builtin::util::{opt_i64, opt_str, require_map, require_str};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use rand::RngCore;
use std::sync::Arc;

const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const API_BASE_URL: &str = "https://cn.bing.com/AS/Suggestions";

fn generate_cvid() -> String {
    let mut array = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut array);
    array[6] = (array[6] & 0x0f) | 0x40;
    array[8] = (array[8] & 0x3f) | 0x80;
    array.iter().map(|b| format!("{b:02X}")).collect()
}

pub struct SuggestBing;

#[async_trait]
impl Action for SuggestBing {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "suggest.bing",
            "Bing Suggest",
            "获取 Bing 搜索建议",
            ActionCategory::Network,
        )
        .with_params(vec![
            ParamSchema::new("qry", SchemaType::Str, true),
            ParamSchema::new("pt", SchemaType::Str, false).with_default("page.home"),
            ParamSchema::new("cp", SchemaType::Int, false),
            ParamSchema::new("csr", SchemaType::Str, false).with_default("1"),
            ParamSchema::new("pths", SchemaType::Str, false).with_default("1"),
            ParamSchema::new("cvid", SchemaType::Str, false),
            ParamSchema::new("user_agent", SchemaType::Str, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let qry = require_str(map, "qry")?;
        let pt = opt_str(map, "pt").unwrap_or_else(|| "page.home".into());
        let cp = opt_i64(map, "cp", qry.len() as i64);
        let csr = opt_str(map, "csr").unwrap_or_else(|| "1".into());
        let pths = opt_str(map, "pths").unwrap_or_else(|| "1".into());
        let cvid = opt_str(map, "cvid")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(generate_cvid);
        let user_agent = opt_str(map, "user_agent").unwrap_or_else(|| DEFAULT_USER_AGENT.into());

        let client = reqwest::Client::new();
        let resp = client
            .get(API_BASE_URL)
            .header(reqwest::header::USER_AGENT, user_agent)
            .header(reqwest::header::ACCEPT, "application/json, text/plain, */*")
            .query(&[
                ("pt", pt),
                ("qry", qry),
                ("cp", cp.to_string()),
                ("csr", csr),
                ("pths", pths),
                ("cvid", cvid),
            ])
            .send()
            .await
            .map_err(|e| ActionError::execution(format!("请求 Bing 失败: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| ActionError::execution(format!("读取响应失败: {e}")))?;
        if !status.is_success() {
            return Err(ActionError::execution(format!(
                "HTTP 错误 {}: {text}",
                status.as_u16()
            )));
        }
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| ActionError::execution(format!("Bing JSON 解析失败: {e}")))?;
        Ok(Value::from_json(json))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(SuggestBing));
}
