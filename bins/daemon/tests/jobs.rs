//! cron / watch 作业生命周期：start / list / stop。

mod harness;

use corex_core::Value;
use corex_ipc::protocol::{JobKind, Request, Response};
use corex_ipc::{Transport, ipc_connect};
use harness::{Daemon, authed, start_with};
use std::path::Path;
use std::time::Duration;

async fn send(endpoint: &Path, request: Request) -> Response {
    ipc_connect(endpoint)
        .send(&authed(request))
        .await
        .expect("send request")
}

fn data(response: Response) -> Value {
    match response {
        Response::Ok { data, .. } => data,
        other => panic!("该成功却失败了: {other:?}"),
    }
}

fn code(response: Response) -> (i32, String) {
    match response {
        Response::Error { error, .. } => (error.code, error.message),
        other => panic!("该失败却成功了: {other:?}"),
    }
}

fn cron_definition(name: &str) -> Value {
    Value::from_json(
        serde_json::from_str(&format!(
            r#"{{
                "name": "{name}",
                "triggers": [{{"type": "cron", "expr": "0 0 12 * * *"}}],
                "steps": [
                    {{"id": "render", "action": "template.render",
                      "params": {{"template": "hi"}}, "save_to": "message"}}
                ]
            }}"#
        ))
        .expect("definition json"),
    )
}

fn plain_definition(name: &str) -> Value {
    Value::from_json(
        serde_json::from_str(&format!(
            r#"{{
                "name": "{name}",
                "steps": [
                    {{"id": "render", "action": "template.render",
                      "params": {{"template": "hi"}}, "save_to": "message"}}
                ]
            }}"#
        ))
        .expect("definition json"),
    )
}

async fn start_clean(tag: &str) -> (tempfile::TempDir, Daemon, std::path::PathBuf) {
    let (dir, daemon, endpoint) = start_with(tag, "\n[directives]\nseed = false\n").await;
    (dir, daemon, endpoint)
}

#[tokio::test]
async fn start_list_and_force_stop_a_cron_job() {
    let (_dir, daemon, endpoint) = start_clean("jobs-cron").await;
    let saved = send(
        &endpoint,
        Request::SaveDirective {
            id: 1,
            auth_token: None,
            name: "timed".into(),
            definition: cron_definition("timed"),
            original_name: None,
        },
    )
    .await;
    let _ = data(saved);

    let started = data(
        send(
            &endpoint,
            Request::StartJob {
                id: 2,
                auth_token: None,
                kind: JobKind::Cron,
                name: "timed".into(),
                immediate: false,
            },
        )
        .await,
    );
    assert_eq!(
        started.find_path("name").and_then(Value::as_str),
        Some("timed")
    );
    assert_eq!(
        started.find_path("kind").and_then(Value::as_str),
        Some("cron")
    );

    let mut alive = false;
    for _ in 0..80 {
        let listed = data(
            send(
                &endpoint,
                Request::Jobs {
                    id: 3,
                    auth_token: None,
                    kind: Some(JobKind::Cron),
                },
            )
            .await,
        );
        let jobs = listed.find_path("jobs").and_then(Value::as_array);
        alive = jobs
            .and_then(|items| items.first())
            .and_then(|job| job.find_path("is_alive"))
            .and_then(Value::as_bool)
            == Some(true);
        if alive {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        alive,
        "supervisor 该活着:\n{}",
        daemon.log_text()
    );

    let _ = data(
        send(
            &endpoint,
            Request::StopJob {
                id: 4,
                auth_token: None,
                kind: JobKind::Cron,
                name: "timed".into(),
                force: true,
            },
        )
        .await,
    );

    let listed = data(
        send(
            &endpoint,
            Request::Jobs {
                id: 5,
                auth_token: None,
                kind: Some(JobKind::Cron),
            },
        )
        .await,
    );
    let jobs = listed.find_path("jobs").and_then(Value::as_array);
    assert!(
        jobs.is_none_or(|items| items.is_empty()),
        "强制停止后不该再列出来: {listed:?}\n{}",
        daemon.log_text()
    );
}

#[tokio::test]
async fn starting_without_a_trigger_is_invalid() {
    let (_dir, _daemon, endpoint) = start_clean("jobs-no-trigger").await;
    let _ = data(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 1,
                auth_token: None,
                name: "plain".into(),
                definition: plain_definition("plain"),
                original_name: None,
            },
        )
        .await,
    );
    let (code, message) = code(
        send(
            &endpoint,
            Request::StartJob {
                id: 2,
                auth_token: None,
                kind: JobKind::Watch,
                name: "plain".into(),
                immediate: false,
            },
        )
        .await,
    );
    assert_eq!(code, 400, "{message}");
    assert!(message.contains("watch"), "{message}");
}
