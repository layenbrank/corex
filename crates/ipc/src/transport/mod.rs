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

/// Platform project data directory for corex.
///
/// Uses `ProjectDirs::from("dev", "", "corex")` so Windows resolves to
/// `%AppData%\corex\data` (not `%AppData%\corex\corex\data`). Falls back to
/// `.corex` when project dirs are unavailable. Creates the directory if needed.
pub fn platform_data_dir() -> std::io::Result<PathBuf> {
    let base = directories::ProjectDirs::from("dev", "", "corex")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".corex"));
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

/// Default IPC endpoint for the given data directory.
///
/// - Unix: `<data_dir>/corex.sock`
/// - Windows: `\\.\pipe\corex`
pub fn default_endpoint(data_dir: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        data_dir.join("corex.sock")
    }
    #[cfg(windows)]
    {
        let _ = data_dir;
        PathBuf::from(r"\\.\pipe\corex")
    }
}

/// Create the platform transport for `endpoint`.
pub fn platform_transport(endpoint: impl Into<PathBuf>) -> PlatformTransport {
    PlatformTransport::new(endpoint)
}

/// Serve newline-delimited JSON requests on the platform transport.
pub async fn serve_platform<F, Fut>(
    endpoint: &Path,
    handler: F,
) -> Result<(), TransportError>
where
    F: FnMut(Request) -> Fut + Send,
    Fut: std::future::Future<Output = Response> + Send,
{
    PlatformTransport::serve(endpoint, handler).await
}
