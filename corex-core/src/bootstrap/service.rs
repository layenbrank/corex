use std::{env, path::PathBuf, process::Command};

use anyhow::Context;

use crate::bootstrap::schema::Args;

#[derive(Debug, Clone, Default)]
pub struct Output;

/// CLI 入口：执行 bootstrap 子命令。
pub fn run(args: &Args) -> anyhow::Result<()> {
    execute(args).map(|_| ())
}

/// 执行 bootstrap 子命令，供 Pipeline / IPC 复用。
pub fn execute(args: &Args) -> anyhow::Result<Output> {
    let exe_dir = exe_dir()?;
    let script = bootstrap_script();

    match args {
        Args::Env => {
            run_ps_script(&script, "insert", &exe_dir)?;
            print_env_success();
        }
        Args::Inspect => inspect_path(&exe_dir)?,
        Args::Force => run_ps_script(&script, "force", &exe_dir)?,
    }

    Ok(Output)
}

/// 当前可执行文件所在目录（用于写入 PATH）。
fn exe_dir() -> anyhow::Result<String> {
    let path = env::current_exe().context("无法获取当前可执行文件路径")?;
    let parent = path
        .parent()
        .context("无法获取可执行文件目录")?
        .to_string_lossy()
        .to_string();
    Ok(parent)
}

fn bootstrap_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("bootstrap.ps1")
}

/// 调用 bootstrap.ps1 更新用户 PATH。
fn run_ps_script(script: &PathBuf, action: &str, target: &str) -> anyhow::Result<()> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
            "-Action",
            action,
            "-Target",
            target,
        ])
        .output()
        .context("执行 PowerShell 脚本失败")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("PowerShell 执行失败: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        println!("{stdout}");
    }
    Ok(())
}

fn print_env_success() {
    println!("初始化完成！");
    println!("📋 使用说明:");
    println!("1. 重启命令行窗口");
    println!("2. 在任意目录下输入工具名称即可使用");
    println!("3. 使用 'corex bootstrap inspect' 验证配置");
}

/// 检查 exe 目录是否已在 PATH 中。
fn inspect_path(exe_dir: &str) -> anyhow::Result<()> {
    use std::path::Path;

    let current_path = env::var("PATH").unwrap_or_default();
    let contains = current_path.split(';').any(|path| {
        Path::new(path).canonicalize().ok() == Path::new(exe_dir).canonicalize().ok()
    });

    if contains {
        println!("✅ 工具已经在系统环境变量中");
        println!("📁 路径: {exe_dir}");
    } else {
        println!("❌ 工具尚未添加到系统环境变量中");
        println!("📁 当前路径: {exe_dir}");
        println!("💡 运行 'corex bootstrap env' 来添加到环境变量");
    }
    Ok(())
}
