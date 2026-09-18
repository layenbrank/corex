//! 传输抽象：Unix domain socket 与 Windows 命名管道。

use crate::progress::{FrameSink, Outlet};
use crate::protocol::{MAX_LINE_BYTES, Request, Response, RpcError};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::pin::pin;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

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

/// 每条连接上**单个请求**的中间帧队列长度。
///
/// 满了就丢帧（见 [`FrameSink`]）：这个数字只是不让一次突发把内存撑大，
/// 而不是背压参数——**进度永远不该让执行等它**。
const FRAME_QUEUE: usize = 64;

/// 每条连接待写的响应队列长度：**所有请求共用这一条**（写口只有一个）。
///
/// 帧满即丢的取舍只在单个请求内部（`FRAME_QUEUE`）；到了这里已经是“要真的写出去”
/// 的东西，包括每条请求的终帧，所以满了就等而不是丢。
const WRITE_QUEUE: usize = 64;

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

/// 连接方要找的 token：`COREX_TOKEN` → 显式配置 → daemon 写下的记录 → `<data>/token`。
///
/// 与 [`find_endpoint`] 同一套优先级、同一处实现：命令行与配置里的东西赢过发现，
/// 发现赢过平台默认（这里是「数据目录里那个文件」）。连接方不得各写一份顺序——
/// CLI 曾经只看「环境变量 + `<data>/token`」，于是「在配置里设了 token 起的 daemon」
/// 在它眼里永远是「已停止」。
///
/// `configured` 是调用方从配置读出的 `[daemon].token`（`corex-ipc` 不解析 TOML）。
///
/// 记录里没有 `token_file` 意味着 daemon 的 token 来自 `COREX_TOKEN` 或配置——那两处的
/// 值属于调用方，不会被复制进记录。此时返回 `None` 而不是去读 `<data>/token`：那个文件
/// 属于**上一个** daemon（或者根本不存在），拿它去连只会得到一句 401。
pub fn find_token(data: &Path, configured: Option<&str>) -> Option<String> {
    if let Ok(token) = std::env::var("COREX_TOKEN")
        && !token.is_empty()
    {
        return Some(token);
    }
    if let Some(token) = configured.filter(|token| !token.is_empty()) {
        return Some(token.to_string());
    }
    match crate::endpoint::discover(data) {
        Some(record) => record.token_file.and_then(|path| read_token(&path)),
        None => read_token(&data.join("token")),
    }
}

/// 读一个 token 文件；空白或读不到都算「没有」。
fn read_token(path: &Path) -> Option<String> {
    let token = std::fs::read_to_string(path).ok()?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
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

/// 一条连接的读写循环：逐行读请求，每个请求**各自一个任务**，写口只有一条。
///
/// 一条连接上因此可以同时有多条请求在飞（协议承诺，见 `docs/reference/IPC协议.md`
/// 的并发模型）：慢请求不再让同一连接的其它请求排在它后面——宿主正是一个客户端一条
/// 连接，探活与「拉目录」都搭在上面。要不要把**执行**也串起来是上层的事
/// （见 daemon 的 `max_jobs`），这里只保证“读到就推进”。
///
/// 两条顺序约束落在 [`run_request`] 里：一个请求的帧排在它自己的终帧之前；
/// 跨请求没有顺序可言（除了一行的字节不被截断——写只有本函数一处）。
///
/// 收 `bye` 或对端断开就结束本连接：前者立刻退（服务正要收摊，在途请求一并中止，
/// 它们也没有答案可送）；后者先把在途请求的答案写完再退。
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
    F: Fn(Request, Outlet) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Response> + Send + 'static,
{
    let mut lines = BufReader::new(reader).lines();
    let (write, mut written) = mpsc::channel::<Response>(WRITE_QUEUE);
    let mut running = JoinSet::new();
    let outcome = loop {
        tokio::select! {
            // `Lines::next_line` 与 `mpsc::Receiver::recv` 都是取消安全的：
            // 没轮到的分支不会丢消息。
            line = lines.next_line() => {
                let Some(line) = line? else {
                    break Connection::Closed;
                };
                if line.trim().is_empty() {
                    continue;
                }
                // 拒收与解析失败不归任何请求，直接写——进队会在这个任务里自等而卡住。
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
                        let refused =
                            Response::error(0, RpcError::invalid(format!("请求解析失败: {e}")));
                        write_response(&mut writer, &refused).await?;
                        continue;
                    }
                };
                running.spawn(run_request(request, handler.clone(), write.clone()));
            }
            Some(response) = written.recv() => {
                write_response(&mut writer, &response).await?;
                if matches!(response, Response::Bye { .. }) {
                    break Connection::Bye;
                }
            }
            // 接住结束了的请求，免得 `JoinSet` 攒着它们的返回值。
            Some(_) = running.join_next(), if !running.is_empty() => {}
        }
    };
    // 对端断开时在途请求也得有答案。先放下自己那份发送端，否则下面这句永远等不到
    // 「所有请求都结束了」——`recv` 只在发送端全没了才回 `None`。
    drop(write);
    if matches!(outcome, Connection::Closed) {
        while let Some(response) = written.recv().await {
            write_response(&mut writer, &response).await?;
        }
    }
    // `bye` 意味着服务正要退：在途请求别接着动（UI 自动化尤其如此），
    // 它们的答案也无处可送。
    running.abort_all();
    Ok(outcome)
}

