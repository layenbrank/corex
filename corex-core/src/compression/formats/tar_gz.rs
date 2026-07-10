use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use flate2::Compression;
use flate2::write::GzEncoder;
use tar::{Archive, Builder, Header};

use crate::compression::formats::collect::collect_files;
use crate::compression::schema::{TarGzDecompressArgs, TarGzFormatArgs};

pub fn compress_tar_gz(args: &TarGzFormatArgs) -> Result<()> {
    if args.io.password.is_some() {
        bail!("tar.gz 不支持 password，请使用 Zip 或 SevenZ");
    }

    let from = Path::new(&args.from);
    let to = Path::new(&args.to);
    let files = collect_files(from, &args.io.includes, &args.io.excludes)?;
    if files.is_empty() {
        println!("没有文件需要压缩");
        return Ok(());
    }

    if let Some(parent) = to.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建输出目录: {}", parent.display()))?;
        }
    }

    let file = File::create(to).with_context(|| format!("创建输出文件: {}", to.display()))?;
    let level = args.level.min(9);
    let enc = GzEncoder::new(file, Compression::new(level));
    let mut builder = Builder::new(enc);

    for (rel, abs) in &files {
        let mut header = Header::new_gnu();
        let data = fs::read(abs).with_context(|| format!("读取文件: {}", abs.display()))?;
        header.set_size(data.len() as u64);
        let mode = file_mode(abs, args.preserve_permissions);
        header.set_mode(mode);
        header.set_cksum();
        builder
            .append_data(&mut header, rel.to_string_lossy().as_ref(), &data[..])
            .with_context(|| format!("写入 tar 条目: {}", rel.display()))?;
    }

    builder
        .into_inner()
        .context("完成 tar.gz 写入失败")?
        .finish()
        .context("完成 gzip 写入失败")?;
    println!("已打包 {} 个文件到 {}", files.len(), to.display());
    Ok(())
}

pub fn decompress_tar_gz(args: &TarGzDecompressArgs) -> Result<()> {
    if args.io.password.is_some() {
        bail!("tar.gz 不支持 password，请使用 Zip 或 SevenZ");
    }

    let from = Path::new(&args.from);
    let to = Path::new(&args.to);
    if !from.is_file() {
        bail!("源路径必须是 tar.gz 文件: {}", from.display());
    }

    fs::create_dir_all(to).with_context(|| format!("创建输出目录: {}", to.display()))?;

    let file = File::open(from).with_context(|| format!("打开 tar.gz: {}", from.display()))?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut archive = Archive::new(dec);

    for entry in archive.entries().context("读取 tar 条目失败")? {
        let mut entry = entry.context("读取 tar 条目失败")?;
        let path = entry.path().context("tar 路径无效")?;
        let out_path = to.join(&path);

        if out_path.exists() && !args.io.overwrite {
            bail!("目标已存在且 overwrite=false: {}", out_path.display());
        }

        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&out_path)
                .with_context(|| format!("创建文件: {}", out_path.display()))?;
            io_copy_entry(&mut entry, &mut out)?;
        }
    }

    println!("已解压到 {}", to.display());
    Ok(())
}

fn io_copy_entry<R: Read + ?Sized, W: Write>(reader: &mut R, writer: &mut W) -> Result<()> {
    std::io::copy(reader, writer).context("写入解压数据失败")?;
    Ok(())
}

fn file_mode(abs: &Path, preserve: bool) -> u32 {
    if !preserve {
        return 0o644;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return abs
            .metadata()
            .map(|m| m.permissions().mode() as u32)
            .unwrap_or(0o644);
    }
    #[cfg(not(unix))]
    {
        let _ = abs;
        0o644
    }
}
