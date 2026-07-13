use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

use crate::invoke::Artifact;

/// Pipeline 执行上下文（v3 变量语法）
#[derive(Debug, Clone, Default)]
pub struct PipelineContext {
    pub variables: HashMap<String, String>,
    pub step_artifacts: HashMap<String, Artifact>,
}

impl PipelineContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_variables(variables: HashMap<String, String>) -> Self {
        Self {
            variables,
            ..Default::default()
        }
    }

    pub fn set_artifact(&mut self, step_id: String, artifact: Artifact) {
        self.step_artifacts.insert(step_id, artifact);
    }

    /// 解析字符串中的 `${var.*}` / `${steps.*}` 占位符（支持变量嵌套引用）。
    pub fn parse(&self, input: &str) -> String {
        let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
        let mut result = input.to_string();
        for _ in 0..32 {
            let next = re
                .replace_all(&result, |caps: &regex::Captures| {
                    self.parse_reference(&caps[1])
                        .unwrap_or_else(|| caps[0].to_string())
                })
                .to_string();
            if next == result {
                break;
            }
            result = next;
        }
        result
    }

    fn parse_reference(&self, reference: &str) -> Option<String> {
        let parts: Vec<&str> = reference.split('.').collect();
        match parts.as_slice() {
            ["env", name] => std::env::var(name).ok(),
            ["var", name] => self.variables.get(*name).cloned(),
            ["steps", step_id, "artifact", "path"] => self
                .step_artifacts
                .get(*step_id)
                .and_then(|a| a.path.as_ref())
                .map(|p| p.to_string_lossy().to_string()),
            ["steps", step_id, "artifact", "data", key] => self
                .step_artifacts
                .get(*step_id)
                .and_then(|a| a.data.get(*key))
                .map(value_to_string),
            _ => None,
        }
    }

    /// 递归解析 JSON 值中的占位符。
    pub fn parse_value(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.parse(s)),
            Value::Object(map) => {
                let parsed: serde_json::Map<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), self.parse_value(v)))
                    .collect();
                Value::Object(parsed)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(|v| self.parse_value(v)).collect()),
            other => other.clone(),
        }
    }

    /// when 条件：非空且不为 false/0/no 视为 true
    pub fn eval_when(&self, expr: &str) -> bool {
        let parsed = self.parse(expr).trim().to_lowercase();
        !parsed.is_empty()
            && parsed != "false"
            && parsed != "0"
            && parsed != "no"
            && parsed != "off"
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parse_resolves_nested_variables() {
        let mut variables = HashMap::new();
        variables.insert("base".into(), "C:\\root".into());
        variables.insert(
            "project".into(),
            "${var.base}\\Vue2\\front\\master".into(),
        );
        let ctx = PipelineContext::with_variables(variables);
        assert_eq!(
            ctx.parse("${var.project}"),
            "C:\\root\\Vue2\\front\\master"
        );
        assert_eq!(
            ctx.parse("${var.project}\\version.json"),
            "C:\\root\\Vue2\\front\\master\\version.json"
        );
    }
}
