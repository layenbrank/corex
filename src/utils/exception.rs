use std::io;

use glob::{GlobError, PatternError};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Exception {
    #[error("IO 错误：{0}")]
    Io(#[from] io::Error),

    #[error("未找到：{0}")]
    NotFound(String),

    #[error("验证错误：{0}")]
    Validation(String),

    #[error("内部错误：{0}")]
    Internal(String),

    #[error("Glob 错误：{0}")]
    Glob(#[from] GlobError),

    #[error("模式错误：{0}")]
    Pattern(#[from] PatternError),
}

impl Serialize for Exception {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
