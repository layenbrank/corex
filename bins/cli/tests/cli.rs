//! 端到端 CLI 行为：只有进程边界才能暴露的那部分。
//!
//! 这些用例通过 Cargo 的 `CARGO_BIN_EXE_corex` 跑真实二进制，因此覆盖
//! `crate::exit` 的退出码映射与 `crate::output` 的 stdout 策略，与 shell 看到的一致。

use std::io::Read;
use std::process::{Command, Output, Stdio};

/// 被测二进制的路径；集成测试由 Cargo 设置。
const COREX: &str = env!("CARGO_BIN_EXE_corex");

fn run(args: &[&str]) -> Output {
    Command::new(COREX).args(args).output().expect("corex runs")
}

/// `corex actions | head -1` 必须退 0：读方看够了不算失败。
/// 在输出层出现之前，`println!` 会在断管上 panic，而 release 档的 `panic = "abort"`
/// 会把进程直接 abort。
#[test]
fn closed_stdout_is_not_a_failure() {
    let mut child = Command::new(COREX)
        .arg("actions")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn corex actions");

    // 读一个字节后就关掉自己这端。动作列表远大于管道缓冲区，
    // 所以子进程之后必然会撞上 EPIPE。
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut first = [0u8; 1];
    let _ = stdout.read(&mut first);
    drop(stdout);

    let out = child.wait_with_output().expect("wait for corex");
    assert!(
        out.status.success(),
        "a closed stdout must not fail the command, got {:?}",
        out.status
    );
    assert!(
        out.stderr.is_empty(),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn actions_are_printed_on_stdout() {
    let out = run(&["actions"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("shell.run"),
        "expected the builtin listing on stdout"
    );
}

/// 格式不合法的指令属于编写错误——退出码 2 就是为它准备的。
/// 把它报成通用失败会变成 1。
#[test]
fn malformed_directive_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("broken.yaml");
    std::fs::write(&path, "steps: [this is not a mapping\n").expect("write fixture");

    let out = run(&["validate", path.to_str().expect("utf-8 path")]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 不存在的指令要在 stderr 上报告，并让命令失败。
///
/// `--dir` 指向临时目录，测试不碰真实的数据目录。
#[test]
fn missing_directive_fails_loudly() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = run(&[
        "--dir",
        dir.path().to_str().expect("utf-8 path"),
        "run",
        "definitely-not-a-directive",
    ]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("错误:"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 全局的 `--config <PATH>` 必须真的被读到。它曾经因为 clap 的 arg id 与某个子命令开关撞名
/// 而静默失效，接着每条命令都在取参数时 panic——是手工跑一次才发现的，所以这里把它写成用例。
#[test]
fn config_flag_selects_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("corex.toml");
    std::fs::write(&path, "[runtime]\nmax_parallel = 4\n").expect("write fixture");

    let out = run(&["--config", path.to_str().expect("utf-8 path"), "validate"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("corex.toml"),
        "the chosen file should be reported: {stdout}"
    );
}

/// 不会中断命令的配置问题也要报告出来。
#[test]
fn config_warning_is_reported_and_tolerated() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("corex.toml");
    // `warning` 不是合法 level；tracing 会把它当成 target 过滤器。
    std::fs::write(&path, "[logging]\nlevel = \"warning\"\n").expect("write fixture");

    let out = run(&["--config", path.to_str().expect("utf-8 path"), "actions"]);
    assert!(
        out.status.success(),
        "a warning must not fail the command: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("logging.level"), "stderr: {stderr}");
}

/// 文件存在却解析失败必须让运行失败，不能回退到默认值——否则 `strict_permissions = true`
/// 旁边写错一个字母，就会悄悄把门禁关掉。
#[test]
fn unparsable_config_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("corex.toml");
    std::fs::write(&path, "[runtime]\nstrict_permissons = true\n").expect("write fixture");

    let out = run(&["--config", path.to_str().expect("utf-8 path"), "actions"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 显式点名的配置文件不在，说明路径写错了，不是跑默认值的理由。它以前会退 0：
/// 读取层会跳过不存在的候选，而且当时没人区分“默认位置一个都没有”和“你指名的那个没有”。
#[test]
fn missing_config_file_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("nope.toml");
    let out = run(&["--config", path.to_str().expect("utf-8 path"), "actions"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// CLI 自身的用法失误带上 `EngineError::Usage`，因此报 2，
/// 而不是裸 `bail!` 过去那种通用的 1。
#[test]
fn misused_flag_is_a_usage_error() {
    let out = run(&["validate", "--strict"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 策略拒绝有自己的退出码，而且必须在指令被检查的两条路径上都成立：
/// `validate --strict`（`validate_permissions` 只给文字）与真实运行
/// （引擎会把动作错误包进 `StepFailed`）。
#[test]
fn policy_refusals_report_denied() {
    let dir = tempfile::tempdir().expect("temp dir");
    // 声明了一个步骤并不需要的类别，于是 `file.write` 没被覆盖。
    let yaml = dir.path().join("too-wide.yaml");
    std::fs::write(
        &yaml,
        concat!(
            "name: too-wide\n",
            "permissions:\n",
            "  network: true\n",
            "steps:\n",
            "  - id: write\n",
            "    action: file.write\n",
            "    params:\n",
            "      path: \"{{env.TEMP}}/corex-policy-probe.txt\"\n",
            "      content: hi\n",
        ),
    )
    .expect("write fixture");
    let yaml = yaml.to_str().expect("utf-8 path");

    let validated = run(&["validate", yaml, "--strict"]);
    assert_eq!(
        validated.status.code(),
        Some(3),
        "strict validate stderr: {}",
        String::from_utf8_lossy(&validated.stderr)
    );

    let cfg = dir.path().join("strict.toml");
    std::fs::write(&cfg, "[runtime]\nstrict_permissions = true\n").expect("write fixture");
    let ran = run(&["--config", cfg.to_str().expect("utf-8 path"), "run", yaml]);
    assert_eq!(
        ran.status.code(),
        Some(3),
        "run stderr: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

/// 指令单纯不存在属于用法失误（2），与引擎给它报的码一致；
/// CLI 有自己的路径解析器，它以前会回 1。
#[test]
fn missing_directive_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = run(&[
        "--dir",
        dir.path().to_str().expect("utf-8 path"),
        "run",
        "definitely-not-a-directive",
    ]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 写一条指令的临时夹具，返回 (目录, yaml 路径)。
fn directive(name: &str, body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(format!("{name}.yaml"));
    std::fs::write(&path, body).expect("write fixture");
    (dir, path)
}

/// 两步骤的最小指令：第一步渲染，第二步把结果写进临时目录。
const TWO_STEPS: &str = concat!(
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
    "      path: \"{{env.TEMP}}/corex-cli-test.txt\"\n",
    "      content: \"{{message}}\"\n",
);

/// 进度是给人看的，而且不能跑进 stdout：结果通道被污染，`| jq` 立刻读不懂。
#[test]
fn progress_goes_to_stderr_not_stdout() {
    let (_dir, path) = directive("probe", TWO_STEPS);
    let out = run(&["run", path.to_str().expect("utf-8 path")]);

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("template.render"), "stderr: {stderr}");
    assert!(stderr.contains("file.write"), "stderr: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("template.render"),
        "progress must not reach stdout: {stdout}"
    );
    // 结果仍然是一份可解析的 JSON 文档。
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is JSON");
    assert!(value.is_object(), "stdout: {stdout}");
}

/// `--quiet` 必须真的安静，否则脚本里的 stderr 会多出没人要的行。
#[test]
fn quiet_drops_the_progress_channel() {
    let (_dir, path) = directive("probe", TWO_STEPS);
    let out = run(&["run", path.to_str().expect("utf-8 path"), "--quiet"]);

    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("template.render") && !stderr.contains("file.write"),
        "stderr: {stderr}"
    );
}

/// 子进程的输出要有去处：`--json-events` 下它是 `step_output` 帧（stdout 保持纯 NDJSON），
/// 默认形态下它原样落在 stdout（与 v11 一致）。
///
/// 这条钉的是 daemon 场景的要害——本地跑时终端看得见不算数，见 `bins/daemon/tests/streaming.rs`。
#[test]
fn shell_output_has_a_channel() {
    let body = concat!(
        "name: probe\n",
        "permissions:\n",
        "  shell: true\n",
        "steps:\n",
        "  - id: echo\n",
        "    action: shell.run\n",
        "    params:\n",
        "      command: \"echo corex-cli-output-probe\"\n",
        "      host: cmd\n",
    );
    let (_dir, path) = directive("probe", body);
    let path = path.to_str().expect("utf-8 path");

    let out = run(&["run", path, "--json-events"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // 每一行都必须是 JSON：子进程原文混进来就会在这里炸掉。
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}")))
        .collect();
    let text: String = events
        .iter()
        .filter(|event| event["kind"] == "step_output" && event["stream"] == "stdout")
        .filter_map(|event| event["text"].as_str())
        .collect();
    assert!(
        text.contains("corex-cli-output-probe"),
        "stdout 上应当有 step_output 帧: {stdout}"
    );

    let plain = run(&["run", path]);
    assert!(plain.status.success());
    assert!(
        String::from_utf8_lossy(&plain.stdout).contains("corex-cli-output-probe"),
        "默认形态下输出仍要落在 stdout: {}",
        String::from_utf8_lossy(&plain.stdout)
    );
}

/// `--json-events`：stdout 是 NDJSON，最后一条固定是 `result`。
///
/// 每一行的负载就是 `corex_ipc::ProgressEvent`——与 daemon 在 `--remote` 下推回来的帧
/// 是同一种词汇，所以宿主不必为两条路径记两套字段名。
#[test]
fn json_events_end_with_the_result() {
    let (_dir, path) = directive("probe", TWO_STEPS);
    let out = run(&["run", path.to_str().expect("utf-8 path"), "--json-events"]);

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}")))
        .collect();
    assert_eq!(
        events.first().and_then(|e| e["kind"].as_str()),
        Some("step_start")
    );
    assert_eq!(
        events.last().and_then(|e| e["kind"].as_str()),
        Some("result")
    );
    assert!(
        events
            .iter()
            .any(|e| e["kind"] == "step_end" && e["ok"] == true),
        "expected a successful step_end: {stdout}"
    );
}

/// 没给指令名时，非终端环境必须当场失败，而不是静默挑一条跑。
#[test]
fn run_without_a_name_needs_a_terminal() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = run(&["--dir", dir.path().to_str().expect("utf-8 path"), "run"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 拼错动作 id 也是用法失误，并且要给出最接近的名字。
#[test]
fn unknown_action_suggests_a_neighbour() {
    let out = run(&["actions", "file.coppy"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("file.copy"), "stderr: {stderr}");
}

/// `-i` 的写法不对属于用法失误：退出码 2，而不是通用失败 1。
#[test]
fn malformed_input_is_a_usage_error() {
    let (_dir, path) = directive("probe", TWO_STEPS);
    let out = run(&["run", path.to_str().expect("utf-8 path"), "-i", "message"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 未声明的输入键要指出来（多半是笔误），但不阻断执行。
#[test]
fn unknown_input_is_reported_but_not_fatal() {
    let (_dir, path) = directive(
        "greet",
        concat!(
            "name: greet\n",
            "inputs:\n",
            "  - name: who\n",
            "    required: false\n",
            "    default: world\n",
            "steps:\n",
            "  - id: render\n",
            "    action: template.render\n",
            "    params:\n",
            "      template: \"hi\"\n",
        ),
    );
    // `--dry-run` 同样走输入解析，但不会执行步骤，测试因此没有副作用。
    let out = run(&[
        "run",
        path.to_str().expect("utf-8 path"),
        "-i",
        "woh=Typo",
        "--dry-run",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("未声明的输入 woh"), "stderr: {stderr}");
}

/// 动作按 bucket 分组列出；`--bucket` 只看一组，不认识的 bucket 是用法失误。
#[test]
fn actions_are_grouped_by_bucket() {
    let out = run(&["actions"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("data（"), "stdout: {stdout}");

    let ui = run(&["actions", "--bucket", "ui"]);
    assert!(ui.status.success());
    let stdout = String::from_utf8_lossy(&ui.stdout);
    assert!(stdout.contains("ui（"), "stdout: {stdout}");
    assert!(!stdout.contains("file.copy"), "只该出现 ui 组: {stdout}");

    let unknown = run(&["actions", "--bucket", "nope"]);
    assert_eq!(
        unknown.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&unknown.stderr)
    );
}

/// `--json` 给的是目录而不是排版结果：宿主与 agent 靠它知道「怎么调一个动作」，
/// 而不是只知道它叫什么。
#[test]
fn actions_json_is_a_catalog() {
    let out = run(&["actions", "--json"]);
    assert!(out.status.success());
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout 应当是一份 JSON 文档");
    let count = doc["count"].as_u64().expect("有 count");
    let actions = doc["actions"].as_array().expect("actions 是数组");
    assert_eq!(actions.len() as u64, count);
    assert!(count > 0, "目录不该是空的");

    let copy = actions
        .iter()
        .find(|item| item["id"].as_str() == Some("file.copy"))
        .expect("file.copy 在目录里");
    assert_eq!(copy["permissions"], serde_json::json!(["filesystem"]));
    assert_eq!(copy["input_schema"]["type"].as_str(), Some("object"));
    assert_eq!(
        copy["input_schema"]["properties"]["from"]["format"].as_str(),
        Some("path"),
        "file 类参数要告诉读方那是路径"
    );
}

/// 给 id 就只要那一个；`--bucket` 则收窄目录。两种都仍然拒绝拼错的 bucket。
#[test]
fn actions_json_narrows_to_one_action_or_bucket() {
    let one = run(&["actions", "file.copy", "--json"]);
    assert!(one.status.success());
    let doc: serde_json::Value =
        serde_json::from_slice(&one.stdout).expect("stdout 应当是一份 JSON 文档");
    assert_eq!(doc["id"].as_str(), Some("file.copy"));
    assert!(
        doc["input_schema"]["required"].as_array().is_some(),
        "必填参数要列出来"
    );

    let ui = run(&["actions", "--bucket", "ui", "--json"]);
    assert!(ui.status.success());
    let doc: serde_json::Value =
        serde_json::from_slice(&ui.stdout).expect("stdout 应当是一份 JSON 文档");
    assert_eq!(doc["bucket"].as_str(), Some("ui"));
    assert!(
        doc["actions"]
            .as_array()
            .expect("actions 是数组")
            .iter()
            .all(|item| item["bucket"].as_str() == Some("ui"))
    );

    let unknown = run(&["actions", "--bucket", "nope", "--json"]);
    assert_eq!(
        unknown.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&unknown.stderr)
    );
}

/// 终端/字体跟不上 Unicode 时，`COREX_ASCII=1` 换一套纯 ASCII 符号。
///
/// `doctor` 会碰数据目录（起步指令就写在那儿），所以钉在临时目录里跑：
/// 测试不该动开发机上的真实数据目录。
#[test]
fn ascii_symbols_are_opt_in() {
    let dir = tempfile::tempdir().expect("temp dir");
    let run_here = |args: &[&str], ascii: bool| {
        Command::new(COREX)
            .args(args)
            .env("COREX_DATA_DIR", dir.path())
            .env("COREX_ASCII", if ascii { "1" } else { "0" })
            .output()
            .expect("corex doctor runs")
    };

    let out = run_here(&["doctor"], true);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ok "), "stdout: {stdout}");
    assert!(!stdout.contains('✓'), "stdout: {stdout}");

    // 默认走 Unicode；非终端不上色，所以符号就是裸的 `✓`。
    let plain = run_here(&["doctor"], false);
    let stdout = String::from_utf8_lossy(&plain.stdout);
    assert!(stdout.contains('✓'), "stdout: {stdout}");
}

/// `--dry-run` 不执行，但要把步骤摊开；权限不覆盖时照旧退 3。
#[test]
fn dry_run_plans_without_executing() {
    let dir = tempfile::tempdir().expect("temp dir");
    // 输出路径跟着临时目录走：别的用例也会往 TEMP 里写文件，共用一个路径就会互相干扰。
    let target = dir.path().join("should-not-exist.txt");
    let path = dir.path().join("probe.yaml");
    let body = format!(
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
            "      path: \"{}\"\n",
            "      content: \"{{{{message}}}}\"\n",
        ),
        // Windows 路径要换成正斜杠：双引号标量里的 `\U` 是 YAML 转义序列。
        target.display().to_string().replace('\\', "/")
    );
    std::fs::write(&path, body).expect("write fixture");

    let out = run(&["run", path.to_str().expect("utf-8 path"), "--dry-run"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("render"), "stdout: {stdout}");
    assert!(stdout.contains("file.write"), "stdout: {stdout}");
    assert!(
        !target.exists(),
        "--dry-run must not execute steps: {}",
        target.display()
    );

    let (_dir2, wide) = directive(
        "too-wide",
        concat!(
            "name: too-wide\n",
            "permissions:\n",
            "  network: true\n",
            "steps:\n",
            "  - id: write\n",
            "    action: file.write\n",
            "    params:\n",
            "      path: \"{{env.TEMP}}/corex-dry-probe.txt\"\n",
            "      content: hi\n",
        ),
    );
    let refused = run(&["run", wide.to_str().expect("utf-8 path"), "--dry-run"]);
    assert_eq!(
        refused.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
}

/// `corex create` 生成的骨架必须能直接通过 `--strict` 校验，
/// 并在指令目录旁放一份 schema 供编辑器引用。
#[test]
fn created_scaffold_validates() {
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_str().expect("utf-8 path").to_string();

    let created = run(&["create", "probe", "-t", "hello", "--dir", &base]);
    assert!(
        created.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let yaml = dir.path().join("probe.yaml");
    let text = std::fs::read_to_string(&yaml).expect("scaffold exists");
    assert!(
        text.contains("yaml-language-server"),
        "the scaffold should carry the editor hint: {text}"
    );
    assert!(
        dir.path().join("directive.schema.json").exists(),
        "a schema copy should sit next to the directive"
    );

    let validated = run(&["validate", yaml.to_str().expect("utf-8 path"), "--strict"]);
    assert!(
        validated.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&validated.stderr)
    );
}

/// `corex schema` 打出来的是完整 JSON，不是一行摘要。
#[test]
fn schema_is_emitted_as_json() {
    let out = run(&["schema"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"$schema\""), "stdout: {stdout}");
    serde_json::from_str::<serde_json::Value>(&stdout).expect("schema is valid JSON");
}

/// `--watch` 要有东西可盯；空转的开关正是本仓库想清掉的东西。
#[test]
fn watch_without_a_path_is_a_usage_error() {
    let out = run(&["validate", "--watch"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// 未注册的动作是「你给的东西不对」，两条路径必须给同一个数字。
/// `validate` 曾经走裸 `bail!`（退 1），而 `run --dry-run` 是 `ActionNotRegistered`（退 2）。
#[test]
fn an_unregistered_action_is_a_usage_error_on_both_paths() {
    let (_dir, path) = directive(
        "probe",
        concat!(
            "name: probe\n",
            "steps:\n",
            "  - id: nope\n",
            "    action: does.not.exist\n",
        ),
    );
    let file = path.to_str().expect("utf-8 path");

    let validated = run(&["validate", file]);
    assert_eq!(
        validated.status.code(),
        Some(2),
        "validate stderr: {}",
        String::from_utf8_lossy(&validated.stderr)
    );

    let previewed = run(&["run", file, "--dry-run"]);
    assert_eq!(
        previewed.status.code(),
        Some(2),
        "dry-run stderr: {}",
        String::from_utf8_lossy(&previewed.stderr)
    );
}

/// `corex doctor` 至少不能崩，且要把数据目录报出来。
#[test]
fn doctor_reports_the_data_directory() {
    let out = run(&["doctor"]);
    assert!(
        out.status.code().is_some_and(|code| code <= 1),
        "doctor should be 0 or 1, got {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("corex"), "stdout: {stdout}");
}

/// 端点发现：daemon 写下的 `endpoint.json` 要盖过平台默认端点。
///
/// 用 `doctor` 的「IPC 端点」一行来断言——它是唯一把解析结果打出来的命令，因此不必真起
/// 一个 daemon。给一份空配置是为了避开仓库根的 `config/corex.toml`：那里若写了
/// `socket_path`，显式配置本来就该赢过发现，这条用例就不在测发现了。
#[test]
fn a_daemon_record_decides_the_endpoint() {
    let dir = tempfile::tempdir().expect("临时目录");
    let endpoint = if cfg!(windows) {
        r"\\.\pipe\corex-from-record"
    } else {
        "/tmp/corex-from-record.sock"
    };
    let record = serde_json::json!({
        "version": 1,
        "pid": 4242,
        "endpoint": endpoint,
        "kind": if cfg!(windows) { "pipe" } else { "socket" },
    });
    std::fs::write(
        dir.path().join("endpoint.json"),
        serde_json::to_vec(&record).expect("序列化记录"),
    )
    .expect("写入端点记录");
    let config = dir.path().join("empty.toml");
    std::fs::write(&config, "[daemon]\n").expect("写空配置");

    let out = Command::new(COREX)
        .args([
            "--config",
            config.to_str().expect("utf-8 配置路径"),
            "doctor",
        ])
        .env("COREX_DATA_DIR", dir.path())
        .output()
        .expect("corex runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("corex-from-record"),
        "doctor 该报出记录里的端点，stdout: {stdout}"
    );
}

/// 补全脚本是**回调式**的：脚本只把 shell 挂回 `corex`，候选由本进程现算。
///
/// 这条链路有两段，而两段写坏的观感完全一样：按 Tab 什么都不出来。
#[test]
fn completions_registers_a_callback() {
    let out = run(&["completions", "bash"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let script = String::from_utf8_lossy(&out.stdout);
    assert!(script.contains("COMPLETE=\"bash\""), "script: {script}");
    assert!(script.contains("_clap_complete_corex"), "script: {script}");
    assert!(script.contains("complete -"), "script: {script}");
}

/// 回调那一段：`COMPLETE=<shell> corex -- <命令行>` 要吐出候选。
///
/// bash 版的游标位置来自环境变量（PowerShell 版来自参数个数），这里补上它就是
/// `corex ru<Tab>` 那一刻。
///
/// `run` 的候选来自数据目录里的指令，所以钉在临时目录里跑，别去翻开发机的真实目录。
#[test]
fn completion_callback_returns_candidates() {
    let dir = tempfile::tempdir().expect("temp dir");
    let out = Command::new(COREX)
        .args(["--", "corex", "ru"])
        .env("COMPLETE", "bash")
        .env("_CLAP_COMPLETE_INDEX", "1")
        .env("COREX_DATA_DIR", dir.path())
        .output()
        .expect("corex completes");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let candidates = String::from_utf8_lossy(&out.stdout);
    assert!(
        candidates.lines().any(|line| line.trim() == "run"),
        "candidates: {candidates:?}"
    );
}

/// `corex paths --json` 是宿主的路径事实来源：每个值都必须是 corex 自己算的，
/// 宿主据此读指令、连 daemon，不必复刻一遍平台规则。
///
/// `COREX_DATA_DIR` 与 `--config` 一起把这次运行钉在临时目录里：
/// 既证明环境变量被尊重，也不让开发机上的真实数据目录与配置影响断言。
#[test]
fn paths_json_reports_the_effective_locations() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("corex.toml");
    std::fs::write(&config, "# 空配置：端点因此只能是平台默认\n").expect("write fixture");

    let out = Command::new(COREX)
        .args([
            "--config",
            config.to_str().expect("utf-8 path"),
            "paths",
            "--json",
        ])
        .env("COREX_DATA_DIR", dir.path())
        .output()
        .expect("corex paths runs");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let listing: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("paths --json 打的是 JSON");
    let text = |key: &str| {
        listing[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} 必须是字符串: {listing}"))
            .to_string()
    };

    assert_eq!(
        std::path::PathBuf::from(text("data_dir")),
        dir.path(),
        "COREX_DATA_DIR 必须原样生效"
    );
    assert_eq!(
        std::path::PathBuf::from(text("directives_dir")),
        dir.path().join("directives"),
        "指令目录就是数据目录下的 directives"
    );
    assert_eq!(
        text("kind"),
        if cfg!(windows) { "pipe" } else { "socket" },
        "没有记录时按平台默认"
    );
    assert!(!text("endpoint").is_empty(), "端点必须给出来");
    assert!(
        listing["token_file"].is_null(),
        "没有 daemon 就没有 token 文件: {listing}"
    );
    assert_eq!(
        text("version"),
        env!("CARGO_PKG_VERSION"),
        "宿主靠版本判断对面支不支持某个字段"
    );
}

/// 空数据目录第一次被 CLI 触碰时应该长出起步指令：这是「初始不该是空页面」那条约定的
/// 落点，也是宿主要求用户去写第一条指令之前能看到的全部。
///
/// 断言落在这里而不是 `starter.rs` 的单元测试里，是因为真正的门槛是**接线**：
/// `Paths::dir` 走的是新目录才播种的那条路，忘了接就没东西可跑。
#[test]
fn empty_data_directory_gets_starter_directives() {
    let dir = tempfile::tempdir().expect("temp dir");
    let run_here = |args: &[&str]| {
        Command::new(COREX)
            .args(args)
            .env("COREX_DATA_DIR", dir.path())
            .output()
            .expect("corex runs")
    };

    let out = run_here(&["paths", "--json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let listing: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("paths --json 打的是 JSON");
    let directives_dir = std::path::PathBuf::from(
        listing["directives_dir"]
            .as_str()
            .expect("directives_dir 必须是字符串"),
    );

    let mut seeded: Vec<String> = std::fs::read_dir(&directives_dir)
        .expect("指令目录应当已经建出来")
        .map(|entry| {
            entry
                .expect("dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    seeded.sort();

    let mut expected: Vec<String> = corex_engine::starter::names()
        .iter()
        .map(|name| format!("{name}.yaml"))
        .collect();
    expected.sort();
    assert_eq!(seeded, expected, "起步指令应当原样落进数据目录");

    // 播种出来的东西就是指令，不是文案：每条都要过 CLI 那道门。
    for name in &expected {
        let path = directives_dir.join(name);
        let out = run_here(&["validate", path.to_str().expect("utf-8 path"), "--strict"]);
        assert!(
            out.status.success(),
            "{name} 不是一条能用的指令: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // 第二次进来不许再写一遍，也不许覆盖别人放进去的东西。
    let mine = directives_dir.join("mine.yaml");
    std::fs::write(&mine, "name: mine\nsteps: []\n").expect("写自己的指令");
    let out = run_here(&["paths", "--json"]);
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_dir(&directives_dir)
            .expect("读回指令目录")
            .count(),
        expected.len() + 1,
        "已有指令的数据目录必须原样不动"
    );
    assert_eq!(
        std::fs::read_to_string(&mine).expect("读回自己的指令"),
        "name: mine\nsteps: []\n"
    );
}
