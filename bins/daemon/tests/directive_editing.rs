//! 宿主编辑指令走的几条 IPC：列目录、读一条、写一条（含改名）、删、导入。
//!
//! v13 起指令的真相是 `<数据目录>/directives.db`，而不是磁盘上的 YAML 文件：编辑器不自己
//! 拼 YAML 也不自己解析 YAML——写出去只有引擎一份——所以这些回话形状只能在真进程上钉住。

mod harness;

use corex_core::Value;
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{Transport, ipc_connect};
use harness::{Daemon, authed, start_with};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// 回 `ok` 就 panic，否则返回 RpcError 的码与消息。
fn code(response: Response) -> (i32, String) {
    match response {
        Response::Error { error, .. } => (error.code, error.message),
        other => panic!("该失败却成功了: {other:?}"),
    }
}

/// 起一个 daemon，并关掉起步指令——否则每条断言都得先把那几条起步指令算进去。
async fn start_clean(tag: &str) -> (tempfile::TempDir, Daemon, PathBuf) {
    start_with(tag, "\n[directives]\nseed = false\n").await
}

async fn save(endpoint: &Path, name: &str, action: &str) -> Value {
    save_definition(endpoint, name, None, definition(name, action)).await
}

async fn save_definition(
    endpoint: &Path,
    name: &str,
    original_name: Option<&str>,
    definition: Value,
) -> Value {
    data(
        send(
            endpoint,
            Request::SaveDirective {
                id: 1,
                auth_token: None,
                name: name.into(),
                definition,
                original_name: original_name.map(str::to_owned),
            },
        )
        .await,
    )
}

async fn read(endpoint: &Path, name: &str) -> Response {
    send(
        endpoint,
        Request::ReadDirective {
            id: 2,
            auth_token: None,
            name: name.into(),
        },
    )
    .await
}

async fn list(endpoint: &Path) -> Value {
    data(
        send(
            endpoint,
            Request::ListDirectives {
                id: 3,
                auth_token: None,
            },
        )
        .await,
    )
}

async fn delete(endpoint: &Path, name: &str) -> Response {
    send(
        endpoint,
        Request::DeleteDirective {
            id: 4,
            auth_token: None,
            name: name.into(),
        },
    )
    .await
}

async fn import(
    endpoint: &Path,
    path: &Path,
    folder: Option<&str>,
    is_overwrite: bool,
    is_dry_run: bool,
) -> Value {
    data(
        send(
            endpoint,
            Request::ImportDirectives {
                id: 5,
                auth_token: None,
                path: path.display().to_string(),
                folder: folder.map(str::to_owned),
                is_overwrite,
                is_dry_run,
            },
        )
        .await,
    )
}

async fn run(endpoint: &Path, name: &str) -> Response {
    send(
        endpoint,
        Request::RunDirective {
            id: 6,
            auth_token: None,
            name: name.into(),
            input: HashMap::new(),
            path: None,
            stream: false,
        },
    )
    .await
}

/// 存一次盘，再把规范化 YAML、模型对齐：两份必须是同一份内容。
///
/// 宿主拿到 `yaml` 才能展示 / 保留自己没改的字段，拿到 `definition` 才能编辑；而读回来那份
/// 必须与它刚拿到的逐字节一致，否则「保存后再读到的东西变了」。
#[tokio::test]
async fn saving_a_directive_gives_the_stored_yaml_and_model_back() {
    let (_dir, _daemon, endpoint) = start_clean("save").await;
    let response = save(&endpoint, "build", "template.render").await;

    let yaml = response
        .find_path("yaml")
        .and_then(|v| v.as_str())
        .expect("回话里该带规范化 YAML");
    assert!(yaml.contains("template.render"), "{yaml}");
    // 默认值不写进 YAML：`on_error` 缺省就是 `abort`，写出来只会让每次保存都产生 diff。
    assert!(!yaml.contains("on_error"), "默认值不该落进 YAML:\n{yaml}");
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
    // 库里新建的没有来源；分组只有导入与显式设置才会有。
    assert!(
        response.find_path("source").is_some_and(Value::is_null),
        "{response:?}"
    );
    assert_eq!(
        response.find_path("created_at_ms").and_then(Value::as_i64),
        response.find_path("updated_at_ms").and_then(Value::as_i64),
        "刚建出来的两个时间戳该是同一个: {response:?}"
    );
    assert!(
        response.find_path("path").is_none(),
        "v13 里指令不再有文件路径: {response:?}"
    );

    // 读回来的是同一份——宿主保存完不必自己猜落库成了什么样。
    let back = data(read(&endpoint, "build").await);
    assert_eq!(
        back.find_path("yaml").and_then(|v| v.as_str()),
        Some(yaml),
        "读回来该是同一份 YAML"
    );
    assert_eq!(
        back.find_path("definition")
            .and_then(|v| v.find_path("name"))
            .and_then(|v| v.as_str()),
        Some("build")
    );
}

