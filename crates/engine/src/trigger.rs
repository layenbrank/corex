//! 把指令触发器解析成运行时配置。

use crate::definition::Trigger;
use serde::{Deserialize, Serialize, Serializer};

pub const DEBOUNCE_MS: u64 = 300;
pub const THROTTLE_MS: u64 = 1_000;

/// 防抖门的默认执行时机：安静期结束再跑。
pub const DEBOUNCE_EDGE: Edge = Edge::TRAILING;
/// 节流门的默认执行时机：立即跑 + 窗口结束补跑一次（与 lodash 默认一致，不丢改动）。
pub const THROTTLE_EDGE: Edge = Edge::BOTH;

/// 执行时机（lodash 的 `leading` / `trailing` 两个选项）。
///
/// 防抖与节流共用这套选项，**三种组合两级都接受**：两级的差别不在边沿，
/// 而在节流多一个 `maxWait`（见 `watch::gate`）。
///
/// 字段私有 ⇒ 外部只能取下面三个常量，构造不出「两个都不选」的非法组合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// YAML 取值。
    name: &'static str,
    leading: bool,
    trailing: bool,
}

impl Edge {
    /// 只在**首次**触发时立即执行。
    ///
    /// ⚠️ 会落后：窗口内（防抖：安静期内；节流：两次执行之间）到达的触发不会
    /// 产生后续执行，而它们发生在上一次执行**之后**，那次执行看不到它们——
    /// 要等下一个窗口外的触发才会重跑。
    pub const LEADING: Self = Self {
        name: "leading",
        leading: true,
        trailing: false,
    };

    /// 只在**安静期 / 窗口结束**时执行一次；会合并不丢触发。
    pub const TRAILING: Self = Self {
        name: "trailing",
        leading: false,
        trailing: true,
    };

    /// 首次立即执行，窗口内再有触发则窗口结束时**再执行一次**（lodash 默认）。
    pub const BOTH: Self = Self {
        name: "both",
        leading: true,
        trailing: true,
    };

    /// 全部取值：解析与 schema 的取值列表都从这里取（加一个取值只加一处）。
    pub const ALL: [Self; 3] = [Self::LEADING, Self::TRAILING, Self::BOTH];

    /// 解析 YAML 取值（大小写不敏感）。
    pub fn parse(raw: &str) -> Result<Self, String> {
        let want = raw.trim().to_ascii_lowercase();
        Self::ALL
            .iter()
            .find(|edge| edge.name == want.as_str())
            .copied()
            .ok_or_else(|| {
                format!(
                    "只支持 {}，收到: {want}",
                    Self::names().collect::<Vec<_>>().join(" / ")
                )
            })
    }

    /// YAML 取值。
    pub fn as_str(self) -> &'static str {
        self.name
    }

    /// 是否在首次触发时立即执行（lodash 的 `leading`）。
    pub fn is_leading(self) -> bool {
        self.leading
    }

    /// 是否在安静期 / 窗口结束时执行一次（lodash 的 `trailing`）。
    pub fn is_trailing(self) -> bool {
        self.trailing
    }

    /// 所有取值的名字。
    pub fn names() -> impl Iterator<Item = &'static str> {
        Self::ALL.iter().map(|edge| edge.name)
    }
}

