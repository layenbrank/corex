//! 进度上报口的覆盖面：`parallel` 分支里的步骤也必须上报。
//!
//! 分支漏挂上报口时，CLI 侧表现是「parallel 里的步骤既没有 spinner 也没有 ✓ 结论行」，
//! 引擎这边看不出异常——所以这条路径值得一个回归测试钉住。
//!
//! 另一件事是**动作吐出来的文本**：它同样只在挂了口子的时候才出得来（见最后一条用例）。

use corex_core::{ExecutionContext, Mark, Observer, RuntimeConfig, Spot, Stream, Unit};
use corex_engine::{Directive, Pipeline};
use corex_registry::ActionRegistry;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 一次分块记录：步骤 id、已完成量、总量与单位。
type Chunk = (String, u64, Option<u64>, Unit);

/// 一次文本输出：步骤 id、流与原文。
type Output = (String, Stream, String);

/// 只记事：步骤 id 与分块进度各存一份，断言用。
#[derive(Debug, Default)]
struct Recorder {
    began: Mutex<Vec<String>>,
    chunks: Mutex<Vec<Chunk>>,
    outputs: Mutex<Vec<Output>>,
    ended: Mutex<Vec<String>>,
}

impl Recorder {
    fn began(&self) -> Vec<String> {
        self.began.lock().unwrap().clone()
    }

    fn outputs(&self) -> Vec<Output> {
        self.outputs.lock().unwrap().clone()
    }

    fn ended(&self) -> Vec<String> {
        self.ended.lock().unwrap().clone()
    }
}

impl Observer for Recorder {
    fn begin(&self, at: Spot<'_>) {
        self.began.lock().unwrap().push(at.id.to_string());
    }

    fn chunk(&self, at: Spot<'_>, mark: Mark) {
        self.chunks
            .lock()
            .unwrap()
            .push((at.id.to_string(), mark.done, mark.total, mark.unit));
    }

    fn output(&self, at: Spot<'_>, stream: Stream, text: &str) {
        self.outputs
            .lock()
            .unwrap()
            .push((at.id.to_string(), stream, text.to_string()));
    }

    fn end(&self, at: Spot<'_>, _took: Duration, _ok: bool) {
        self.ended.lock().unwrap().push(at.id.to_string());
    }
}

fn registry() -> Arc<ActionRegistry> {
    let mut r = ActionRegistry::new();
    r.register_builtins();
    Arc::new(r)
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[tokio::test]
async fn parallel_children_report_begin_and_end() {
    let yaml = r#"
name: observer-parallel
steps:
  - id: fanout
    max_concurrency: 2
    parallel:
      - id: a
        action: template.render
        params:
          template: "A"
      - id: b
        action: template.render
        params:
          template: "B"
  - id: after
    action: template.render
    params:
      template: "C"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let recorder = Arc::new(Recorder::default());
    let pipeline = Pipeline::new(registry()).with_observer(recorder.clone());

    pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect("directive should succeed");

    // parallel 本身不是动作步骤，上报的只有它的两个分支和随后的顶层步骤。
    assert_eq!(
        sorted(recorder.began()),
        vec!["a", "after", "b"],
        "parallel 分支必须上报 begin"
    );
    assert_eq!(
        sorted(recorder.ended()),
        vec!["a", "after", "b"],
        "parallel 分支必须上报 end"
    );
}

#[tokio::test]
async fn chunk_reaches_the_observer() {
    let dir = std::env::temp_dir().join(format!("corex-observer-{}", std::process::id()));
    let from = dir.join("from.bin");
    let to = dir.join("to.bin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&from, vec![7u8; 4096]).unwrap();

    let yaml = format!(
        r#"
name: observer-chunk
permissions:
  filesystem: true
steps:
  - id: copy
    action: file.copy
    params:
      from: "{}"
      to: "{}"
"#,
        from.display().to_string().replace('\\', "/"),
        to.display().to_string().replace('\\', "/"),
    );
    let directive = Directive::from_yaml_str(&yaml).unwrap();
    let recorder = Arc::new(Recorder::default());
    let pipeline = Pipeline::new(registry()).with_observer(recorder.clone());

    pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect("copy should succeed");

    let chunks = recorder.chunks.lock().unwrap().clone();
    assert!(
        chunks.iter().any(|(id, done, total, unit)| id == "copy"
            && *done == 4096
            && *total == Some(4096)
            && *unit == Unit::Bytes),
        "分块进度应当带着步骤 id 与字节数抵达上报口，实得: {chunks:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// 子进程的输出必须在**还在跑的时候**经上报口走，而不是只落回本进程的 stdout。
///
/// 后者在 daemon 场景等于丢失：子进程的流接的是 daemon 自己的控制台，宿主一个字节也看不到，
/// 表现就是「指令日志里只有步骤，没有命令的输出」。`host: cmd` 让这条用例两边都能跑
/// （Windows 走 `cmd /C`，别处走 `sh -c`）。
#[tokio::test]
async fn shell_output_reaches_the_observer() {
    let yaml = r#"
name: observer-output
permissions:
  shell: true
steps:
  - id: echo
    action: shell.run
    params:
      command: "echo corex-output-probe"
      host: cmd
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let recorder = Arc::new(Recorder::default());
    let pipeline = Pipeline::new(registry()).with_observer(recorder.clone());

    let result = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect("shell.run should succeed");

    let text: String = recorder
        .outputs()
        .iter()
        .filter(|(id, stream, _)| id == "echo" && *stream == Stream::Stdout)
        .map(|(_, _, text)| text.as_str())
        .collect();
    assert!(
        text.contains("corex-output-probe"),
        "stdout 应当原文抵达上报口，实得: {:?}",
        recorder.outputs()
    );
    // 上报不等于不再收集：动作的返回值仍是同一条输出。
    assert!(
        result
            .find_path("stdout")
            .and_then(|v| v.as_str())
            .is_some_and(|stdout| stdout.contains("corex-output-probe")),
        "动作结果里也该有这段输出: {result:?}"
    );
}
