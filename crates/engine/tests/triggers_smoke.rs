//! Trigger parsing and cron expr smoke tests.

use corex_engine::{find_cron_trigger, find_watch_trigger, parse_cron_expr, Directive};

#[test]
fn cron_expr_five_to_six_fields() {
    assert_eq!(parse_cron_expr("0 9 * * 1-5").unwrap(), "0 0 9 * * 1-5");
}

#[test]
fn parse_watch_paths() {
    let yaml = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: watch
    paths: ["./src"]
    debounce_ms: 500
"#;
    let d = Directive::from_yaml_str(yaml).unwrap();
    let w = find_watch_trigger(&d.triggers).unwrap().unwrap();
    assert_eq!(w.paths, vec!["./src"]);
    assert_eq!(w.debounce_ms, 500);
}

#[test]
fn cron_trigger_find() {
    let yaml = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: cron
    expr: "0 9 * * *"
"#;
    let d = Directive::from_yaml_str(yaml).unwrap();
    let c = find_cron_trigger(&d.triggers).unwrap().unwrap();
    assert_eq!(c.expr, "0 9 * * *");
    assert!(c.timezone.is_none());
}

#[test]
fn cron_trigger_timezone() {
    use corex_engine::{effective_cron_timezone, parse_cron_timezone, ResolvedCronTz};
    let yaml = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: cron
    expr: "0 9 * * 1-5"
    timezone: "+08:00"
"#;
    let d = Directive::from_yaml_str(yaml).unwrap();
    let c = find_cron_trigger(&d.triggers).unwrap().unwrap();
    assert_eq!(c.timezone.as_deref(), Some("+08:00"));
    let effective = effective_cron_timezone(c.timezone.as_deref(), "local");
    assert_eq!(effective, "+08:00");
    assert!(matches!(
        parse_cron_timezone(&effective).unwrap(),
        ResolvedCronTz::Fixed(_)
    ));
}

#[test]
fn watch_filter_excludes() {
    use corex_engine::path_matches;
    assert!(!path_matches(
        "node_modules/x",
        &[],
        &["**/node_modules/**".into()]
    ));
}

#[test]
fn reject_duplicate_cron_trigger() {
    let yaml = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: cron
    expr: "0 9 * * *"
  - type: cron
    expr: "0 10 * * *"
"#;
    let d = Directive::from_yaml_str(yaml).unwrap();
    assert!(find_cron_trigger(&d.triggers).is_err());
}
