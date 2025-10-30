use anyhow::Context;

use crate::bootstrap::controller::Args;
use std::{env, path::Path, process::Command};

pub fn run(args: &Args) {
    let file_path = env::current_exe()
        .context("无法获取当前可执行文件路径")
        .expect("当前可执行文件路径不存在");

    let file_dir = file_path
        .parent()
        .context("无法获取可执行文件目录")
        .expect("可执行文件目录不存在")
        .to_string_lossy()
        .to_string();

    let current_path = env::var("PATH").unwrap_or_default();

    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("bootstrap.ps1");

    match args {
        Args::Env => {
            // 处理环境变量设置
            // 使用 PowerShell 添加到用户环境变量（不需要管理员权限）

            let output = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &script.to_string_lossy(),
                    "-Action",
                    "insert", // Args::Env 用 "insert"；Args::Force 用 "force"
                    "-Target",
                    &file_dir,
                ])
                .output()
                .context("执行 PowerShell 脚本失败")
                .expect("执行 PowerShell 脚本失败");

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return eprintln!("❌ PowerShell 执行失败: {}", stderr);
                // return Err(anyhow::anyhow!("PowerShell 执行失败: {}", stderr));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("{}", stdout);

            println!("初始化完成！");
            println!("📋 使用说明:");
            println!("1. 重启命令行窗口");
            println!("2. 在任意目录下输入工具名称即可使用");
            println!("3. 使用 'corex bootstrap inspect' 验证配置");
        }
        Args::Inspect => {
            let contains_dir = current_path.split(';').any(|path| {
                Path::new(path).canonicalize().ok() == Path::new(&file_dir).canonicalize().ok()
            });

            // 处理检查环境
            if contains_dir {
                println!("✅ 工具已经在系统环境变量中");
                println!("📁 路径: {}", file_dir);
            } else {
                println!("❌ 工具尚未添加到系统环境变量中");
                println!("📁 当前路径: {}", file_dir);
                println!("💡 运行 'corex bootstrap env' 来添加到环境变量");
            }
        }
        Args::Force => {
            // 处理强制设置：改为调用外部脚本，避免内联编码问题
            let output = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &script.to_string_lossy(),
                    "-Action",
                    "force", // Args::Env 用 "force"；Args::Force 用 "force"
                    "-Target",
                    &file_dir,
                ])
                .output()
                .context("执行 PowerShell 脚本失败")
                .expect("执行 PowerShell 强制更新脚本失败");

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return eprintln!("❌ PowerShell 执行失败: {}", stderr);
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                println!("{}", stdout);
            }
        }
    }

    // if args.verbose {
    //     println!("可执行文件路径: {:?}", file_path);
    //     println!("可执行文件目录: {}", file_dir);
    // }
}