/// 推进一条请求：把它的帧与终帧按顺序交给连接的写口。
///
async fn run_request<F, Fut>(request: Request, handler: F, write: mpsc::Sender<Response>)
where
    F: Fn(Request, Outlet) -> Fut,
    Fut: std::future::Future<Output = Response>,
{
    let (frames, mut pending_frames) = mpsc::channel(FRAME_QUEUE);
    let outlet = Outlet::new(request.id(), frames);
    let mut pending = pin!(handler(request, outlet));
    let response = loop {
        tokio::select! {
            response = &mut pending => break response,
            Some(frame) = pending_frames.recv() => {
                // 写口满就等：背压只压这条请求，而帧仍排在它的终帧之前。
                if write.send(frame).await.is_err() {
                    return;
                }
            }
        }
    };
    // handler 结束时它的局部变量（连同持有帧发送端的 [`Outlet`]）一起 drop；
    // 那时把残留的帧排干净再发终帧，顺序就不会错。
    while let Ok(frame) = pending_frames.try_recv() {
        if write.send(frame).await.is_err() {
            return;
        }
    }
    let _ = write.send(response).await;
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
    use crate::progress::ProgressEvent;
    use std::time::{Duration, Instant};
    use tokio::io::{DuplexStream, Lines, ReadHalf, WriteHalf};

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

    /// 环境变量那一档优先级最高，但测试进程里没法可靠地设/清它（用例并行跑，而在
    /// edition 2024 里改进程环境是 unsafe）——设了就跳过下面几条，别让它们假红。
    fn ambient_token() -> Option<String> {
        std::env::var("COREX_TOKEN")
            .ok()
            .filter(|token| !token.is_empty())
    }

    /// daemon 在记录里指明的那个文件赢过 `<data>/token`：它才是「正在跑的那个」的 token。
    #[test]
    fn a_token_from_the_record_beats_the_data_dir_file() {
        if ambient_token().is_some() {
            return;
        }
        let dir = tempfile::tempdir().expect("临时目录");
        std::fs::write(dir.path().join("token"), "默认文件\n").expect("写默认 token");
        let recorded = dir.path().join("token.recorded");
        std::fs::write(&recorded, "  记录里的  \n").expect("写记录 token");
        crate::endpoint::publish(
            dir.path(),
            &crate::endpoint::Record::new(other_endpoint(dir.path()), Some(recorded)),
        )
        .expect("写入记录");

        assert_eq!(
            find_token(dir.path(), None).as_deref(),
            Some("记录里的"),
            "该读记录指的那个文件，并去掉首尾空白"
        );
    }

    /// 显式配置赢过发现——与端点那一侧同一条规矩。
    #[test]
    fn an_explicit_token_beats_the_record() {
        if ambient_token().is_some() {
            return;
        }
        let dir = tempfile::tempdir().expect("临时目录");
        let recorded = dir.path().join("token");
        std::fs::write(&recorded, "文件里的").expect("写 token");
        crate::endpoint::publish(
            dir.path(),
            &crate::endpoint::Record::new(other_endpoint(dir.path()), Some(recorded)),
        )
        .expect("写入记录");

        assert_eq!(
            find_token(dir.path(), Some("配置里的")).as_deref(),
            Some("配置里的")
        );
        assert_eq!(
            find_token(dir.path(), Some("")).as_deref(),
            Some("文件里的"),
            "配置里给空串等于没给"
        );
    }

    /// 记录里没有 `token_file`：说明 daemon 的 token 来自 `COREX_TOKEN` 或配置，属于调用方。
    /// 此时去读 `<data>/token` 只会拿到**上一个** daemon 留下的东西，所以宁可不给。
    #[test]
    fn a_record_without_a_token_file_leaves_the_token_to_the_caller() {
        if ambient_token().is_some() {
            return;
        }
        let dir = tempfile::tempdir().expect("临时目录");
        std::fs::write(dir.path().join("token"), "上一个 daemon 留下的").expect("写旧 token");
        crate::endpoint::publish(
            dir.path(),
            &crate::endpoint::Record::new(other_endpoint(dir.path()), None),
        )
        .expect("写入记录");

        assert_eq!(find_token(dir.path(), None), None);
    }

    /// 没有记录（daemon 还没起过）时，数据目录里的那个文件就是答案。
    #[test]
    fn without_a_record_the_data_dir_file_is_read() {
        if ambient_token().is_some() {
            return;
        }
        let dir = tempfile::tempdir().expect("临时目录");
        std::fs::write(dir.path().join("token"), "文件里的\n").expect("写 token");
        assert_eq!(find_token(dir.path(), None).as_deref(), Some("文件里的"));

        std::fs::write(dir.path().join("token"), "   \n").expect("写空白 token");
        assert_eq!(find_token(dir.path(), None), None, "空白等于没有");
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

    /// 一条连接的两端：`serve_connection` 那半边跑在任务里，客户端这半边交回来。
    ///
    /// 用内存里的双向流冒充连接：并发那条承诺是 `serve_connection` 的职责，与底层是
    /// 命名管道还是 socket 无关，所以这里不必真开一个端点（真传输由
    /// `tests/streaming.rs` 覆盖）。
    fn connection<F, Fut>(
        handler: F,
    ) -> (
        tokio::task::JoinHandle<Result<Connection, TransportError>>,
        Peer,
    )
    where
        F: Fn(Request, Outlet) -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = Response> + Send + 'static,
    {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (read, write) = tokio::io::split(server);
        let serving = tokio::spawn(serve_connection(read, write, handler));
        let (read, write) = tokio::io::split(client);
        let peer = Peer {
            writer: write,
            lines: BufReader::new(read).lines(),
        };
        (serving, peer)
    }

    /// 客户端那半边：写请求、按行收响应（一条连接上可以有多条请求在飞）。
    struct Peer {
        writer: WriteHalf<DuplexStream>,
        lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    }

    impl Peer {
        async fn send(&mut self, request: &Request) {
            let mut line = serde_json::to_string(request).expect("请求可序列化");
            line.push('\n');
            self.writer
                .write_all(line.as_bytes())
                .await
                .expect("写请求");
        }

        /// 原样写一行（用来喂坏请求）。
        async fn send_raw(&mut self, line: &str) {
            self.writer
                .write_all(line.as_bytes())
                .await
                .expect("写原始行");
        }

        async fn next(&mut self) -> Response {
            let line = self
                .lines
                .next_line()
                .await
                .expect("读响应")
                .expect("连接还在");
            serde_json::from_str(&line).expect("响应是 JSON")
        }
    }

    fn ping(id: u64) -> Request {
        Request::Ping {
            id,
            auth_token: None,
        }
    }

    fn slow_request(id: u64) -> Request {
        Request::RunDirective {
            id,
            auth_token: None,
            name: "probe".into(),
            input: Default::default(),
            path: None,
            stream: true,
        }
    }

    /// 对照 daemon 的服务器：`run_directive` 慢且推一帧，探活立刻答，`shutdown` 回 `bye`。
    fn slow_server(
        slow: Duration,
    ) -> impl Fn(
        Request,
        Outlet,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send>>
    + Clone
    + Send
    + 'static {
        move |request: Request, outlet: Outlet| {
            let slow = slow;
            Box::pin(async move {
                match request {
                    Request::RunDirective { id, .. } => {
                        outlet.offer(ProgressEvent::StepStart {
                            seq: 1,
                            step: "copy".into(),
                            action: "file.copy".into(),
                        });
                        tokio::time::sleep(slow).await;
                        Response::ok(id, "done")
                    }
                    Request::Ping { id, .. } => Response::Pong { id },
                    Request::Shutdown { id, .. } => Response::Bye { id },
                    other => Response::error(other.id(), RpcError::invalid("未预期的请求")),
                }
            })
        }
    }

    /// 慢请求不再挡住同一条连接上的其它请求。
    ///
    /// 这曾经是坏的：`serve_connection` 逐行读、跑完一条才读下一条，于是宿主（一个客户端
    /// 一条连接）在一条几分钟的指令期间连探活都发不出去。Rust 侧传输是每请求新建连接，
    /// 所以只有手写多路复用的宿主（如 `packages/corex-client`）会撞上。
    #[tokio::test]
    async fn a_slow_request_does_not_hold_up_its_neighbours() {
        let slow = Duration::from_millis(400);
        let (serving, mut peer) = connection(slow_server(slow));

        let started = Instant::now();
        peer.send(&slow_request(1)).await;
        peer.send(&ping(2)).await;

        // 两个任务是并发调度的，所以慢请求那一帧与探活的回话谁先到都可能。要钉的是
        // **探活不必等它睡完**：在它睡完之前到的，只能是探活的回话或它自己那一帧。
        let pong_ms = loop {
            match peer.next().await {
                Response::Pong { id } => {
                    assert_eq!(id, 2);
                    break started.elapsed().as_millis();
                }
                Response::Event { id, .. } => assert_eq!(id, 1, "别的请求的帧不该串进来"),
                other => panic!("慢请求睡完之前不该有它的终帧: {other:?}"),
            }
        };
        assert!(
            pong_ms < slow.as_millis() / 2,
            "探活被慢请求挡住了：{pong_ms}ms"
        );

        // 它自己的终帧排在它自己的帧之后（那一帧可能已经先到过）。
        loop {
            match peer.next().await {
                Response::Event { id, .. } => assert_eq!(id, 1, "帧不该串到别的请求上"),
                Response::Ok { id, .. } => {
                    assert_eq!(id, 1);
                    break;
                }
                other => panic!("慢请求之后不该有别的东西: {other:?}"),
            }
        }

        peer.send(&Request::Shutdown {
            id: 3,
            auth_token: None,
        })
        .await;
        assert!(matches!(peer.next().await, Response::Bye { id: 3 }));
        assert!(
            matches!(serving.await.expect("服务任务"), Ok(Connection::Bye)),
            "`bye` 该让整条连接结束"
        );
    }

    /// 坏请求只拒收它自己，连接与在途请求都还在。
    #[tokio::test]
    async fn a_broken_line_is_refused_on_its_own() {
        let (serving, mut peer) = connection(slow_server(Duration::from_millis(50)));

        peer.send_raw("这不是 JSON\n").await;
        assert!(
            matches!(peer.next().await, Response::Error { id: 0, .. }),
            "解析失败该回一条 id 为 0 的错误"
        );
        peer.send(&ping(1)).await;
        assert!(matches!(peer.next().await, Response::Pong { id: 1 }));

        serving.abort();
    }
}
