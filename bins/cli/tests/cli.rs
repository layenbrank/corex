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
