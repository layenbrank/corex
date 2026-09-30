//! 指令库的行为：CRUD、改名、导入、迁移与播种。
//!
//! 这里的每条断言都对应一个曾经真实发生过的问题（v12 的文件时代）：改名留下旧条目、
//! 模型里的名字与键不一致、坏条目从列表里消失、重名导入悄悄覆盖。库这一层要把它们钉死。

use corex_engine::Directive;
use corex_store::{
    DirectiveStore, ImportOptions, ImportStatus, StoreError, directives_db_path, validate_name,
};
use std::fs;
use std::path::Path;

/// 不校验任何东西的校验器：库的行为与注册表无关，测试里也不该拖进注册表。
fn accept(_directive: &Directive) -> Result<(), String> {
    Ok(())
}

fn parse(yaml: &str) -> Directive {
    Directive::from_yaml_str(yaml).expect("测试用的 YAML 应当能解析")
}

fn write(dir: &Path, name: &str, body: &str) {
    fs::write(dir.join(name), body).unwrap();
}

#[test]
fn stores_and_reads_back_a_directive() {
    let store = DirectiveStore::open_in_memory().unwrap();
    let definition = parse("name: demo\nsteps:\n  - id: a\n    action: file.read\n");

    let record = store.put("demo", &definition, None, None).unwrap();
    assert_eq!(record.name, "demo");
    assert_eq!(record.definition.steps.len(), 1);
    assert!(record.yaml.contains("action: file.read"), "{}", record.yaml);

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "demo");
    let summary = listed[0].summary.as_ref().unwrap();
    assert_eq!(summary.step_count, 1);
}

/// 库的键是唯一真相：模型里的 `name` 由写入时的键决定。
#[test]
fn the_key_wins_over_the_models_name() {
    let store = DirectiveStore::open_in_memory().unwrap();
    let definition = parse("name: inside-the-file\nsteps: []\n");

    store.put("the-key", &definition, None, None).unwrap();

    let record = store.get("the-key").unwrap();
    assert_eq!(record.definition.name, "the-key");
    assert!(store.find("inside-the-file").unwrap().is_none());
}

/// 保存只换模型：分组和来源是库的元信息，不该被编辑器顺带改掉。
#[test]
fn save_keeps_folder_and_source() {
    let store = DirectiveStore::open_in_memory().unwrap();
    let definition = parse("name: demo\nsteps: []\n");
    store
        .put("demo", &definition, Some("build"), Some("/tmp/demo.yaml"))
        .unwrap();

    store
        .save(
            "demo",
            &parse("name: demo\ndescription: 改过了\nsteps: []\n"),
        )
        .unwrap();

    let record = store.get("demo").unwrap();
    assert_eq!(record.folder.as_deref(), Some("build"));
    assert_eq!(record.source.as_deref(), Some("/tmp/demo.yaml"));
    assert_eq!(record.definition.description, "改过了");
}

/// 改名是一次 UPDATE：旧名字随之消失，不存在第二份。
#[test]
fn rename_moves_the_row_instead_of_copying_it() {
    let store = DirectiveStore::open_in_memory().unwrap();
    store.save("old", &parse("name: old\nsteps: []\n")).unwrap();

    let renamed = store.rename("old", "new").unwrap();
    assert_eq!(renamed.definition.name, "new");
    assert!(store.find("old").unwrap().is_none());
    assert_eq!(store.count().unwrap(), 1);
}

#[test]
fn rename_refuses_a_taken_name_and_a_missing_source() {
    let store = DirectiveStore::open_in_memory().unwrap();
    store.save("a", &parse("name: a\nsteps: []\n")).unwrap();
    store.save("b", &parse("name: b\nsteps: []\n")).unwrap();

    let taken = store.rename("a", "b").unwrap_err();
    assert_eq!(taken.kind(), "conflict");
    assert_eq!(store.count().unwrap(), 2);

    let missing = store.rename("nope", "c").unwrap_err();
    assert_eq!(missing.kind(), "not_found");
}

/// 改名与保存一起做（编辑器改了名再按保存）时，分组与来源要跟着走到新名字上。
#[test]
fn save_with_rename_carries_metadata() {
    let store = DirectiveStore::open_in_memory().unwrap();
    store
        .put(
            "old",
            &parse("name: old\nsteps: []\n"),
            Some("build"),
            Some("/tmp/old.yaml"),
        )
        .unwrap();

    let record = store
        .save_with_rename(
            Some("old"),
            "new",
            &parse("name: new\ndescription: 改过\nsteps: []\n"),
        )
        .unwrap();

    assert_eq!(record.folder.as_deref(), Some("build"));
    assert_eq!(record.source.as_deref(), Some("/tmp/old.yaml"));
    assert_eq!(record.definition.description, "改过");
    assert!(store.find("old").unwrap().is_none());
    assert_eq!(store.count().unwrap(), 1);
}

