//! 控制流测试：if / repeat / parallel。

use corex_core::{ExecutionContext, RuntimeConfig};
use corex_engine::{Directive, Pipeline};
use corex_registry::ActionRegistry;
use std::sync::Arc;

fn registry() -> Arc<ActionRegistry> {
    let mut r = ActionRegistry::new();
    r.register_builtins();
    Arc::new(r)
}

#[tokio::test]
async fn if_then_branch() {
    let yaml = r#"
name: if-then
variables:
  flag: true
steps:
  - id: branch
    if:
      eq: ["{{flag}}", true]
    then:
      - id: write_yes
        action: template.render
        params:
          template: "yes"
    else:
      - id: write_no
        action: template.render
        params:
          template: "no"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("yes"));
}

#[tokio::test]
async fn if_else_branch() {
    let yaml = r#"
name: if-else
variables:
  flag: false
steps:
  - id: branch
    if:
      eq: ["{{flag}}", true]
    then:
      - id: write_yes
        action: template.render
        params:
          template: "yes"
    else:
      - id: write_no
        action: template.render
        params:
          template: "no"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("no"));
}

#[tokio::test]
async fn repeat_count() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("count.txt");
    let path = out.to_string_lossy().replace('\\', "/");

    let yaml = format!(
        r#"
name: repeat-count
steps:
  - id: loop
    repeat:
      count: 3
      as: i
    steps:
      - id: append
        action: file.write
        params:
          path: "{path}"
          content: "n={{{{i}}}}"
"#
    );

    let directive = Directive::from_yaml_str(&yaml).unwrap();
    // 调试用：确认 as_var 解析成功
    match &directive.steps[0] {
        corex_engine::Step::Repeat(r) => assert_eq!(r.repeat.as_var, "i"),
        other => panic!("expected repeat, got {other:?}"),
    }

    let pipeline = Pipeline::new(registry());
    pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert_eq!(text, "n=2");
}

#[tokio::test]
async fn parallel_merges_step_outputs() {
    let yaml = r#"
name: parallel-merge
steps:
  - id: fanout
    max_concurrency: 2
    parallel:
      - id: a
        action: template.render
        params:
          template: "A"
        save_to: va
      - id: b
        action: template.render
        params:
          template: "B"
        save_to: vb
  - id: join
    action: template.render
    params:
      template: "{{va}}-{{vb}}"
      context:
        va: "{{va}}"
        vb: "{{vb}}"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("A-B"));
}

/// `repeat` 的 `max_concurrency` 与它**同级**：填了就并发跑，每个元素一份上下文副本，
/// 但**按元素顺序**合并回来，所以「后者覆盖前者」仍然可预期。
#[tokio::test]
async fn repeat_max_concurrency_runs_collection() {
    let yaml = r#"
name: repeat-concurrent
variables:
  items: [1, 2, 3, 4]
steps:
  - id: loop
    max_concurrency: 4
    repeat:
      each: "{{items}}"
      as: n
    steps:
      - id: render
        action: template.render
        params:
          template: "n={{n}}"
        save_to: last_n
  - id: echo
    action: template.render
    params:
      template: "{{last_n}}"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let result = Pipeline::new(registry())
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    // 合并按元素顺序 → 第 4 个元素的值胜出。
    assert_eq!(result.as_str(), Some("n=4"));
}

/// 省略 `max_concurrency` 与 `max_concurrency: 1` 都是串行，`> 1` 才是并发。
/// 两种跑法**语义不同**，不是快慢之别：同一份循环体，串行时元素共享一份上下文
/// （`carry` 累加成 `123`），并发时每个元素从同一份快照出发（只剩最后一个写的 `3`）。
#[tokio::test]
async fn repeat_serial_shares_context_but_concurrent_isolates_it() {
    let template = r#"
name: repeat-context
variables:
  carry: ""
  items: [1, 2, 3]
steps:
  - id: loop
{{LOOP}}
  - id: echo
    action: template.render
    params:
      template: "{{carry}}"
"#;
    let run = |loop_step: &str| {
        let yaml = template.replace("{{LOOP}}", loop_step);
        async move {
            let directive = Directive::from_yaml_str(&yaml).unwrap();
            let value = Pipeline::new(registry())
                .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
                .await
                .unwrap();
            value.as_str().map(str::to_owned)
        }
    };

    let serial = r#"    repeat:
      each: "{{items}}"
      as: n
    steps:
      - id: carry_on
        action: template.render
        params:
          template: "{{carry}}{{n}}"
        save_to: carry"#;
    let concurrent = r#"    max_concurrency: 3
    repeat:
      each: "{{items}}"
      as: n
    steps:
      - id: carry_on
        action: template.render
        params:
          template: "{{carry}}{{n}}"
        save_to: carry"#;

    assert_eq!(run(serial).await.as_deref(), Some("123"));
    assert_eq!(run(concurrent).await.as_deref(), Some("3"));
}

