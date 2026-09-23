//! 校验 examples 里的指令 YAML 能解析，且引用了已注册的动作。

mod common;

use corex_registry::ActionRegistry;
use std::path::PathBuf;

#[test]
fn examples() {
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    common::validate_dir(&base.join("directives"), &reg);
    common::validate_dir(&base.join("actions"), &reg);
}
