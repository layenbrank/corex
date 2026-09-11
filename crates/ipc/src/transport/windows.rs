//! 经 `interprocess` 的 Windows 命名管道传输（换行分隔的 JSON）。

use super::{Transport, TransportError, read_final, serve_connection, write_request};
use crate::progress::{FrameSink, Outlet};
use crate::protocol::{Request, Response};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// 在 Windows 命名管道上跑换行分隔的 JSON（如 `\\.\pipe\corex`）。
#[derive(Debug, Clone)]
pub struct NamedPipeTransport {
    path: PathBuf,
}

impl NamedPipeTransport {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Windows IPC 的规范命名管道路径。
    pub fn canonical_pipe_path() -> PathBuf {
        PathBuf::from(r"\\.\pipe\corex")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 服务连接：对每个换行分隔的 JSON 请求调用 `handler`。
    pub async fn serve<F, Fut>(path: &Path, mut handler: F) -> Result<(), TransportError>
    where
        F: FnMut(Request, Outlet) -> Fut + Send,
        Fut: std::future::Future<Output = Response> + Send,
    {
        use interprocess::os::windows::named_pipe::{PipeListenerOptions, pipe_mode};

        // 默认安全设置：当前用户可访问的管道（未显式指定 SD 的命名管道，
        // 按操作系统默认通常是仅本机可用）。
        let listener = PipeListenerOptions::new()
            .path(path)
            .create_tokio_duplex::<pipe_mode::Bytes>()
            .map_err(|e| TransportError::Connect(format!("{}: {e}", path.display())))?;

        tracing::info!(path = %path.display(), "IPC Named Pipe 已监听");

        loop {
            let conn = listener.accept().await?;
            let [reader, writer] = [&conn; 2];
            serve_connection(reader, writer, &mut handler).await?;
        }
    }
}

#[async_trait]
impl Transport for NamedPipeTransport {
    async fn send_events(
        &mut self,
        request: &Request,
        sink: &dyn FrameSink,
    ) -> Result<Response, TransportError> {
        use interprocess::os::windows::named_pipe::{pipe_mode, tokio::DuplexPipeStream};

        let conn = DuplexPipeStream::<pipe_mode::Bytes>::connect_by_path(self.path.as_path())
            .await
            .map_err(|e| TransportError::Connect(format!("{}: {e}", self.path.display())))?;

        let [reader, mut writer] = [&conn; 2];
        write_request(&mut writer, request).await?;
        read_final(reader, sink).await
    }
}
