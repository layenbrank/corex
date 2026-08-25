//! Windows named pipe transport via `interprocess` (newline-delimited JSON).

use super::{Transport, TransportError};
use crate::protocol::{Request, Response};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Newline-delimited JSON over a Windows named pipe (e.g. `\\.\pipe\corex`).
#[derive(Debug, Clone)]
pub struct NamedPipeTransport {
    path: PathBuf,
}

impl NamedPipeTransport {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Canonical default pipe path.
    pub fn default_path() -> PathBuf {
        PathBuf::from(r"\\.\pipe\corex")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Serve connections: for each newline-delimited JSON request, call `handler`.
    pub async fn serve<F, Fut>(path: &Path, mut handler: F) -> Result<(), TransportError>
    where
        F: FnMut(Request) -> Fut + Send,
        Fut: std::future::Future<Output = Response> + Send,
    {
        use interprocess::os::windows::named_pipe::{pipe_mode, PipeListenerOptions};

        let listener = PipeListenerOptions::new()
            .path(path)
            .create_tokio_duplex::<pipe_mode::Bytes>()
            .map_err(|e| TransportError::Connect(format!("{}: {e}", path.display())))?;

        tracing::info!(path = %path.display(), "IPC Named Pipe 已监听");

        loop {
            let conn = listener.accept().await?;
            // Shared refs: named pipes do not expose into_split(); read + write via &conn.
            let [reader_half, mut writer] = [&conn; 2];
            let mut reader = BufReader::new(reader_half);
            let mut line = String::new();
            loop {
                line.clear();
                let n = reader.read_line(&mut line).await?;
                if n == 0 {
                    break;
                }
                if line.trim().is_empty() {
                    continue;
                }
                let req: Request = serde_json::from_str(&line).map_err(|e| {
                    TransportError::Protocol(format!("请求解析失败: {e}"))
                })?;
                let resp = handler(req).await;
                let mut payload = serde_json::to_string(&resp)
                    .map_err(|e| TransportError::Protocol(e.to_string()))?;
                payload.push('\n');
                writer.write_all(payload.as_bytes()).await?;
                writer.flush().await?;

                if matches!(resp, Response::Bye { .. }) {
                    return Ok(());
                }
            }
        }
    }
}

#[async_trait]
impl Transport for NamedPipeTransport {
    async fn send(&mut self, request: &Request) -> Result<Response, TransportError> {
        use interprocess::os::windows::named_pipe::{pipe_mode, tokio::DuplexPipeStream};

        let conn = DuplexPipeStream::<pipe_mode::Bytes>::connect_by_path(&self.path)
            .await
            .map_err(|e| TransportError::Connect(format!("{}: {e}", self.path.display())))?;

        let [reader_half, mut writer] = [&conn; 2];
        let mut payload = serde_json::to_string(request)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        payload.push('\n');
        writer.write_all(payload.as_bytes()).await?;
        writer.flush().await?;

        let mut lines = BufReader::new(reader_half).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| TransportError::Protocol("连接已关闭".into()))?;
        let resp: Response = serde_json::from_str(&line)
            .map_err(|e| TransportError::Protocol(format!("响应解析失败: {e}")))?;
        Ok(resp)
    }
}
