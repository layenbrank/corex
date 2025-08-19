use crate::copy::controller::CopyArgs;
use anyhow::{Context, Result};
use glob::Pattern;
use indicatif::{ProgressBar, ProgressStyle};
use std::{fs, path::Path, sync::Arc, time::Instant};
use walkdir::WalkDir;

// 不能在常量中直接使用 Vec，因为 Vec 的分配是在堆上完成的，
// 而常量要求所有内容在编译期就确定且存储在只读内存中。
// 你可以使用数组（如 IGNORES_VEC），在需要 Vec 时再转换：

// pub fn run(source: &Path, target: &Path, empty: bool, ignores: Vec<String>) -> Result<()> {
pub fn run(args: &CopyArgs) -> Result<()> {
    let (from, to) = (Path::new(&args.from), Path::new(&args.to));
    let patterns = compile_patterns(&args.ignore);

    let (count, size) = scan(&from, &patterns)?;

    if count == 0 {
        println!("📂 没有文件需要复制");
        return Ok(());
    }

    println!("📊 找到 {} 个文件，总大小: {}", count, format_size(size));

    let progress = progress_bar(count);
    copy(&from, &to, args.empty, &patterns, progress)
}

/// 编译 glob 模式
fn compile_patterns(patterns: &[String]) -> Vec<Pattern> {
    patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect()
}

fn scan(from: &Path, patterns: &[Pattern]) -> Result<(u64, u64)> {
    let spinner = spinner("正在扫描文件...");
    let resp = calc_size(from, patterns);
    spinner.finish_and_clear();
    resp
}

fn spinner(msg: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );

    spinner.set_message(msg.to_string());
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    spinner
}

fn progress_bar(total: u64) -> Arc<ProgressBar> {
    let progress = ProgressBar::new(total);
    progress.set_style(
        ProgressStyle::default_bar()
            .template(&format!(
                "{}\n{} 📁 [{}] {}/{} ({}%) | ⏱️  {} | 🚀 {}",
                "{msg}",
                "{spinner:.green}",
                "{bar:40.cyan/blue}",
                "{pos:>7}",
                "{len:7}",
                "{percent:>3}",
                "{elapsed_precise}",
                "{eta_precise}"
            ))
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  ")
            .tick_strings(&["🔄", "🔃", "⚡", "✨", "💫", "⭐"]),
    );
    progress.enable_steady_tick(std::time::Duration::from_millis(120));
    Arc::new(progress)
}

/// 更新进度显示
fn update_progress(source: &Path, stats: &CopyStats, progress: &ProgressBar, start: Instant) {
    if let Some(filename) = source.file_name() {
        let name = truncate(&filename.to_string_lossy(), 30);
        let speed = calc_speed(stats.bytes, start.elapsed());

        progress.set_message(format!(
            "📄 {} | ⏱️ {} | 🚀 {}",
            name,
            format_duration(start.elapsed()),
            format!("{}/s", format_size(speed)) // 格式化传输速度
        ));
    }
}

/// 完成复制操作
fn finish(progress: &ProgressBar, stats: &CopyStats, start_time: Instant) {
    let elapsed = start_time.elapsed();
    let avg_speed = calc_speed(stats.bytes, elapsed);

    progress.finish_with_message(format!(
        "✅ 完成 {} 各文件, {}, 用时 {}, 平均 {}",
        stats.files,
        format_size(stats.bytes),
        format_duration(elapsed),
        format!("{}/s", format_size(avg_speed))
    ));
}

/// 格式化持续时间
fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{:.1}s", duration.as_secs_f64())
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn copy(
    from: &Path,
    to: &Path,
    empty: bool,
    patterns: &[Pattern],
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

        if is_ignored(relative, patterns) {
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

/// 计算传输速度
fn calc_speed(bytes: u64, elapsed: std::time::Duration) -> u64 {
    let seconds = elapsed.as_secs();
    if seconds > 0 { bytes / seconds } else { 0 }
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

/// 检查路径是否被忽略
fn is_ignored(path: &Path, patterns: &[Pattern]) -> bool {
    let path_str = path.to_string_lossy();

    // 检查完整路径
    if patterns.iter().any(|p| p.matches(&path_str)) {
        return true;
    }

    // 检查文件名
    if let Some(filename) = path.file_name() {
        let filename_str = filename.to_string_lossy();
        if patterns.iter().any(|p| p.matches(&filename_str)) {
            return true;
        }
    }

    // 检查路径组件
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .any(|component| patterns.iter().any(|p| p.matches(&component)))
}

/// 计算需要复制的文件数量和总大小
fn calc_size(from: &Path, patterns: &[Pattern]) -> Result<(u64, u64)> {
    let (mut count, mut size) = (0u64, 0u64);

    let entries = WalkDir::new(from).into_iter().filter_map(|e| e.ok());

    for entry in entries {
        let path = entry.path();

        if let Ok(relative) = path.strip_prefix(from) {
            if !is_ignored(relative, patterns) && path.is_file() {
                count += 1;
                size += fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    anyhow::Ok((count, size))
}

/// 格式化文件大小
fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// 安全地截断文件名，避免 Unicode 字符边界问题
fn truncate(string: &str, max_len: usize) -> String {
    if string.len() <= max_len {
        return string.to_string();
    }

    // 使用字符迭代器来安全处理 Unicode
    let chars: Vec<char> = string.chars().collect();
    if chars.len() <= max_len {
        return string.to_string();
    }

    let prefix_len = max_len.saturating_sub(3) / 2;
    let suffix_len = max_len.saturating_sub(3).saturating_sub(prefix_len);

    if prefix_len + suffix_len + 3 > chars.len() {
        return string.to_string();
    }

    format!(
        "{}...{}",
        chars[..prefix_len].iter().collect::<String>(),
        chars[chars.len().saturating_sub(suffix_len)..]
            .iter()
            .collect::<String>()
    )
}
