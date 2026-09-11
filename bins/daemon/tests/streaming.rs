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
    fn spawn(config: &Path, directives: &Path, log: &Path) -> Self {
        let out = std::fs::File::create(log).expect("daemon log");
        let err = out.try_clone().expect("daemon log clone");
        let child = Command::new(env!("CARGO_BIN_EXE_corex-daemon"))
            .arg("--config")
            .arg(config)
            .arg("--directives")
            .arg(directives)
            .env("COREX_TOKEN", TOKEN)
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
fn write_config(dir: &Path, endpoint: &Path) -> PathBuf {
    let path = dir.join("corex-daemon-test.toml");
    // 用 TOML 的字面量字符串（单引号）：Windows 管道路径里的反斜杠不该被当成转义。
    let text = format!(
        "[daemon]\nsocket_path = '{}'\nlock_path = '{}'\ntoken = '{TOKEN}'\n",
        endpoint.display(),
        dir.join("daemon.lock").display()
    );
    std::fs::write(&path, text).expect("write config");
    path
}

/// 起一个 daemon，并把它的指令目录与端点一起交回来。
async fn start(tag: &str) -> (tempfile::TempDir, Daemon, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let directives = dir.path().join("directives");
    std::fs::create_dir_all(&directives).expect("directives dir");
    let endpoint = endpoint(tag);
    let config = write_config(dir.path(), &endpoint);
    let mut daemon = Daemon::spawn(&config, &directives, &dir.path().join("daemon.log"));
    daemon.wait_ready(&endpoint, TOKEN).await;
    (dir, daemon, endpoint)
}

fn authed(request: Request) -> Request {
    request.with_auth_token(TOKEN)
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
