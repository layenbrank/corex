//! Action 参考表里不能出现已经不存在动作名。
//!
//! `docs/reference/内置Action.md` is hand-curated prose and deliberately **cannot** be
//! 那张表不能机械地全量校验：
//!
//! * 相关动作会合并到一行并写成简写（`| `ui.element.get` / `set` |`），所以
//!   被记录下来的 id 集合无法机械重建——`set` 并不带前缀；
//! * 已移除的动作会留一行墓碑，使从旧版本迁移过来的用户能找到
//!   替代品，而不是靠猜。
//!
//! 两者都携带了生成器会毁掉的信息。这也是为什么这里检查的是真正会悄悄
//! 腐坏的那个方向，而不是去重新生成该文件：表中出现了一个后来被改名或删掉的
//! 动作。

use corex_registry::ActionRegistry;

const DOC: &str = include_str!("../../../docs/reference/内置Action.md");

/// 刻意保留、用来把迁移中的用户指向替代动作的行的标记。
const TOMBSTONE: &str = "已移除";

#[test]
fn documented_actions_still_exist() {
    let mut registry = ActionRegistry::new();
    registry.register_builtins();

    let stale: Vec<&str> = DOC
        .lines()
        .filter(|line| !line.contains(TOMBSTONE))
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split('`').next())
        .filter(|id| id.contains('.') && !registry.contains(id))
        .collect();
    assert!(
        stale.is_empty(),
        "docs/reference/内置Action.md 引用了已不存在的动作：{stale:#?}"
    );
}
