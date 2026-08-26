//! Dynamic value type used across actions, directives, and IPC.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

/// Untagged dynamic value. `File` / `Bytes` prefer structural forms when
/// deserializing ambiguous JSON (string → `Str`, array of numbers → `List`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    /// Absolute or relative filesystem path. Serializes as a string.
    File(PathBuf),
    /// Raw bytes. Prefer constructing via helpers; JSON may round-trip as a list of ints.
    Bytes(Vec<u8>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::File(p) => p.to_str(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn into_string(self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s),
            Value::File(p) => Some(p.display().to_string()),
            Value::Bool(b) => Some(b.to_string()),
            Value::Int(i) => Some(i.to_string()),
            Value::Float(f) => Some(f.to_string()),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Map(m) => !m.is_empty(),
            Value::File(_) => true,
            Value::Bytes(b) => !b.is_empty(),
        }
    }

    /// Parse a CLI `-i KEY=VALUE` literal into a typed [`Value`] when unambiguous.
    pub fn from_cli_literal(raw: &str) -> Self {
        let s = raw.trim();
        match s.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" => Value::Bool(true),
            "false" | "no" | "off" => Value::Bool(false),
            _ => {
                if let Ok(i) = s.parse::<i64>() {
                    Value::Int(i)
                } else if let Ok(f) = s.parse::<f64>() {
                    Value::Float(f)
                } else {
                    Value::Str(s.to_string())
                }
            }
        }
    }

    /// Dot-path lookup, e.g. `"user.name"` or `"items.0"`.
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        if path.is_empty() {
            return Some(self);
        }
        let mut current = self;
        for segment in path.split('.') {
            current = match current {
                Value::Map(m) => m.get(segment)?,
                Value::List(l) => {
                    let idx: usize = segment.parse().ok()?;
                    l.get(idx)?
                }
                _ => return None,
            };
        }
        Some(current)
    }

    /// Mutable variant of [`get_path`].
    pub fn get_path_mut(&mut self, path: &str) -> Option<&mut Value> {
        if path.is_empty() {
            return Some(self);
        }
        let mut current = self;
        for segment in path.split('.') {
            current = match current {
                Value::Map(m) => m.get_mut(segment)?,
                Value::List(l) => {
                    let idx: usize = segment.parse().ok()?;
                    l.get_mut(idx)?
                }
                _ => return None,
            };
        }
        Some(current)
    }

    pub fn insert_map(&mut self, key: impl Into<String>, value: Value) {
        match self {
            Value::Map(m) => {
                m.insert(key.into(), value);
            }
            _ => {
                let mut m = BTreeMap::new();
                m.insert(key.into(), value);
                *self = Value::Map(m);
            }
        }
    }

    pub fn from_json(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Null
                }
            }
            serde_json::Value::String(s) => Value::Str(s),
            serde_json::Value::Array(a) => {
                Value::List(a.into_iter().map(Value::from_json).collect())
            }
            serde_json::Value::Object(o) => Value::Map(
                o.into_iter()
                    .map(|(k, v)| (k, Value::from_json(v)))
                    .collect(),
            ),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Int(i) => serde_json::json!(*i),
            Value::Float(f) => serde_json::json!(*f),
            Value::Str(s) => serde_json::Value::String(s.clone()),
            Value::List(l) => {
                serde_json::Value::Array(l.iter().map(Value::to_json).collect())
            }
            Value::Map(m) => serde_json::Value::Object(
                m.iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ),
            Value::File(p) => serde_json::Value::String(p.display().to_string()),
            Value::Bytes(b) => {
                serde_json::Value::Array(b.iter().map(|x| serde_json::json!(*x)).collect())
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::List(l) => {
                write!(f, "[")?;
                for (i, v) in l.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Map(m) => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::File(p) => write!(f, "{}", p.display()),
            Value::Bytes(b) => write!(f, "<{} bytes>", b.len()),
        }
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Null
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(v as i64)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Int(v as i64)
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::Int(v as i64)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Float(v as f64)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}

impl From<PathBuf> for Value {
    fn from(v: PathBuf) -> Self {
        Value::File(v)
    }
}

impl From<&std::path::Path> for Value {
    fn from(v: &std::path::Path) -> Self {
        Value::File(v.to_path_buf())
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::List(v)
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(v: BTreeMap<String, Value>) -> Self {
        Value::Map(v)
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(v)
    }
}

impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        Value::from_json(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_path_nested() {
        let mut map = BTreeMap::new();
        let mut inner = BTreeMap::new();
        inner.insert("name".into(), Value::Str("corex".into()));
        map.insert("user".into(), Value::Map(inner));
        let v = Value::Map(map);
        assert_eq!(v.get_path("user.name").and_then(|x| x.as_str()), Some("corex"));
    }

    #[test]
    fn display_and_from() {
        assert_eq!(Value::from(true).to_string(), "true");
        assert_eq!(Value::from(42i64).to_string(), "42");
        assert_eq!(Value::from("hi").to_string(), "hi");
    }

    #[test]
    fn from_cli_literal_parses_bool_and_int() {
        assert_eq!(Value::from_cli_literal("false"), Value::Bool(false));
        assert_eq!(Value::from_cli_literal("true"), Value::Bool(true));
        assert!(!Value::from_cli_literal("false").is_truthy());
        assert_eq!(Value::from_cli_literal("120000"), Value::Int(120000));
        assert_eq!(Value::from_cli_literal("hello"), Value::Str("hello".into()));
    }
}
