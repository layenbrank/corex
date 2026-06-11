use std::{fs, path::Path, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use indicatif::ProgressBar;
use walkdir::WalkDir;

use crate::copy::schema::Args;
use crate::utils::{file, ignore::Filter, notify, progress};

pub fn run(args: &Args) -> Result<()> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);

    if from.is_file() {
        copy_single_file(from, to)
    } else if from.is_dir() {
        copy_directory(from, to, args)
    } else {
        anyhow::bail!("源路径不存在或不是有效的文件/目录: {}", args.from)
    }
}

// ─── 单文件复制（支持重命名）────────────────────────────────────────────────

fn copy_single_file(from: &Path, to: &Path) -> Result<()> {
    // 如果 to 是已有目录，则把文件放入该目录（保持原名）
    let target = if to.is_dir() {
        to.join(from.file_name().unwrap_or_default())
    } else {
        // to 是文件路径（可能重命名），确保父目录存在
        if let Some(parent) = to.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建目标目录失败: {}", parent.display()))?;
        }
        to.to_path_buf()
    };

    let size = fs::metadata(from)?.len();

    println!("📄 复制文件: {} → {}", from.display(), target.display());

    fs::copy(from, &target).with_context(|| format!("复制文件失败: {:?} -> {:?}", from, target))?;

    let _ = notify::success("复制成功", "文件复制操作已成功完成");
    println!("✅ 完成，大小: {}", file::size(size));
    Ok(())
}

// ─── 目录复制 ────────────────────────────────────────────────────────────────

fn copy_directory(from: &Path, to: &Path, args: &Args) -> Result<()> {
    let filter = Filter::new(&args.includes, &args.excludes);

    let (count, size) = scan(from, &filter).context("扫描失败")?;

    if count == 0 {
        println!("📂 没有文件需要复制");
        return Ok(());
    }

    println!("📊 找到 {} 个文件，总大小: {}", count, file::size(size));

    let pb = progress::progress(count);
    let status = copy_dir(from, to, args.empty, &filter, pb);

    match &status {
        Ok(_) => {
            let _ = notify::success("复制成功", "文件复制操作已成功完成");
        }
        Err(e) => {
            let _ = notify::error("文件复制失败", &format!("复制过程中发生错误: {}", e));
        }
    }
    status
}

fn scan(from: &Path, filter: &Filter) -> Result<(u64, u64)> {
    let spinner = progress::spinner("正在扫描文件...");
    let resp = calc_size(from, filter);
    spinner.finish_and_clear();
    resp
}

/// 更新进度显示
fn update_progress(source: &Path, stats: &CopyStats, progress: &ProgressBar, start: Instant) {
    if let Some(filename) = source.file_name() {
        let name = file::truncate(&filename.to_string_lossy(), 30);
        let speed = file::speed(stats.bytes, start.elapsed());

        progress.set_message(format!(
            "⏱️ {} | 🚀 {}/s | 📄 {}",
            file::duration(start.elapsed()),
            file::size(speed),
            name,
        ));
    }
}

/// 完成复制操作
fn finish(progress: &ProgressBar, stats: &CopyStats, start_time: Instant) {
    let elapsed = start_time.elapsed();
    let avg_speed = file::speed(stats.bytes, elapsed);

    progress.finish_with_message(format!(
        "✅ 完成 {} 个文件, {}, 用时 {}, 平均 {}/s",
        stats.files,
        file::size(stats.bytes),
        file::duration(elapsed),
        file::size(avg_speed)
    ));
}

fn copy_dir(
    from: &Path,
    to: &Path,
    empty: bool,
    filter: &Filter,
    progress: Arc<ProgressBar>,
) -> Result<()> {
    ensure_dir(to)?;

    if empty {
        progress.set_message("清空目标目录...");
        empty_dir(to)?;
    }

    let mut stats = CopyStats::new();
    let start = Instant::now();

    let entries = WalkDir::new(from).into_iter().filter_map(Result::ok);

    for entry in entries {
        let source = entry.path();
        let relative = source.strip_prefix(from).context("路径解析失败")?;
        let target = to.join(relative);

        if filter.is_filtered(relative) {
            continue;
        }

        if source.is_dir() {
            ensure_dir(&target)?;
        } else if source.is_file() {
            copy_file(source, &target, &mut stats, &progress, start)?;
        }
    }

    finish(&progress, &stats, start);
    Ok(())
}

/// 复制单个文件
fn copy_file(
    source: &Path,
    target: &Path,
    stats: &mut CopyStats,
    progress: &ProgressBar,
    start: Instant,
) -> Result<()> {
    if let Some(parent) = target.parent() {
        ensure_dir(parent)?;
    }

    let size = fs::metadata(source)?.len();

    update_progress(source, stats, progress, start);

    fs::copy(source, target).context(format!("复制文件失败: {:?} -> {:?}", source, target))?;

    stats.add(size);
    progress.set_position(stats.files);

    Ok(())
}

/// 复制统计信息
#[derive(Default)]
struct CopyStats {
    files: u64,
    bytes: u64,
}

impl CopyStats {
    fn new() -> Self {
        Self::default()
    }

    fn add(&mut self, size: u64) {
        self.files += 1;
        self.bytes += size;
    }
}

fn ensure_dir(to: &Path) -> Result<()> {
    if !to.exists() {
        fs::create_dir_all(to).context("创建目标目录失败")?;
    }
    Ok(())
}

/// 清空目录内容
fn empty_dir(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// 计算需要复制的文件数量和总大小
fn calc_size(from: &Path, filter: &Filter) -> Result<(u64, u64)> {
    let (mut count, mut size) = (0u64, 0u64);

    let entries = WalkDir::new(from).into_iter().filter_map(|e| e.ok());

    for entry in entries {
        let path = entry.path();

        if let Ok(relative) = path.strip_prefix(from)
            && !filter.is_filtered(relative)
            && path.is_file()
        {
            count += 1;
            size += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        }
    }

    anyhow::Ok((count, size))
}
