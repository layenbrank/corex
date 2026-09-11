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

/// 终端/字体跟不上 Unicode 时，`COREX_ASCII=1` 换一套纯 ASCII 符号。
#[test]
fn ascii_symbols_are_opt_in() {
    let out = Command::new(COREX)
        .arg("doctor")
        .env("COREX_ASCII", "1")
        .output()
        .expect("corex doctor runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ok "), "stdout: {stdout}");
    assert!(!stdout.contains('✓'), "stdout: {stdout}");

    // 默认走 Unicode；非终端不上色，所以符号就是裸的 `✓`。
    let plain = run(&["doctor"]);
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
