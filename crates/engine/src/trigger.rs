//! Parse directive triggers into runtime configs.

use crate::definition::Trigger;
use serde::{Deserialize, Serialize, Serializer};

pub const DEBOUNCE_MS: u64 = 300;
pub const COOLDOWN_MS: u64 = 1_000;

/// Default excludes (Vite-style): always skip VCS, deps, and test output unless overridden.
pub const WATCH_EXCLUDES: &[&str] = &[
    "**/.git/**",
    "**/node_modules/**",
    "**/test-results/**",
];

/// Parsed watch trigger (paths may include files or directories).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    pub paths: Vec<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub debounce_ms: u64,
    pub cooldown_ms: u64,
    #[serde(default)]
    pub immediate: bool,
    #[serde(default)]
    pub poll: bool,
    #[serde(default)]
    pub events: Vec<String>,
}

/// Parsed cron trigger.
#[derive(Debug, Clone)]
pub struct CronConfig {
    pub expr: String,
    /// Optional override; empty/`None` → use `RuntimeConfig.cron_timezone`.
    pub timezone: Option<String>,
}

/// Raw YAML trigger.
#[derive(Debug, Deserialize)]
struct RawTrigger {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    expr: Option<String>,
    #[serde(default)]
    paths: Option<Vec<String>>,
    #[serde(default)]
    includes: Option<Vec<String>>,
    #[serde(default)]
    excludes: Option<Vec<String>>,
    #[serde(default)]
    debounce_ms: Option<u64>,
    #[serde(default)]
    cooldown_ms: Option<u64>,
    #[serde(default)]
    immediate: Option<bool>,
    #[serde(default)]
    poll: Option<bool>,
    #[serde(default)]
    events: Option<Vec<String>>,
    #[serde(default)]
    timezone: Option<String>,
}

impl Trigger {
    pub fn parse_watch(&self) -> Option<WatchConfig> {
        match self {
            Trigger::Watch(cfg) => Some(cfg.clone()),
            _ => None,
        }
    }

    pub fn parse_cron(&self) -> Option<CronConfig> {
        match self {
            Trigger::Cron { expr, timezone } => Some(CronConfig {
                expr: expr.clone(),
                timezone: timezone.clone(),
            }),
            _ => None,
        }
    }
}

impl Serialize for Trigger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        match self {
            Trigger::Cron { expr, timezone } => {
                let fields = if timezone.is_some() { 3 } else { 2 };
                let mut s = serializer.serialize_struct("Trigger", fields)?;
                s.serialize_field("type", "cron")?;
                s.serialize_field("expr", expr)?;
                if let Some(tz) = timezone {
                    s.serialize_field("timezone", tz)?;
                }
                s.end()
            }
            Trigger::Watch(w) => {
                let mut s = serializer.serialize_struct("Trigger", 9)?;
                s.serialize_field("type", "watch")?;
                s.serialize_field("paths", &w.paths)?;
                s.serialize_field("includes", &w.includes)?;
                s.serialize_field("excludes", &w.excludes)?;
                s.serialize_field("debounce_ms", &w.debounce_ms)?;
                s.serialize_field("cooldown_ms", &w.cooldown_ms)?;
                s.serialize_field("immediate", &w.immediate)?;
                s.serialize_field("poll", &w.poll)?;
                s.serialize_field("events", &w.events)?;
                s.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for Trigger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawTrigger::deserialize(deserializer)?;
        let kind = raw.kind.to_ascii_lowercase();
        match kind.as_str() {
            "cron" => {
                let expr = raw
                    .expr
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| serde::de::Error::custom("cron 需要 expr"))?;
                let timezone = raw
                    .timezone
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                Ok(Trigger::Cron { expr, timezone })
            }
            "watch" => parse_watch_fields(raw)
                .map(Trigger::Watch)
                .map_err(serde::de::Error::custom),
            other => Err(serde::de::Error::custom(format!(
                "未知 trigger.type: {other}（支持 cron、watch）"
            ))),
        }
    }
}

