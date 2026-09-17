//! 传输抽象：Unix domain socket 与 Windows 命名管道。

use crate::progress::{FrameSink, Outlet};
use crate::protocol::{MAX_LINE_BYTES, Request, Response, RpcError};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::pin::pin;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

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

/// 每条连接上待写的中间帧队列长度。
///
/// 满了就丢帧（见 [`FrameSink`]）：这个数字只是不让一次突发把内存撑大，
/// 而不是背压参数——**进度永远不该让执行等它**。
const FRAME_QUEUE: usize = 64;

/// 丢掉中间帧的落点。[`Transport::send`] 用它把流式读取退化成旧的一问一答。
struct Discard;

impl FrameSink for Discard {
    fn frame(&self, _progress: &crate::progress::ProgressEvent) {}
}

/// IPC 传输抽象。
#[async_trait]
pub trait Transport: Send + Sync {
    /// 发送请求，并在等待终帧的期间把中间帧交给 `sink`。
    ///
    /// 这是唯一需要实现的方法。读取循环**必须跳过** `Response::Event`
    /// （它们是中间帧），直到读到终帧（`ok` / `error` / `pong` / `bye`）才返回。
    async fn send_events(
        &mut self,
        request: &Request,
        sink: &dyn FrameSink,
    ) -> Result<Response, TransportError>;

    /// 发送请求并取回终帧，途中的进度帧直接丢掉。
    async fn send(&mut self, request: &Request) -> Result<Response, TransportError> {
        self.send_events(request, &Discard).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("连接失败: {0}")]
    Connect(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("协议错误: {0}")]
    Protocol(String),
    #[error("端点无效: {0}")]
    InvalidEndpoint(String),
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
/// 顺序：**`COREX_DATA_DIR`** → 可写的 exe 目录 → 操作系统的项目数据目录 → `.corex`。
///
/// 第一个来源是给宿主与测试用的：随应用分发 corex 时，“数据到底落在哪”不该由 exe 在不在
/// 可写目录这类事实决定；测试也需要一个钉得住的目录，否则用例会写到构建产物旁边去。
pub fn data_dir() -> std::io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("COREX_DATA_DIR").filter(|v| !v.is_empty()) {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir)?;
        return Ok(dir);
    }
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
///
/// 这是**唯一**的默认端点来源：配置里不写 `socket_path` 就走它，所以默认值天然是按平台的。
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

/// 解析配置里的路径：绝对路径原样；相对路径拼到 `data` 下。
///
/// Windows 上 `\\.\pipe\...`（以及 `//./pipe/...`）那一类属于命名管道名字，直接用。
pub fn resolve_data_relative(data: &Path, path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        if is_pipe_name(path) {
            return path.to_path_buf();
        }
    }
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        data.join(path)
    }
}

/// 从配置解析出实际使用的 IPC 端点；`configured` 为空时就是平台默认端点。
///
/// 这是**监听方**的判定：只认配置与平台默认。它刻意不读 `endpoint.json`——崩溃残留的
/// 记录会把新起的 daemon 带到上一个进程的端点上，而 daemon 该在哪监听只由它自己的
/// 配置决定。连接方要找的是另一个问题，见 [`find_endpoint`]。
///
/// Windows 上端点**只能是命名管道**。配置里写成 Unix 风格的 `corex.sock` 时，
/// 底层的 `CreateNamedPipe` 会报「文件名、目录名或卷标语法不正确」（os error 123）——
/// 那句话完全指不出真正的原因，所以在这里先拦下。这类配置在 Unix 上完全正确，
/// 跨平台共用一份配置时很容易踩到。
pub fn resolve_endpoint(data: &Path, configured: Option<&Path>) -> Result<PathBuf, TransportError> {
    let Some(configured) = configured else {
        return Ok(ipc_endpoint(data));
    };
    let endpoint = resolve_data_relative(data, configured);
    #[cfg(windows)]
    {
        if !is_pipe_name(&endpoint) {
            return Err(TransportError::InvalidEndpoint(format!(
                "Windows 上的 IPC 端点是命名管道，不能是文件路径: {}（写成 {}，或省略该项以用平台默认）",
                endpoint.display(),
                ipc_endpoint(data).display()
            )));
        }
    }
    Ok(endpoint)
}

/// 连接方要找的端点：显式配置 → daemon 写下的记录 → 平台默认。
///
/// 与 [`resolve_endpoint`] 的区别只有一个：这里**读** `endpoint.json`。连接方与 daemon
/// 可能拿着不同的配置（宿主自己带了 `--config`、用户手工 `corex daemon start` 过），
/// 此时只有 daemon 自己写下的那份记录说的是事实。
///
/// 显式配置仍然排在最前面：命令行与配置里写死的东西必须赢过发现，否则“我明明指定了”
/// 会变成最难查的那类问题。
pub fn find_endpoint(data: &Path, configured: Option<&Path>) -> Result<PathBuf, TransportError> {
    if configured.is_none()
        && let Some(record) = crate::endpoint::discover(data)
    {
        return Ok(record.endpoint);
    }
    resolve_endpoint(data, configured)
}

