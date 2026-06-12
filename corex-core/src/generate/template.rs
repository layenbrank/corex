use handlebars::{Handlebars, Helper, HelperResult, Output, RenderContext};
use rand::RngExt;

pub fn create_handlebars() -> anyhow::Result<Handlebars<'static>> {
    let mut hb = Handlebars::new();
    hb.set_strict_mode(true);

    // 注册核心 Helper
    hb.register_helper("now", Box::new(helper_now));
    hb.register_helper("uuid_v4", Box::new(helper_uuid_v4));
    hb.register_helper("random_string", Box::new(helper_random_string));

    Ok(hb)
}

// ==================== Helper 实现 ====================

fn helper_now(
    h: &Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let format = h
        .param(0)
        .and_then(|v| v.value().as_str())
        .unwrap_or("iso8601");

    let now = chrono::Local::now();
    let result = match format {
        "iso" | "iso8601" => now.to_rfc3339(),
        "unix" => now.timestamp().to_string(),
        other => now.format(other).to_string(),
    };
    out.write(&result)?;
    Ok(())
}

fn helper_uuid_v4(
    h: &Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let uppercase = h
        .param(0)
        .and_then(|v| v.value().as_bool())
        .unwrap_or(false);

    let id = uuid::Uuid::new_v4().to_string();
    let result = if uppercase { id.to_uppercase() } else { id };
    out.write(&result)?;
    Ok(())
}

fn helper_random_string(
    h: &Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let length = h.param(0).and_then(|v| v.value().as_u64()).unwrap_or(16) as usize;

    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    let result: String = (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    out.write(&result)?;
    Ok(())
}
