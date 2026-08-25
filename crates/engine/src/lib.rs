//! Shortcut pipeline engine: load YAML, resolve variables, execute steps.

pub mod control_flow;
pub mod definition;
pub mod pipeline;
pub mod resolver;
pub mod scheduler;

pub use definition::{
    Condition, InputDecl, OnError, Permissions, Shortcut, Step, Trigger,
};
pub use pipeline::Pipeline;
pub use resolver::Resolver;
