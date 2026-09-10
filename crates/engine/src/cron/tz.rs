//! 为 `Job::new_async_tz` 解析 cron 时区（仅涉及 chrono）。

use chrono::FixedOffset;
use corex_core::EngineError;

/// 注册作业时使用的时区种类。
#[derive(Debug, Clone, Copy)]
pub enum ResolvedCronTz {
    /// 按 UTC 解释表达式。
    Utc,
    /// 按宿主本地时区解释表达式。
    Local,
    /// 固定 UTC 偏移（如 `+08:00`），不涉夏令时规则。
    Fixed(FixedOffset),
}

/// 选出生效的时区字符串：触发器覆盖优先，否则用运行时默认。
pub fn effective_cron_timezone(trigger_tz: Option<&str>, runtime_tz: &str) -> String {
    trigger_tz
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| runtime_tz.trim())
        .to_string()
}

/// 把时区名解析成具体的 chrono 时区。
///
/// 接受的值：
/// - `local` / `system` —— 宿主本地时区
/// - `utc` / `z` —— UTC
/// - 固定偏移：`+08:00`、`-05:00`、`+0800`、`+8`
pub fn parse_cron_timezone(name: &str) -> Result<ResolvedCronTz, EngineError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(EngineError::ParseError(
            "cron timezone 不能为空（可用 local、utc 或 ±HH:MM）".into(),
        ));
    }
    if trimmed.eq_ignore_ascii_case("local") || trimmed.eq_ignore_ascii_case("system") {
        return Ok(ResolvedCronTz::Local);
    }
    if trimmed.eq_ignore_ascii_case("utc") || trimmed.eq_ignore_ascii_case("z") {
        return Ok(ResolvedCronTz::Utc);
    }
    if let Some(offset) = parse_fixed_offset(trimmed) {
        return Ok(ResolvedCronTz::Fixed(offset));
    }
    Err(EngineError::ParseError(format!(
        "未知 cron timezone: {trimmed}（可用 local、utc 或固定偏移如 +08:00；不支持 IANA 名）"
    )))
}

fn parse_fixed_offset(input: &str) -> Option<FixedOffset> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let sign = match bytes[0] {
        b'+' => 1_i32,
        b'-' => -1_i32,
        _ => return None,
    };
    let rest = &input[1..];
    let (hours, minutes) = parse_hh_mm(rest)?;
    if !(0..=14).contains(&hours) || !(0..60).contains(&minutes) {
        return None;
    }
    let secs = sign.checked_mul(hours * 3600 + minutes * 60)?;
    FixedOffset::east_opt(secs)
}

fn parse_hh_mm(rest: &str) -> Option<(i32, i32)> {
    if let Some((h, m)) = rest.split_once(':') {
        let hours: i32 = h.parse().ok()?;
        let minutes: i32 = m.parse().ok()?;
        return Some((hours, minutes));
    }
    match rest.len() {
        1 | 2 => {
            let hours: i32 = rest.parse().ok()?;
            Some((hours, 0))
        }
        4 => {
            let hours: i32 = rest.get(0..2)?.parse().ok()?;
            let minutes: i32 = rest.get(2..4)?.parse().ok()?;
            Some((hours, minutes))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_and_utc_aliases() {
        assert!(matches!(
            parse_cron_timezone("local").unwrap(),
            ResolvedCronTz::Local
        ));
        assert!(matches!(
            parse_cron_timezone("UTC").unwrap(),
            ResolvedCronTz::Utc
        ));
    }

    #[test]
    fn fixed_offsets() {
        let plus8 = parse_cron_timezone("+08:00").unwrap();
        assert!(matches!(plus8, ResolvedCronTz::Fixed(_)));
        let plus8b = parse_cron_timezone("+0800").unwrap();
        assert!(matches!(plus8b, ResolvedCronTz::Fixed(_)));
        let minus5 = parse_cron_timezone("-05:00").unwrap();
        assert!(matches!(minus5, ResolvedCronTz::Fixed(_)));
    }

    #[test]
    fn rejects_iana() {
        assert!(parse_cron_timezone("Asia/Shanghai").is_err());
    }

    #[test]
    fn effective_prefers_trigger() {
        assert_eq!(effective_cron_timezone(Some("+08:00"), "local"), "+08:00");
        assert_eq!(effective_cron_timezone(None, "utc"), "utc");
        assert_eq!(effective_cron_timezone(Some("  "), "local"), "local");
    }
}
