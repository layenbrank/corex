use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

/// codec 顶层：encode / decode / hash
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 编码
    Encode(EncodeArgs),
    /// 解码
    Decode(DecodeArgs),
    /// 摘要
    Hash(HashArgs),
}

/// 编码：必须先指定算法（目前仅 base64）
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct EncodeArgs {
    #[command(subcommand)]
    pub scheme: EncodeScheme,
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum EncodeScheme {
    Base64(Base64Args),
}

/// 解码：必须先指定算法（目前仅 base64）
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct DecodeArgs {
    #[command(subcommand)]
    pub scheme: DecodeScheme,
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum DecodeScheme {
    Base64(Base64Args),
}

/// 摘要：必须先指定算法（目前仅 md5）
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct HashArgs {
    #[command(subcommand)]
    pub scheme: HashScheme,
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum HashScheme {
    Md5(Md5Args),
}

/// base64 编解码共用参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct Base64Args {
    /// 文本输入（与 --file 二选一）
    #[arg(long)]
    pub input: Option<String>,
    #[arg(long, value_parser = verifier::path)]
    pub file: Option<String>,
    #[arg(long, value_parser = verifier::path)]
    pub output: Option<String>,
}

/// md5 摘要参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct Md5Args {
    #[arg(long)]
    pub input: Option<String>,
    #[arg(long, value_parser = verifier::path)]
    pub file: Option<String>,
    #[arg(long, value_parser = verifier::path)]
    pub output: Option<String>,
}