/// 列目录带上分组、来源、更新时间与卡片要用的元信息。
///
/// 分类得解析模型才知道，而宿主画卡片不该为了显示描述、步骤数把每条再读一遍。
#[tokio::test]
async fn listing_directives_carries_folder_source_and_card_metadata() {
    let (dir, _daemon, endpoint) = start_clean("list").await;
    let yaml = dir.path().join("yaml");
    std::fs::create_dir_all(yaml.join("pack")).expect("mkdir pack");
    std::fs::write(
        yaml.join("pack").join("pack.yaml"),
        "name: pack\nbucket: data\ndescription: 打包\ntriggers:\n  - type: watch\n    paths: [src]\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write pack");
    std::fs::write(
        yaml.join("plain.yml"),
        "name: plain\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write plain");
    std::fs::write(yaml.join("notes.txt"), "不是指令\n").expect("write notes");
    import(&endpoint, &yaml, None, false, false).await;

    let entries = list(&endpoint).await;
    assert_eq!(
        field_of(&entries, "name"),
        vec!["pack", "plain"],
        "按名字排序且不含非 YAML"
    );

    let pack = entry_of(&entries, "pack");
    assert_eq!(
        pack.find_path("folder").and_then(Value::as_str),
        Some("pack"),
        "相对子目录成为分组: {pack:?}"
    );
    assert!(
        pack.find_path("source")
            .and_then(Value::as_str)
            .is_some_and(|source| source.ends_with("pack.yaml")),
        "条目该带导入来源: {pack:?}"
    );
    assert!(
        pack.find_path("updated_at_ms")
            .and_then(Value::as_i64)
            .is_some_and(|at| at > 0),
        "条目该带更新时间: {pack:?}"
    );
    assert_eq!(
        pack.find_path("bucket").and_then(Value::as_str),
        Some("data"),
        "分类该是 corex 的那一套小写名: {pack:?}"
    );
    assert_eq!(
        pack.find_path("summary")
            .and_then(|v| v.find_path("description"))
            .and_then(Value::as_str),
        Some("打包"),
        "宿主画卡片要描述: {pack:?}"
    );
    assert_eq!(
        pack.find_path("summary")
            .and_then(|v| v.find_path("step_count"))
            .and_then(Value::as_i64),
        Some(1),
        "卡片要步骤数: {pack:?}"
    );
    assert_eq!(
        pack.find_path("summary")
            .and_then(|v| v.find_path("trigger_count"))
            .and_then(Value::as_i64),
        Some(1),
        "卡片要触发器数: {pack:?}"
    );

    let plain = entry_of(&entries, "plain");
    assert!(
        plain.find_path("folder").is_some_and(Value::is_null),
        "没进子目录就没有分组: {plain:?}"
    );
    assert_eq!(
        plain
            .find_path("summary")
            .and_then(|v| v.find_path("description"))
            .and_then(Value::as_str),
        Some(""),
        "没写描述就是空串，不是 null: {plain:?}"
    );
}

/// 指向不存在的动作、或根本不是指令，都是调用方的问题（400），而且**一条都不该入库**。
#[tokio::test]
async fn an_unusable_definition_is_rejected_before_anything_is_stored() {
    let (_dir, _daemon, endpoint) = start_clean("reject").await;
    let (status, message) = code(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 5,
                auth_token: None,
                name: "ghost".into(),
                definition: definition("ghost", "does.not.exist"),
                original_name: None,
            },
        )
        .await,
    );
    assert_eq!(status, 400, "{message}");
    assert!(
        message.contains("does.not.exist"),
        "该点出是哪个动作: {message}"
    );
    assert_eq!(
        code(read(&endpoint, "ghost").await).0,
        404,
        "校验没过就不该入库"
    );

    let (status, _) = code(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 6,
                auth_token: None,
                name: "junk".into(),
                definition: Value::Str("这不是指令".into()),
                original_name: None,
            },
        )
        .await,
    );
    assert_eq!(status, 400);
    assert_eq!(code(read(&endpoint, "junk").await).0, 404);
}

