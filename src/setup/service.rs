use anyhow::{Context, Result};

use crate::setup::controller::SetupArgs;
use std::{env, path::Path, process::Command};

pub fn run(args: &SetupArgs) -> Result<()> {
    let file_path = env::current_exe().context("无法获取当前可执行文件路径")?;

    let file_dir = file_path
        .parent()
        .context("无法获取可执行文件目录")?
        .to_string_lossy()
        .to_string();

    let current_path = env::var("PATH").unwrap_or_default();

    let contains_dir = current_path.split(';').any(|path| {
        Path::new(path).canonicalize().ok() == Path::new(&file_dir).canonicalize().ok()
    });

    match args {
        SetupArgs::Env => {
            // 处理环境变量设置
            // 使用 PowerShell 添加到用户环境变量（不需要管理员权限）
            let ps_command = format!(
                r#"
        $currentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
        $newPath = "{}"
        if ($currentPath -split ';' -notcontains $newPath) {{
            $updatedPath = if ($currentPath) {{ "$currentPath;$newPath" }} else {{ $newPath }}
            [Environment]::SetEnvironmentVariable("PATH", $updatedPath, "User")
            Write-Host "✅ 成功添加到用户环境变量"
            Write-Host "📁 路径: $newPath"
            Write-Host "🔄 请重启命令行窗口使更改生效"
        }} else {{
            Write-Host "✅ 路径已存在于用户环境变量中"
            Write-Host "📁 路径: $newPath"
        }}
        "#,
                file_dir.replace("\\", "\\\\")
            );

            let output = Command::new("powershell")
                .args(["-Command", &ps_command])
                .output()
                .context("执行 PowerShell 命令失败")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("PowerShell 执行失败: {}", stderr));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("{}", stdout);

            println!("初始化完成！");
            println!("📋 使用说明:");
            println!("1. 重启命令行窗口");
            println!("2. 在任意目录下输入工具名称即可使用");
            println!("3. 使用 'corex setup check' 验证配置");
        }
        SetupArgs::Check => {
            // 处理检查环境
            if contains_dir {
                println!("✅ 工具已经在系统环境变量中");
                println!("📁 路径: {}", file_dir);
            } else {
                println!("❌ 工具尚未添加到系统环境变量中");
                println!("📁 当前路径: {}", file_dir);
                println!("💡 运行 'corex setup env' 来添加到环境变量");
            }
        }
        SetupArgs::Force => {
            // 处理强制设置
        }
    }

    // if args.verbose {
    //     println!("可执行文件路径: {:?}", file_path);
    //     println!("可执行文件目录: {}", file_dir);
    // }

    Ok(())
}
