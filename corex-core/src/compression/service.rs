use std::{
    fs::{File, create_dir_all},
    io::copy as io_copy,
    path::Path,
    time::Instant,
};

use anyhow::{Context, Result};
use walkdir::{DirEntry, WalkDir};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::FileOptions};

use crate::compression::schema::{Args, UnzipArgs, ZipArgs};
use crate::utils::{file, notify, progress};

pub fn run(args: &Args) -> Result<()> {
    match args {
        Args::Zip(a) => match zip_task(a) {
            Ok(_) => {
                let _ = notify::success("压缩成功", "压缩操作已成功完成");
            }
            Err(e) => {
                let _ = notify::error("压缩失败", &format!("压缩过程中发生错误: {}", e));
                return Err(e);
            }
        },
        Args::Unzip(a) => match unzip_task(a) {
            Ok(_) => {
                let _ = notify::success("解压成功", "解压操作已成功完成");
            }
            Err(e) => {
                let _ = notify::error("解压失败", &format!("解压过程中发生错误: {}", e));
                return Err(e);
            }
        },
    }
    Ok(())
}

// ─── ZIP 压缩 ────────────────────────────────────────────────────────────────

/// 纯压缩：将 `from` 目录下所有文件打包为 ZIP 写入 `to`
pub fn zip_task(args: &ZipArgs) -> Result<()> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    if !from.is_dir() {
        anyhow::bail!("源路径必须是目录: {}", from.display());
    }

    // 确保输出目录存在
    if let Some(parent) = to.parent() {
        create_dir_all(parent).with_context(|| format!("创建输出目录: {}", to.display()))?;
    }

    // 预扫描文件列表（只遍历一次）
    let spinner = progress::spinner("正在扫描文件...");
    let entries: Vec<DirEntry> = WalkDir::new(from)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();
    spinner.finish_and_clear();

    let file_count = entries.len();
    if file_count == 0 {
        println!("没有文件需要压缩");
        return Ok(());
    }

    println!("找到 {} 个文件", file_count);

    let pb = progress::progress(file_count as u64);
    pb.set_message("正在压缩...");
    pb.tick(); // 强制初始渲染
    let start = Instant::now();
    let mut total_bytes: u64 = 0;

    let output_file =
        File::create(to).with_context(|| format!("创建输出文件: {}", to.display()))?;
    let mut zip = ZipWriter::new(output_file);
    let options: FileOptions<()> = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));

    for entry in &entries {
        let path = entry.path();

        // 更新进度条显示当前文件
        if let Some(name) = path.file_name() {
            pb.set_message(file::truncate(&name.to_string_lossy(), 30));
        }

        // 计算相对路径（ZIP 内路径）
        let rel_path = path
            .strip_prefix(from)
            .map_err(|_| anyhow::anyhow!("路径计算失败: {}", path.display()))?;

        // Windows 路径分隔符统一为 '/'（ZIP 规范要求）
        let zip_path = rel_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        let file_size = path.metadata().map(|m| m.len()).unwrap_or(0);
        total_bytes += file_size;

        zip.start_file(&zip_path, options)?;
        let mut file = File::open(path).with_context(|| format!("打开文件: {}", path.display()))?;
        let _ = std::io::copy(&mut file, &mut zip)
            .with_context(|| format!("压缩文件: {}", path.display()))?;

        pb.inc(1);
    }

    zip.finish()?;

    let elapsed = start.elapsed();
    let avg_speed = file::speed(total_bytes, elapsed);
    pb.finish_with_message(format!(
        "完成 {} 个文件, {}, 用时 {}, 平均 {}/s",
        file_count,
        file::size(total_bytes),
        file::duration(elapsed),
        file::size(avg_speed)
    ));

    Ok(())
}

// ─── ZIP 解压缩 ──────────────────────────────────────────────────────────────

/// 解压 ZIP 文件到目标目录
pub fn unzip_task(args: &UnzipArgs) -> Result<()> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    if !from.is_file() {
        anyhow::bail!("源路径必须是 ZIP 文件: {}", from.display());
    }

    create_dir_all(to).with_context(|| format!("创建输出目录: {}", to.display()))?;

    let file = File::open(from).with_context(|| format!("打开 ZIP 文件: {}", from.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("读取 ZIP 文件: {}", from.display()))?;

    // 预扫描统计文件数
    let spinner = progress::spinner("正在扫描压缩包...");
    let file_count = archive.len();
    spinner.finish_and_clear();

    if file_count == 0 {
        println!("压缩包为空");
        return Ok(());
    }

    println!("找到 {} 个文件", file_count);

    let pb = progress::progress(file_count as u64);
    pb.set_message("正在解压...");
    pb.tick();
    let start = Instant::now();
    let mut total_bytes: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        pb.set_message(file::truncate(&name, 30));

        let out_path = to.join(entry.name());

        if entry.is_dir() {
            create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)
                .with_context(|| format!("创建文件: {}", out_path.display()))?;
            let bytes = io_copy(&mut entry, &mut out_file)
                .with_context(|| format!("解压文件: {}", name))?;
            total_bytes += bytes;
        }

        pb.inc(1);
    }

    let elapsed = start.elapsed();
    let avg_speed = file::speed(total_bytes, elapsed);
    pb.finish_with_message(format!(
        "解压 {} 个文件, {}, 用时 {}, 平均 {}/s",
        file_count,
        file::size(total_bytes),
        file::duration(elapsed),
        file::size(avg_speed)
    ));

    Ok(())
}
