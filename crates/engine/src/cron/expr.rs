//! cron 表达式归一化。

use corex_core::EngineError;

/// 把 cron 表达式归一化成 6 段（只有 5 段时补上秒）。
pub fn parse_cron_expr(expr: &str) -> Result<String, EngineError> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err(EngineError::ParseError("cron expr 不能为空".into()));
    }
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    let normalized = match fields.len() {
        5 => format!("0 {}", fields.join(" ")),
        6 => fields.join(" "),
        n => {
            return Err(EngineError::ParseError(format!(
                "cron expr 需要 5 或 6 字段，当前 {n} 字段: {trimmed}"
            )));
        }
    };
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_fields_prepends_zero_seconds() {
        let parsed = parse_cron_expr("0 9 * * 1-5").unwrap();
        assert_eq!(parsed, "0 0 9 * * 1-5");
    }

    #[test]
    fn six_fields_unchanged() {
        let parsed = parse_cron_expr("0 0 9 * * 1-5").unwrap();
        assert_eq!(parsed, "0 0 9 * * 1-5");
    }
}
