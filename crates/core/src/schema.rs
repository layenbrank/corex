//! 动作参数的轻量 schema 类型标签。

use serde::{Deserialize, Serialize};

/// 参数 / 返回值的声明类型，用于文档与软校验。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaType {
    Null,
    Bool,
    Int,
    Float,
    Str,
    Array,
    Map,
    File,
    Bytes,
    Any,
}

impl SchemaType {
    pub fn as_str(self) -> &'static str {
        match self {
            SchemaType::Null => "null",
            SchemaType::Bool => "bool",
            SchemaType::Int => "int",
            SchemaType::Float => "float",
            SchemaType::Str => "str",
            SchemaType::Array => "array",
            SchemaType::Map => "map",
            SchemaType::File => "file",
            SchemaType::Bytes => "bytes",
            SchemaType::Any => "any",
        }
    }
}

impl std::fmt::Display for SchemaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
