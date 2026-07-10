use serde_json::Value;

/// Dry-run 输出时对 password 脱敏
pub fn redact_sensitive_params(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if k.eq_ignore_ascii_case("password") {
                    out.insert(k.clone(), Value::String("***".to_string()));
                } else {
                    out.insert(k.clone(), redact_sensitive_params(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(redact_sensitive_params).collect()),
        other => other.clone(),
    }
}
