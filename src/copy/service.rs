use crate::copy::controller::Args;
use crate::utils::{file::File, ignore::Ignore, notify::Notification, progress::Progress};
use anyhow::{Context, Result};
use indicatif::ProgressBar;
use std::{fs, path::Path, sync::Arc, time::Instant};
use walkdir::WalkDir;

// 不能在常量中直接使用 Vec，因为 Vec 的分配是在堆上完成的，
// 而常量要求所有内容在编译期就确定且存储在只读内存中。
// 你可以使用数组（如 IGNORES_VEC），在需要 Vec 时再转换：

// pub fn run(source: &Path, target: &Path, empty: bool, ignores: Vec<String>) -> Result<()> {
pub fn run(args: &Args) {
    let (from, to) = (Path::new(&args.from), Path::new(&args.to));
    let patterns = Ignore::new(&args.ignores);

    let (count, size) = scan(&from, &patterns).expect("扫描失败");

    if count == 0 {
        return println!("📂 没有文件需要复制");
    }

    println!(
        "📊 找到 {} 个文件，总大小: {}",
        count,
        File::format_size(size)
    );

    let progress = Progress::progress(count);
    let status = copy(&from, &to, args.empty, &patterns, progress);

    match &status {
        Ok(_) => {
            let _ = Notification::success("复制成功", "文件复制操作已成功完成");
        }
        Err(e) => {
            let _ = Notification::error("文件复制失败", &format!("复制过程中发生错误: {}", e));
        }
    }
}

fn scan(from: &Path, patterns: &Ignore) -> Result<(u64, u64)> {
    let spinner = Progress::spinner("正在扫描文件...");
    let resp = calc_size(from, patterns);
    spinner.finish_and_clear();
    resp
}

/// 更新进度显示
fn update_progress(source: &Path, stats: &CopyStats, progress: &ProgressBar, start: Instant) {
    if let Some(filename) = source.file_name() {
        let name = File::truncate(&filename.to_string_lossy(), 30);
        let speed = File::calc_speed(stats.bytes, start.elapsed());

        progress.set_message(format!(
            "⏱️ {} | 🚀 {} | 📄 {}",
            File::format_duration(start.elapsed()),
            format!("{}/s", File::format_size(speed)), // 格式化传输速度
            name,
        ));
    }
}

/// 完成复制操作
fn finish(progress: &ProgressBar, stats: &CopyStats, start_time: Instant) {
    let elapsed = start_time.elapsed();
    let avg_speed = File::calc_speed(stats.bytes, elapsed);

    progress.finish_with_message(format!(
        "✅ 完成 {} 各文件, {}, 用时 {}, 平均 {}",
        stats.files,
        File::format_size(stats.bytes),
        File::format_duration(elapsed),
        format!("{}/s", File::format_size(avg_speed))
    ));
}

fn copy(
    from: &Path,
    to: &Path,
    empty: bool,
    patterns: &Ignore,
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

        if patterns.ignored(relative) {
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
    // 确保父目录存在
    if let Some(parent) = target.parent() {
        ensure_dir(parent)?;
    }

    let size = fs::metadata(source)?.len();

    // 更新进度信息
    update_progress(source, stats, progress, start);

    // 执行复制
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
fn calc_size(from: &Path, patterns: &Ignore) -> Result<(u64, u64)> {
    let (mut count, mut size) = (0u64, 0u64);

    let entries = WalkDir::new(from).into_iter().filter_map(|e| e.ok());

    for entry in entries {
        let path = entry.path();

        if let Ok(relative) = path.strip_prefix(from) {
            if !patterns.ignored(relative) && path.is_file() {
                count += 1;
                size += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    anyhow::Ok((count, size))
}
