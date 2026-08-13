#[cfg(feature = "invoke")]
pub mod parse;
pub mod schema;
pub mod service;

#[cfg(feature = "invoke")]
pub use parse::parse_args;
pub use service::run;
