use handlebars::{Handlebars, Helper, HelperResult, Output, RenderContext};
use rand::RngExt;

/// 构建已注册所有 Helper 的模板引擎
pub fn engine() -> anyhow::Result<Handlebars<'static>> {
    let mut hb = Handlebars::new();
    hb.set_strict_mode(true);

    hb.register_helper("now", Box::new(now_helper));
    hb.register_helper("uuid", Box::new(uuid_helper));
    hb.register_helper("rand", Box::new(rand_helper));

    Ok(hb)
}

// ─── Helper 实现 ─────────────────────────────────────────────────────────────

/// `{{now "format"}}` — 输出当前时间
///
/// - `iso`     → `2026-06-11T12:00:00+08:00`
/// - `unix`    → `1781236800`（秒级时间戳）
/// - `unix_ms` → `1781236800000`（毫秒级时间戳）
/// - 其他值直接作为 chrono strftime 格式串，如 `{{now "%Y-%m-%d %H:%M:%S"}}`
fn now_helper(
    h: &Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let fmt = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("iso");

    let now = chrono::Local::now();
    let result = match fmt {
        "iso" | "iso8601" => now.to_rfc3339(),
        "unix" => now.timestamp().to_string(),
        "unix_ms" => now.timestamp_millis().to_string(),
        other => now.format(other).to_string(),
    };
    out.write(&result)?;
    Ok(())
}

/// `{{uuid true}}` — 输出 UUID v4，可选大写
fn uuid_helper(
    h: &Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let upper = h
        .param(0)
        .and_then(|v| v.value().as_bool())
        .unwrap_or(false);

    let id = uuid::Uuid::new_v4().to_string();
    let result = if upper { id.to_uppercase() } else { id };
    out.write(&result)?;
    Ok(())
}

/// `{{rand 32}}` — 生成指定长度的随机字符串（字母数字），默认 16 位
fn rand_helper(
    h: &Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let len = h.param(0).and_then(|v| v.value().as_u64()).unwrap_or(16) as usize;

    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    let result: String = (0..len)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    out.write(&result)?;
    Ok(())
}
