//! 权限门禁不能悄悄漂移。
//!
//! 权限要求现在长在动作自己身上（`Action::permissions`）。下表是门禁**搬过去之前**的行为——
//! 它是对已删除的 `permission_kind_for` id 表的誊写——冻结在这里，使以后任何
//! 声明上的改动都必须在这里体现为一次刻意、经过审阅的修改，而不是悄无声息地通过。
//!
//! 强制两条不变式：
//!
//! 1. 每个已注册动作仍然声明着旧表当年报出的权限；
//! 2. 快照里不缺任何已注册动作。
//!
//! 第二条才是整件事的重点：旧表以 `_ => None` 收尾，于是
//! 一个没人想起来登记的动作会在 `strict_permissions` 下**不受限**地跑。

use corex_core::{ActionStore, PermissionKind, PermissionSet};
use corex_registry::ActionRegistry;

/// 动作 id → 已移除的 id 表当年为它报的权限类别。
#[rustfmt::skip]
const LEGACY_GATE: &[(&str, PermissionKind)] = &[
    // Shell：派生进程、打开 URL，以及引导辅助。
    ("bootstrap.env", PermissionKind::Shell),
    ("bootstrap.force", PermissionKind::Shell),
    ("bootstrap.inspect", PermissionKind::Shell),
    ("exec.run", PermissionKind::Shell),
    ("process.kill", PermissionKind::Shell),
    ("process.list", PermissionKind::Shell),
    ("shell.run", PermissionKind::Shell),
    ("url.open", PermissionKind::Shell),

    // 网络。
    ("http.send", PermissionKind::Network),

    // 剪贴板 / 通知 / 密钥。
    ("clipboard.get", PermissionKind::Clipboard),
    ("clipboard.set", PermissionKind::Clipboard),
    ("notify.send", PermissionKind::Notifications),
    ("keyring.get", PermissionKind::Secret),
    ("keyring.set", PermissionKind::Secret),

    // 截图。
    ("capture.find", PermissionKind::Capture),
    ("capture.monitors", PermissionKind::Capture),
    ("capture.ocr", PermissionKind::Capture),
    ("capture.screenshot", PermissionKind::Capture),
    ("capture.crop", PermissionKind::Filesystem),

    // UI.
    ("dialog.alert", PermissionKind::Ui),
    ("dialog.confirm", PermissionKind::Ui),
    ("dialog.prompt", PermissionKind::Ui),
    ("ui.click", PermissionKind::Ui),
    ("ui.drag", PermissionKind::Ui),
    ("ui.element.click", PermissionKind::Ui),
    ("ui.element.exists", PermissionKind::Ui),
    ("ui.element.find", PermissionKind::Ui),
    ("ui.element.get", PermissionKind::Ui),
    ("ui.element.list", PermissionKind::Ui),
    ("ui.element.pick", PermissionKind::Ui),
    ("ui.element.point", PermissionKind::Ui),
    ("ui.element.set", PermissionKind::Ui),
    ("ui.element.wait", PermissionKind::Ui),
    ("ui.key", PermissionKind::Ui),
    ("ui.scroll", PermissionKind::Ui),
    ("ui.type", PermissionKind::Ui),
    ("ui.wait", PermissionKind::Ui),
    ("ui.window.desktop", PermissionKind::Ui),
    ("ui.window.find", PermissionKind::Ui),
    ("ui.window.focus", PermissionKind::Ui),
    ("ui.window.list", PermissionKind::Ui),
    ("ui.window.wait", PermissionKind::Ui),

    // 文件系统。
    ("codec.base64.decode", PermissionKind::Filesystem),
    ("codec.base64.encode", PermissionKind::Filesystem),
    ("codec.hash.md5", PermissionKind::Filesystem),
    ("compression.compress", PermissionKind::Filesystem),
    ("compression.decompress", PermissionKind::Filesystem),
    ("copy.run", PermissionKind::Filesystem),
    ("dir.read", PermissionKind::Filesystem),
    ("dir.remove", PermissionKind::Filesystem),
    ("dir.update", PermissionKind::Filesystem),
    ("dir.write", PermissionKind::Filesystem),
    ("file.copy", PermissionKind::Filesystem),
    ("file.read", PermissionKind::Filesystem),
    ("file.remove", PermissionKind::Filesystem),
    ("file.update", PermissionKind::Filesystem),
    ("file.write", PermissionKind::Filesystem),
    ("generate.chunks", PermissionKind::Filesystem),
    ("generate.hash", PermissionKind::Filesystem),
    ("generate.path", PermissionKind::Filesystem),
    ("morph.export", PermissionKind::Filesystem),
    ("morph.merge", PermissionKind::Filesystem),
    ("morph.meta", PermissionKind::Filesystem),
    ("morph.render", PermissionKind::Filesystem),
    ("morph.split", PermissionKind::Filesystem),
    ("scrub.run", PermissionKind::Filesystem),
    ("shade.convert", PermissionKind::Filesystem),

    // 什么都不需要。
    ("codec.json.parse", PermissionKind::None),
    ("codec.json.pick", PermissionKind::None),
    ("codec.url.decode", PermissionKind::None),
    ("codec.url.encode", PermissionKind::None),
    ("cron.schedule", PermissionKind::None),
    ("generate.cvid", PermissionKind::None),
    ("generate.timestamp", PermissionKind::None),
    ("generate.uuid", PermissionKind::None),
    // HTML 提取只吃字符串：不碰网络（响应由 `http.send` 拿）也不碰磁盘。
    ("html.links", PermissionKind::None),
    ("html.select", PermissionKind::None),
    ("html.text", PermissionKind::None),
    ("scan.os", PermissionKind::None),
    ("template.render", PermissionKind::None),
];

