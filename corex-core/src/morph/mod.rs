pub mod parse;
pub mod schema;
mod pdfium;
mod search;
pub mod service;

pub use parse::parse_args;
pub use service::run;
