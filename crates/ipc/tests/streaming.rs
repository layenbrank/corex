//! 流式进度帧的端到端行为：只有跨进程的线协议才能暴露的那部分。
//!
//! 这里跑的是**真实的平台传输**（Windows 命名管道 / Unix domain socket），不是内存里的
//! mock：帧与终帧的顺序、以及「一问一答的客户端拿到的线是否和从前一样」，都只有在真连接上
//! 才验得准。

use corex_core::{Mark, Observer, Spot, Stream, Unit};
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{FrameSink, Outlet, ProgressEvent, Transport, ipc_connect, serve_ipc};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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

/// 每个用例用自己的端点：同一测试二进制里的用例是并行跑的，而进程 id 是共的——
/// 两个用例抢同一个管道名时，一边 `abort` 掉监听就会让另一边看到“连接已关闭”。
fn test_endpoint(dir: &Path, tag: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let _ = dir;
        PathBuf::from(format!(
            r"\\.\pipe\corex-ipc-test-{}-{tag}",
            std::process::id()
        ))
    }
    #[cfg(unix)]
    {
        dir.join(format!("corex-ipc-test-{tag}.sock"))
    }
}

/// 等服务器把端点建起来再连：命名管道与 socket 都不是 `serve` 一调用就绪的。
async fn wait_for_server(endpoint: &Path) {
    for _ in 0..200 {
        let ping = Request::Ping {
            id: 0,
            auth_token: None,
        };
        if ipc_connect(endpoint).send(&ping).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("服务器没有起来: {}", endpoint.display());
}

/// 起一个对照 daemon 行为的服务器：只有请求置了 `stream` 才推帧，否则一行多余输出也没有。
fn spawn_server(endpoint: PathBuf) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _ = serve_ipc(&endpoint, |request: Request, outlet: Outlet| async move {
            let id = request.id();
            if request.wants_stream() {
                outlet.offer(ProgressEvent::StepStart {
                    seq: 1,
                    step: "copy".into(),
                    action: "file.copy".into(),
                });
                outlet.offer(ProgressEvent::StepProgress {
                    step: "copy".into(),
                    action: "file.copy".into(),
                    done: 7,
                    total: Some(10),
                    unit: Unit::Items,
                });
                outlet.offer(ProgressEvent::StepOutput {
                    step: "copy".into(),
                    action: "file.copy".into(),
                    stream: Stream::Stdout,
                    text: "copied 7 items\n".into(),
                });
                outlet.offer(ProgressEvent::StepEnd {
                    step: "copy".into(),
                    action: "file.copy".into(),
                    took_ms: 12,
                    ok: true,
                });
            }
            Response::ok(id, "done")
        })
        .await;
    })
}

fn run_request(stream: bool) -> Request {
    Request::RunDirective {
        id: 7,
        auth_token: None,
        name: "probe".into(),
        input: Default::default(),
        path: None,
        stream,
    }
}

/// 置了 `stream` 的请求：帧先到、顺序与推的顺序一致，终帧仍然是 `ok`。
#[tokio::test]
async fn frames_arrive_before_the_final_response() {
    let dir = tempfile::tempdir().expect("temp dir");
    let endpoint = test_endpoint(dir.path(), "frames");
    let server = spawn_server(endpoint.clone());
    wait_for_server(&endpoint).await;

    let recorder = Recorder::default();
    let response = ipc_connect(&endpoint)
        .send_events(&run_request(true), &recorder)
        .await
        .expect("streaming request");

    assert!(
        matches!(response, Response::Ok { id, .. } if id == 7),
        "终帧应当是 ok: {response:?}"
    );
    assert_eq!(
        recorder.take(),
        vec![
            ProgressEvent::StepStart {
                seq: 1,
                step: "copy".into(),
                action: "file.copy".into()
            },
            ProgressEvent::StepProgress {
                step: "copy".into(),
                action: "file.copy".into(),
                done: 7,
                total: Some(10),
                unit: Unit::Items
            },
            ProgressEvent::StepOutput {
                step: "copy".into(),
                action: "file.copy".into(),
                stream: Stream::Stdout,
                text: "copied 7 items\n".into()
            },
            ProgressEvent::StepEnd {
                step: "copy".into(),
                action: "file.copy".into(),
                took_ms: 12,
                ok: true
            },
        ]
    );

    server.abort();
}

/// 不置 `stream` 的请求：一帧都不会来，终帧照旧。
/// 这条断言守着向后兼容——旧客户端不问，就不该收到任何新东西。
#[tokio::test]
async fn a_plain_request_gets_no_frames() {
    let dir = tempfile::tempdir().expect("temp dir");
    let endpoint = test_endpoint(dir.path(), "plain");
    let server = spawn_server(endpoint.clone());
    wait_for_server(&endpoint).await;

    let recorder = Recorder::default();
    let response = ipc_connect(&endpoint)
        .send_events(&run_request(false), &recorder)
        .await
        .expect("plain request");

    assert!(matches!(response, Response::Ok { id, .. } if id == 7));
    assert!(recorder.take().is_empty(), "没要 stream 的请求不该收到帧");

    server.abort();
}

