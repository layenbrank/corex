#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{run_server, send_request, send_shutdown};

#[cfg(not(windows))]
pub fn run_server(
    _options: &crate::serve::ServeOptions,
    _state: &mut crate::serve::state::DaemonState,
) -> anyhow::Result<()> {
    anyhow::bail!("corex serve 当前仅支持 Windows Named Pipe")
}

#[cfg(not(windows))]
pub fn send_request(
    _pipe_name: &str,
    _module: &str,
    _args: serde_json::Value,
    _id: u64,
) -> anyhow::Result<crate::serve::protocol::Response> {
    anyhow::bail!("corex serve 当前仅支持 Windows Named Pipe")
}

#[cfg(not(windows))]
pub fn send_shutdown(_pipe_name: &str) -> anyhow::Result<()> {
    anyhow::bail!("corex serve 当前仅支持 Windows Named Pipe")
}
