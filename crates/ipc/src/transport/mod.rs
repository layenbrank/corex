//! 传输抽象：Unix domain socket 与 Windows 命名管道。

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

/// 平台默认的 IPC 传输（Unix socket 或 Windows 命名管道）。
#[cfg(unix)]
pub type PlatformTransport = UnixSocketTransport;
#[cfg(windows)]
pub type PlatformTransport = NamedPipeTransport;

/// IPC 传输抽象。
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

/// 运行中二进制所在的可写目录（若有）。
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

/// 指令 / token / 历史 / 配置的数据根目录。
///
/// 顺序：可写的 exe 目录 → 操作系统的项目数据目录 → `.corex`。
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

/// 配置 TOML 的搜索路径（先命中的先用）：数据目录，然后是当前目录。
pub fn config_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(root) = data_dir() {
        out.push(root.join("config").join("corex.toml"));
        out.push(root.join("config.toml"));
    }
    out.push(PathBuf::from("config/corex.toml"));
    out
}

/// `data` 的默认 IPC 端点。
///
/// - Unix：`<data>/corex.sock`
/// - Windows：`\\.\pipe\corex`
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

/// `endpoint` 的客户端传输。
pub fn ipc_connect(endpoint: impl Into<PathBuf>) -> PlatformTransport {
    PlatformTransport::new(endpoint)
}

/// 在平台传输上服务 NDJSON 请求。
pub async fn serve_ipc<F, Fut>(endpoint: &Path, handler: F) -> Result<(), TransportError>
where
    F: FnMut(Request) -> Fut + Send,
    Fut: std::future::Future<Output = Response> + Send,
{
    PlatformTransport::serve(endpoint, handler).await
}
