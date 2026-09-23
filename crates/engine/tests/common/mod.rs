//! 集成测试共用：把目录里的指令 YAML 逐条解析，检查动作已注册、权限声明没漏。

#![allow(dead_code)]

use corex_engine::{Directive, Step};
use corex_registry::ActionRegistry;
use std::path::{Path, PathBuf};

/// 目录下所有指令 YAML（只看一层，按文件名排序）。
pub fn directive_files(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yaml") | Some("yml")
            )
        })
        .collect();
    found.sort();
    found
}

/// 解析一条指令；解析不了说明资产本身写坏了，直接 panic 掉整条用例。
pub fn parse(path: &Path) -> Directive {
    Directive::from_yaml_file(path).unwrap_or_else(|e| panic!("解析 {} 失败: {e}", path.display()))
}

pub fn walk_steps(
    steps: &[Step],
    reg: &ActionRegistry,
    directive: &Directive,
    missing: &mut Vec<String>,
    permission_errors: &mut Vec<String>,
) {
    for s in steps {
        match s {
            Step::Action(a) => {
                if !reg.contains(&a.action) {
                    missing.push(a.action.clone());
                }
                if !directive.permissions.is_unrestricted()
                    && let Err(e) = directive.permissions.allows_action(reg, &a.action)
                {
                    permission_errors.push(format!("{}: {}", a.action, e));
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
            Step::Steps(s) => walk_steps(&s.steps, reg, directive, missing, permission_errors),
        }
    }
}

/// 逐条校验并返回解析结果：动作都在注册表里、声明的权限能覆盖自己用的动作。
pub fn validate_dir(dir: &Path, reg: &ActionRegistry) -> Vec<Directive> {
    let mut directives = Vec::new();
    for path in directive_files(dir) {
        let directive = parse(&path);
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
            "{} 引用了未注册的动作: {missing:?}",
            path.display()
        );
        assert!(
            permission_errors.is_empty(),
            "{} 权限声明不足: {permission_errors:?}",
            path.display()
        );
        directives.push(directive);
    }
    directives
}
