//! CLI 的 stdout / stderr 写入层。
//!
//! 命令要打印的一切都走这里，于是同一套策略能覆盖全部输出——人类可读的行、`--json` 文档、
//! 状态消息都算。这个模块存在的原因：`println!` 在 `EPIPE` 上会 *panic*，而
//! `[profile.release]` 设了 `panic = "abort"`，于是提前走掉的消费者
//!
//! ```text
//! corex actions | head -1
//! ```
//!
//! 会把进程直接 abort，而不是安静地结束。`rg` 和 `git` 在这种情况都退 0，本 CLI 也一样：
//! 管道关闭只说明读方已经看够了，不说明命令失败。
//!
//! 调用点优先用 [`outln!`] / [`errln!`] 而不是 [`line`] / [`error_line`]，
//! 这样能继续用 `println!` 风格的格式化。

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// stdout 的读方是否已经离开，只置位一次。
static STDOUT_CLOSED: AtomicBool = AtomicBool::new(false);

/// 往 stdout 写一行，容忍管道关闭。
pub fn line(text: &str) {
    // 读方走后，后续每次写入都只会以同样方式失败；直接跳过，
    // 免得长列表白白付出几千次注定失败的 syscall。
    if is_stdout_closed() {
        return;
    }
    let stdout = io::stdout();
    let result = {
        let mut out = stdout.lock();
        writeln!(out, "{text}")
    };
    match result {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {
            STDOUT_CLOSED.store(true, Ordering::Relaxed);
        }
        Err(err) => tracing::debug!(error = %err, "写入 stdout 失败"),
    }
}

/// 把原始字节流写到 stdout，容忍管道关闭。
///
/// 日志跟踪用它转发 supervisor 写下的内容（不是整行）。会 flush，保证跟踪视图是实时的。
pub fn bytes(data: &[u8]) -> io::Result<()> {
    if is_stdout_closed() {
        return Ok(());
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match out.write_all(data).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {
            STDOUT_CLOSED.store(true, Ordering::Relaxed);
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// 往 stderr 写一行，容忍管道关闭。
///
/// 用于最终错误报告与提示信息：它们绝不能污染机器可读的 stdout。
pub fn error_line(text: &str) {
    let stderr = io::stderr();
    let mut out = stderr.lock();
    if let Err(err) = writeln!(out, "{text}") {
        tracing::debug!(error = %err, "写入 stderr 失败");
    }
}

/// stdout 的读方是否曾经关闭过管道。
///
/// `main` 据此返回退出码 0，而不是失败。
pub fn is_stdout_closed() -> bool {
    STDOUT_CLOSED.load(Ordering::Relaxed)
}

/// 往 stdout 写一行；见 [`line`]。
macro_rules! outln {
    ($($arg:tt)*) => {
        $crate::output::line(&::std::format!($($arg)*))
    };
}

/// 往 stderr 写一行；见 [`error_line`]。
macro_rules! errln {
    ($($arg:tt)*) => {
        $crate::output::error_line(&::std::format!($($arg)*))
    };
}

pub(crate) use {errln, outln};
