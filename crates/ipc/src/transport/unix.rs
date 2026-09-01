//! Unix domain socket transport (newline-delimited JSON).

use super::{Transport, TransportError};
use crate::protocol::{MAX_LINE_BYTES, Request, Response, RpcError};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Newline-delimited JSON over a Unix domain socket.
#[derive(Debug, Clone)]
pub struct UnixSocketTransport {
    path: PathBuf,
}

impl UnixSocketTransport {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
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
        use std::os::unix::fs::PermissionsExt;
        use tokio::net::UnixListener;

        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(path)?;
        // Restrict to current user only.
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        tracing::info!(path = %path.display(), "IPC Unix socket 已监听");

        loop {
            let (stream, _) = listener.accept().await?;
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            while let Some(line) = lines.next_line().await? {
                if line.trim().is_empty() {
                    continue;
                }
                if line.len() > MAX_LINE_BYTES {
                    let resp = Response::error(
                        0,
                        RpcError::invalid(format!("请求超过最大长度 {MAX_LINE_BYTES} 字节")),
                    );
                    write_response(&mut writer, &resp).await?;
                    continue;
                }
                let resp = match serde_json::from_str::<Request>(&line) {
                    Ok(req) => handler(req).await,
                    Err(e) => Response::error(0, RpcError::invalid(format!("请求解析失败: {e}"))),
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
impl Transport for UnixSocketTransport {
    async fn send(&mut self, request: &Request) -> Result<Response, TransportError> {
        use tokio::net::UnixStream;

        let stream = UnixStream::connect(&self.path)
            .await
            .map_err(|e| TransportError::Connect(format!("{}: {e}", self.path.display())))?;
        let (reader, mut writer) = stream.into_split();
        let mut payload =
            serde_json::to_string(request).map_err(|e| TransportError::Protocol(e.to_string()))?;
        if payload.len() > MAX_LINE_BYTES {
            return Err(TransportError::Protocol(format!(
                "请求超过最大长度 {MAX_LINE_BYTES} 字节"
            )));
        }
        payload.push('\n');
        writer.write_all(payload.as_bytes()).await?;
        writer.flush().await?;

        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| TransportError::Protocol("连接已关闭".into()))?;
        let resp: Response = serde_json::from_str(&line)
            .map_err(|e| TransportError::Protocol(format!("响应解析失败: {e}")))?;
        Ok(resp)
    }
}
