//! Lightweight schema type tags for action parameters.

use serde::{Deserialize, Serialize};

/// Declared parameter / return type for documentation and soft validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaType {
    Null,
    Bool,
    Int,
    Float,
    Str,
    List,
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
            SchemaType::List => "list",
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
