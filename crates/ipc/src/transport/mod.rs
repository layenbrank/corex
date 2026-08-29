//! Transport abstraction: Unix domain sockets and Windows named pipes.

use crate::protocol::{Request, Response};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::UnixSocketTransport;
#[cfg(windows)]
pub use windows::NamedPipeTransport;

/// Platform-default IPC transport (Unix socket or Windows named pipe).
#[cfg(unix)]
pub type PlatformTransport = UnixSocketTransport;
#[cfg(windows)]
pub type PlatformTransport = NamedPipeTransport;

/// Abstract IPC transport.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&mut self, request: &Request) -> Result<Response, TransportError>;
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("连接失败: {0}")]
    Connect(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("协议错误: {0}")]
    Protocol(String),
    #[error("未实现: {0}")]
    Unsupported(String),
}

/// Writable directory of the running binary, if any.
fn try_exe_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.to_path_buf();
    let probe = dir.join(".corex-write-check");
    match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Some(dir)
        }
        Err(_) => None,
    }
}

/// Data root for directives / token / history / config.
///
/// Order: writable exe dir → OS project data dir → `.corex`.
pub fn data_dir() -> std::io::Result<PathBuf> {
    if let Some(dir) = try_exe_dir() {
        return Ok(dir);
    }
    let base = directories::ProjectDirs::from("dev", "", "corex")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".corex"));
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Config TOML search paths (first hit wins): data dir, then cwd.
pub fn config_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(root) = data_dir() {
        out.push(root.join("config").join("corex.toml"));
        out.push(root.join("config.toml"));
    }
    out.push(PathBuf::from("config/corex.toml"));
    out
}

/// Default IPC endpoint for `data`.
///
/// - Unix: `<data>/corex.sock`
/// - Windows: `\\.\pipe\corex`
pub fn ipc_endpoint(data: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        data.join("corex.sock")
    }
    #[cfg(windows)]
    {
        let _ = data;
        PathBuf::from(r"\\.\pipe\corex")
    }
}

/// Client transport for `endpoint`.
pub fn ipc_connect(endpoint: impl Into<PathBuf>) -> PlatformTransport {
    PlatformTransport::new(endpoint)
}

/// Serve NDJSON requests on the platform transport.
pub async fn serve_ipc<F, Fut>(endpoint: &Path, handler: F) -> Result<(), TransportError>
where
    F: FnMut(Request) -> Fut + Send,
    Fut: std::future::Future<Output = Response> + Send,
{
    PlatformTransport::serve(endpoint, handler).await
}
