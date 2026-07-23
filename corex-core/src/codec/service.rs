use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use md5::{Digest, Md5};
use serde_json::{Value, json};

use crate::codec::schema::{
    Args, DecodeAlgorithm, DecodeArgs, EncodeAlgorithm, EncodeArgs, HashAlgorithm, HashArgs,
};
use crate::utils::paths::{validate_read_file, validate_write_path};

#[derive(Debug, Clone)]
pub struct Output {
    pub text: Option<String>,
    pub path: Option<String>,
}

impl Output {
    pub fn into_ipc_data(self) -> Option<Value> {
        self.text.map(|text| json!({ "text": text }))
    }

    /// 转为 invoke 层统一结果。
    pub fn into_invoke_result(self) -> crate::invoke::InvokeResult {
        use crate::invoke::{Artifact, InvokeResult};
        let artifact = match &self.path {
            Some(p) => Artifact::from_path(p.clone()),
            None => Artifact::default(),
        };
        let artifact = if let Some(text) = self.text {
            artifact.with_data("text", Value::String(text))
        } else {
            artifact
        };
        InvokeResult::from_artifact(artifact)
    }
}

pub fn run(args: &Args) -> Result<()> {
    let output = execute(args)?;
    if let Some(text) = &output.text {
        println!("{text}");
    }
    if let Some(path) = &output.path {
        println!("✅ 已写入: {path}");
    }
    Ok(())
}

pub fn execute(args: &Args) -> Result<Output> {
    match args {
        Args::Encode(a) => encode(a),
        Args::Decode(a) => decode(a),
        Args::Hash(a) => hash(a),
    }
}

fn encode(args: &EncodeArgs) -> Result<Output> {
    match &args.algorithm {
        EncodeAlgorithm::Base64(a) => {
            let bytes = read_bytes(a.input.as_deref(), a.file.as_deref())?;
            let text = STANDARD.encode(&bytes);
            if let Some(path) = a.output.as_deref() {
                validate_write_path(path)?;
            }
            write_if_set(a.output.as_deref(), text.as_bytes())?;
            Ok(Output {
                text: Some(text),
                path: a.output.clone(),
            })
        }
    }
}

fn decode(args: &DecodeArgs) -> Result<Output> {
    match &args.algorithm {
        DecodeAlgorithm::Base64(a) => {
            let input = read_text(a.input.as_deref(), a.file.as_deref())?;
            let bytes = STANDARD.decode(input.trim()).context("base64 解码失败")?;
            if let Some(path) = a.output.as_deref() {
                validate_write_path(path)?;
            }
            write_if_set(a.output.as_deref(), &bytes)?;
            let text = if a.output.is_some() {
                None
            } else {
                Some(decode_text_for_ipc(&bytes)?)
            };
            Ok(Output {
                text,
                path: a.output.clone(),
            })
        }
    }
}

fn hash(args: &HashArgs) -> Result<Output> {
    match &args.algorithm {
        HashAlgorithm::Md5(a) => {
            let bytes = read_bytes(a.input.as_deref(), a.file.as_deref())?;
            let digest = Md5::digest(&bytes);
            let text = digest
                .iter()
                .map(|byte| format!("{:02x}", byte))
                .collect::<String>();
            if let Some(path) = a.output.as_deref() {
                validate_write_path(path)?;
            }
            write_if_set(a.output.as_deref(), text.as_bytes())?;
            Ok(Output {
                text: Some(text),
                path: a.output.clone(),
            })
        }
    }
}

fn decode_text_for_ipc(bytes: &[u8]) -> Result<String> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_string()),
        Err(_) => Ok(bytes.iter().map(|b| format!("{:02x}", b)).collect()),
    }
}

fn read_bytes(input: Option<&str>, file: Option<&str>) -> Result<Vec<u8>> {
    match (input, file) {
        (Some(text), None) => Ok(text.as_bytes().to_vec()),
        (None, Some(path)) => {
            validate_read_file(path)?;
            fs::read(path).with_context(|| format!("读取文件失败: {path}"))
        }
        (Some(_), Some(_)) => bail!("请只指定 --input 或 --file 之一"),
        (None, None) => bail!("请指定 --input 或 --file"),
    }
}

fn read_text(input: Option<&str>, file: Option<&str>) -> Result<String> {
    match (input, file) {
        (Some(text), None) => Ok(text.to_string()),
        (None, Some(path)) => {
            validate_read_file(path)?;
            fs::read_to_string(path).with_context(|| format!("读取文件失败: {path}"))
        }
        (Some(_), Some(_)) => bail!("请只指定 --input 或 --file 之一"),
        (None, None) => bail!("请指定 --input 或 --file"),
    }
}

fn write_if_set(path: Option<&str>, bytes: &[u8]) -> Result<()> {
    if let Some(path) = path {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("创建输出目录失败: {}", parent.display()))?;
            }
        }
        fs::write(path, bytes).with_context(|| format!("写入文件失败: {path}"))?;
    }
    Ok(())
}
