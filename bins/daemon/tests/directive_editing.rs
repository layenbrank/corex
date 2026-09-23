//! 宿主编辑指令走的三条 IPC：列目录、读一条、写一条。
//!
//! 编辑器不自己拼 YAML 也不自己解析 YAML——写盘格式（键序、哪些默认值该省）只有引擎
//! 一份，所以这些回话形状得在真进程上钉住。

mod harness;

use corex_core::Value;
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{Transport, ipc_connect};
use harness::{authed, start};
use std::collections::HashMap;
use std::path::Path;

/// 从条目数组里取某个字段的值。`find_path` 只按索引走数组，所以这里得自己遍历。
fn field_of(entries: &Value, field: &str) -> Vec<String> {
    entries
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.find_path(field).and_then(|v| v.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// 一条最小的、动作都注册过的指令定义。
fn definition(name: &str, action: &str) -> Value {
    Value::from_json(
        serde_json::from_str(&format!(
            r#"{{
                "name": "{name}",
                "steps": [
                    {{"id": "render", "action": "{action}",
                      "params": {{"template": "hi"}}, "save_to": "message"}}
                ]
            }}"#
        ))
        .expect("definition json"),
    )
}

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

/// 回 `ok` 就 panic，否则返回 RpcError 的码。
fn code(response: Response) -> (i32, String) {
    match response {
        Response::Error { error, .. } => (error.code, error.message),
        other => panic!("该失败却成功了: {other:?}"),
    }
}

/// 存一次盘，再把原文、模型、磁盘内容对齐：三份必须是同一份。
///
/// 宿主拿到 `text` 才能展示 / 保留自己没改的字段，拿到 `definition` 才能编辑；
/// 而磁盘上那份必须与它刚拿到的 `text` 逐字节一致，否则「保存后再读到的东西变了」。
#[tokio::test]
async fn saving_a_directive_gives_the_stored_text_and_model_back() {
    let (dir, _daemon, endpoint) = start("save").await;
    let response = data(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 1,
                auth_token: None,
                name: "build".into(),
                definition: definition("build", "template.render"),
                dir: None,
            },
        )
        .await,
    );

    let path = dir.path().join("directives").join("build.yaml");
    assert_eq!(
        response.find_path("path").and_then(|v| v.as_str()),
        Some(corex_core::path::display_path(&path).as_str()),
        "{response:?}"
    );
    let text = response
        .find_path("text")
        .and_then(|v| v.as_str())
        .expect("回话里该带原文");
    assert_eq!(
        std::fs::read_to_string(&path).expect("落盘了"),
        text,
        "回话里的原文该与磁盘一致"
    );
    assert_eq!(
        response
            .find_path("definition")
            .and_then(|v| v.find_path("steps"))
            .and_then(|v| v.as_array())
            .and_then(|steps| steps.first())
            .and_then(|step| step.find_path("action"))
            .and_then(|v| v.as_str()),
        Some("template.render"),
        "模型该带上步骤: {response:?}"
    );
    // 默认值不写进文件：`on_error` 缺省就是 `abort`，写出来只会让每次保存都产生 diff。
    assert!(!text.contains("on_error"), "默认值不该落盘:\n{text}");
}

/// 读回来的是同一份原文与模型——宿主不必为了拿模型而自己解析 YAML。
#[tokio::test]
async fn reading_a_directive_returns_the_file_and_its_model() {
    let (dir, _daemon, endpoint) = start("read").await;
    let directives = dir.path().join("directives");
    let text = concat!(
        "name: probe\n",
        "steps:\n",
        "  - id: render\n",
        "    action: template.render\n",
        "    params:\n",
        "      template: \"hi\"\n",
    );
    std::fs::write(directives.join("probe.yml"), text).expect("write");

    let response = data(
        send(
            &endpoint,
            Request::ReadDirective {
                id: 2,
                auth_token: None,
                name: "probe".into(),
                dir: None,
            },
        )
        .await,
    );
    assert_eq!(
        response.find_path("text").and_then(|v| v.as_str()),
        Some(text),
        "原文该逐字节回来"
    );
    assert_eq!(
        response.find_path("path").and_then(|v| v.as_str()),
        Some(corex_core::path::display_path(&directives.join("probe.yml")).as_str()),
        ".yml 的指令也该找得到"
    );
    assert_eq!(
        response
            .find_path("definition")
            .and_then(|v| v.find_path("name"))
            .and_then(|v| v.as_str()),
        Some("probe")
    );
}

