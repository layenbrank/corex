use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

use anyhow::Context;
use xcap::Monitor;

use crate::screenshot::schema::Args;

/// 执行截图并返回输出文件路径
pub fn capture(args: &Args, cached_monitors: Option<&[Monitor]>) -> anyhow::Result<PathBuf> {
    let to = Path::new(&args.to);
    let start = Instant::now();

    let owned_monitors;
    let monitors = match cached_monitors {
        Some(monitors) => monitors,
        None => {
            owned_monitors = Monitor::all().map_err(|e| anyhow::anyhow!(e.to_string()))?;
            &owned_monitors
        }
    };

    fs::create_dir_all(to).with_context(|| format!("创建输出目录失败: {}", to.display()))?;

    let target = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or_else(|| anyhow::anyhow!("没有找到可用显示器"))?;

    let image = target
        .capture_image()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .as_millis();

    let monitor_name = normalized(
        target
            .friendly_name()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?,
    );

    let filename = format!("screenshot-{}-{}.png", monitor_name, timestamp);
    let output_path = to.join(&filename);

    image
        .save(&output_path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let duration = start.elapsed();
    eprintln!(
        "screenshot saved: {} ({:?})",
        output_path.display(),
        duration
    );

    Ok(output_path)
}

pub fn run(args: &Args) -> anyhow::Result<()> {
    let output_path = capture(args, None)?;
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .as_millis();

    println!("📸 截图时间戳: {}", timestamp);
    println!("✅ 截图已保存: {}", output_path.display());

    Ok(())
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}
