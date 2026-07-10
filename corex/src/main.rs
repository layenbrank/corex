use clap::Parser;
use cx::command::{Args, dispatch};
use cx::pipeline::report::StepFail;
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
            let step_id_owned = e.downcast_ref::<StepFail>().map(|f| f.step.clone());
            let app_err: AppError = e.into();
            let code = match &app_err {
                AppError::Config(_) => "CONFIG_INVALID",
                AppError::Usage(_) => "USAGE_ERROR",
                AppError::Io(_) => "IO_ERROR",
                AppError::Internal(_) => "INTERNAL_ERROR",
                AppError::Runtime(_) => "RUNTIME_ERROR",
            };
            let step_id = step_id_owned.as_deref().or_else(|| match &app_err {
                AppError::Runtime(msg) => runtime::parse_fail(msg).map(|(id, _)| id),
                _ => None,
            });
            let _ = runtime::state()
                .emitter
                .error(code, app_err.to_string(), step_id);
            app_err.exit_status()
        }
    }
}
