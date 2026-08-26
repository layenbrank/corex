//! Validate example Directive YAML files parse and reference registered actions.

use corex_engine::Directive;
use corex_registry::ActionRegistry;
use std::path::Path;

fn walk_steps(steps: &[corex_engine::Step], reg: &ActionRegistry, missing: &mut Vec<String>) {
    use corex_engine::Step;
    for s in steps {
        match s {
            Step::Action(a) => {
                if !reg.contains(&a.action) {
                    missing.push(a.action.clone());
                }
            }
            Step::If(i) => {
                walk_steps(&i.then, reg, missing);
                walk_steps(&i.else_steps, reg, missing);
            }
            Step::Repeat(r) => walk_steps(&r.steps, reg, missing),
            Step::Parallel(p) => walk_steps(&p.parallel, reg, missing),
        }
    }
}

#[test]
fn example_directives_validate() {
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/directives");
    for entry in std::fs::read_dir(dir).expect("examples/directives") {
        let path = entry.expect("entry").path();
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) {
            continue;
        }
        let directive = Directive::from_yaml_file(&path).expect("parse yaml");
        let mut missing = Vec::new();
        walk_steps(&directive.steps, &reg, &mut missing);
        assert!(
            missing.is_empty(),
            "{} missing actions: {:?}",
            path.display(),
            missing
        );
    }
}
