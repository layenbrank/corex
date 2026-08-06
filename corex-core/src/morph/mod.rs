//! Morph（PDF）模块
//!
//! # 命名约定
//!
//! | 类别 | 风格 | 示例 |
//! |------|------|------|
//! | Args 变体 / IPC action | PascalCase / kebab | `Render`↔`render`，`Match`↔`match` |
//! | 搜索结果类型 | 短名 | `Hit`（勿与 `Match` 操作混淆） |
//! | service 方法 | camelCase | `toMeta` / `toRender` / `toDocument` |
//! | schema 字段 | snake_case 短名 | `count` / `limit` / `offset` / `dir` / `base64` |

pub mod parse;
pub mod schema;
mod pdfium;
mod hit;
#[allow(non_snake_case)]
pub mod service;

pub use parse::parse_args;
pub use service::run;
