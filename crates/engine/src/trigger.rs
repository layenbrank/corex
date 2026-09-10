//! 把指令触发器解析成运行时配置。

use crate::definition::Trigger;
use serde::{Deserialize, Serialize, Serializer};

pub const DEBOUNCE_MS: u64 = 300;
pub const THROTTLE_MS: u64 = 1_000;

/// 默认排除项（Vite 风格）：除非被覆盖，否则总是跳过版本库目录、依赖和测试产物。
pub const WATCH_EXCLUDES: &[&str] = &["**/.git/**", "**/node_modules/**", "**/test-results/**"];

/// 解析好的 watch 触发器（paths 可以是文件也可以是目录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    pub paths: Vec<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    /// 文件系统静默期去抖（`notify_debouncer_full`），不是 lodash debounce。
    pub debounce_ms: u64,
    /// 流水线运行的节流间隔（类 lodash 的 leading+trailing）。
    pub throttle_ms: u64,
    #[serde(default)]
    pub immediate: bool,
    #[serde(default)]
    pub poll: bool,
    #[serde(default)]
    pub events: Vec<String>,
}

/// 触发器的对外格式，由上面手写的 `Serialize` / `Deserialize` 定义：
/// 内部标签的 `type: cron | watch`；`expr` 对 cron 是必需的，对 watch 则会被拒绍。
/// 没有哪个 derive 能表达这一点，所以约束写在这里——紧挨着定义格式的那些
/// 实现，而不是放在生成的文件里。
#[cfg(feature = "schema")]
impl schemars::JsonSchema for Trigger {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Trigger".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "oneOf": [
                {
                    "type": "object",
                    "required": ["type", "expr"],
                    "properties": {
                        "type": { "const": "cron" },
                        "expr": { "type": "string" },
                        "timezone": {
                            "type": "string",
                            "description": "local | utc | ±HH:MM; overrides runtime.cron_timezone"
                        }
                    },
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "required": ["type", "paths"],
                    "properties": {
                        "type": { "const": "watch" },
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "minItems": 1
                        },
                        "includes": { "type": "array", "items": { "type": "string" } },
                        "excludes": { "type": "array", "items": { "type": "string" } },
                        "debounce_ms": { "type": "integer", "minimum": 0 },
                        "throttle_ms": { "type": "integer", "minimum": 1 },
                        "immediate": { "type": "boolean", "default": false },
                        "poll": { "type": "boolean", "default": false },
                        "events": {
                            "type": "array",
                            "items": {
                                "type": "string",
                                "enum": ["create", "modify", "remove", "access"]
                            }
                        }
                    },
                    "additionalProperties": false
                }
            ]
        })
    }
}

/// 解析好的 cron 触发器。
#[derive(Debug, Clone)]
pub struct CronConfig {
    pub expr: String,
    /// 可选覆盖；为空 / `None` → 使用 `RuntimeConfig.cron_timezone`。
    pub timezone: Option<String>,
}

/// 原始 YAML 触发器。
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
    throttle_ms: Option<u64>,
    /// 已移除的字段——保留只为能硬失败（不做别名）。
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
                s.serialize_field("throttle_ms", &w.throttle_ms)?;
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
    if raw.cooldown_ms.is_some() {
        return Err("watch.cooldown_ms 已移除，请改用 throttle_ms（无兼容 alias）".into());
    }
    let paths = raw.paths.unwrap_or_default();
    if paths.is_empty() {
        return Err("watch 需要 paths".into());
    }
    let debounce_ms = raw.debounce_ms.unwrap_or(DEBOUNCE_MS);
    let throttle_ms = match raw.throttle_ms {
        Some(0) => {
            return Err("watch.throttle_ms 必须大于 0".into());
        }
        Some(v) => v,
        None => debounce_ms.saturating_mul(2).max(THROTTLE_MS),
    };
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
        throttle_ms,
        immediate: raw.immediate.unwrap_or(false),
        poll: raw.poll.unwrap_or(false),
        events: raw.events.unwrap_or_default(),
    })
}

pub fn find_watch_trigger(
    triggers: &[Trigger],
) -> Result<Option<WatchConfig>, corex_core::EngineError> {
    let mut iter = triggers.iter().filter_map(|t| t.parse_watch());
    let first = iter.next();
    if iter.next().is_some() {
        return Err(corex_core::EngineError::ParseError(
            "指令仅能声明一个 watch 触发器".into(),
        ));
    }
    Ok(first)
}

pub fn find_cron_trigger(
    triggers: &[Trigger],
) -> Result<Option<CronConfig>, corex_core::EngineError> {
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
        // 默认节流：max(debounce*2, 1000)
        assert_eq!(w.throttle_ms, 1000);
    }

    #[test]
    fn default_throttle_ms_is_max_debounce_times_two_or_1000() {
        let yaml_small = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: watch
    paths: ["./src"]
    debounce_ms: 300
"#;
        let w = find_watch_trigger(&Directive::from_yaml_str(yaml_small).unwrap().triggers)
            .unwrap()
            .unwrap();
        assert_eq!(w.throttle_ms, 1000);

        let yaml_large = r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: watch
    paths: ["./src"]
    debounce_ms: 800
"#;
        let w = find_watch_trigger(&Directive::from_yaml_str(yaml_large).unwrap().triggers)
            .unwrap()
            .unwrap();
        assert_eq!(w.throttle_ms, 1600);
    }

    #[test]
    fn reject_cooldown_ms_explicitly() {
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
    cooldown_ms: 1000
"#;
        let err = Directive::from_yaml_str(yaml).unwrap_err().to_string();
        assert!(
            err.contains("cooldown_ms") && err.contains("throttle_ms"),
            "expected hard fail mentioning cooldown_ms → throttle_ms, got: {err}"
        );
    }

    #[test]
    fn reject_throttle_ms_zero() {
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
    throttle_ms: 0
"#;
        let err = Directive::from_yaml_str(yaml).unwrap_err().to_string();
        assert!(
            err.contains("throttle_ms") && err.contains("大于 0"),
            "expected throttle_ms > 0 error, got: {err}"
        );
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
            find_watch_trigger(&d.triggers)
                .unwrap()
                .unwrap()
                .paths
                .len(),
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
