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

use std::io::{self, IsTerminal, Write};
use std::sync::OnceLock;
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

/// 一行文本的角色：决定用什么颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// 成功、正常。
    Ok,
    /// 失败、拒绝。
    Bad,
    /// 中性提示（暂停、无状态）。
    Note,
}

/// 角色 → ANSI 前景色。数据表而不是 match：加角色时只动这里。
const PAINT: [(Role, &str); 3] = [
    (Role::Ok, "\x1b[32m"),
    (Role::Bad, "\x1b[31m"),
    (Role::Note, "\x1b[2m"),
];

const RESET: &str = "\x1b[0m";

/// 给 stdout 上的文本上色；不满足上色条件时原样返回。
///
/// 只按 stdout 判断：把结果重定向进文件时，文件里就不该出现转义序列。
pub(crate) fn paint(role: Role, text: &str) -> String {
    painted(role, text, std::io::stdout().is_terminal())
}

/// 给 stderr 上的文本上色；判据同理，看的是 stderr。
pub(crate) fn paint_err(role: Role, text: &str) -> String {
    painted(role, text, std::io::stderr().is_terminal())
}

fn painted(role: Role, text: &str, is_terminal: bool) -> String {
    if !is_terminal || colors_off() {
        return text.to_string();
    }
    match PAINT.iter().find(|(each, _)| *each == role) {
        Some((_, code)) => format!("{code}{text}{RESET}"),
        None => text.to_string(),
    }
}

/// `NO_COLOR` 一律关；`CLICOLOR=0` 也关（同一条约定的两种写法）。
fn colors_off() -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return true;
    }
    std::env::var_os("CLICOLOR").is_some_and(|v| v == "0")
}

/// 终端符号。Unicode 是默认；字体/终端跟不上时用 `COREX_ASCII=1` 退成 ASCII。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Symbols {
    /// 成功
    pub ok: &'static str,
    /// 失败
    pub bad: &'static str,
    /// 中性
    pub note: &'static str,
    /// spinner 的帧，最后一个是收尾空白帧
    pub tick: &'static str,
}

pub(crate) const UNICODE: Symbols = Symbols {
    ok: "✓",
    bad: "✗",
    note: "·",
    tick: "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ",
};

/// 一行的前缀：角色对应的符号，并按该角色上色（stdout 侧）。
pub(crate) fn mark(role: Role) -> String {
    paint(role, symbols().of(role))
}

pub(crate) const ASCII: Symbols = Symbols {
    ok: "ok",
    bad: "!!",
    note: "-",
    tick: "|/-\\",
};

impl Symbols {
    /// 角色对应的那一个符号。
    pub(crate) fn of(self, role: Role) -> &'static str {
        match role {
            Role::Ok => self.ok,
            Role::Bad => self.bad,
            Role::Note => self.note,
        }
    }
}

static SYMBOLS: OnceLock<Symbols> = OnceLock::new();

/// 本次运行用哪套符号：只读一次环境，之后各处共用同一个答案。
pub(crate) fn symbols() -> Symbols {
    *SYMBOLS.get_or_init(|| match std::env::var_os("COREX_ASCII") {
        Some(v) if v != "0" && !v.is_empty() => ASCII,
        _ => UNICODE,
    })
}

/// 把控制台输出/输入代码页设成 UTF-8。
///
/// 只管**直连的控制台**：中文与 `✓` 经由 console 时按当前代码页解码，cp936 下会乱码。
/// 输出被重定向（或经 PowerShell cmdlet 管道）时解码由读方决定，这里帮不上忙——
/// 那种情况下的乱码要在读方设 `[Console]::OutputEncoding`，调用失败就忽略。
///
/// 注意：`shell.run` / `exec.run` 捕获的是**管道**字节，与这里无关；
/// 管道侧的 OEM/GBK 解码在 `process_launch` 里处理。
#[cfg(windows)]
pub(crate) fn use_utf8_console() {
    use windows::Win32::System::Console::{SetConsoleCP, SetConsoleOutputCP};
    /// `CP_UTF8`
    const UTF8_CODE_PAGE: u32 = 65001;
    // 失败只意味着“没有 console 可设”，不是错误。
    unsafe {
        let _ = SetConsoleOutputCP(UTF8_CODE_PAGE);
        let _ = SetConsoleCP(UTF8_CODE_PAGE);
    }
}

#[cfg(not(windows))]
pub(crate) fn use_utf8_console() {}

/// 控制台当前的输出代码页；没有连接控制台时为 `None`。
///
/// 进程启动时 [`use_utf8_console`] 会把它设成 UTF-8，所以这里读到 65001 是预期结果；
/// 读到别的值（或读不到）说明那次设置没生效，而中文乱码正是从那里开始的。
#[cfg(windows)]
pub(crate) fn console_page() -> Option<u32> {
    use windows::Win32::System::Console::GetConsoleOutputCP;
    // 没有控制台时返回 0，而不是报错。
    let page = unsafe { GetConsoleOutputCP() };
    (page != 0).then_some(page)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 非终端（管道、重定向）里绝不能出现转义序列——那正是 `| jq` 读不懂的来源。
    #[test]
    fn no_escape_sequences_off_terminal() {
        assert_eq!(painted(Role::Ok, "✓", false), "✓");
        assert_eq!(painted(Role::Bad, "x", false), "x");
    }

    #[test]
    fn each_role_has_its_own_color() {
        // 环境里设了 NO_COLOR（用户的偏好）时跳过：那是上色被主动关掉的情形。
        if colors_off() {
            return;
        }
        assert_eq!(painted(Role::Ok, "ok", true), "\x1b[32mok\x1b[0m");
        assert_eq!(painted(Role::Bad, "bad", true), "\x1b[31mbad\x1b[0m");
        assert_eq!(painted(Role::Note, "·", true), "\x1b[2m·\x1b[0m");
    }

    /// 符号表要覆盖每个角色，且 ASCII 那套必须真的只有 ASCII。
    #[test]
    fn ascii_symbols_stay_ascii() {
        for role in [Role::Ok, Role::Bad, Role::Note] {
            assert!(ASCII.of(role).is_ascii(), "{role:?}");
            assert!(!UNICODE.of(role).is_empty(), "{role:?}");
        }
        assert!(ASCII.tick.is_ascii());
    }
}
