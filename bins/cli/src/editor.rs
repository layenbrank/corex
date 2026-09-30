//! 用用户的编辑器打开指令。
//!
//! 只服务于一处用途：`corex directive edit` 的往返——打开、等它退出、把改过的内容读回来。
//! 因此这里**等**编辑器，并且给它本进程的 stdio（vim / nano 这类终端编辑器要控制台才有用）。

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// 打开 `path` 并**等编辑器退出**。
///
/// 两处刻意的选择：
/// - 编辑器继承本进程的 stdio，vim / nano 这类终端编辑器才能拿到控制台；
/// - 没有配置编辑器时不用 `start` / `open`（它们打开 GUI 就返回，等不到任何东西），而是用
///   平台上一个**会阻塞**的编辑器（Windows 的 notepad、别处的 vi）。
pub fn edit_and_wait(path: &Path) -> Result<()> {
    let Some(spec) = editor_spec() else {
        return blocking_default(path);
    };
    let status = shell_command(&spec, path)
        .status()
        .with_context(|| format!("无法启动编辑器: {spec}"))?;
    if !status.success() {
        // 编辑器非零退出不一定表示失败（有的用退出码表示「没改动」），所以只提示；
        // 读回来的内容仍按实际情况处理。
        eprintln!("提示: 编辑器退出码 {:?}", status.code());
    }
    Ok(())
}

/// 用户配置的编辑器命令行；没配就是 `None`。
fn editor_spec() -> Option<String> {
    let spec = std::env::var("COREX_EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .or_else(|_| std::env::var("EDITOR"))
        .ok()?;
    let spec = spec.trim().to_string();
    (!spec.is_empty()).then_some(spec)
}

/// 把 `<编辑器> <文件>` 交给 shell：用户常把参数一起写进 `EDITOR`（`code --wait`、
/// `vim -u NONE`），自己拆词只会把它拆坏。
fn shell_command(spec: &str, path: &Path) -> Command {
    let quoted = path.display().to_string();
    #[cfg(windows)]
    {
        // 这里必须用 `raw_arg` 而不是 `arg`：`cmd` 只认**原始命令行**里 `/C` 之后的那串，而
        // `Command::arg` 会按 MSVC 的规则给含引号的参数加转义（`\"`）——`cmd` 看不懂那个转义，
        // 报的是「文件名、目录名或卷标语法不正确」。路径里有空格时必踩，不能靠运气。
        use std::os::windows::process::CommandExt;

        let mut command = Command::new("cmd");
        command.arg("/C");
        command.raw_arg(format!("{spec} \"{quoted}\""));
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = Command::new("sh");
        command.arg("-c").arg(format!("{spec} \"$1\"")).arg(path);
        command
    }
}

/// 没配编辑器时的**阻塞**兜底。
fn blocking_default(path: &Path) -> Result<()> {
    #[cfg(windows)]
    let (program, args): (&str, &[&str]) = ("notepad", &[]);
    #[cfg(target_os = "macos")]
    let (program, args): (&str, &[&str]) = ("open", &["-W", "-t"]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let (program, args): (&str, &[&str]) = ("vi", &[]);

    Command::new(program)
        .args(args)
        .arg(path)
        .status()
        .with_context(|| {
            format!(
                "无法启动编辑器 {program}；用 COREX_EDITOR / VISUAL / EDITOR 指定一个能等待的编辑器"
            )
        })?;
    Ok(())
}
