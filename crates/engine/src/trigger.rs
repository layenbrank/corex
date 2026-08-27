//! Parse directive triggers into runtime configs.

use crate::definition::Trigger;
use serde::{Deserialize, Serialize, Serializer};

pub const DEBOUNCE_MS: u64 = 300;
pub const COOLDOWN_MS: u64 = 1_000;

/// Parsed watch trigger (paths may include files or directories).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    pub paths: Vec<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub debounce_ms: u64,
    pub cooldown_ms: u64,
}

/// Parsed cron trigger.
#[derive(Debug, Clone)]
pub struct CronConfig {
    pub expr: String,
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
            Trigger::Cron { expr } => Some(CronConfig { expr: expr.clone() }),
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
            Trigger::Cron { expr } => {
                let mut s = serializer.serialize_struct("Trigger", 2)?;
                s.serialize_field("type", "cron")?;
                s.serialize_field("expr", expr)?;
                s.end()
            }
            Trigger::Watch(w) => {
                let mut s = serializer.serialize_struct("Trigger", 6)?;
                s.serialize_field("type", "watch")?;
                s.serialize_field("paths", &w.paths)?;
                s.serialize_field("includes", &w.includes)?;
                s.serialize_field("excludes", &w.excludes)?;
                s.serialize_field("debounce_ms", &w.debounce_ms)?;
                s.serialize_field("cooldown_ms", &w.cooldown_ms)?;
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
                Ok(Trigger::Cron { expr })
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
    Ok(WatchConfig {
        paths,
        includes: raw.includes.unwrap_or_default(),
        excludes: raw.excludes.unwrap_or_default(),
        debounce_ms,
        cooldown_ms,
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
