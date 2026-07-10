use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

/// compression 顶层：compress / decompress
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 压缩
    Compress(CompressArgs),
    /// 解压
    Decompress(DecompressArgs),
}

/// 压缩：先选格式 scheme
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct CompressArgs {
    #[command(subcommand)]
    pub scheme: CompressScheme,
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum CompressScheme {
    Zip(ZipFormatArgs),
    TarGz(TarGzFormatArgs),
    SevenZ(SevenZFormatArgs),
}

/// 解压：先选格式 scheme
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct DecompressArgs {
    #[command(subcommand)]
    pub scheme: DecompressScheme,
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum DecompressScheme {
    Zip(ZipDecompressArgs),
    TarGz(TarGzDecompressArgs),
    SevenZ(SevenZDecompressArgs),
}

/// 归档 IO 共用字段（压缩/解压）
#[derive(Debug, Parser, Clone, Serialize, Deserialize, Default)]
pub struct ArchiveIoArgs {
    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub includes: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    #[serde(default)]
    pub excludes: Vec<String>,
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub password: Option<String>,
    #[arg(long, default_value_t = false)]
    #[serde(default)]
    pub overwrite: bool,
    #[arg(long, default_value_t = true)]
    #[serde(default = "default_preserve_timestamps")]
    pub preserve_timestamps: bool,
}

fn default_preserve_timestamps() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_zip_level() -> u8 {
    6
}

fn default_tar_level() -> u32 {
    6
}

fn default_seven_level() -> u32 {
    5
}

/// ZIP 压缩参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ZipFormatArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,
    #[arg(short, long)]
    pub to: String,
    #[arg(long, default_value_t = default_zip_level())]
    #[serde(default = "default_zip_level")]
    pub level: u8,
    #[arg(long, default_value = "deflated", value_enum)]
    #[serde(default)]
    pub method: ZipMethod,
    #[arg(long, default_value = "none", value_enum)]
    #[serde(default)]
    pub encryption: ZipEncryption,
    #[command(flatten)]
    #[serde(flatten, default)]
    pub io: ArchiveIoArgs,
    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}

/// ZIP 解压参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ZipDecompressArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,
    #[arg(short, long)]
    pub to: String,
    #[command(flatten)]
    #[serde(flatten)]
    pub io: ArchiveIoArgs,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}

/// tar.gz 压缩参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct TarGzFormatArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,
    #[arg(short, long)]
    pub to: String,
    #[arg(long, default_value_t = default_tar_level())]
    #[serde(default = "default_tar_level")]
    pub level: u32,
    #[arg(long, default_value_t = true)]
    #[serde(default = "default_true")]
    pub preserve_permissions: bool,
    #[command(flatten)]
    #[serde(flatten)]
    pub io: ArchiveIoArgs,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}

/// tar.gz 解压参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct TarGzDecompressArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,
    #[arg(short, long)]
    pub to: String,
    #[command(flatten)]
    #[serde(flatten)]
    pub io: ArchiveIoArgs,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}

/// 7z 压缩参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct SevenZFormatArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,
    #[arg(short, long)]
    pub to: String,
    #[arg(long, default_value_t = default_seven_level())]
    #[serde(default = "default_seven_level")]
    pub level: u32,
    #[arg(long, default_value_t = true)]
    #[serde(default = "default_true")]
    pub solid: bool,
    #[arg(long, default_value_t = true)]
    #[serde(default = "default_true")]
    pub encrypt_header: bool,
    #[command(flatten)]
    #[serde(flatten)]
    pub io: ArchiveIoArgs,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}

/// 7z 解压参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct SevenZDecompressArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,
    #[arg(short, long)]
    pub to: String,
    #[command(flatten)]
    #[serde(flatten)]
    pub io: ArchiveIoArgs,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ZipMethod {
    #[default]
    Deflated,
    Stored,
    Bzip2,
    Zstd,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ZipEncryption {
    #[default]
    None,
    Aes128,
    Aes256,
}

impl Args {
    /// 输出路径（Pipeline / IPC path 字段）
    pub fn output_path(&self) -> Option<String> {
        match self {
            Args::Compress(a) => Some(match &a.scheme {
                CompressScheme::Zip(z) => z.to.clone(),
                CompressScheme::TarGz(t) => t.to.clone(),
                CompressScheme::SevenZ(s) => s.to.clone(),
            }),
            Args::Decompress(a) => Some(match &a.scheme {
                DecompressScheme::Zip(z) => z.to.clone(),
                DecompressScheme::TarGz(t) => t.to.clone(),
                DecompressScheme::SevenZ(s) => s.to.clone(),
            }),
        }
    }
}
