use std::io::{self, Write};

use serde::Serialize;
use serde_json::Value;

use super::opts::ColorChoice;

/// 输出格式
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Human,
    Json,
}

/// 统一输出出口
#[derive(Debug, Clone)]
pub struct Emitter {
    format: OutputFormat,
    quiet: bool,
    color: ColorChoice,
}

impl Emitter {
    pub fn new(format: OutputFormat, quiet: bool, color: ColorChoice) -> Self {
        Self {
            format,
            quiet,
            color,
        }
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    pub fn use_color(&self) -> bool {
        match self.color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => std::env::var("NO_COLOR").is_err(),
        }
    }

    /// 人类可读消息（stderr，除非 quiet）
    pub fn message(&self, msg: impl AsRef<str>) {
        if self.quiet || self.format == OutputFormat::Json {
            return;
        }
        eprintln!("{}", msg.as_ref());
    }

    /// 结构化 JSON 输出到 stdout
    pub fn json<T: Serialize>(&self, value: &T) -> io::Result<()> {
        if self.format == OutputFormat::Json {
            let line = serde_json::to_string(value)?;
            writeln!(io::stdout(), "{line}")?;
        }
        Ok(())
    }

    /// 结构化 JSON Value 到 stdout
    pub fn json_value(&self, value: &Value) -> io::Result<()> {
        if self.format == OutputFormat::Json {
            writeln!(io::stdout(), "{}", serde_json::to_string(value)?)?;
        }
        Ok(())
    }

    /// 错误输出
    pub fn error(
        &self,
        code: &str,
        message: impl AsRef<str>,
        step_id: Option<&str>,
    ) -> io::Result<()> {
        if self.format == OutputFormat::Json {
            let mut err = serde_json::json!({
                "error": {
                    "code": code,
                    "message": message.as_ref(),
                }
            });
            if let Some(id) = step_id {
                err["error"]["step_id"] = Value::String(id.to_string());
            }
            self.json_value(&err)?;
        } else if !self.quiet {
            eprintln!("错误 [{code}]: {}", message.as_ref());
        }
        Ok(())
    }
}
