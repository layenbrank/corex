//! Open a Directive YAML in the user's editor or OS default app.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Open `path` with `COREX_EDITOR` / `VISUAL` / `EDITOR`, else the platform default handler.
pub fn open_in_editor(path: &Path) -> Result<()> {
    if let Ok(spec) = std::env::var("COREX_EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .or_else(|_| std::env::var("EDITOR"))
    {
        let spec = spec.trim();
        if !spec.is_empty() {
            return spawn_shell_command(spec, path);
        }
    }
    open_with_system_default(path)
}

fn spawn_shell_command(spec: &str, path: &Path) -> Result<()> {
    let quoted = path.display().to_string();
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", &format!("{spec} \"{quoted}\"")])
            .spawn()
            .with_context(|| format!("无法启动编辑器: {spec}"))?;
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .arg("-c")
            .arg(format!("{spec} \"$1\""))
            .arg(path)
            .spawn()
            .with_context(|| format!("无法启动编辑器: {spec}"))?;
    }
    Ok(())
}

fn open_with_system_default(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &path.display().to_string()])
            .spawn()
            .context("无法调用系统打开方式（start）")?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .context("无法调用 open")?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .context("无法调用 xdg-open")?;
    }
    Ok(())
}