impl Serialize for Edge {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Edge {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// 默认排除项（Vite 风格）：除非被覆盖，否则总是跳过版本库目录、依赖和测试产物。
pub const WATCH_EXCLUDES: &[&str] = &["**/.git/**", "**/node_modules/**", "**/test-results/**"];

/// 解析好的 watch 触发器（paths 可以是文件也可以是目录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    pub paths: Vec<String>,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    /// 去抖窗口：安静期结束才放行（默认 [`DEBOUNCE_EDGE`]）。
    pub debounce_ms: u64,
    /// 去抖门的执行时机。
    pub debounce: Edge,
    /// 流水线运行的最小间隔。
    pub throttle_ms: u64,
    /// 节流门的执行时机。
    pub throttle: Edge,
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
                        "debounce": {
                            "type": "string",
                            "enum": Edge::names().collect::<Vec<_>>(),
                            "default": DEBOUNCE_EDGE.as_str(),
                            "description": "防抖执行时机(lodash leading/trailing): trailing 安静期结束跑一次(默认); leading 仅首次触发立即跑; both 首个立即跑+安静期结束补跑"
                        },
                        "throttle_ms": { "type": "integer", "minimum": 1 },
                        "throttle": {
                            "type": "string",
                            "enum": Edge::names().collect::<Vec<_>>(),
                            "default": THROTTLE_EDGE.as_str(),
                            "description": "节流执行时机(lodash leading/trailing): both 立即跑+窗口结束补跑一次(默认, 不丢改动); leading 仅首次立即跑; trailing 仅窗口结束跑一次"
                        },
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
    debounce: Option<String>,
    #[serde(default)]
    throttle_ms: Option<u64>,
    #[serde(default)]
    throttle: Option<String>,
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
                let mut s = serializer.serialize_struct("Trigger", 11)?;
                s.serialize_field("type", "watch")?;
                s.serialize_field("paths", &w.paths)?;
                s.serialize_field("includes", &w.includes)?;
                s.serialize_field("excludes", &w.excludes)?;
                s.serialize_field("debounce_ms", &w.debounce_ms)?;
                s.serialize_field("debounce", &w.debounce)?;
                s.serialize_field("throttle_ms", &w.throttle_ms)?;
                s.serialize_field("throttle", &w.throttle)?;
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
    let debounce = parse_edge(raw.debounce.as_deref(), "watch.debounce")?.unwrap_or(DEBOUNCE_EDGE);
    let throttle = parse_edge(raw.throttle.as_deref(), "watch.throttle")?.unwrap_or(THROTTLE_EDGE);
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
        debounce,
        throttle_ms,
        throttle,
        immediate: raw.immediate.unwrap_or(false),
        poll: raw.poll.unwrap_or(false),
        events: raw.events.unwrap_or_default(),
    })
}

/// 解析 `debounce` / `throttle` 的取值；留空取默认边沿。
fn parse_edge(raw: Option<&str>, key: &str) -> Result<Option<Edge>, String> {
    raw.map(|value| Edge::parse(value).map_err(|e| format!("{key} {e}")))
        .transpose()
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
    fn watch_edges_default_to_debounce_trailing_throttle_both() {
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
        let w = find_watch_trigger(&Directive::from_yaml_str(yaml).unwrap().triggers)
            .unwrap()
            .unwrap();
        assert_eq!(w.debounce, DEBOUNCE_EDGE);
        assert_eq!(w.throttle, THROTTLE_EDGE);
        // 默认不丢触发。
        assert!(w.debounce.is_trailing() && w.throttle.is_trailing());
    }

    #[test]
    fn parse_watch_edges_case_insensitively() {
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
    debounce: LEADING
    throttle: both
"#;
        let w = find_watch_trigger(&Directive::from_yaml_str(yaml).unwrap().triggers)
            .unwrap()
            .unwrap();
        assert_eq!(w.debounce, Edge::LEADING);
        assert_eq!(w.throttle, Edge::BOTH);
    }

    #[test]
    fn reject_unknown_watch_edge() {
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
    throttle: eager
"#;
        let err = Directive::from_yaml_str(yaml).unwrap_err().to_string();
        assert!(
            err.contains("watch.throttle") && err.contains("eager"),
            "expected a watch.throttle error naming the bad value, got: {err}"
        );
    }

    /// 取值、名字与解析互为逆运算（加取值时这条测试会盯着它们同步）。
    #[test]
    fn edge_names_and_parse_round_trip() {
        for edge in Edge::ALL {
            assert_eq!(Edge::parse(edge.as_str()).unwrap(), edge);
            assert_eq!(Edge::parse(&edge.as_str().to_uppercase()).unwrap(), edge);
        }
        assert_eq!(Edge::names().count(), Edge::ALL.len());
        assert!(Edge::parse("eager").unwrap_err().contains("eager"));
    }

    /// 三种边沿两级都接受（lodash 里 `leading` / `trailing` 是两级的共同选项）。
    #[test]
    fn both_stages_accept_all_three_edges() {
        for key in ["debounce", "throttle"] {
            for raw in Edge::names() {
                let yaml = format!(
                    r#"
name: t
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: watch
    paths: ["./src"]
    {key}: {raw}
"#
                );
                let w = find_watch_trigger(&Directive::from_yaml_str(&yaml).unwrap().triggers)
                    .unwrap()
                    .unwrap();
                let actual = if key == "debounce" {
                    w.debounce
                } else {
                    w.throttle
                };
                assert_eq!(actual, Edge::parse(raw).unwrap(), "{key}: {raw}");
            }
        }
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
