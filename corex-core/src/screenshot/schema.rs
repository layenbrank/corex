use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

/// screenshot 子命令
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 截取主显示器并保存 PNG
    Capture(CaptureArgs),
    /// 枚举显示器
    Monitors,
    /// 枚举可见窗口
    Windows,
    /// 裁剪图片并保存
    Crop(CropArgs),
    /// 裁剪图片并写入剪贴板
    Clipboard(ClipboardArgs),
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct CaptureArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub to: String,
    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct CropArgs {
    #[arg(long, value_parser = verifier::file)]
    pub source: String,
    /// 输出目录（自动生成 screenshot-crop-{ts}.png，与 Capture.to 一致）
    #[arg(long, value_parser = verifier::path)]
    pub to: String,
    #[arg(long, default_value_t = 0)]
    pub x: u32,
    #[arg(long, default_value_t = 0)]
    pub y: u32,
    #[arg(long)]
    pub w: u32,
    #[arg(long)]
    pub h: u32,
    /// 已裁剪 PNG 文件路径（IPC 推荐，避免 base64 超行限）
    #[arg(long, value_parser = verifier::file)]
    pub image_file: Option<String>,
    /// 已裁剪 PNG 的 base64（与 image_file 二选一；IPC 大图可能超 64KB 行限）
    #[arg(long)]
    pub final_image_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct ClipboardArgs {
    #[arg(long, value_parser = verifier::file)]
    pub source: String,
    #[arg(long, default_value_t = 0)]
    pub x: u32,
    #[arg(long, default_value_t = 0)]
    pub y: u32,
    #[arg(long)]
    pub w: u32,
    #[arg(long)]
    pub h: u32,
    #[arg(long, value_parser = verifier::file)]
    pub image_file: Option<String>,
    #[arg(long)]
    pub final_image_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_minimized: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capture_args_roundtrip() {
        let args = Args::Capture(CaptureArgs {
            to: "C:/out".to_string(),
            description: None,
        });
        let v = serde_json::to_value(&args).unwrap();
        let back: Args = serde_json::from_value(v).unwrap();
        assert!(matches!(back, Args::Capture(_)));
    }

    #[test]
    fn unit_variant_monitors() {
        let v = json!({"Monitors": null});
        let args: Args = serde_json::from_value(v).unwrap();
        assert!(matches!(args, Args::Monitors));
    }

    #[test]
    fn crop_with_to_dir() {
        let v = json!({
            "Crop": {
                "source": "C:/in.png",
                "to": "C:/out",
                "x": 0,
                "y": 0,
                "w": 100,
                "h": 100
            }
        });
        let args: Args = serde_json::from_value(v).unwrap();
        assert!(matches!(args, Args::Crop(_)));
    }
}
