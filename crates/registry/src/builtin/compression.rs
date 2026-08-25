//! Compression actions: zip / tar.gz / 7z compress & decompress.

use crate::builtin::filter::Filter;
use crate::builtin::util::{
    ensure_parent, opt_i64, opt_str, require_map, require_path,
};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::copy as io_copy;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tar::{Archive, Builder, Header};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub struct CompressionCompress;
pub struct CompressionDecompress;

#[async_trait]
impl Action for CompressionCompress {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "compression.compress",
            "Compress",
            "压缩目录/文件为 zip、tar.gz 或 7z",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("format", SchemaType::Str, false)
                .with_default("zip")
                .with_description("zip | tar.gz | 7z"),
            ParamSchema::new("level", SchemaType::Int, false).with_default(6),
            ParamSchema::new("includes", SchemaType::List, false),
            ParamSchema::new("excludes", SchemaType::List, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = require_path(map, "from")?;
        let to = require_path(map, "to")?;
        let format = opt_str(map, "format").unwrap_or_else(|| "zip".into());
        let level = opt_i64(map, "level", 6) as u32;
        let includes = crate::builtin::util::opt_str_list(map, "includes");
        let excludes = crate::builtin::util::opt_str_list(map, "excludes");
        ensure_parent(&to)?;

        match format.to_lowercase().as_str() {
            "zip" => compress_zip(&from, &to, level as i64, &includes, &excludes)?,
            "tar.gz" | "tgz" | "targz" => {
                compress_tar_gz(&from, &to, level.min(9), &includes, &excludes)?
            }
            "7z" | "sevenz" => {
                return Err(ActionError::execution(
                    "7z 压缩未在此构建中启用；请使用 zip 或 tar.gz",
                ))
            }
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的压缩格式: {other}"
                )))
            }
        }
        Ok(Value::File(to))
    }
}

#[async_trait]
impl Action for CompressionDecompress {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "compression.decompress",
            "Decompress",
            "解压 zip、tar.gz 或 7z",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("format", SchemaType::Str, false)
                .with_description("可选；缺省时按扩展名推断"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = require_path(map, "from")?;
        let to = require_path(map, "to")?;
        let format = opt_str(map, "format").unwrap_or_else(|| infer_format(&from));
        std::fs::create_dir_all(&to)?;

        match format.to_lowercase().as_str() {
            "zip" => decompress_zip(&from, &to)?,
            "tar.gz" | "tgz" | "targz" => decompress_tar_gz(&from, &to)?,
            "7z" | "sevenz" => {
                return Err(ActionError::execution(
                    "7z 解压未在此构建中启用；请使用 zip 或 tar.gz",
                ))
            }
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的解压格式: {other}"
                )))
            }
        }
        Ok(Value::File(to))
    }
}

fn infer_format(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let lower = name.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        "tar.gz".into()
    } else if lower.ends_with(".7z") {
        "7z".into()
    } else {
        "zip".into()
    }
}

fn collect_files(
    from: &Path,
    includes: &[String],
    excludes: &[String],
) -> Result<Vec<(PathBuf, PathBuf)>, ActionError> {
    let filter = Filter::new(includes, excludes);
    if from.is_file() {
        let name = from.file_name().unwrap_or_default();
        return Ok(vec![(PathBuf::from(name), from.to_path_buf())]);
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(from).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(from)
            .map_err(|e| ActionError::execution(e.to_string()))?;
        if filter.is_filtered(rel) {
            continue;
        }
        out.push((rel.to_path_buf(), path.to_path_buf()));
    }
    Ok(out)
}

fn compress_zip(
    from: &Path,
    to: &Path,
    level: i64,
    includes: &[String],
    excludes: &[String],
) -> Result<(), ActionError> {
    let entries = collect_files(from, includes, excludes)?;
    if entries.is_empty() {
        return Err(ActionError::execution("没有文件需要压缩"));
    }
    let file = File::create(to)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(level.clamp(0, 9)));
    for (rel, abs) in &entries {
        let name = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        zip.start_file(&name, options)
            .map_err(|e| ActionError::execution(format!("zip start_file: {e}")))?;
        let mut f = File::open(abs)?;
        io_copy(&mut f, &mut zip)?;
    }
    zip.finish()
        .map_err(|e| ActionError::execution(format!("zip finish: {e}")))?;
    Ok(())
}

fn decompress_zip(from: &Path, to: &Path) -> Result<(), ActionError> {
    let file = File::open(from)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| ActionError::execution(format!("打开 zip: {e}")))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| ActionError::execution(format!("读取 zip 条目: {e}")))?;
        let outpath = match file.enclosed_name() {
            Some(p) => to.join(p),
            None => continue,
        };
        if file.name().ends_with('/') {
            std::fs::create_dir_all(&outpath)?;
        } else {
            ensure_parent(&outpath)?;
            let mut outfile = File::create(&outpath)?;
            io_copy(&mut file, &mut outfile)?;
        }
    }
    Ok(())
}

fn compress_tar_gz(
    from: &Path,
    to: &Path,
    level: u32,
    includes: &[String],
    excludes: &[String],
) -> Result<(), ActionError> {
    let files = collect_files(from, includes, excludes)?;
    if files.is_empty() {
        return Err(ActionError::execution("没有文件需要压缩"));
    }
    let file = File::create(to)?;
    let enc = GzEncoder::new(file, Compression::new(level));
    let mut builder = Builder::new(enc);
    for (rel, abs) in &files {
        let mut header = Header::new_gnu();
        let data = std::fs::read(abs)?;
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, rel.to_string_lossy().as_ref(), &data[..])
            .map_err(|e| ActionError::execution(format!("写入 tar: {e}")))?;
    }
    builder
        .into_inner()
        .map_err(|e| ActionError::execution(format!("完成 tar: {e}")))?
        .finish()
        .map_err(|e| ActionError::execution(format!("完成 gzip: {e}")))?;
    Ok(())
}

fn decompress_tar_gz(from: &Path, to: &Path) -> Result<(), ActionError> {
    let file = File::open(from)?;
    let dec = GzDecoder::new(file);
    let mut archive = Archive::new(dec);
    archive
        .unpack(to)
        .map_err(|e| ActionError::execution(format!("解压 tar.gz: {e}")))?;
    Ok(())
}




pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(CompressionCompress));
    registry.register(Arc::new(CompressionDecompress));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;
    use tempfile::tempdir;

    #[tokio::test]
    async fn zip_roundtrip() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        let zip_path = dir.path().join("out.zip");
        let out_dir = dir.path().join("out");

        let mut params = BTreeMap::new();
        params.insert("from".into(), Value::Str(src.to_string_lossy().into()));
        params.insert("to".into(), Value::Str(zip_path.to_string_lossy().into()));
        params.insert("format".into(), Value::Str("zip".into()));
        let mut ctx = ExecutionContext::default();
        CompressionCompress
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(zip_path.exists());

        let mut params = BTreeMap::new();
        params.insert("from".into(), Value::Str(zip_path.to_string_lossy().into()));
        params.insert("to".into(), Value::Str(out_dir.to_string_lossy().into()));
        CompressionDecompress
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert_eq!(std::fs::read_to_string(out_dir.join("a.txt")).unwrap(), "hello");
    }
}