/// `send` 是 `send_events` 加一个丢弃帧的落点：即使对面推了帧，也照样拿到终帧，
/// 不会把第一个帧当成回答。
#[tokio::test]
async fn send_survives_frames_it_did_not_ask_for() {
    let dir = tempfile::tempdir().expect("temp dir");
    let endpoint = test_endpoint(dir.path(), "ignored");
    let server = spawn_server(endpoint.clone());
    wait_for_server(&endpoint).await;

    let response = ipc_connect(&endpoint)
        .send(&run_request(true))
        .await
        .expect("plain send");
    assert!(matches!(response, Response::Ok { id, .. } if id == 7));

    server.abort();
}

/// 帧是 NDJSON 里的一行，且带发起请求的 `id`——客户端据此认领自己的进度。
#[test]
fn a_frame_is_a_tagged_line() {
    let line = serde_json::to_string(&Response::Event {
        id: 7,
        progress: ProgressEvent::StepProgress {
            step: "copy".into(),
            action: "file.copy".into(),
            done: 1,
            total: None,
            unit: Unit::Bytes,
        },
    })
    .expect("serialize frame");

    let value: serde_json::Value = serde_json::from_str(&line).expect("frame is JSON");
    assert_eq!(value["type"], "event");
    assert_eq!(value["id"], 7);
    assert_eq!(value["progress"]["kind"], "step_progress");
    assert_eq!(value["progress"]["unit"], "bytes");
    assert!(value["progress"]["total"].is_null());
}

/// 输出帧在线上就是 `kind: step_output` 加一个 `stream` 判别式；
/// 解码回来必须与原值逐字段相等（宿主靠这些字段重建终端画面）。
#[test]
fn an_output_frame_keeps_its_stream_and_text() {
    let original = ProgressEvent::StepOutput {
        step: "build".into(),
        action: "shell.run".into(),
        stream: Stream::Stderr,
        text: "warning: unused import\n".into(),
    };
    let line = serde_json::to_string(&Response::Event {
        id: 3,
        progress: original.clone(),
    })
    .expect("serialize frame");

    let value: serde_json::Value = serde_json::from_str(&line).expect("frame is JSON");
    assert_eq!(value["progress"]["kind"], "step_output");
    assert_eq!(value["progress"]["stream"], "stderr");
    assert_eq!(value["progress"]["text"], "warning: unused import\n");

    let decoded: Response = serde_json::from_str(&line).expect("frame decode");
    match decoded {
        Response::Event { id, progress } => {
            assert_eq!(id, 3);
            assert_eq!(progress, original, "解码后应当与原帧逐字段一致");
        }
        other => panic!("应当解出事件帧: {other:?}"),
    }
}

/// 记下每次回调的上报口，用来验证帧的重放顺序与内容。
#[derive(Debug, Default)]
struct CallLog {
    seen: Mutex<Vec<String>>,
}

impl Observer for CallLog {
    fn begin(&self, at: Spot<'_>) {
        self.seen
            .lock()
            .expect("call log")
            .push(format!("begin {}", at.id));
    }

    fn chunk(&self, at: Spot<'_>, mark: Mark) {
        self.seen
            .lock()
            .expect("call log")
            .push(format!("chunk {} {}/{:?}", at.id, mark.done, mark.total));
    }

    fn output(&self, at: Spot<'_>, stream: Stream, text: &str) {
        self.seen
            .lock()
            .expect("call log")
            .push(format!("output {} {stream:?} {text}", at.id));
    }

    fn end(&self, at: Spot<'_>, _took: Duration, ok: bool) {
        self.seen
            .lock()
            .expect("call log")
            .push(format!("end {} {ok}", at.id));
    }
}

/// 帧重放进上报口时，四次回调的次数、顺序与载荷都按帧里写的来。
/// 这是「本地执行」与「远程执行」共用一套渲染器的前提。
#[test]
fn replay_feeds_all_four_callbacks_in_order() {
    let log = CallLog::default();
    let frames = [
        ProgressEvent::StepStart {
            seq: 1,
            step: "build".into(),
            action: "shell.run".into(),
        },
        ProgressEvent::StepProgress {
            step: "build".into(),
            action: "shell.run".into(),
            done: 1,
            total: None,
            unit: Unit::Bytes,
        },
        ProgressEvent::StepOutput {
            step: "build".into(),
            action: "shell.run".into(),
            stream: Stream::Stderr,
            text: "boom".into(),
        },
        ProgressEvent::StepEnd {
            step: "build".into(),
            action: "shell.run".into(),
            took_ms: 5,
            ok: false,
        },
    ];
    for frame in &frames {
        frame.replay(&log);
    }

    assert_eq!(
        log.seen.lock().expect("call log").clone(),
        vec![
            "begin build".to_string(),
            "chunk build 1/None".to_string(),
            "output build Stderr boom".to_string(),
            "end build false".to_string(),
        ]
    );
}