/// `repeat.count` 也能并发：`count` 下 `as` / `index` 绑的都是序号。
#[tokio::test]
async fn repeat_count_can_run_concurrently() {
    let yaml = r#"
name: repeat-count-concurrent
steps:
  - id: loop
    max_concurrency: 4
    repeat:
      count: 3
      as: i
    steps:
      - id: render
        action: template.render
        params:
          template: "i={{i}} index={{index}}"
        save_to: last
  - id: echo
    action: template.render
    params:
      template: "{{last}}"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let result = Pipeline::new(registry())
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("i=2 index=2"));
}

/// `parallel` 的分支可以是一组步骤。
/// 真实流程常有「先算再提交」这种两步串起来的分支（分片上传的「算整文件摘要 →
/// 算完立刻 PATCH」就是），而分支里只能放一个步骤时它写不出来。
#[tokio::test]
async fn parallel_branch_can_hold_a_sequence() {
    let yaml = r#"
name: parallel-sequence
steps:
  - id: fanout
    max_concurrency: 2
    parallel:
      - id: first
        steps:
          - id: a
            action: template.render
            params:
              template: "A"
            save_to: va
          - id: b
            action: template.render
            params:
              template: "{{va}}-B"
            save_to: vb
      - id: second
        action: template.render
        params:
          template: "C"
        save_to: vc
  - id: join
    action: template.render
    params:
      template: "{{vb}}|{{vc}}"
      context:
        vb: "{{vb}}"
        vc: "{{vc}}"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let result = Pipeline::new(registry())
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("A-B|C"));
}

/// 对象数组按**子集**匹配：服务端的对象常带我们没写的字段。
///
/// 断点续传靠这一条：只想问「有没有 index=1 且 hash 一致的那一项」，
/// 服务端多带 `size` / `updatedAt` 之类不该影响判断。
#[tokio::test]
async fn contains_matches_object_subsets() {
    for (hash, expected) in [("bb", "hit"), ("cc", "miss")] {
        let yaml = format!(
            r#"
name: contains-object
variables:
  uploaded:
    - {{ index: 0, hash: "aa", size: 10 }}
    - {{ index: 1, hash: "bb", size: 20 }}
  want: 1
steps:
  - id: branch
    if:
      contains:
        - "{{{{uploaded}}}}"
        - index: "{{{{want}}}}"
          hash: "{hash}"
    then:
      - id: hit
        action: template.render
        params:
          template: "hit"
    else:
      - id: miss
        action: template.render
        params:
          template: "miss"
"#
        );
        let directive = Directive::from_yaml_str(&yaml).unwrap();
        let result = Pipeline::new(registry())
            .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
            .await
            .unwrap();
        assert_eq!(result.as_str(), Some(expected), "hash={hash}");
    }
}

/// `contains` 是断点续传的判据：服务端已上传的分片序号里有没有这一片。
///
/// 序号在本地是 `Int`，在服务端回的分片表里常常是字符串——两边都算「有」。
#[tokio::test]
async fn contains_matches_array_members_across_number_and_text() {
    for (index, expected) in [(0, "skip"), (3, "upload")] {
        let yaml = format!(
            r#"
name: contains-array
variables:
  uploaded: ["0", 1, 2]
  index: {index}
steps:
  - id: branch
    if:
      contains: ["{{{{uploaded}}}}", "{{{{index}}}}"]
    then:
      - id: hit
        action: template.render
        params:
          template: "skip"
    else:
      - id: miss
        action: template.render
        params:
          template: "upload"
"#
        );
        let directive = Directive::from_yaml_str(&yaml).unwrap();
        let result = Pipeline::new(registry())
            .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
            .await
            .unwrap();
        assert_eq!(result.as_str(), Some(expected), "index={index}");
    }
}

/// 字符串 haystack 按子串判断，map 按键判断。
#[tokio::test]
async fn contains_handles_text_and_map() {
    let yaml = r#"
name: contains-text
variables:
  uploaded: { "0": true, "1": true }
steps:
  - id: branch
    if:
      and:
        - contains: ["hello world", "world"]
        - contains: ["{{uploaded}}", "1"]
    then:
      - id: hit
        action: template.render
        params:
          template: "yes"
    else:
      - id: miss
        action: template.render
        params:
          template: "no"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let result = Pipeline::new(registry())
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("yes"));
}