/// 刻意与旧表不同的声明，以及原因。
///
/// 用表格而不是 `match`：这是数据，一项就一行。旧表每个动作只能标
/// **一个**类别，于是三个只是读图片的动作被标成 `capture`，
/// 而真正两者都需要的两个又只能二选一。
const DIFFERENCES: &[(&str, PermissionSet)] = &[
    // 既抓屏幕*又*写文件。
    (
        "capture.screenshot",
        PermissionSet::CAPTURE.union(PermissionSet::FILESYSTEM),
    ),
    // 这三个只是读图片文件并做算术；完全不碰屏幕。旧标签既少算了文件访问，
    // 又多算了截图。
    ("capture.crop", PermissionSet::FILESYSTEM),
    ("capture.find", PermissionSet::FILESYSTEM),
    ("capture.ocr", PermissionSet::FILESYSTEM),
    // `format: image` 要从磁盘读图。这一条只是*多*了一个类别。
    (
        "clipboard.set",
        PermissionSet::CLIPBOARD.union(PermissionSet::FILESYSTEM),
    ),
];

fn builtins() -> ActionRegistry {
    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    registry
}

#[test]
fn declarations_match_the_frozen_gate() {
    let registry = builtins();
    let mut mismatches = Vec::new();
    for (id, reported) in LEGACY_GATE {
        let actual = registry.permissions_of(id);
        let expected = DIFFERENCES
            .iter()
            .find(|(action, _)| action == id)
            .map_or_else(|| reported.only(), |(_, set)| *set);
        if actual != expected {
            mismatches.push(format!("{id}: 现为 {actual:?}，应当是 {expected:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "权限声明与迁移前的门控不一致：\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn every_registered_action_is_pinned_by_the_snapshot() {
    let registry = builtins();
    let unpinned: Vec<String> = registry
        .actions()
        .into_iter()
        .map(|meta| meta.id)
        .filter(|id| !LEGACY_GATE.iter().any(|(pinned, _)| pinned == id))
        .collect();
    assert!(
        unpinned.is_empty(),
        "以下动作未在权限快照中登记，无法确认其需求：{unpinned:#?}"
    );
}
