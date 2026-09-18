//! 经 `interprocess` 的 Windows 命名管道传输（换行分隔的 JSON）。

use super::{
    Connection, Transport, TransportError, read_final, serve_connection, stop_channel,
    write_request,
};
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

    /// 服务连接：每条连接跑在自己的任务里，对每个换行分隔的 JSON 请求调用 `handler`。
    ///
    /// `ready` 在管道真的可以连之后调一次（见 [`serve_ipc_ready`](crate::serve_ipc_ready)）。
    pub async fn serve<F, Fut>(
        path: &Path,
        ready: impl FnOnce(),
        handler: F,
    ) -> Result<(), TransportError>
    where
        F: Fn(Request, Outlet) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        use interprocess::os::windows::named_pipe::{PipeListenerOptions, pipe_mode};

        // 默认安全设置：当前用户可访问的管道（未显式指定 SD 的命名管道，
        // 按操作系统默认通常是仅本机可用）。
        //
        // 建不出来时把话说清楚：管道名被占时 Windows 报的是「拒绝访问（os error 5）」，
        // 听起来像权限问题，实际原因通常是「已经有 daemon 在用这个端点」。
        let listener = PipeListenerOptions::new()
            .path(path)
            .create_tokio_duplex::<pipe_mode::Bytes>()
            .map_err(|e| {
                TransportError::Connect(format!(
                    "创建命名管道 {} 失败: {e}（该端点多半已被占用：已有 daemon 在跑，或别的程序占了同名管道；需要并存就显式设 `socket_path`）",
                    path.display()
                ))
            })?;

        tracing::info!(path = %path.display(), "IPC Named Pipe 已监听");
        ready();

        let (stop, mut stopped) = stop_channel();
        loop {
            tokio::select! {
                _ = stopped.recv() => return Ok(()),
                accepted = listener.accept() => {
                    let conn = accepted?;
                    let handler = handler.clone();
                    let stop = stop.clone();
                    tokio::spawn(async move {
                        let [reader, writer] = [&conn; 2];
                        match serve_connection(reader, writer, handler).await {
                            Ok(Connection::Bye) => {
                                let _ = stop.try_send(());
                            }
                            Ok(Connection::Closed) => {}
                            // 一条连接的 IO 错误（对端粗鲁地断开）不该带走整个 daemon。
                            Err(e) => tracing::debug!(error = %e, "连接结束"),
                        }
                    });
                }
            }
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
