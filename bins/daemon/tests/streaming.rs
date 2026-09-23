//! `corex-daemon` 的进度帧：只有真的把这个二进制拉起来才验得准。
//!
//! 所以这里不 mock 任何东西——起真进程、连真的命名管道 / Unix socket、跑一条真的指令。
//! 起重与握手在 [`harness`] 里，这份只关心帧。

mod harness;

use corex_core::{Stream, Value};
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{FrameSink, ProgressEvent, Transport, ipc_connect};
use harness::{authed, start, start_with, strings};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// 发一条流式 `ui.wait`，帧交给 `recorder`，返回终帧。
///
/// 端点与落点都持所有权：调用方要把它 `tokio::spawn` 出去，才能让它在另一条请求
/// 还在飞的时候一直跑下去。
#[cfg(windows)]
async fn invoke_streaming(
    endpoint: PathBuf,
    recorder: Arc<Recorder>,
    id: u64,
    ms: i64,
) -> Response {
    let mut transport = ipc_connect(&endpoint);
    transport
        .send_events(
            &authed(Request::Invoke {
                id,
                auth_token: None,
                action: "ui.wait".into(),
                params: wait_params(ms),
                stream: true,
            }),
            recorder.as_ref(),
        )
        .await
        .expect("invoke")
}

/// 等到 `recorder` 收到第一个步骤帧：那一刻这条请求已经拿到执行名额、真的在跑了。
#[cfg(windows)]
async fn wait_until_started(recorder: &Recorder) {
    for _ in 0..200 {
        if !recorder.take().is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("第一条请求迟迟没开始");
}

/// `list_actions` 回的不是一串 id，而是完整目录：宿主与 agent 靠它知道「怎么调」——
/// 参数类型、默认值与要声明的权限都在里面。形状与 `corex actions --json` **完全一致**，
/// 连 `version` 都在（宿主据此判断参数表要不要重拉）。
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
    assert_eq!(
        data.find_path("version").and_then(|v| v.as_str()),
        Some(env!("CARGO_PKG_VERSION")),
        "目录该带上 corex 版本: {data:?}"
    );
    let actions = data
        .find_path("actions")
        .and_then(|v| v.as_array())
        .expect("目录文档该带 actions 数组");
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

/// `max_jobs = 1` 时执行是**串行**的：两条 400ms 的请求不可能少于 800ms 跑完。
///
/// 这条是 UI 自动化的保险：两条指令同时驱鼠标键盘必然互相踩。资源门也会在默认并发度
/// 下提供同样的保证；这里保留 `max_jobs = 1` 作为全局队列的回归测试。
#[cfg(windows)]
#[tokio::test]
async fn a_serial_jobs_limit_runs_them_one_by_one() {
    let (_dir, _daemon, endpoint) = start_with("serial-jobs", "max_jobs = 1\n").await;
    let elapsed = two_waits(&endpoint, 400).await;
    assert!(elapsed >= 720, "max_jobs = 1 该是串行的，实测 {elapsed}ms");
}

/// 默认配置下两条 UI 请求也不会并排跑：共享输入设备的资源门不能被 `max_jobs`
/// 的默认并发度绕过。
#[cfg(windows)]
#[tokio::test]
async fn the_default_serializes_interactive_jobs() {
    let (_dir, _daemon, endpoint) = start("default-jobs").await;
    let elapsed = two_waits(&endpoint, 400).await;
    assert!(elapsed >= 720, "默认 UI 资源该串行，实测 {elapsed}ms");
}

/// 提高全局并发度也不能让两条 UI 请求争用共享设备。
#[cfg(windows)]
#[tokio::test]
async fn a_larger_jobs_limit_still_serializes_interactive_jobs() {
    let (_dir, _daemon, endpoint) = start_with("parallel-jobs", "max_jobs = 2\n").await;
    let elapsed = two_waits(&endpoint, 400).await;
    assert!(
        elapsed >= 720,
        "两条 UI 请求不该因 max_jobs = 2 而并排跑，实测 {elapsed}ms"
    );
}

/// 排队中的流式请求会收到 `is_queued` 心跳，跑起来之后收到的不再带这个标记。
///
/// 这是「排队」与「卡死」的唯一区别：没有心跳时两者在客户端看来都是「一段时间没有任何帧」，
/// 而排队久到撞穿宿主的请求时限时，请求会被误报成失败——它其实还在队列里，之后照样执行。
#[cfg(windows)]
#[tokio::test]
async fn a_queued_request_gets_a_heartbeat() {
    let (_dir, _daemon, endpoint) = start_with("queued-heartbeat", "max_jobs = 1\n").await;
    let running = Arc::new(Recorder::default());
    let queued = Arc::new(Recorder::default());
    // 谁拿到唯一的名额得由我们说了算：两条一起发出去，1ms 那条完全可能先落地，
    // 于是排队等着的反倒成了 3.5s 那条。所以先发占名额的，等它真的跑起来再发第二条。
    let holder = tokio::spawn(invoke_streaming(endpoint.clone(), running.clone(), 1, 3500));
    wait_until_started(&running).await;
    // 第一条占着名额 3.5s，第二条只能在队列里等；心跳间隔 2s，所以它至少收得到一帧「还在排队」。
    let second = invoke_streaming(endpoint.clone(), queued.clone(), 2, 1).await;
    let first = holder.await.expect("第一条");
    assert!(matches!(first, Response::Ok { .. }), "{first:?}");
    assert!(matches!(second, Response::Ok { .. }), "{second:?}");

    let heartbeats: Vec<_> = queued
        .take()
        .into_iter()
        .filter(|event| matches!(event, ProgressEvent::Heartbeat { .. }))
        .collect();
    assert!(
        heartbeats.iter().any(|event| matches!(
            event,
            ProgressEvent::Heartbeat {
                is_queued: true,
                ..
            }
        )),
        "排队中该收到 is_queued 心跳: {heartbeats:?}"
    );
    assert!(
        running.take().iter().any(|event| matches!(
            event,
            ProgressEvent::Heartbeat {
                is_queued: false,
                ..
            }
        )),
        "跑起来之后的心跳不该再报排队"
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

/// 指令里的 `shell.run` 把子进程 stdout 变成帧送到客户端。
///
/// 这条是整件事的**要害**：daemon 里子进程的输出接在 daemon 自己的控制台上，客户端
/// （Studio 的运行面板）不看帧就一个字节也拿不到——它能显示命令输出，全靠这条通路。
#[tokio::test]
async fn run_directive_streams_shell_output() {
    let (dir, _daemon, endpoint) = start("output").await;
    let directives = dir.path().join("directives");
    // `host: cmd`：Windows 走 `cmd /C`，别处走 `sh -c`，两边都能 echo。
    std::fs::write(
        directives.join("probe.yaml"),
        concat!(
            "name: probe\n",
            "permissions:\n",
            "  shell: true\n",
            "steps:\n",
            "  - id: echo\n",
            "    action: shell.run\n",
            "    params:\n",
            "      command: \"echo corex-step-output-probe\"\n",
            "      host: cmd\n",
        ),
    )
    .expect("write directive");

    let recorder = Recorder::default();
    let response = ipc_connect(&endpoint)
        .send_events(
            &authed(Request::RunDirective {
                id: 6,
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
        matches!(response, Response::Ok { id, .. } if id == 6),
        "{response:?}"
    );
    let seen = recorder.take();
    let text: String = seen
        .iter()
        .filter_map(|event| match event {
            ProgressEvent::StepOutput {
                step,
                stream: Stream::Stdout,
                text,
                ..
            } if step == "echo" => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        text.contains("corex-step-output-probe"),
        "stdout 应当经帧抵达客户端，实得: {seen:?}"
    );
    assert!(
        seen.iter().any(|event| matches!(
            event,
            ProgressEvent::StepEnd { step, ok: true, .. } if step == "echo"
        )),
        "输出帧之后仍要有这一步的结束帧: {seen:?}"
    );
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
