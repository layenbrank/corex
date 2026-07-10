use clap::Parser;
use cx::command::{Args, dispatch};
use cx::runtime::{self, AppError, ExitStatus};

fn main() -> ExitStatus {
    let args = Args::parse();
    if let Err(e) = runtime::init(args.runtime.clone()) {
        eprintln!("{e}");
        return e.exit_status();
    }
    match dispatch(args) {
        Ok(()) => ExitStatus::Success,
        Err(e) => {
            let app_err: AppError = e.into();
            let code = match &app_err {
                AppError::Config(_) => "CONFIG_INVALID",
                AppError::Usage(_) => "USAGE_ERROR",
                AppError::Io(_) => "IO_ERROR",
                AppError::Internal(_) => "INTERNAL_ERROR",
                AppError::Runtime(_) => "RUNTIME_ERROR",
            };
            let _ = runtime::state()
                .emitter
                .error(code, app_err.to_string(), None);
            app_err.exit_status()
        }
    }
}