#[test]
fn delete_reports_missing() {
    let store = DirectiveStore::open_in_memory().unwrap();
    store.save("a", &parse("name: a\nsteps: []\n")).unwrap();

    store.delete("a").unwrap();
    assert_eq!(store.count().unwrap(), 0);
    assert_eq!(store.delete("a").unwrap_err().kind(), "not_found");
}

/// 坏条目不能从列表里消失：编辑器正是靠这份列表把它打开来修的。
#[test]
fn broken_rows_are_still_listed() {
    let dir = tempfile::tempdir().unwrap();
    let path = directives_db_path(dir.path());
    let store = DirectiveStore::open(&path).unwrap();
    store
        .save("good", &parse("name: good\nsteps: []\n"))
        .unwrap();

    // 绕过 store 写一行坏 JSON：只有手改库 / 旧版写坏才会出现这种情况。
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute(
        "INSERT INTO directives (name, folder, source, definition_json, created_at_ms, updated_at_ms) \
         VALUES ('broken', NULL, NULL, '{ not json', 1, 1)",
        [],
    )
    .unwrap();
    drop(raw);

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 2);
    let broken = listed.iter().find(|meta| meta.name == "broken").unwrap();
    assert!(broken.summary.is_none());
    // 读它时才报错，报的是「定义不合法」而不是「找不到」。
    assert_eq!(store.get("broken").unwrap_err().kind(), "parse");
}

#[test]
fn imports_files_and_folders() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "top.yaml", "name: top\nsteps: []\n");
    fs::create_dir_all(root.join("build")).unwrap();
    write(
        &root.join("build"),
        "intern.yaml",
        "name: intern\nsteps: []\n",
    );

    let store = DirectiveStore::open_in_memory().unwrap();
    let report = store
        .import_dir(root, &ImportOptions::default(), &accept)
        .unwrap();

    assert_eq!(report.created(), 2);
    assert!(report.is_clean());
    // 相对子目录成为分组。
    assert_eq!(
        store.get("intern").unwrap().folder.as_deref(),
        Some("build")
    );
    assert_eq!(store.get("top").unwrap().folder, None);
    // 来源记的是原文件，方便日后对账。
    assert!(
        store
            .get("intern")
            .unwrap()
            .source
            .unwrap()
            .ends_with("intern.yaml")
    );
}

#[test]
fn import_skips_existing_names_unless_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "demo.yaml",
        "name: demo\ndescription: 新的一份\nsteps: []\n",
    );

    let store = DirectiveStore::open_in_memory().unwrap();
    store
        .save(
            "demo",
            &parse("name: demo\ndescription: 库里那份\nsteps: []\n"),
        )
        .unwrap();

    let skipped = store
        .import_file(
            &dir.path().join("demo.yaml"),
            &ImportOptions::default(),
            &accept,
        )
        .unwrap();
    assert_eq!(skipped.status, ImportStatus::Skipped);
    assert_eq!(
        store.get("demo").unwrap().definition.description,
        "库里那份"
    );

    let overwrite = ImportOptions {
        is_overwrite: true,
        ..ImportOptions::default()
    };
    let updated = store
        .import_file(&dir.path().join("demo.yaml"), &overwrite, &accept)
        .unwrap();
    assert_eq!(updated.status, ImportStatus::Updated);
    assert_eq!(
        store.get("demo").unwrap().definition.description,
        "新的一份"
    );
}

#[test]
fn import_reports_bad_yaml_and_bad_names() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "broken.yaml", "name: [\n");
    write(dir.path(), "weird.yaml", "name: ../escape\nsteps: []\n");

    let store = DirectiveStore::open_in_memory().unwrap();
    let report = store
        .import_dir(dir.path(), &ImportOptions::default(), &accept)
        .unwrap();

    assert_eq!(report.failed(), 2);
    assert!(!report.is_clean());
    assert_eq!(store.count().unwrap(), 0);
    // 坏 YAML 的名字退化成文件名主干，报告里仍然指得出是哪个文件。
    assert!(report.entries.iter().any(|entry| entry.name == "broken"));
}

/// 校验没过就不该落库——写进去的是一条跑不起来的指令。
#[test]
fn import_refuses_what_the_validator_rejects() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "demo.yaml", "name: demo\nsteps: []\n");

    let store = DirectiveStore::open_in_memory().unwrap();
    let reject = |_: &Directive| Err("动作未注册: nope.act".to_owned());
    let report = store
        .import_dir(dir.path(), &ImportOptions::default(), &reject)
        .unwrap();

    assert_eq!(report.failed(), 1);
    assert_eq!(store.count().unwrap(), 0);
}