/// 列目录带上分类、路径与卡片要用的元信息；坏文件照样列出来。
///
/// 分类得解析文件才知道，但**一条坏指令不该让整个列表失败**——用户正是要靠这份列表
/// 在编辑器里打开它去修。`.txt` 那类同目录的垃圾文件不是指令，不该混进来。
#[tokio::test]
async fn listing_directives_carries_the_bucket_and_the_path() {
    let (dir, _daemon, endpoint) = start("list").await;
    let directives = dir.path().join("directives");
    std::fs::write(
        directives.join("pack.yaml"),
        "name: pack\nbucket: data\ndescription: 打包\ntriggers:\n  - type: watch\n    paths: [src]\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write pack");
    std::fs::write(
        directives.join("plain.yml"),
        "name: plain\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write plain");
    std::fs::write(directives.join("broken.yaml"), "name: [未闭合\n").expect("write broken");
    std::fs::write(directives.join("notes.txt"), "不是指令\n").expect("write notes");

    let response = data(
        send(
            &endpoint,
            Request::ListDirectives {
                id: 3,
                auth_token: None,
                dir: None,
            },
        )
        .await,
    );
    let entries = response.as_array().expect("该回一串条目");
    assert_eq!(
        field_of(&response, "name"),
        vec!["broken", "pack", "plain"],
        "按名字排序且不含非 YAML"
    );
    assert_eq!(
        entries[1].find_path("bucket").and_then(|v| v.as_str()),
        Some("data"),
        "分类该是 corex 的那一套小写名: {:?}",
        entries[1]
    );
    assert!(
        entries[0].find_path("bucket").is_some_and(Value::is_null),
        "坏文件的分类该是 null: {:?}",
        entries[0]
    );
    assert!(
        entries[1]
            .find_path("path")
            .and_then(|v| v.as_str())
            .is_some_and(|path| path.ends_with("pack.yaml")),
        "条目该带可交给外部编辑器的路径: {:?}",
        entries[1]
    );
    assert_eq!(
        entries[1]
            .find_path("summary")
            .and_then(|v| v.find_path("description"))
            .and_then(|v| v.as_str()),
        Some("打包"),
        "宿主画卡片要描述，不该自己再读一遍文件: {:?}",
        entries[1]
    );
    assert_eq!(
        entries[1]
            .find_path("summary")
            .and_then(|v| v.find_path("step_count"))
            .and_then(Value::as_i64),
        Some(1),
        "卡片要步骤数: {:?}",
        entries[1]
    );
    assert_eq!(
        entries[1]
            .find_path("summary")
            .and_then(|v| v.find_path("trigger_count"))
            .and_then(Value::as_i64),
        Some(1),
        "卡片要触发器数: {:?}",
        entries[1]
    );
    assert!(
        entries[0].find_path("summary").is_some_and(Value::is_null),
        "坏文件没有元信息可给: {:?}",
        entries[0]
    );
    assert_eq!(
        entries[2]
            .find_path("summary")
            .and_then(|v| v.find_path("description"))
            .and_then(|v| v.as_str()),
        Some(""),
        "没写描述就是空串，不是 null: {:?}",
        entries[2]
    );
}

/// 写回已有的 `.yml`，不另生一份 `.yaml`。
///
/// 生的那份会成为「同名的另一条指令」，而哪份生效取决于扩展名顺序——最难排查的一类。
#[tokio::test]
async fn saving_back_to_an_existing_yml_keeps_the_extension() {
    let (dir, _daemon, endpoint) = start("yml").await;
    let directives = dir.path().join("directives");
    std::fs::write(
        directives.join("legacy.yml"),
        "name: legacy\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write legacy");

    data(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 4,
                auth_token: None,
                name: "legacy".into(),
                definition: definition("legacy", "template.render"),
                dir: None,
            },
        )
        .await,
    );
    assert!(directives.join("legacy.yml").is_file(), "该写回 .yml");
    assert!(
        !directives.join("legacy.yaml").exists(),
        "不该另生一份 .yaml"
    );
    // 临时文件是「先写再改名」的中间态，落盘之后不该留下。
    assert!(!directives.join("legacy.tmp").exists(), "不该留下临时文件");
}

