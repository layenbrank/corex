//! Unix domain socket 传输（换行分隔的 JSON）。

use super::{Transport, TransportError, read_final, serve_connection, write_request};
use crate::progress::{FrameSink, Outlet};
use crate::protocol::{Request, Response};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// 在 Unix domain socket 上跑换行分隔的 JSON。
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

    /// 服务连接：对每个换行分隔的 JSON 请求调用 `handler`。
    pub async fn serve<F, Fut>(path: &Path, mut handler: F) -> Result<(), TransportError>
    where
        F: FnMut(Request, Outlet) -> Fut + Send,
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
        // 只限当前用户访问。
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        tracing::info!(path = %path.display(), "IPC Unix socket 已监听");

        loop {
            let (stream, _) = listener.accept().await?;
            let (reader, writer) = stream.into_split();
            serve_connection(reader, writer, &mut handler).await?;
        }
    }
}

#[async_trait]
impl Transport for UnixSocketTransport {
    async fn send_events(
        &mut self,
        request: &Request,
        sink: &dyn FrameSink,
    ) -> Result<Response, TransportError> {
        use tokio::net::UnixStream;

        let stream = UnixStream::connect(&self.path)
            .await
            .map_err(|e| TransportError::Connect(format!("{}: {e}", self.path.display())))?;
        let (reader, mut writer) = stream.into_split();
        write_request(&mut writer, request).await?;
        read_final(reader, sink).await
    }
}