#[test]
fn dry_run_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "demo.yaml", "name: demo\nsteps: []\n");

    let store = DirectiveStore::open_in_memory().unwrap();
    let opts = ImportOptions {
        is_dry_run: true,
        ..ImportOptions::default()
    };
    let report = store.import_dir(dir.path(), &opts, &accept).unwrap();

    assert_eq!(report.created(), 1);
    assert_eq!(store.count().unwrap(), 0);
}

/// 旧目录只导入一次：之后的启动不再扫它。
#[test]
fn legacy_import_happens_once() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("directives");
    fs::create_dir_all(&legacy).unwrap();
    write(&legacy, "old.yaml", "name: old\nsteps: []\n");

    let store = DirectiveStore::open(&directives_db_path(dir.path())).unwrap();
    let first = store.import_legacy_dir(&legacy, &accept).unwrap().unwrap();
    assert_eq!(first.created(), 1);

    // 之后往旧目录再加文件也不会被自动捡起来（要显式 import）。
    write(&legacy, "newer.yaml", "name: newer\nsteps: []\n");
    assert!(store.import_legacy_dir(&legacy, &accept).unwrap().is_none());
    assert!(store.find("newer").unwrap().is_none());
}

#[test]
fn seeds_starters_only_into_an_empty_store() {
    let store = DirectiveStore::open_in_memory().unwrap();
    let seeded = store.seed_starters().unwrap();
    assert!(!seeded.is_empty());
    assert_eq!(store.count().unwrap(), seeded.len());

    // 再播种一次不会重复写，也不会把用户删掉的补回来。
    store.delete(&seeded[0]).unwrap();
    assert!(store.seed_starters().unwrap().is_empty());
    assert_eq!(store.count().unwrap(), seeded.len() - 1);
}

#[test]
fn open_in_data_dir_bootstraps_both_steps() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("directives");
    fs::create_dir_all(&legacy).unwrap();
    write(&legacy, "old.yaml", "name: old\nsteps: []\n");

    let (store, report) =
        DirectiveStore::open_in_data_dir(dir.path(), Default::default(), &accept).unwrap();

    assert_eq!(report.legacy.as_ref().unwrap().created(), 1);
    // 有旧指令可导时就不播种了：那不是空库。
    assert!(report.seeded.is_empty());
    assert!(store.find("old").unwrap().is_some());
    assert!(directives_db_path(dir.path()).is_file());
}

#[test]
fn name_rules_are_the_v12_ones() {
    assert!(validate_name("build-intern").is_ok());
    assert!(validate_name("中文名").is_ok());

    for bad in ["", ".", "..", "a/b", "a\\b", "../escape", "a..b"] {
        assert!(validate_name(bad).is_err(), "{bad} 不该被接受");
    }
    let error = validate_name("../escape").unwrap_err();
    assert!(matches!(error, StoreError::InvalidName(name) if name == "../escape"));
}

/// 两个进程（CLI 与 daemon）同时开着时靠 WAL + busy_timeout；这里至少确认能同时打开。
#[test]
fn two_handles_can_open_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = directives_db_path(dir.path());

    let first = DirectiveStore::open(&path).unwrap();
    let second = DirectiveStore::open(&path).unwrap();
    first
        .save("demo", &parse("name: demo\nsteps: []\n"))
        .unwrap();

    assert_eq!(second.get("demo").unwrap().definition.name, "demo");
    assert_eq!(second.list().unwrap().len(), 1);
}

/// 导出是「库里那份 → 文件」的单向快照：导出再导入要能读回同样的模型。
#[test]
fn export_round_trips_and_does_not_clobber_by_default() {
    let store = DirectiveStore::open_in_memory().unwrap();
    store
        .save(
            "demo",
            &parse("name: demo\ndescription: 带输入\ninputs:\n  - name: who\nsteps:\n  - id: a\n    action: file.read\n"),
        )
        .unwrap();

    let out = tempfile::tempdir().unwrap();
    let written = store.export_dir(out.path(), false).unwrap();
    assert_eq!(written.len(), 1);
    assert!(written[0].ends_with("demo.yaml"));

    // 第二次导出默认跳过已有文件（旧版的指令目录往往还在原地，不该被盖掉）。
    assert!(store.export_dir(out.path(), false).unwrap().is_empty());
    assert_eq!(store.export_dir(out.path(), true).unwrap().len(), 1);

    let other = DirectiveStore::open_in_memory().unwrap();
    let report = other
        .import_dir(out.path(), &ImportOptions::default(), &accept)
        .unwrap();
    assert_eq!(report.created(), 1);
    let back = other.get("demo").unwrap().definition;
    assert_eq!(back.inputs.len(), 1);
    assert_eq!(back.steps.len(), 1);
}
