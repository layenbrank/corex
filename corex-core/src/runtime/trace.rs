use super::exit::AppError;

/// 初始化 tracing subscriber
pub fn init_tracing(verbose: u8, quiet: bool) -> Result<(), AppError> {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let default = if quiet {
        "error"
    } else if verbose >= 2 {
        "corex=debug"
    } else if verbose >= 1 {
        "corex=info"
    } else {
        "corex=warn"
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));

    if std::env::var("COREX_LOG_FORMAT").as_deref() == Ok("json") {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_target(false))
            .init();
    }

    Ok(())
}
