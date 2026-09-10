//! UI 自动化域：window / element / input 门面 + Windows 适配层。
//!
//! 分层：动作门面 → 领域（`win` 服务）→ Win32/UIA。
//! CLI 探测调用同一批 `win::*` 入口（或 Action::execute）。

#[macro_use]
mod macros;

pub mod element;
pub mod input;
/// 与平台无关的 selector / 等待原语，门面与 Windows 适配层共用。
pub mod kernel;
pub mod window;

#[cfg(windows)]
pub(crate) mod win;

use crate::ActionRegistry;

/// 注册全部 `ui.*` 动作。
pub fn register(registry: &mut ActionRegistry) {
    window::register(registry);
    element::register(registry);
    input::register(registry);
}