/// 权限声明不够同样跑不起来——`run` 的第二道门，这里要在入库前就拦下。
///
/// 与「动作没注册」分开量：那类是 400（调用方写错了名字），这类是 403（调用方没被允许），
/// 宿主得能靠码分辨该让用户改定义还是改授权。
#[tokio::test]
async fn a_directive_that_asks_for_less_than_its_steps_need_is_rejected() {
    let (_dir, _daemon, endpoint) = start_clean("permissions").await;
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
                original_name: None,
            },
        )
        .await,
    );
    assert_eq!(status, 403, "{message}");
    assert!(
        message.contains("filesystem"),
        "该点出缺的是哪一类权限: {message}"
    );
    assert_eq!(code(read(&endpoint, "restricted").await).0, 404);
}

/// 指令名是**裸名**：`..` 与分隔符会让宿主拿去拼临时文件名、写进 IPC 请求，一律 400。
///
/// 这里量的是「错的是调用方还是服务端」：名字有问题、库里没有都不是 500。
#[tokio::test]
async fn a_bad_name_is_a_client_error_and_a_missing_one_is_not_found() {
    let (_dir, _daemon, endpoint) = start_clean("names").await;
    let mut bad = vec!["../secret", "a/b", "a\\b"];
    if cfg!(windows) {
        // 盘符相对名：`is_absolute()` 是 false，但仍不是「恰好一个普通组件」。
        bad.extend(["D:evil", "D:", "C:windows"]);
    }
    for name in bad {
        let (status, message) = code(read(&endpoint, name).await);
        assert_eq!(status, 400, "{name}: {message}");

        let (status, message) = code(
            send(
                &endpoint,
                Request::SaveDirective {
                    id: 7,
                    auth_token: None,
                    name: name.into(),
                    definition: definition(name, "template.render"),
                    original_name: None,
                },
            )
            .await,
        );
        assert_eq!(status, 400, "{name}: {message}");
    }
    assert_eq!(code(read(&endpoint, "nope").await).0, 404);

    // 跑一条不存在的指令也是 404，不是 500：错的是调用方点的名字，不是服务端。
    let (status, message) = code(run(&endpoint, "nope").await);
    assert_eq!(status, 404, "{message}");
}

/// 改名在库里是一次搬行：新名字在、旧名字没，不存在「两个名字各有一半」。
#[tokio::test]
async fn renaming_a_directive_leaves_no_second_entry() {
    let (_dir, _daemon, endpoint) = start_clean("rename").await;
    save(&endpoint, "build", "template.render").await;

    let renamed = save_definition(
        &endpoint,
        "build2",
        Some("build"),
        definition("build2", "template.render"),
    )
    .await;
    assert_eq!(
        renamed.find_path("name").and_then(Value::as_str),
        Some("build2")
    );

    assert_eq!(code(read(&endpoint, "build").await).0, 404, "旧名字该没了");
    assert_eq!(
        field_of(&list(&endpoint).await, "name"),
        vec!["build2"],
        "库里只该留一条"
    );
}

/// 改名撞上已有名字是 409：调用方能做的事与 400 不同——换个名字，或先删旧的。
#[tokio::test]
async fn renaming_onto_an_existing_name_conflicts() {
    let (_dir, _daemon, endpoint) = start_clean("conflict").await;
    save(&endpoint, "taken", "template.render").await;

    let (status, message) = code(
        send(
            &endpoint,
            Request::SaveDirective {
                id: 8,
                auth_token: None,
                name: "taken".into(),
                definition: definition("taken", "template.render"),
                original_name: Some("other".into()),
            },
        )
        .await,
    );
    assert_eq!(status, 409, "{message}");
    assert_eq!(
        field_of(&list(&endpoint).await, "name"),
        vec!["taken"],
        "冲突时两边都该保持原样"
    );
}

/// 删掉就是删掉：再读是 404，再删也是 404（而不是静默成功）。
#[tokio::test]
async fn deleting_a_directive_removes_it_once() {
    let (_dir, _daemon, endpoint) = start_clean("delete").await;
    save(&endpoint, "gone", "template.render").await;

    let reply = data(delete(&endpoint, "gone").await);
    assert_eq!(
        reply.find_path("name").and_then(Value::as_str),
        Some("gone")
    );
    assert_eq!(code(read(&endpoint, "gone").await).0, 404);
    assert_eq!(code(delete(&endpoint, "gone").await).0, 404);
}

