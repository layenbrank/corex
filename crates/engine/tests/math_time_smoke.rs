//! 端到端冒烟：`math.*` 与 `time.*` 走完整流水线（含参数校验与模板取用）。
//!
//! 刻意省掉所有可选参数（`format` / `utc` / `hours` / `minutes`），
//! 让「可选参数被误标成必填」这类元数据错误在引擎校验里就暴露出来。

use corex_core::{ExecutionContext, RuntimeConfig};
use corex_engine::{Directive, Pipeline};
use corex_registry::ActionRegistry;
use std::sync::Arc;

#[tokio::test]
async fn math_and_time_steps_run_end_to_end() {
    let yaml = r#"
name: math-time-smoke
steps:
  - id: eval
    action: math.eval
    params:
      expr: "1 + 2 * 3"
    save_to: ev
  - id: round
    action: math.round
    params:
      n: 3.6
    save_to: r3
  - id: stats
    action: math.stats
    params:
      values: [1, 2, 3.5, 4]
    save_to: st
  - id: parse
    action: time.parse
    params:
      text: "2024-01-02T03:04:05Z"
    save_to: p
  - id: add
    action: time.add
    params:
      at: "{{p.iso8601}}"
      days: 1
      seconds: 30
    save_to: a
  - id: sub
    action: time.sub
    params:
      at: "{{p.iso8601}}"
      hours: 1
    save_to: s
  - id: tz
    action: time.tz
    params:
      at: "{{p.iso8601}}"
      offset: "+08:00"
    save_to: z
  - id: report
    action: template.render
    params:
      template: >-
        eval={{ev}} round={{r3}} stats={{st.sum}}/{{st.mean}}/{{st.count}}
        parse={{p.iso8601}} unix={{p.unix}} add={{a.iso8601}} sub={{s.iso8601}} tz={{z.iso8601}}
    save_to: report
"#;

    let directive = Directive::from_yaml_str(yaml).expect("parse Directive");
    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    let pipeline = Pipeline::new(Arc::new(registry));

    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let result = pipeline.execute(&directive, ctx).await.expect("execute");

    assert_eq!(
        result.as_str(),
        Some(
            "eval=7 round=4 stats=10.5/2.625/4 parse=2024-01-02T03:04:05+00:00 \
             unix=1704164645 add=2024-01-03T03:04:35+00:00 sub=2024-01-02T02:04:05+00:00 \
             tz=2024-01-02T11:04:05+08:00"
        )
    );
}
