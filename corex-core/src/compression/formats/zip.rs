use std::fs::{self, File};
use std::io::copy as io_copy;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use zip::write::FileOptions;
use zip::{AesMode, CompressionMethod, ZipArchive, ZipWriter};

use crate::compression::formats::collect::collect_dir_entries;
use crate::compression::schema::{ZipDecompressArgs, ZipEncryption, ZipFormatArgs, ZipMethod};
use crate::utils::{file, progress};

pub fn compress_zip(args: &ZipFormatArgs) -> Result<()> {
    validate_zip_compress(args)?;

    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    let entries = collect_dir_entries(from, &args.io.includes, &args.io.excludes)?;
    if entries.is_empty() {
        println!("没有文件需要压缩");
        return Ok(());
    }

    if let Some(parent) = to.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建输出目录: {}", parent.display()))?;
        }
    }

    let file_count = entries.len();
    println!("找到 {} 个文件", file_count);

    let pb = progress::progress(file_count as u64);
    pb.set_message("正在压缩 ZIP...");
    pb.tick();
    let start = Instant::now();
    let mut total_bytes: u64 = 0;

    let output_file =
        File::create(to).with_context(|| format!("创建输出文件: {}", to.display()))?;
    let mut zip = ZipWriter::new(output_file);

    for entry in &entries {
        let path = entry.path();
        if let Some(name) = path.file_name() {
            pb.set_message(file::truncate(&name.to_string_lossy(), 30));
        }

        let rel_path = path
            .strip_prefix(from)
            .map_err(|_| anyhow::anyhow!("路径计算失败: {}", path.display()))?;
        let zip_path = rel_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);
        total_bytes += file_size;

        let mut options: FileOptions<'_, ()> = FileOptions::default()
            .compression_method(zip_method(&args.method))
            .compression_level(Some(args.level as i64));

        if args.encryption != ZipEncryption::None {
            let password = args
                .io
                .password
                .as_deref()
                .context("encryption 需要 password")?;
            let mode = match args.encryption {
                ZipEncryption::Aes128 => AesMode::Aes128,
                ZipEncryption::Aes256 => AesMode::Aes256,
                ZipEncryption::None => unreachable!(),
            };
            options = options.with_aes_encryption(mode, password);
        }

        zip.start_file(&zip_path, options)?;
        let mut file = File::open(path).with_context(|| format!("打开文件: {}", path.display()))?;
        io_copy(&mut file, &mut zip).with_context(|| format!("压缩文件: {}", path.display()))?;
        pb.inc(1);
    }

    zip.finish()?;

    let elapsed = start.elapsed();
    pb.finish_with_message(format!(
        "完成 {} 个文件, {}, 用时 {}",
        file_count,
        file::size(total_bytes),
        file::duration(elapsed)
    ));
    Ok(())
}

pub fn decompress_zip(args: &ZipDecompressArgs) -> Result<()> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    if !from.is_file() {
        bail!("源路径必须是 ZIP 文件: {}", from.display());
    }

    fs::create_dir_all(to).with_context(|| format!("创建输出目录: {}", to.display()))?;

    let file = File::open(from).with_context(|| format!("打开 ZIP 文件: {}", from.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("读取 ZIP 文件: {}", from.display()))?;

    let file_count = archive.len();
    if file_count == 0 {
        println!("压缩包为空");
        return Ok(());
    }

    println!("找到 {} 个文件", file_count);
    let pb = progress::progress(file_count as u64);
    pb.set_message("正在解压 ZIP...");
    pb.tick();
    let start = Instant::now();
    let mut total_bytes: u64 = 0;
    let password = args.io.password.as_deref();

    for i in 0..file_count {
        let mut entry = if let Some(pw) = password {
            archive
                .by_index_decrypt(i, pw.as_bytes())
                .with_context(|| format!("解密/读取条目 {i} 失败"))?
        } else {
            archive
                .by_index(i)
                .with_context(|| format!("读取条目 {i} 失败"))?
        };

        let name = entry.name().to_string();
        pb.set_message(file::truncate(&name, 30));

        let Some(safe_name) = entry.enclosed_name() else {
            pb.inc(1);
            continue;
        };
        let out_path = to.join(safe_name);

        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if out_path.exists() && !args.io.overwrite {
                bail!("目标已存在且 overwrite=false: {}", out_path.display());
            }
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)
                .with_context(|| format!("创建文件: {}", out_path.display()))?;
            let bytes = io_copy(&mut entry, &mut out_file)
                .with_context(|| format!("解压文件: {}", name))?;
            total_bytes += bytes;
            let _ = args.io.preserve_timestamps;
        }
        pb.inc(1);
    }

    let elapsed = start.elapsed();
    pb.finish_with_message(format!(
        "解压 {} 个文件, {}, 用时 {}",
        file_count,
        file::size(total_bytes),
        file::duration(elapsed)
    ));
    Ok(())
}

fn validate_zip_compress(args: &ZipFormatArgs) -> Result<()> {
    if args.encryption != ZipEncryption::None && args.io.password.is_none() {
        bail!("Zip encryption 需要 password");
    }
    if args.level == 0 || args.level > 9 {
        bail!("Zip level 必须在 1-9 之间");
    }
    Ok(())
}

fn zip_method(method: &ZipMethod) -> CompressionMethod {
    match method {
        ZipMethod::Deflated => CompressionMethod::Deflated,
        ZipMethod::Stored => CompressionMethod::Stored,
        ZipMethod::Bzip2 => CompressionMethod::Bzip2,
        ZipMethod::Zstd => CompressionMethod::Zstd,
    }
}
