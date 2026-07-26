use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::{Value, json};

use crate::engine::schema::{Args, Suggestion, SuggestionArgs, UrlParams};
use crate::generate::generate_secure_cvid;

const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const ACCEPT_HEADER: &str = "application/json, text/plain, */*";
const API_BASE_URL: &str = "https://cn.bing.com/AS/Suggestions";

pub fn run(args: &Args) -> Result<()> {
    let suggestion = execute(args)?;
    let json = serde_json::to_string_pretty(&suggestion)?;
    println!("{json}");
    Ok(())
}

pub fn execute(args: &Args) -> Result<Suggestion> {
    match args {
        Args::Suggestion(a) => fetch_suggestion(a),
    }
}

fn fetch_suggestion(args: &SuggestionArgs) -> Result<Suggestion> {
    let cvid = args
        .cvid
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(generate_secure_cvid);

    let params = UrlParams {
        pt: args.pt.clone(),
        qry: args.qry.clone(),
        cp: args.cp,
        csr: args.csr.clone(),
        pths: args.pths.clone(),
        cvid,
    };

    let user_agent = args
        .user_agent
        .as_deref()
        .filter(|ua| !ua.is_empty())
        .unwrap_or(DEFAULT_USER_AGENT);

    let mut headers = HeaderMap::new();
    let ua_header = HeaderValue::from_str(user_agent).unwrap_or_else(|_| {
        HeaderValue::from_static(DEFAULT_USER_AGENT)
    });
    headers.insert(USER_AGENT, ua_header);
    headers.insert(ACCEPT, HeaderValue::from_static(ACCEPT_HEADER));

    let client = Client::new();
    let response = client
        .get(API_BASE_URL)
        .headers(headers)
        .query(&params)
        .send()
        .context("请求 Bing Suggestions 失败")?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().unwrap_or_default();
        bail!("HTTP 错误 {}: {}", status.as_u16(), error_text);
    }

    let text = response.text().context("读取响应正文失败")?;
    serde_json::from_str(&text).with_context(|| {
        let preview = if text.len() > 1000 {
            format!("{}...", &text[..1000])
        } else {
            text.clone()
        };
        format!("Bing 响应 JSON 解析失败\n响应内容: {preview}")
    })
}

impl Suggestion {
    pub fn into_ipc_value(self) -> Value {
        json!(self)
    }

    pub fn into_invoke_result(self) -> crate::invoke::InvokeResult {
        use crate::invoke::{Artifact, InvokeResult};
        InvokeResult::from_artifact(Artifact::default().with_data("data", self.into_ipc_value()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::schema::{EmptySchema, ISchema};

    #[test]
    fn unknown_t_deserializes() {
        let json = r#"{
            "s": [{"id":"1","q":"rust","u":"/search","t":"XYZ"}],
            "i": {"ig":"abc"}
        }"#;
        let suggestion: Suggestion = serde_json::from_str(json).expect("parse");
        assert_eq!(suggestion.s[0].t, "XYZ");
        assert_eq!(suggestion.i.ig, "abc");
    }

    #[test]
    fn suggestion_roundtrip() {
        let s = Suggestion {
            s: vec![EmptySchema {
                id: "1".into(),
                q: "hi".into(),
                u: "/u".into(),
                t: "LT".into(),
            }],
            i: ISchema {
                ig: "ig".into(),
            },
        };
        let v = s.into_ipc_value();
        assert_eq!(v["s"][0]["t"], "LT");
    }
}