/// 同名 `.yaml` 与 `.yml` 并存时，写回**真正会被执行的那一份**。
///
/// 两份并存多半是手写留下的历史状态，而 `resolve_directive` 先看 `.yaml`；写错文件
/// 的表现是「改了没生效」。
#[tokio::test]
async fn saving_prefers_the_file_that_a_run_would_pick() {
    let (dir, _daemon, endpoint) = start("both").await;
    let directives = dir.path().join("directives");
    for ext in ["yaml", "yml"] {
        std::fs::write(
            directives.join(format!("dual.{ext}")),
            format!("name: dual\nsteps:\n  - id: {ext}\n    action: template.render\n"),
        )
        .expect("write dual");
    }

    data(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 5,
                auth_token: None,
                name: "dual".into(),
                definition: definition("dual", "template.render"),
                dir: None,
            },
        )
        .await,
    );

    let yaml = std::fs::read_to_string(directives.join("dual.yaml")).expect("read dual.yaml");
    assert!(yaml.contains("id: render"), ".yaml 是跑的那份，该被覆盖");
    let yml = std::fs::read_to_string(directives.join("dual.yml")).expect("read dual.yml");
    assert_eq!(
        yml, "name: dual\nsteps:\n  - id: yml\n    action: template.render\n",
        ".yml 不生效，不该被动到"
    );
}

/// 指向不存在的动作、或根本不是指令，都是调用方的问题（400），而且**一个字都不该落盘**。
#[tokio::test]
async fn an_unusable_definition_is_rejected_before_anything_is_written() {
    let (dir, _daemon, endpoint) = start("reject").await;
    let (status, message) = code(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 5,
                auth_token: None,
                name: "ghost".into(),
                definition: definition("ghost", "does.not.exist"),
                dir: None,
            },
        )
        .await,
    );
    assert_eq!(status, 400, "{message}");
    assert!(
        message.contains("does.not.exist"),
        "该点出是哪个动作: {message}"
    );
    assert!(
        !dir.path().join("directives").join("ghost.yaml").exists(),
        "校验没过就不该落盘"
    );

    let (status, _) = code(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 6,
                auth_token: None,
                name: "junk".into(),
                definition: Value::Str("这不是指令".into()),
                dir: None,
            },
        )
        .await,
    );
    assert_eq!(status, 400);
}

/// 权限声明不够同样跑不起来——`run` 的第二道门，这里要在落盘前就拦下。
///
/// 与「动作没注册」分开量：那类是 400（调用方写错了名字），这类是 403（调用方没被允许），
/// 宿主得能靠码分辨该让用户改定义还是改授权。
#[tokio::test]
async fn a_directive_that_asks_for_less_than_its_steps_need_is_rejected() {
    let (dir, _daemon, endpoint) = start("permissions").await;
    let (status, message) = code(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 11,
                auth_token: None,
                name: "restricted".into(),
                definition: Value::from_json(
                    serde_json::from_str(
                        r#"{
                            "name": "restricted",
                            "permissions": {"network": true},
                            "steps": [
                                {"id": "write", "action": "file.write",
                                 "params": {"path": "x.txt", "content": "hi"}}
                            ]
                        }"#,
                    )
                    .expect("definition json"),
                ),
                dir: None,
            },
        )
        .await,
    );
    assert_eq!(status, 403, "{message}");
    assert!(
        message.contains("filesystem"),
        "该点出缺的是哪一类权限: {message}"
    );
    assert!(
        !dir.path().join("directives").join("restricted.yaml").exists(),
        "校验没过就不该落盘"
    );
}