/// Windows 命名管道名字（`\\.\pipe\...` / `//./pipe/...`）。
#[cfg(windows)]
fn is_pipe_name(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.starts_with(r"\\.\pipe\") || s.starts_with("//./pipe/")
}

/// `endpoint` 的客户端传输。
pub fn ipc_connect(endpoint: impl Into<PathBuf>) -> PlatformTransport {
    PlatformTransport::new(endpoint)
}

/// 在平台传输上服务 NDJSON 请求。
///
/// `handler` 除请求外还会收到一个 [`Outlet`]：只有当请求置了 `stream` 时 daemon
/// 才拿它推帧；不推的请求一行多余输出也不会有。
///
/// **每条连接跑在自己的任务里**，所以 `handler` 必须可克隆、可跨任务移动
/// （用 `Arc` 捕获共享状态即可）。串行地一条条服务会把慢请求变成对所有人的阻塞：
/// 一条几分钟的指令期间，另一个客户端连 `ping` 都发不出去——而探活正是宿主判断
/// “它还活着吗”的手段。要不要把**执行**也串起来是上层的事（见 daemon 的 `max_jobs`）。
///
/// 收到 `bye` 的那条连接会让整个服务返回（见 [`Connection`]）。
pub async fn serve_ipc<F, Fut>(endpoint: &Path, handler: F) -> Result<(), TransportError>
where
    F: Fn(Request, Outlet) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Response> + Send + 'static,
{
    PlatformTransport::serve(endpoint, handler).await
}

/// 一条连接的结局。
///
/// 服务方必须把「对端断开」与「收到 `bye`」分开：前者只是这次连接结束，后者意味着整个
/// 服务该退了（`shutdown` 请求的回答）。分不开的话 `corex daemon stop` 只会让 daemon
/// 停止干活，进程却一直挂在 `accept` 上——于是它写下的端点记录永远等不到清理。
pub(crate) enum Connection {
    /// 对端断开，或输入读完。
    Closed,
    /// 收到 `bye`：服务方该返回了。
    Bye,
}

/// 「有一条连接收到 `bye`」的信号。
///
/// 每连接一个任务之后，accept 循环看不到 [`Connection::Bye`] 的返回值了，所以用一个
/// 容量 1 的通道把它送回来。容量 1 就够——整个服务只需要被通知一次，
/// 后来的通知会被 `try_send` 静默丢掉。
pub(crate) fn stop_channel() -> (
    tokio::sync::mpsc::Sender<()>,
    tokio::sync::mpsc::Receiver<()>,
) {
    tokio::sync::mpsc::channel(1)
}

/// 一条连接的读写循环：逐行读请求，允许 handler 推中间帧，最后写回终帧。
///
/// **帧必须先于终帧出去**，所以帧的出队与 handler 的推进在同一个任务里多路复用，
/// 而不是把 writer 交给 handler 自己抢。两个平台传输的这段逻辑完全一致，只有底层流
/// 类型不同，因此只写一份。
///
/// handler 结束时它的局部变量（连同 [`Outlet`]）一起 drop，发送端随即关闭；
/// 那时把残留的帧排干净再写终帧，顺序就不会错。
///
/// `handler` 按值传：调用方（平台传输）把克隆出来的一份移进本连接所在的任务。
pub(crate) async fn serve_connection<R, W, F, Fut>(
    reader: R,
    mut writer: W,
    handler: F,
) -> Result<Connection, TransportError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: Fn(Request, Outlet) -> Fut,
    Fut: std::future::Future<Output = Response>,
{
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MAX_LINE_BYTES {
            let refused = Response::error(
                0,
                RpcError::invalid(format!("请求超过最大长度 {MAX_LINE_BYTES} 字节")),
            );
            write_response(&mut writer, &refused).await?;
            continue;
        }
        let request = match serde_json::from_str::<Request>(&line) {
            Ok(request) => request,
            Err(e) => {
                let refused = Response::error(0, RpcError::invalid(format!("请求解析失败: {e}")));
                write_response(&mut writer, &refused).await?;
                continue;
            }
        };

        let (tx, mut frames) = tokio::sync::mpsc::channel(FRAME_QUEUE);
        let outlet = Outlet::new(request.id(), tx);
        let mut pending = pin!(handler(request, outlet));
        let response = loop {
            tokio::select! {
                response = &mut pending => break response,
                // `mpsc::Receiver::recv` 是取消安全的：没轮到时不会丢消息。
                Some(frame) = frames.recv() => write_response(&mut writer, &frame).await?,
            }
        };
        while let Ok(frame) = frames.try_recv() {
            write_response(&mut writer, &frame).await?;
        }
        write_response(&mut writer, &response).await?;

        if matches!(response, Response::Bye { .. }) {
            return Ok(Connection::Bye);
        }
    }
    Ok(Connection::Closed)
}

