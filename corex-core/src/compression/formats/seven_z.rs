use std::path::Path;

use anyhow::{Context, Result, bail};
use sevenz_rust2::{
    Password, compress_to_path, compress_to_path_encrypted, decompress_file,
    decompress_file_with_password,
};

use crate::compression::schema::{SevenZDecompressArgs, SevenZFormatArgs};

pub fn compress_seven_z(args: &SevenZFormatArgs) -> Result<()> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    if !from.is_dir() {
        bail!("源路径必须是目录: {}", from.display());
    }
    if args.io.includes.iter().any(|s| !s.is_empty())
        || args.io.excludes.iter().any(|s| !s.is_empty())
    {
        eprintln!("⚠️  SevenZ 暂不支持 includes/excludes 过滤，将打包整个目录");
    }

    if let Some(pw) = args.io.password.as_deref() {
        compress_to_path_encrypted(from, to, Password::from(pw))
            .with_context(|| format!("7z 加密压缩失败: {} -> {}", from.display(), to.display()))?;
    } else {
        compress_to_path(from, to)
            .with_context(|| format!("7z 压缩失败: {} -> {}", from.display(), to.display()))?;
    }

    let _ = (args.level, args.solid, args.encrypt_header);
    println!("已压缩到 {}", to.display());
    Ok(())
}

pub fn decompress_seven_z(args: &SevenZDecompressArgs) -> Result<()> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    if !from.is_file() {
        bail!("源路径必须是 7z 文件: {}", from.display());
    }

    if to.exists() && !args.io.overwrite {
        bail!("目标目录已存在且 overwrite=false: {}", to.display());
    }

    if let Some(pw) = args.io.password.as_deref() {
        decompress_file_with_password(from, to, Password::from(pw))
            .with_context(|| format!("7z 解密解压失败: {}", from.display()))?;
    } else {
        decompress_file(from, to).with_context(|| format!("7z 解压失败: {}", from.display()))?;
    }

    println!("已解压到 {}", to.display());
    Ok(())
}