/// 指令名是**裸名**：`..` 与分隔符会让读写跑到指令根之外，找不到的则是 404。
///
/// 这里量的是「错的是调用方还是服务端」：名字有问题、文件没找到都不是 500。
#[tokio::test]
async fn a_bad_name_is_a_client_error_and_a_missing_one_is_not_found() {
    let (_dir, _daemon, endpoint) = start("names").await;
    let mut bad = vec!["../secret", "a/b", "a\\b"];
    if cfg!(windows) {
        // 盘符相对名：`is_absolute()` 是 false，但 `dir.join` 会把指令根整个丢掉，
        // 落点变成「D 盘的当前目录」——照样在指令根之外。
        bad.extend(["D:evil", "D:", "C:windows"]);
    }
    for name in bad {
        let (status, message) = code(
            send(
                &endpoint,
                Request::ReadDirective {
                    id: 7,
                    auth_token: None,
                    name: name.into(),
                    dir: None,
                },
            )
            .await,
        );
        assert_eq!(status, 400, "{name}: {message}");
    }
    let (status, message) = code(
        send(
            &endpoint,
            Request::ReadDirective {
                id: 8,
                auth_token: None,
                name: "nope".into(),
                dir: None,
            },
        )
        .await,
    );
    assert_eq!(status, 404, "{message}");

    // 跑一条不存在的指令也是 404，不是 500：错的是调用方点的名字，不是服务端。
    let (status, message) = code(
        send(
            &endpoint,
            Request::RunDirective {
                id: 9,
                auth_token: None,
                name: "nope".into(),
                input: HashMap::new(),
                path: None,
                stream: false,
            },
        )
        .await,
    );
    assert_eq!(status, 404, "{message}");
}

/// 子目录里的指令：`dir` 相对指令根，越界的一律 403。
#[tokio::test]
async fn the_subdirectory_argument_stays_under_the_directives_root() {
    let (dir, _daemon, endpoint) = start("subdir").await;
    let nested = dir.path().join("directives").join("pack");
    std::fs::create_dir_all(&nested).expect("mkdir");
    std::fs::write(nested.join("inner.yaml"), "name: inner\nsteps: []\n").expect("write");

    let response = data(
        send(
            &endpoint,
            Request::ListDirectives {
                id: 9,
                auth_token: None,
                dir: Some("pack".into()),
            },
        )
        .await,
    );
    assert_eq!(field_of(&response, "name"), vec!["inner"], "{response:?}");

    let (status, message) = code(
        send(
            &endpoint,
            Request::ListDirectives {
                id: 10,
                auth_token: None,
                dir: Some("../..".into()),
            },
        )
        .await,
    );
    assert_eq!(status, 403, "{message}");
}

/// 卡片上的「上次执行」按 **YAML 里的 `name`** 查账本，不是按文件名。
///
/// 账本是流水线写的，它只认模型里的 `name`；两者不一致时按文件名查永远查不到，卡片就会
/// 一直显示「未运行」——而这条指令刚刚才跑完。条目自己的 `name` 仍是文件名（宿主拿它
/// 读写），两个键各有各的用处。
#[tokio::test]
async fn last_run_is_looked_up_by_the_name_inside_the_yaml() {
    let (dir, _daemon, endpoint) = start("last-run").await;
    std::fs::write(
        dir.path().join("directives").join("file-stem.yaml"),
        "name: declared-name\nsteps:\n  - id: render\n    action: template.render\n    params:\n      template: hi\n",
    )
    .expect("write directive");
    data(
        send(
            &endpoint,
            Request::RunDirective {
                id: 12,
                auth_token: None,
                name: "file-stem".into(),
                input: HashMap::new(),
                path: None,
                stream: false,
            },
        )
        .await,
    );

    let entries = data(
        send(
            &endpoint,
            Request::ListDirectives {
                id: 13,
                auth_token: None,
                dir: None,
            },
        )
        .await,
    );
    assert_eq!(field_of(&entries, "name"), vec!["file-stem"], "{entries:?}");
    let last = entries
        .as_array()
        .and_then(|items| items.first())
        .and_then(|entry| entry.find_path("last_run"))
        .unwrap_or_else(|| panic!("跑过就该有 last_run: {entries:?}"));
    assert_eq!(
        last.find_path("ok").and_then(Value::as_bool),
        Some(true),
        "{last:?}"
    );
    assert_eq!(
        last.find_path("run_count").and_then(Value::as_i64),
        Some(1),
        "{last:?}"
    );
}