fn parse_watch_fields(raw: RawTrigger) -> Result<WatchConfig, String> {
    let paths = raw.paths.unwrap_or_default();
    if paths.is_empty() {
        return Err("watch 需要 paths".into());
    }
    let debounce_ms = raw.debounce_ms.unwrap_or(DEBOUNCE_MS);
    let cooldown_ms = raw
        .cooldown_ms
        .unwrap_or_else(|| debounce_ms.saturating_mul(2).max(COOLDOWN_MS));
    let mut excludes = raw.excludes.unwrap_or_default();
    for pat in WATCH_EXCLUDES {
        if !excludes.iter().any(|e| e == pat) {
            excludes.push((*pat).to_string());
        }
    }
    Ok(WatchConfig {
        paths,
        includes: raw.includes.unwrap_or_default(),
        excludes,
        debounce_ms,
        cooldown_ms,
        immediate: raw.immediate.unwrap_or(false),
        poll: raw.poll.unwrap_or(false),
        events: raw.events.unwrap_or_default(),
    })
}

pub fn find_watch_trigger(triggers: &[Trigger]) -> Result<Option<WatchConfig>, corex_core::EngineError> {
    let mut iter = triggers.iter().filter_map(|t| t.parse_watch());
    let first = iter.next();
    if iter.next().is_some() {
        return Err(corex_core::EngineError::ParseError(
            "指令仅能声明一个 watch 触发器".into(),
        ));
    }
    Ok(first)
}

pub fn find_cron_trigger(triggers: &[Trigger]) -> Result<Option<CronConfig>, corex_core::EngineError> {
    let mut iter = triggers.iter().filter_map(|t| t.parse_cron());
    let first = iter.next();
    if iter.next().is_some() {
        return Err(corex_core::EngineError::ParseError(
            "指令仅能声明一个 cron 触发器".into(),
        ));
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::Directive;

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
    fn parse_watch_merges_default_excludes() {
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
"#;
        let d = Directive::from_yaml_str(yaml).unwrap();
        let w = find_watch_trigger(&d.triggers).unwrap().unwrap();
        assert!(w.excludes.iter().any(|e| e.contains(".git")));
        assert!(w.excludes.iter().any(|e| e.contains("node_modules")));
        assert!(w.excludes.iter().any(|e| e.contains("test-results")));
    }

    #[test]
    fn parse_watch_immediate_and_poll() {
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
    immediate: true
    poll: true
    events: ["create", "modify"]
"#;
        let d = Directive::from_yaml_str(yaml).unwrap();
        let w = find_watch_trigger(&d.triggers).unwrap().unwrap();
        assert!(w.immediate);
        assert!(w.poll);
        assert_eq!(w.events, vec!["create", "modify"]);
    }

    #[test]
    fn parse_watch_multi_paths() {
        let yaml = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: watch
    paths: ["./a", "./b"]
"#;
        let d = Directive::from_yaml_str(yaml).unwrap();
        assert_eq!(
            find_watch_trigger(&d.triggers).unwrap().unwrap().paths.len(),
            2
        );
    }

    #[test]
    fn reject_unknown_trigger_type() {
        let yaml = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: unknown_trigger
    keys: "Ctrl+K"
"#;
        assert!(Directive::from_yaml_str(yaml).is_err());
    }

    #[test]
    fn watch_and_cron_may_coexist() {
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
  - type: watch
    paths: ["./src"]
"#;
        let d = Directive::from_yaml_str(yaml).unwrap();
        assert!(find_watch_trigger(&d.triggers).unwrap().is_some());
        assert!(find_cron_trigger(&d.triggers).unwrap().is_some());
    }

    #[test]
    fn reject_duplicate_watch_trigger() {
        let yaml = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: watch
    paths: ["./a"]
  - type: watch
    paths: ["./b"]
"#;
        let d = Directive::from_yaml_str(yaml).unwrap();
        assert!(find_watch_trigger(&d.triggers).is_err());
    }
}
