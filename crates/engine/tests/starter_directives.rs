//! 起步指令资产：这些 YAML 会被原样写进用户的数据目录，是**真**的指令而不是样本，
//! 所以要像 examples 一样过动作/权限检查，还要额外保证严格权限门禁过得去、
//! 文件名与播种清单一一对应。

mod common;

use corex_engine::starter;
use corex_registry::ActionRegistry;
use std::path::PathBuf;

fn assets_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/starters")
}

fn registry() -> ActionRegistry {
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    reg
}

/// 资产名必须和 `starter::seed` 的清单、以及 YAML 里的 `name` 三者一致：daemon 拿文件
/// stem 当指令名，任何一处对不上，`corex run <名字>` 就会落到另一条（或不存在的）指令上。
#[test]
fn asset_names_match_seed_list() {
    let mut from_files = Vec::new();
    for path in common::directive_files(&assets_dir()) {
        let stem = path
            .file_stem()
            .expect("starter file stem")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            common::parse(&path).name,
            stem,
            "{} 里的 name 与文件名不一致",
            path.display()
        );
        from_files.push(stem);
    }

    let mut seeded: Vec<String> = starter::names().iter().map(|n| n.to_string()).collect();
    from_files.sort();
    seeded.sort();
    assert_eq!(from_files, seeded);
}

/// 动作必须已注册、声明的权限必须够用——写进用户目录后跑不起来就是坏资产。
#[test]
fn assets_are_runnable() {
    common::validate_dir(&assets_dir(), &registry());
}

/// 企业 `strict_permissions` 会拒掉「没声明权限」的指令，起步指令得开局就过这道门
/// （纯 NONE 动作也一样：那类指令必须显式声明一类权限，见 `system-info.yaml`）。
#[test]
fn assets_are_strict_clean() {
    let reg = registry();
    for path in common::directive_files(&assets_dir()) {
        let directive = common::parse(&path);
        if let Err(e) = corex_engine::validate_permissions(&reg, &directive) {
            panic!("{} 过不了 strict 门禁: {e}", path.display());
        }
    }
}