/// 导入逐条报告：进库的、跳过的、坏了的分得开，坏的那条带原因。
///
/// 导入是用户拿自己的文件来换库里的内容，「哪个文件为什么没进来」必须能一眼看到——而不是
/// 只回一句「成功 3 条」。
#[tokio::test]
async fn importing_reports_every_entry() {
    let (dir, _daemon, endpoint) = start_clean("import").await;
    let yaml = dir.path().join("yaml");
    std::fs::create_dir_all(&yaml).expect("mkdir");
    std::fs::write(
        yaml.join("fresh.yaml"),
        "name: fresh\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write fresh");
    std::fs::write(yaml.join("broken.yaml"), "name: [未闭合\n").expect("write broken");
    std::fs::write(
        yaml.join("taken.yaml"),
        "name: taken\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write taken");
    save(&endpoint, "taken", "template.render").await;

    let report = import(&endpoint, &yaml, None, false, false).await;
    assert_eq!(report.find_path("created").and_then(Value::as_i64), Some(1));
    assert_eq!(report.find_path("updated").and_then(Value::as_i64), Some(0));
    assert_eq!(report.find_path("skipped").and_then(Value::as_i64), Some(1));
    assert_eq!(report.find_path("failed").and_then(Value::as_i64), Some(1));

    let entries = report.find_path("entries").expect("entries").clone();
    let broken = entry_of(&entries, "broken");
    assert_eq!(
        broken.find_path("status").and_then(Value::as_str),
        Some("failed")
    );
    assert!(
        broken.find_path("error").and_then(Value::as_str).is_some(),
        "坏了要说为什么: {broken:?}"
    );
    assert_eq!(
        entry_of(&entries, "taken")
            .find_path("status")
            .and_then(Value::as_str),
        Some("skipped"),
        "同名默认不覆盖: {entries:?}"
    );
    assert_eq!(
        field_of(&list(&endpoint).await, "name"),
        vec!["fresh", "taken"],
        "坏的那条不该把别的也带下水"
    );

    // 开着覆盖就成了更新（fresh 与 taken 两条），且不再跳过。
    let report = import(&endpoint, &yaml, None, true, false).await;
    assert_eq!(report.find_path("updated").and_then(Value::as_i64), Some(2));
    assert_eq!(report.find_path("skipped").and_then(Value::as_i64), Some(0));

    // dry-run 只说不做：报告照给，库里不留痕。
    std::fs::write(
        yaml.join("peek.yaml"),
        "name: peek\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write peek");
    let report = import(&endpoint, &yaml, Some("pack"), true, true).await;
    assert_eq!(report.find_path("created").and_then(Value::as_i64), Some(1));
    assert_eq!(code(read(&endpoint, "peek").await).0, 404, "dry-run 不写库");

    // `folder` 是目录导入的兜底分组（子目录优先），文件导入时就是它的分组。
    let single = dir.path().join("single.yaml");
    std::fs::write(
        &single,
        "name: single\nsteps:\n  - id: a\n    action: template.render\n",
    )
    .expect("write single");
    import(&endpoint, &single, Some("手工"), false, false).await;
    assert_eq!(
        entry_of(&list(&endpoint).await, "single")
            .find_path("folder")
            .and_then(Value::as_str),
        Some("手工")
    );
}

/// 库的键永远盖过模型里的 `name`：跑、查账本、卡片三处用的是同一个键。
///
/// v12 里文件主干与 YAML 的 `name` 可以不一致（执行只认模型），于是卡片按文件名查账本永远
/// 查不到、一直显示「未运行」。库里不存在第二个名字。
#[tokio::test]
async fn the_library_key_overwrites_the_name_inside_the_model() {
    let (_dir, _daemon, endpoint) = start_clean("key").await;
    let saved = save_definition(
        &endpoint,
        "file-stem",
        None,
        definition("declared-name", "template.render"),
    )
    .await;
    assert_eq!(
        saved
            .find_path("definition")
            .and_then(|v| v.find_path("name"))
            .and_then(Value::as_str),
        Some("file-stem"),
        "模型里的名字该被库键盖掉: {saved:?}"
    );

    data(run(&endpoint, "file-stem").await);
    let entries = list(&endpoint).await;
    assert_eq!(field_of(&entries, "name"), vec!["file-stem"], "{entries:?}");
    let entry = entry_of(&entries, "file-stem");
    let last = entry
        .find_path("last_run")
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
