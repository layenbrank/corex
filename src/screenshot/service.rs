use std::{
    fs,
    path::Path,
    time::{Instant, SystemTime},
};

use serde::Serialize;
use xcap::Monitor;

use crate::screenshot::schema::Args;

/// 显示器信息
#[derive(Serialize, Clone)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

pub fn run(args: &Args) -> anyhow::Result<()> {
    bootstrap(args)
}

fn bootstrap(args: &Args) -> anyhow::Result<()> {
    let to = Path::new(&args.to);

    let start = Instant::now();

    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let infos: Vec<MonitorInfo> = monitors.iter().filter_map(to_monitor_info).collect();

    fs::create_dir_all(to.join("screenshot")).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // 选择主显示器，否则第一个
    let target = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or_else(|| anyhow::anyhow!("没有找到可用显示器"))?;

    let image = target
        .capture_image()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    // 当前时间毫秒级 完整时间戳
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
        .as_millis();

    println!("当前时间时间戳: {}", timestamp);
    image
        .save(format!(
            "screenshot/screenshot-{}-{}.png",
            normalized(
                target
                    .friendly_name()
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?
            ),
            timestamp
        ))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let duration = start.elapsed();
    println!("运行耗时: {:?}", duration);
    fs::write("./monitors.txt", format!("运行耗时: {:?}", duration))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(())

    // for monitor in monitors {
    //     let image = monitor.capture_image().unwrap();

    //     // 当前时间毫秒级 完整时间戳
    //     let timestamp = SystemTime::now()
    //         .duration_since(SystemTime::UNIX_EPOCH)
    //         .unwrap()
    //         .as_millis();
    //     println!("当前时间时间戳: {}", timestamp);
    //     image
    //         .save(format!(
    //             "./monitors/monitor-{}-{}.png",
    //             normalized(monitor.friendly_name().unwrap()),
    //             timestamp
    //         ))
    //         .unwrap();
    // }
    // let duration = start.elapsed();
    // println!("运行耗时: {:?}", duration);
    // fs::write("./monitors.txt", format!("运行耗时: {:?}", duration)).unwrap();
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}

/// 将 xcap Monitor 转换为 MonitorInfo
fn to_monitor_info(m: &Monitor) -> Option<MonitorInfo> {
    Some(MonitorInfo {
        id: m.id().ok()?,
        name: m.friendly_name().unwrap_or_else(|_| {
            m.name().unwrap_or_else(|_| {
                m.id()
                    .map(|id| format!("Monitor-{}", id))
                    .unwrap_or_else(|_| "Unknown".into())
            })
        }),
        x: m.x().ok()?,
        y: m.y().ok()?,
        width: m.width().ok()?,
        height: m.height().ok()?,
        primary: m.is_primary().unwrap_or(false),
    })
}
