//! 运行历史的只读出口：`list_runs` 与 `list_directives` 条目上的 `last_run`。
//!
//! 卡片的「上次跑成什么样」、运行台里的记录，都出自引擎自己写的那份账本（`corex history`
//! 读的也是它）。宿主再攒一份必然与它对不上，所以回话形状只能在真进程上钉住。
//!
//! 历史默认开，文件落在 `COREX_DATA_DIR` 下——harness 已经把它钉在用例自己的临时目录上。

mod harness;

use corex_core::Value;
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{Transport, ipc_connect};
use harness::{authed, start, start_with};
use std::collections::HashMap;
use std::path::Path;

async fn send(endpoint: &Path, request: Request) -> Response {
    ipc_connect(endpoint)
        .send(&authed(request))
        .await
        .expect("send request")
}

/// 回 `error` 就 panic，否则返回 `data`。
fn data(response: Response) -> Value {
    match response {
        Response::Ok { data, .. } => data,
        other => panic!("该成功却失败了: {other:?}"),
    }
}

/// 写一条指令：模板里没有占位符就渲染得出来，写个未定义的变量就一定失败。
fn write_directive(dir: &Path, name: &str, template: &str) {
    let text = format!(
        "name: {name}\nsteps:\n  - id: render\n    action: template.render\n    params:\n      template: '{template}'\n    save_to: message\n"
    );
    std::fs::write(dir.join("directives").join(format!("{name}.yaml")), text).expect("写指令");
}

/// 跑一条指令；`Err` 就是这次运行失败了。
async fn run(endpoint: &Path, name: &str) -> Result<(), String> {
    let request = Request::RunDirective {
        id: 0,
        auth_token: None,
        name: name.to_owned(),
        input: HashMap::new(),
        path: None,
        stream: false,
    };
    match send(endpoint, request).await {
        Response::Ok { .. } => Ok(()),
        Response::Error { error, .. } => Err(error.message),
        other => panic!("执行的回话既不是 ok 也不是 error: {other:?}"),
    }
}

async fn run_ok(endpoint: &Path, name: &str) {
    if let Err(message) = run(endpoint, name).await {
        panic!("{name} 该成功却失败了: {message}");
    }
}

async fn run_failing(endpoint: &Path, name: &str) {
    assert!(run(endpoint, name).await.is_err(), "{name} 该失败却成功了");
}

async fn list_runs(endpoint: &Path, name: Option<&str>, limit: Option<usize>) -> Value {
    let request = Request::ListRuns {
        id: 1,
        auth_token: None,
        name: name.map(str::to_owned),
        limit,
    };
    data(send(endpoint, request).await)
}

async fn list_directives(endpoint: &Path) -> Value {
    let request = Request::ListDirectives {
        id: 2,
        auth_token: None,
        dir: None,
    };
    data(send(endpoint, request).await)
}

/// `entries` 里的指令名，按回话顺序。
fn entry_names(reply: &Value) -> Vec<String> {
    reply
        .find_path("entries")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.find_path("directive")?.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// 列目录回话里某条指令的条目。
fn entry_of(entries: &Value, name: &str) -> Value {
    entries
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item.find_path("name").and_then(Value::as_str) == Some(name))
        .cloned()
        .unwrap_or_else(|| panic!("列目录里没有 {name}: {entries:?}"))
}

/// 跑三次（含一次失败）：新的在前、失败也在、`name` 与 `limit` 各管一段。
#[tokio::test]
async fn runs_are_read_back_newest_first_with_failures_kept() {
    let (dir, _daemon, endpoint) = start("runs-order").await;
    write_directive(dir.path(), "build", "hi");
    write_directive(dir.path(), "deploy", "{{ missing }}");

    run_ok(&endpoint, "build").await;
    run_failing(&endpoint, "deploy").await;
    run_ok(&endpoint, "build").await;

    let reply = list_runs(&endpoint, None, None).await;
    assert_eq!(
        reply
            .find_path("is_history_enabled")
            .and_then(Value::as_bool),
        Some(true),
        "历史默认是开的: {reply:?}"
    );
    assert_eq!(
        entry_names(&reply),
        ["build", "deploy", "build"],
        "{reply:?}"
    );

    // 失败的记录带着原因——宿主不该为了知道「为什么失败」再去翻 JSONL。
    let entries = reply
        .find_path("entries")
        .and_then(Value::as_array)
        .expect("entries");
    let failed = entries
        .iter()
        .find(|entry| entry.find_path("ok").and_then(Value::as_bool) == Some(false))
        .expect("有一条失败记录");
    assert!(
        failed.find_path("error").and_then(Value::as_str).is_some(),
        "{failed:?}"
    );

    // `limit` 的条数在按名过滤**之后**算：要的是「最近跑的这条指令的 N 次」。
    assert_eq!(
        entry_names(&list_runs(&endpoint, Some("build"), None).await),
        ["build", "build"]
    );
    assert_eq!(
        entry_names(&list_runs(&endpoint, Some("build"), Some(1)).await),
        ["build"]
    );
    assert!(entry_names(&list_runs(&endpoint, None, Some(0)).await).is_empty());
}

/// 卡片要的「上次跑成什么样」随列目录一起回，省掉逐条问历史。
#[tokio::test]
async fn list_directives_carries_the_last_run() {
    let (dir, _daemon, endpoint) = start("runs-card").await;
    write_directive(dir.path(), "build", "{{ missing }}");
    write_directive(dir.path(), "idle", "hi");

    run_failing(&endpoint, "build").await;
    run_failing(&endpoint, "build").await;

    let entries = list_directives(&endpoint).await;
    let last = entry_of(&entries, "build")
        .find_path("last_run")
        .expect("跑过就该有 last_run")
        .clone();
    assert_eq!(
        last.find_path("ok").and_then(Value::as_bool),
        Some(false),
        "{last:?}"
    );
    assert!(
        last.find_path("error").and_then(Value::as_str).is_some(),
        "{last:?}"
    );
    assert!(
        last.find_path("started_at_ms")
            .and_then(Value::as_i64)
            .is_some(),
        "{last:?}"
    );
    assert_eq!(last.find_path("run_count").and_then(Value::as_i64), Some(2));
    assert_eq!(
        last.find_path("failed_count").and_then(Value::as_i64),
        Some(2)
    );

    // 没跑过的指令不带这个字段：「没跑过」与「历史没开」才分得开。
    assert!(
        entry_of(&entries, "idle").find_path("last_run").is_none(),
        "{entries:?}"
    );
}

/// 历史关掉时说清楚：空表不等于「一条都没跑过」。
#[tokio::test]
async fn a_disabled_history_says_so() {
    let (dir, _daemon, endpoint) = start_with("runs-off", "\n[history]\nenabled = false\n").await;
    write_directive(dir.path(), "build", "hi");
    run_ok(&endpoint, "build").await;

    let reply = list_runs(&endpoint, None, None).await;
    assert_eq!(
        reply
            .find_path("is_history_enabled")
            .and_then(Value::as_bool),
        Some(false),
        "{reply:?}"
    );
    assert!(entry_names(&reply).is_empty(), "{reply:?}");
    assert!(
        entry_of(&list_directives(&endpoint).await, "build")
            .find_path("last_run")
            .is_none(),
        "历史关掉时没有 last_run"
    );
}
