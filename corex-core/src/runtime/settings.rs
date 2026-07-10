use std::collections::HashMap;

/// 合并 CLI / 环境变量覆盖到 variables
pub fn merge_variables(
    base: HashMap<String, String>,
    cli_overrides: &[(String, String)],
) -> HashMap<String, String> {
    let mut out = base;
    for (k, v) in std::env::vars() {
        if let Some(name) = k.strip_prefix("COREX_VAR_") {
            out.insert(name.to_string(), v);
        }
    }
    for (k, v) in cli_overrides {
        out.insert(k.clone(), v.clone());
    }
    out
}

/// 解析 `-D key=value` clap 参数
pub fn parse_define(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("无效定义 '{s}'，期望 key=value"))?;
    if k.is_empty() {
        return Err(format!("无效定义 '{s}'：键不能为空"));
    }
    Ok((k.to_string(), v.to_string()))
}
