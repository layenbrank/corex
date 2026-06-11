use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

use crate::tasks::TaskOutput;

/// Pipeline 执行上下文 —— 在步骤间传递数据与协作信息
///
/// Sequential 模式：步骤可以通过 `${step_id.output}` 和 `${step_id.metadata.key}` 引用前序步骤
/// Parallel 模式：仅支持全局变量 `${var.name}`，禁止跨步骤引用
#[derive(Debug, Clone, Default)]
pub struct PipelineContext {
    /// 全局变量（来自 YAML `variables` 段）
    pub variables: HashMap<String, String>,

    /// 各步骤的执行输出（step_id → TaskOutput）
    pub step_outputs: HashMap<String, TaskOutput>,
}

impl PipelineContext {
    /// 创建空上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 用全局变量初始化上下文
    pub fn with_variables(variables: HashMap<String, String>) -> Self {
        Self {
            variables,
            ..Default::default()
        }
    }

    /// 记录某步骤的输出
    pub fn set_step_output(&mut self, step_id: String, output: TaskOutput) {
        self.step_outputs.insert(step_id, output);
    }

    /// 解析字符串中的变量引用
    ///
    /// 支持的语法：
    /// - `${var.name}`            → 全局变量
    /// - `${step_id.output}`      → 步骤输出路径
    /// - `${step_id.metadata.key}` → 步骤元数据
    pub fn resolve(&self, input: &str) -> String {
        let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
        re.replace_all(input, |caps: &regex::Captures| {
            let reference = &caps[1];
            self.resolve_reference(reference)
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string()
    }

    /// 解析单个引用路径
    fn resolve_reference(&self, reference: &str) -> Option<String> {
        let parts: Vec<&str> = reference.splitn(3, '.').collect();

        match parts.as_slice() {
            // ${var.name} → 全局变量
            ["var", name] => self.variables.get(*name).cloned(),

            // ${step_id.output} → 步骤输出路径
            [step_id, "output"] => self
                .step_outputs
                .get(*step_id)
                .and_then(|o| o.path.as_ref())
                .map(|p| p.to_string_lossy().to_string()),

            // ${step_id.metadata.key} → 步骤元数据
            [step_id, "metadata", key] => self
                .step_outputs
                .get(*step_id)
                .and_then(|o| o.metadata.get(*key))
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                }),

            _ => None,
        }
    }

    /// 对 Value 中的所有字符串值进行变量替换
    pub fn resolve_value(&self, value: &Value) -> Value {
        match value {
            Value::String(s) => Value::String(self.resolve(s)),
            Value::Object(map) => {
                let resolved: serde_json::Map<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), self.resolve_value(v)))
                    .collect();
                Value::Object(resolved)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(|v| self.resolve_value(v)).collect()),
            other => other.clone(),
        }
    }
}
