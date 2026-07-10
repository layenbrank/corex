//! 应用运行时：全局选项、输出、退出码、tracing。

mod emit;
mod exit;
mod opts;
mod settings;
mod trace;

pub use emit::{Emitter, OutputFormat};
pub use exit::{AppError, ExitStatus, app_error_from_anyhow};
pub use opts::{ColorChoice, RuntimeOpts};
pub use settings::{merge_variables, parse_define};
pub use trace::init_tracing;

use std::sync::OnceLock;

static RUNTIME: OnceLock<RuntimeState> = OnceLock::new();

/// 进程级运行时状态
pub struct RuntimeState {
    pub opts: RuntimeOpts,
    pub emitter: Emitter,
}

/// 初始化运行时（main 入口调用一次）
pub fn init(opts: RuntimeOpts) -> Result<(), AppError> {
    init_tracing(opts.verbose, opts.quiet)?;
    let emitter = Emitter::new(opts.format, opts.quiet, opts.color);
    RUNTIME
        .set(RuntimeState { opts, emitter })
        .map_err(|_| AppError::Internal("runtime 已初始化".into()))?;
    Ok(())
}

/// 获取运行时状态
pub fn state() -> &'static RuntimeState {
    RUNTIME
        .get()
        .expect("runtime 未初始化，请先调用 runtime::init")
}

/// 当前是否为 JSON 输出模式
pub fn is_json_output() -> bool {
    RUNTIME
        .get()
        .is_some_and(|s| s.opts.format == OutputFormat::Json)
}

/// 是否静默模式
pub fn is_quiet() -> bool {
    RUNTIME.get().is_some_and(|s| s.opts.quiet)
}
