use std::{collections::HashMap, path::PathBuf};

/// Pipeline 执行上下文，在步骤间传递数据
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// 键值对形式的任意数据
    pub data: HashMap<String, String>,
    /// 上一步骤的输出路径（供下一步骤作为输入使用）
    pub last_output: Option<PathBuf>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入一个键值对
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.data.insert(key.into(), value.into());
    }

    /// 读取一个键值对
    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }

    /// 设置 last_output 并同步写入 data["last_output"]
    pub fn set_output(&mut self, path: PathBuf) {
        self.data.insert(
            "last_output".to_string(),
            path.to_string_lossy().to_string(),
        );
        self.last_output = Some(path);
    }

    /// 若 value 为 "$last_output" 则返回上一步输出路径，否则返回原值
    pub fn resolve(&self, value: &str) -> String {
        if value == "$last_output" {
            self.last_output
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| value.to_string())
        } else {
            value.to_string()
        }
    }
}