/// 写一条 NDJSON 帧。
pub(crate) async fn write_response<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    response: &Response,
) -> Result<(), TransportError> {
    let mut payload =
        serde_json::to_string(response).map_err(|e| TransportError::Protocol(e.to_string()))?;
    payload.push('\n');
    writer.write_all(payload.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

/// 写一条请求，并校验单行长度上限。
pub(crate) async fn write_request<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    request: &Request,
) -> Result<(), TransportError> {
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
    Ok(())
}

/// 从一条 NDJSON 连接上读到终帧：中途的 `event` 帧交给 `sink`。
pub(crate) async fn read_final<R: AsyncRead + Unpin>(
    reader: R,
    sink: &dyn FrameSink,
) -> Result<Response, TransportError> {
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let response: Response = serde_json::from_str(&line)
            .map_err(|e| TransportError::Protocol(format!("响应解析失败: {e}")))?;
        match response {
            Response::Event { progress, .. } => sink.frame(&progress),
            final_response => return Ok(final_response),
        }
    }
    Err(TransportError::Protocol("连接已关闭".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_configured_means_the_platform_default() {
        let data = Path::new("/data/corex");
        assert_eq!(
            resolve_endpoint(data, None).expect("平台默认端点总是有效的"),
            ipc_endpoint(data)
        );
    }

    /// 相对路径落到数据目录下（Unix），在 Windows 上则因为“不是管道”被拦下——
    /// 这是同一份跨平台配置在两边得到不同答案的唯一一点，必须显式钉住。
    #[test]
    fn a_relative_file_path_is_platform_dependent() {
        let data = Path::new("base");
        let resolved = resolve_endpoint(data, Some(Path::new("custom.sock")));
        #[cfg(unix)]
        assert_eq!(resolved.expect("相对 socket 名"), data.join("custom.sock"));
        #[cfg(windows)]
        assert!(
            matches!(resolved, Err(TransportError::InvalidEndpoint(_))),
            "Windows 上的文件路径不是合法端点: {resolved:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_pipe_name_is_taken_as_is() {
        let pipe = Path::new(r"\\.\pipe\custom");
        assert_eq!(
            resolve_endpoint(Path::new("base"), Some(pipe)).expect("管道名"),
            pipe
        );
        // 正斜杠那一种写法同样认。
        let slashed = Path::new("//./pipe/custom");
        assert_eq!(
            resolve_endpoint(Path::new("base"), Some(slashed)).expect("管道名"),
            slashed
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_absolute_path_is_taken_as_is() {
        let absolute = Path::new("/var/run/corex.sock");
        assert_eq!(
            resolve_endpoint(Path::new("base"), Some(absolute)).expect("绝对路径"),
            absolute
        );
    }

    /// 一个与平台默认**明确不同**的合法端点，用来区分「读到了记录」与「退回默认」。
    fn other_endpoint(data: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            let _ = data;
            PathBuf::from(r"\\.\pipe\corex-test-other")
        }
        #[cfg(unix)]
        {
            data.join("other.sock")
        }
    }

    #[test]
    fn a_published_record_beats_the_platform_default() {
        let dir = tempfile::tempdir().expect("临时目录");
        let published = other_endpoint(dir.path());
        crate::endpoint::publish(
            dir.path(),
            &crate::endpoint::Record::new(published.clone(), None),
        )
        .expect("写入记录");

        assert_eq!(find_endpoint(dir.path(), None).expect("读记录"), published);
    }

    #[test]
    fn without_a_record_it_falls_back_to_the_platform_default() {
        let dir = tempfile::tempdir().expect("临时目录");
        assert_eq!(
            find_endpoint(dir.path(), None).expect("平台默认"),
            ipc_endpoint(dir.path())
        );
    }

    /// 显式配置必须赢过发现：命令行与配置里写死的东西被一份记录盖掉，是最难查的那类问题。
    #[test]
    fn an_explicit_endpoint_beats_the_record() {
        let dir = tempfile::tempdir().expect("临时目录");
        crate::endpoint::publish(
            dir.path(),
            &crate::endpoint::Record::new(other_endpoint(dir.path()), None),
        )
        .expect("写入记录");

        let wanted = ipc_endpoint(dir.path());
        assert_eq!(
            find_endpoint(dir.path(), Some(&wanted)).expect("显式端点"),
            wanted
        );
    }

    /// 监听方**不读**记录：崩溃残留会让新 daemon 去监听上一个进程的端点。
    #[test]
    fn the_listener_ignores_the_record() {
        let dir = tempfile::tempdir().expect("临时目录");
        crate::endpoint::publish(
            dir.path(),
            &crate::endpoint::Record::new(other_endpoint(dir.path()), None),
        )
        .expect("写入记录");

        assert_eq!(
            resolve_endpoint(dir.path(), None).expect("平台默认"),
            ipc_endpoint(dir.path())
        );
    }
}
