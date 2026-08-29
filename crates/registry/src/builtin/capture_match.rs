//! Template image matching (no OpenCV).

use corex_core::ActionError;
use image::{GrayImage, Luma};

const MAX_HAYSTACK_PIXELS: u64 = 8_000_000;

/// Search `needle` inside `haystack` (optional region). Returns best match.
pub fn find_template(
    haystack: &GrayImage,
    needle: &GrayImage,
    region: Option<(u32, u32, u32, u32)>,
    step: u32,
    threshold: f64,
) -> Result<MatchResult, ActionError> {
    let (hw, hh) = haystack.dimensions();
    let (nw, nh) = needle.dimensions();
    if nw == 0 || nh == 0 {
        return Err(ActionError::InvalidParams("needle 尺寸无效".into()));
    }
    if (hw as u64) * (hh as u64) > MAX_HAYSTACK_PIXELS {
        return Err(ActionError::execution(format!(
            "haystack 超过 {MAX_HAYSTACK_PIXELS} 像素上限"
        )));
    }
    let (rx, ry, rw, rh) = region.unwrap_or((0, 0, hw, hh));
    let x0 = rx.min(hw.saturating_sub(1));
    let y0 = ry.min(hh.saturating_sub(1));
    let x1 = (rx.saturating_add(rw)).min(hw);
    let y1 = (ry.saturating_add(rh)).min(hh);
    if x1.saturating_sub(x0) < nw || y1.saturating_sub(y0) < nh {
        return Ok(MatchResult {
            found: false,
            score: 0.0,
            x: 0,
            y: 0,
            width: nw,
            height: nh,
        });
    }

    let step = step.max(1);
    let mut best = MatchResult {
        found: false,
        score: f64::NEG_INFINITY,
        x: 0,
        y: 0,
        width: nw,
        height: nh,
    };

    let mut y = y0;
    while y + nh <= y1 {
        let mut x = x0;
        while x + nw <= x1 {
            let score = ncc_at(haystack, needle, x, y);
            if score > best.score {
                best.score = score;
                best.x = x;
                best.y = y;
            }
            x += step;
        }
        y += step;
    }

    best.found = best.score >= threshold;
    if !best.found && best.score == f64::NEG_INFINITY {
        best.score = 0.0;
    }
    Ok(best)
}

fn ncc_at(hay: &GrayImage, needle: &GrayImage, ox: u32, oy: u32) -> f64 {
    let (nw, nh) = needle.dimensions();
    let n = (nw * nh) as f64;
    if n == 0.0 {
        return 0.0;
    }
    let mut sum_h = 0.0;
    let mut sum_n = 0.0;
    let mut sum_hh = 0.0;
    let mut sum_nn = 0.0;
    let mut sum_hn = 0.0;
    for j in 0..nh {
        for i in 0..nw {
            let hv = hay.get_pixel(ox + i, oy + j).0[0] as f64;
            let nv = needle.get_pixel(i, j).0[0] as f64;
            sum_h += hv;
            sum_n += nv;
            sum_hh += hv * hv;
            sum_nn += nv * nv;
            sum_hn += hv * nv;
        }
    }
    let mean_h = sum_h / n;
    let mean_n = sum_n / n;
    let var_h = sum_hh - n * mean_h * mean_h;
    let var_n = sum_nn - n * mean_n * mean_n;
    let cov = sum_hn - n * mean_h * mean_n;
    let denom = (var_h * var_n).sqrt();
    if denom < 1e-9 {
        return 0.0;
    }
    (cov / denom).clamp(-1.0, 1.0)
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub found: bool,
    pub score: f64,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub fn to_gray(img: image::DynamicImage) -> GrayImage {
    img.to_luma8()
}

#[allow(dead_code)]
fn _use_luma(_: Luma<u8>) {}
