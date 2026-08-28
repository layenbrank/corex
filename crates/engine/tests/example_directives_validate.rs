//! Validate example Directive YAML files parse and reference registered actions.

use corex_engine::Directive;
use corex_registry::ActionRegistry;
use std::path::{Path, PathBuf};

fn walk_steps(
    steps: &[corex_engine::Step],
    reg: &ActionRegistry,
    directive: &Directive,
    missing: &mut Vec<String>,
    permission_errors: &mut Vec<String>,
) {
    use corex_engine::Step;
    for s in steps {
        match s {
            Step::Action(a) => {
                if !reg.contains(&a.action) {
                    missing.push(a.action.clone());
                }
                if !directive.permissions.is_unrestricted() {
                    if let Err(e) = directive.permissions.allows_action(&a.action) {
                        permission_errors.push(format!("{}: {}", a.action, e));
                    }
                }
            }
            Step::If(i) => {
                walk_steps(&i.then, reg, directive, missing, permission_errors);
                walk_steps(&i.else_steps, reg, directive, missing, permission_errors);
            }
            Step::Repeat(r) => walk_steps(&r.steps, reg, directive, missing, permission_errors),
            Step::Parallel(p) => {
                walk_steps(&p.parallel, reg, directive, missing, permission_errors)
            }
        }
    }
}

fn validate_examples_dir(dir: &Path, reg: &ActionRegistry) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("entry").path();
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) {
            continue;
        }
        let directive = Directive::from_yaml_file(&path).expect("parse yaml");
        let mut missing = Vec::new();
        let mut permission_errors = Vec::new();
        walk_steps(
            &directive.steps,
            reg,
            &directive,
            &mut missing,
            &mut permission_errors,
        );
        assert!(
            missing.is_empty(),
            "{} missing actions: {:?}",
            path.display(),
            missing
        );
        assert!(
            permission_errors.is_empty(),
            "{} permission errors: {:?}",
            path.display(),
            permission_errors
        );
    }
}

#[test]
fn example_directives_validate() {
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    validate_examples_dir(&base.join("directives"), &reg);
    validate_examples_dir(&base.join("actions"), &reg);
}
