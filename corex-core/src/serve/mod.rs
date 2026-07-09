pub mod dispatch;
pub mod pipe;
pub mod protocol;
pub mod state;

/// Daemon 启动选项
#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// Named Pipe 路径，默认 `\\.\pipe\corex`
    pub pipe_name: String,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            pipe_name: r"\\.\pipe\corex".to_string(),
        }
    }
}

/// 启动 corex serve Daemon
pub fn run(options: ServeOptions) -> anyhow::Result<()> {
    let state = state::DaemonState::init()?;
    pipe::run_server(&options, &state)
}

/// IPC 客户端：调用指定模块
pub fn request(
    pipe_name: &str,
    module: &str,
    args: serde_json::Value,
) -> anyhow::Result<protocol::Response> {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    pipe::send_request(pipe_name, module, args, id)
}

/// IPC 客户端：请求 Daemon 退出
pub fn shutdown(pipe_name: &str) -> anyhow::Result<()> {
    pipe::send_shutdown(pipe_name)
}
