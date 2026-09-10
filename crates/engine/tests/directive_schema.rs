//! 发布的指令 schema 必须与它声称描述的类型一致。
//!
//! `schemas/directive.schema.json` 是编辑器用来校验与补全指令的依据，
//! 一份过期的副本会无声地误导每一个写指令的人。重新生成方式：
//!
//! ```text
//! COREX_BLESS_SCHEMA=1 cargo test -p corex-engine --features schema --test directive_schema
//! ```

use std::path::PathBuf;

/// 入库 schema 的位置；从 crate 根解析，使测试在任何工作目录下都能跑。
fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/directive.schema.json")
}

#[test]
fn checked_in_schema_matches_the_generated_one() {
    let generated = corex_engine::schema::directive_schema_json();
    let path = schema_path();

    if std::env::var("COREX_BLESS_SCHEMA").is_ok() {
        std::fs::write(&path, &generated).expect("写入 schema 失败");
        println!("已重新生成 {}", path.display());
        return;
    }

    // 比较的是内容，不是换行风格：`core.autocrlf=true`（Windows 默认）会把签出文件
    // 写成 CRLF，而生成器输出永远是 LF——否则这条闸门在 Windows 上永远是红的。
    let checked_in = std::fs::read_to_string(&path)
        .expect("读取 schema 失败")
        .replace("\r\n", "\n");
    assert_eq!(
        checked_in,
        generated,
        "{} 与代码不一致；用 COREX_BLESS_SCHEMA=1 重新生成（并把它记入 docs/changelog）。",
        path.display()
    );
}
