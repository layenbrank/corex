//! Windows named pipe transport via `interprocess` (newline-delimited JSON).

use super::{Transport, TransportError};
use crate::protocol::{Request, Response, RpcError, MAX_LINE_BYTES};
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

        // Default security: current-user accessible pipe (OS default for named pipes
        // created without an explicit SD is typically local-only).
        let listener = PipeListenerOptions::new()
            .path(path)
            .create_tokio_duplex::<pipe_mode::Bytes>()
            .map_err(|e| TransportError::Connect(format!("{}: {e}", path.display())))?;

        tracing::info!(path = %path.display(), "IPC Named Pipe 已监听");

        loop {
            let conn = listener.accept().await?;
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
                if line.len() > MAX_LINE_BYTES {
                    let resp = Response::error(
                        0,
                        RpcError::invalid(format!(
                            "请求超过最大长度 {MAX_LINE_BYTES} 字节"
                        )),
                    );
                    write_response(&mut writer, &resp).await?;
                    continue;
                }
                let resp = match serde_json::from_str::<Request>(&line) {
                    Ok(req) => handler(req).await,
                    Err(e) => Response::error(
                        0,
                        RpcError::invalid(format!("请求解析失败: {e}")),
                    ),
                };
                write_response(&mut writer, &resp).await?;

                if matches!(resp, Response::Bye { .. }) {
                    return Ok(());
                }
            }
        }
    }
}

async fn write_response<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    resp: &Response,
) -> Result<(), TransportError> {
    let mut payload =
        serde_json::to_string(resp).map_err(|e| TransportError::Protocol(e.to_string()))?;
    payload.push('\n');
    writer.write_all(payload.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

#[async_trait]
impl Transport for NamedPipeTransport {
    async fn send(&mut self, request: &Request) -> Result<Response, TransportError> {
        use interprocess::os::windows::named_pipe::{pipe_mode, tokio::DuplexPipeStream};

        let conn = DuplexPipeStream::<pipe_mode::Bytes>::connect_by_path(self.path.as_path())
            .await
            .map_err(|e| TransportError::Connect(format!("{}: {e}", self.path.display())))?;

        let [reader_half, mut writer] = [&conn; 2];
        let mut payload = serde_json::to_string(request)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        if payload.len() > MAX_LINE_BYTES {
            return Err(TransportError::Protocol(format!(
                "请求超过最大长度 {MAX_LINE_BYTES} 字节"
            )));
        }
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
