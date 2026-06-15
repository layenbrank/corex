use std::{
    fs,
    path::Path,
    time::{Instant, SystemTime},
};

use anyhow::Context;
use xcap::Monitor;

use crate::screenshot::schema::Args;

pub fn run(args: &Args) -> anyhow::Result<()> {
    let to = Path::new(&args.to);
    let start = Instant::now();

    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // 确保输出目录存在
    fs::create_dir_all(to).with_context(|| format!("创建输出目录失败: {}", to.display()))?;

    // 选择主显示器，否则取第一个
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

    println!("📸 截图时间戳: {}", timestamp);

    image
        .save(&output_path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let duration = start.elapsed();
    println!(
        "✅ 截图已保存: {} (耗时 {:?})",
        output_path.display(),
        duration
    );

    Ok(())
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}
