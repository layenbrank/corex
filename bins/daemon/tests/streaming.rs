//! `corex-daemon` 的进度帧：只有真的把这个二进制拉起来才验得准。
//!
//! 所以这里不 mock 任何东西——起真进程、连真的命名管道 / Unix socket、跑一条真的指令。
//! 守护进程是唯一能加载 WASM 插件的执行者，它的 IPC 回话形状只能这样验。

use corex_core::Value;
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{FrameSink, ProgressEvent, Transport, ipc_connect};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

/// 固定的测试 token：`COREX_TOKEN` 优先于配置文件，两边都给同一个值才不必读文件。
const TOKEN: &str = "corex-daemon-streaming-test";

/// 把收到的帧按顺序记下来的落点。
#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<ProgressEvent>>,
}

impl Recorder {
    fn take(&self) -> Vec<ProgressEvent> {
        self.seen.lock().expect("recorder lock").clone()
    }
}

impl FrameSink for Recorder {
    fn frame(&self, progress: &ProgressEvent) {
        self.seen
            .lock()
            .expect("recorder lock")
            .push(progress.clone());
    }
}

/// 起进程的护栏：测试无论怎么退出（含 panic）都要把 daemon 收掉，
/// 否则下一次运行会撞上单实例锁。
struct Daemon {
    child: Child,
    log: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Daemon {
    fn spawn(config: &Path, directives: &Path, log: &Path, data: &Path) -> Self {
        let out = std::fs::File::create(log).expect("daemon log");
        let err = out.try_clone().expect("daemon log clone");
        let child = Command::new(env!("CARGO_BIN_EXE_corex-daemon"))
            .arg("--config")
            .arg(config)
            .arg("--directives")
            .arg(directives)
            .env("COREX_TOKEN", TOKEN)
            // 钉住数据目录。不钉的话 `data_dir()` 会退到二进制所在的目录（构建产物旁边），
            // 端点记录与历史都写到那儿去，而且几个用例共用一份会互相覆盖。
            .env("COREX_DATA_DIR", data)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .spawn()
            .expect("spawn corex-daemon");
        Self {
            child,
            log: log.to_path_buf(),
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// 等到进程自己退出。只有走有序清理路径（收 `shutdown`）才会发生。
    async fn wait_exit(&mut self) {
        for _ in 0..300 {
            if let Some(status) = self.child.try_wait().expect("poll daemon") {
                assert!(
                    status.success(),
                    "corex-daemon 退出码异常 {status}:\n{}",
                    self.log_text()
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("corex-daemon 没有在 6s 内退出:\n{}", self.log_text());
    }

    fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// 连上为止；连不上就把 daemon 的日志贴出来——否则失败只剩一句“连接失败”。
    async fn wait_ready(&mut self, endpoint: &Path, token: &str) {
        for _ in 0..300 {
            if let Some(status) = self.child.try_wait().expect("poll daemon") {
                panic!(
                    "corex-daemon 提前退出（{status}）:\n{}",
                    std::fs::read_to_string(&self.log).unwrap_or_default()
                );
            }
            let ping = Request::Ping {
                id: 0,
                auth_token: None,
            }
            .with_auth_token(token);
            if ipc_connect(endpoint).send(&ping).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "corex-daemon 没有在 6s 内就绪:\n{}",
            std::fs::read_to_string(&self.log).unwrap_or_default()
        );
    }
}

/// 每个用例自己的端点与锁：命名管道是全局的，而测试是并行跑的。
fn endpoint(tag: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(format!(
            r"\\.\pipe\corex-daemon-test-{}-{tag}",
            std::process::id()
        ))
    }
    #[cfg(unix)]
    {
        std::env::temp_dir().join(format!(
            "corex-daemon-test-{}-{tag}.sock",
            std::process::id()
        ))
    }
}

/// 写一份只服务这个用例的配置：端点、锁、token 都钉住，不碰用户的数据目录。
///
/// `extra` 原样接在 `[daemon]` 里，用来直接写要测的那个键（如 `max_jobs = 2`）。
fn write_config(dir: &Path, endpoint: &Path, extra: &str) -> PathBuf {
    let path = dir.join("corex-daemon-test.toml");
    // 用 TOML 的字面量字符串（单引号）：Windows 管道路径里的反斜杠不该被当成转义。
    let text = format!(
        "[daemon]\nsocket_path = '{}'\nlock_path = '{}'\ntoken = '{TOKEN}'\n{extra}",
        endpoint.display(),
        dir.join("daemon.lock").display()
    );
    std::fs::write(&path, text).expect("write config");
    path
}

/// 起一个 daemon，并把它的指令目录与端点一起交回来。
async fn start(tag: &str) -> (tempfile::TempDir, Daemon, PathBuf) {
    start_with(tag, "").await
}

/// 同上，但可以往 `[daemon]` 里多写几行。
async fn start_with(tag: &str, extra: &str) -> (tempfile::TempDir, Daemon, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let directives = dir.path().join("directives");
    std::fs::create_dir_all(&directives).expect("directives dir");
    let endpoint = endpoint(tag);
    let config = write_config(dir.path(), &endpoint, extra);
    let mut daemon = Daemon::spawn(
        &config,
        &directives,
        &dir.path().join("daemon.log"),
        dir.path(),
    );
    daemon.wait_ready(&endpoint, TOKEN).await;
    (dir, daemon, endpoint)
}

fn authed(request: Request) -> Request {
    request.with_auth_token(TOKEN)
}

/// `ui.wait` 的参数。
///
/// 跨平台可用的「可靠慢动作」只有它：其余内置动作要么依赖外部环境（shell / UI / 网络），
/// 要么快得没法用来测时序。`ui.*` 的实现在 Windows 上，所以这一组用例也只在那里跑。
#[cfg(windows)]
fn wait_params(ms: i64) -> Value {
    Value::Map(std::collections::BTreeMap::from([(
        "ms".to_string(),
        Value::Int(ms),
    )]))
}

/// 同时发两条 `ui.wait`，返回总共花了多久。
#[cfg(windows)]
async fn two_waits(endpoint: &Path, ms: i64) -> u128 {
    let target = endpoint.to_path_buf();
    // 传输要在 async 块内部造：`send` 借的是 `&mut self`，把临时值放在块外会直接编译不过。
    let invoke = |id: u64| {
        let target = target.clone();
        async move {
            let mut transport = ipc_connect(target);
            transport
                .send(&authed(Request::Invoke {
                    id,
                    auth_token: None,
                    action: "ui.wait".into(),
                    params: wait_params(ms),
                    stream: false,
                }))
                .await
        }
    };
    let started = std::time::Instant::now();
    // 两个 future 先都建好，`join!` 再一起推进：两条请求是**同时**在飞的。
    let (first, second) = tokio::join!(invoke(1), invoke(2));
    first.expect("第一条");
    second.expect("第二条");
    started.elapsed().as_millis()
}

/// 取一个字符串数组字段；缺失或类型不对都当空表。
fn strings(value: &Value, path: &str) -> Vec<String> {
    value
        .find_path(path)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `list_actions` 回的不是一串 id，而是完整目录：宿主与 agent 靠它知道「怎么调」——
/// 参数类型、默认值与要声明的权限都在里面。形状与 `corex actions --json` 一致。
#[tokio::test]
async fn list_actions_carries_the_action_catalog() {
    let (_dir, _daemon, endpoint) = start("actions").await;
    let response = ipc_connect(&endpoint)
        .send(&authed(Request::ListActions {
            id: 1,
            auth_token: None,
        }))
        .await
        .expect("list_actions");
    let Response::Ok { data, .. } = response else {
        panic!("list_actions 失败: {response:?}");
    };
    let Value::Array(actions) = data else {
        panic!("list_actions 应当回一个数组: {data:?}");
    };
    let copy = actions
        .iter()
        .find(|item| item.find_path("id").and_then(|v| v.as_str()) == Some("file.copy"))
        .expect("file.copy 在目录里");
    assert_eq!(strings(copy, "permissions"), vec!["filesystem"]);
    assert!(
        copy.find_path("params")
            .and_then(|v| v.as_array())
            .is_some_and(|items| !items.is_empty()),
        "参数表不该是空的"
    );

    let schema = copy
        .find_path("input_schema")
        .expect("目录里带 input_schema");
    assert_eq!(
        schema.find_path("type").and_then(|v| v.as_str()),
        Some("object")
    );
    let required = strings(schema, "required");
    assert!(required.iter().any(|name| name == "from"), "{required:?}");
    assert!(required.iter().any(|name| name == "to"), "{required:?}");
}

/// daemon 把「它到底监听在哪」写进数据目录，有序退出时删掉。
///
/// 连接方（宿主、脚本、JS SDK）靠这份记录，不必再自己猜端点是哪一个、数据目录在哪。
/// 这里走 `shutdown` 而不是 kill：只有有序退出才跑得到清理，而 kill 留下的残留记录
/// 是这份设计**明确接受**的代价（`endpoint::discover` 的注释里写清了）。
#[tokio::test]
async fn the_endpoint_is_discoverable_while_running() {
    let (dir, mut daemon, endpoint) = start("discover").await;

    let record = corex_ipc::endpoint::discover(dir.path()).expect("运行中该有端点记录");
    assert_eq!(record.endpoint, endpoint);
    assert_eq!(record.pid, daemon.pid());
    // token 来自 `COREX_TOKEN`，属于调用方，不该被复制进记录。
    assert_eq!(record.token_file, None);
    #[cfg(windows)]
    assert_eq!(record.kind, corex_ipc::endpoint::Kind::Pipe);
    #[cfg(unix)]
    assert_eq!(record.kind, corex_ipc::endpoint::Kind::Socket);

    let response = ipc_connect(&endpoint)
        .send(&authed(Request::Shutdown {
            id: 9,
            auth_token: None,
        }))
        .await
        .expect("shutdown");
    assert!(matches!(response, Response::Bye { .. }), "{response:?}");
    daemon.wait_exit().await;

    assert!(
        corex_ipc::endpoint::discover(dir.path()).is_none(),
        "有序退出后该删掉记录"
    );
}

/// 长请求期间别的客户端还得能探活。
///
/// 这曾经是坏的：accept 是**串行**的，一个客户端正跑着长请求时，另一个客户端连都连不上——
/// 而探活正是宿主判断“daemon 还活着吗”的手段，被一条几分钟的指令堵住是最糟的形态。
/// 修法是每连接一个任务；“执行要不要也串起来”是另一件事，见下面两条用例。
#[cfg(windows)]
#[tokio::test]
async fn a_long_invoke_does_not_block_ping() {
    let (_dir, _daemon, endpoint) = start("ping-while-busy").await;

    let slow_endpoint = endpoint.clone();
    let slow = tokio::spawn(async move {
        let mut transport = ipc_connect(slow_endpoint);
        transport
            .send(&authed(Request::Invoke {
                id: 1,
                auth_token: None,
                action: "ui.wait".into(),
                params: wait_params(1200),
                stream: false,
            }))
            .await
    });
    // 给慢请求一点提前量，让它真的发出去。它要跑 1200ms，这点提前量不影响判断：
    // 修之前这条探活得等它跑完（≈1200ms），修之后是毫秒级。
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut ping_transport = ipc_connect(endpoint);
    let started = std::time::Instant::now();
    let ping = ping_transport
        .send(&authed(Request::Ping {
            id: 2,
            auth_token: None,
        }))
        .await;
    let ping_ms = started.elapsed().as_millis();

    assert!(matches!(ping, Ok(Response::Pong { .. })), "{ping:?}");
    assert!(ping_ms < 400, "探活被长请求堵住了：{ping_ms}ms");
    assert!(slow.await.expect("慢请求任务").is_ok(), "慢请求该正常结束");
}

/// 默认（`max_jobs = 1`）执行是**串行**的：两条 400ms 的请求不可能少于 800ms 跑完。
///
/// 这条是 UI 自动化的保险：两条指令同时驱鼠标键盘必然互相踩。所以“并行”是**显式选择**，
/// 不是升级后的默认。
#[cfg(windows)]
#[tokio::test]
async fn the_default_serializes_execution() {
    let (_dir, _daemon, endpoint) = start("serial-jobs").await;
    let elapsed = two_waits(&endpoint, 400).await;
    assert!(elapsed >= 720, "默认该是串行的，实测 {elapsed}ms");
}

/// `max_jobs = 2` 时两条请求真的并排跑：400ms 级的活儿不该串成 800ms。
#[cfg(windows)]
#[tokio::test]
async fn a_larger_jobs_limit_runs_them_together() {
    let (_dir, _daemon, endpoint) = start_with("parallel-jobs", "max_jobs = 2\n").await;
    let elapsed = two_waits(&endpoint, 400).await;
    assert!(
        elapsed < 720,
        "两条 400ms 的请求该并排跑完，实测 {elapsed}ms"
    );
}

/// `invoke` 上置了 `stream` 时，动作内部的 `ctx.chunk()` 要真的变成帧。
///
/// 这条路径与指令那条不同：它绕过了 `Pipeline`，所以 `enter_step` 得自己补——
/// 补漏了的话帧会一个都不来（`ctx.chunk()` 是空操作），而这是静默的。
#[tokio::test]
async fn invoke_streams_the_action_chunk_progress() {
    let (dir, _daemon, endpoint) = start("invoke").await;
    let source = dir.path().join("big.bin");
    // 3 MiB：够 file.copy 分三次上报（每块 1 MiB），又不至于让测试变慢。
    std::fs::write(&source, vec![0u8; 3 * 1024 * 1024]).expect("write source");
    let target = dir.path().join("big-copy.bin");

    let recorder = Recorder::default();
    let response = ipc_connect(&endpoint)
        .send_events(
            &authed(Request::Invoke {
                id: 3,
                auth_token: None,
                action: "file.copy".into(),
                params: corex_core::Value::Map(std::collections::BTreeMap::from([
                    ("from".to_string(), Value::Str(source.display().to_string())),
                    ("to".to_string(), Value::Str(target.display().to_string())),
                ])),
                stream: true,
            }),
            &recorder,
        )
        .await
        .expect("invoke");

    assert!(
        matches!(response, Response::Ok { id, .. } if id == 3),
        "{response:?}"
    );
    let seen = recorder.take();
    assert!(
        matches!(seen.first(), Some(ProgressEvent::StepStart { step, action, .. })
            if step == "invoke" && action == "file.copy"),
        "第一步应当是 file.copy 的开始帧: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|event| matches!(event, ProgressEvent::StepProgress { done, .. } if *done > 0)),
        "应当收到分块进度: {seen:?}"
    );
    assert!(
        matches!(seen.last(), Some(ProgressEvent::StepEnd { ok: true, .. })),
        "最后应当是成功的结束帧: {seen:?}"
    );
    assert!(target.exists(), "动作确实执行了");
}

/// `run_directive` 上置了 `stream` 时，每个动作步骤都要有开始 / 结束帧，
/// 且终帧排在所有帧之后。
#[tokio::test]
async fn run_directive_streams_every_step() {
    let (dir, _daemon, endpoint) = start("directive").await;
    let directives = dir.path().join("directives");
    std::fs::write(
        directives.join("probe.yaml"),
        concat!(
            "name: probe\n",
            "permissions:\n",
            "  filesystem: true\n",
            "steps:\n",
            "  - id: render\n",
            "    action: template.render\n",
            "    params:\n",
            "      template: \"hi\"\n",
            "    save_to: message\n",
            "  - id: write\n",
            "    action: file.write\n",
            "    params:\n",
            "      path: \"{{env.TEMP}}/corex-daemon-stream-probe.txt\"\n",
            "      content: \"{{message}}\"\n",
        ),
    )
    .expect("write directive");

    let recorder = Recorder::default();
    let response = ipc_connect(&endpoint)
        .send_events(
            &authed(Request::RunDirective {
                id: 4,
                auth_token: None,
                name: "probe".into(),
                input: HashMap::new(),
                path: None,
                stream: true,
            }),
            &recorder,
        )
        .await
        .expect("run_directive");

    assert!(
        matches!(response, Response::Ok { id, .. } if id == 4),
        "{response:?}"
    );
    let steps: Vec<String> = recorder
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ProgressEvent::StepStart { step, seq, .. } => Some(format!("{seq}:{step}")),
            _ => None,
        })
        .collect();
    assert_eq!(steps, vec!["1:render", "2:write"]);
}

/// 不问就不给：没置 `stream` 的请求一帧都不该收到，线的形状与旧版完全一致。
#[tokio::test]
async fn a_plain_request_stays_one_shot() {
    let (dir, _daemon, endpoint) = start("plain").await;
    let directives = dir.path().join("directives");
    std::fs::write(
        directives.join("probe.yaml"),
        concat!(
            "name: probe\n",
            "steps:\n",
            "  - id: render\n",
            "    action: template.render\n",
            "    params:\n",
            "      template: \"hi\"\n",
        ),
    )
    .expect("write directive");

    let recorder = Recorder::default();
    let response = ipc_connect(&endpoint)
        .send_events(
            &authed(Request::RunDirective {
                id: 5,
                auth_token: None,
                name: "probe".into(),
                input: HashMap::new(),
                path: None,
                stream: false,
            }),
            &recorder,
        )
        .await
        .expect("run_directive");

    assert!(
        matches!(response, Response::Ok { id, .. } if id == 5),
        "{response:?}"
    );
    assert!(recorder.take().is_empty(), "没要 stream 却收到了帧");
}
