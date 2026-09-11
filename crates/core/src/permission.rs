//! 权限词表与运行时门禁，daemon Invoke 与 `corex ui` 探测共用。
//!
//! 权限要求由动作自己声明（[`Action::permissions`]）；本模块只负责词表
//! 以及消费它们的检查。
//!
//! 这里刻意**没有**动作 id → 权限的映射表。那种表是第二个事实来源，
//! 而它的 `_ => None` 兜底意味着：一个没人想起来登记的动作会在
//! `strict_permissions` 下**不受限**地跑。把要求声明在动作旁边、
//! 且 trait 不提供默认实现，就把那个静默漏洞变成了编译错误。

use crate::action::ActionStore;
use crate::context::RuntimeConfig;
use crate::error::ActionError;

/// 动作可以要求的权限类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionKind {
    None,
    Network,
    Filesystem,
    Shell,
    Clipboard,
    Notifications,
    Ui,
    Capture,
    Secret,
}

impl PermissionKind {
    /// 指令可以授予的类别，按报告中使用的顺序。
    pub const GRANTABLE: [Self; 8] = [
        Self::Network,
        Self::Filesystem,
        Self::Shell,
        Self::Clipboard,
        Self::Notifications,
        Self::Ui,
        Self::Capture,
        Self::Secret,
    ];

    /// 只含该类别一个元素的集合（`None` 得到空集）。
    pub const fn only(self) -> PermissionSet {
        match self {
            Self::None => PermissionSet::NONE,
            Self::Network => PermissionSet::NETWORK,
            Self::Filesystem => PermissionSet::FILESYSTEM,
            Self::Shell => PermissionSet::SHELL,
            Self::Clipboard => PermissionSet::CLIPBOARD,
            Self::Notifications => PermissionSet::NOTIFICATIONS,
            Self::Ui => PermissionSet::UI,
            Self::Capture => PermissionSet::CAPTURE,
            Self::Secret => PermissionSet::SECRET,
        }
    }

    /// 稳定名称，用于错误消息与文档。
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Network => "network",
            Self::Filesystem => "filesystem",
            Self::Shell => "shell",
            Self::Clipboard => "clipboard",
            Self::Notifications => "notifications",
            Self::Ui => "ui",
            Self::Capture => "capture",
            Self::Secret => "secret",
        }
    }
}

/// 动作声明的权限类别集合。
///
/// 用集合而不是单个类别，因为一个动作可能需要好几个：截图既驱动屏幕
/// **又**写文件，单靠 [`PermissionKind::Capture`] 或
/// [`PermissionKind::Filesystem`] 都表达不了。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PermissionSet(u16);

impl PermissionSet {
    /// 什么都不需要。
    pub const NONE: Self = Self(0);
    pub const NETWORK: Self = Self(0b0000_0001);
    pub const FILESYSTEM: Self = Self(0b0000_0010);
    pub const SHELL: Self = Self(0b0000_0100);
    pub const CLIPBOARD: Self = Self(0b0000_1000);
    pub const NOTIFICATIONS: Self = Self(0b0001_0000);
    pub const UI: Self = Self(0b0010_0000);
    pub const CAPTURE: Self = Self(0b0100_0000);
    pub const SECRET: Self = Self(0b1000_0000);

    /// 两个集合的并集，使声明可以写成 `A.union(B)`。
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// 该动作完全不需要权限时为 `true`。
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, kind: PermissionKind) -> bool {
        self.0 & kind.only().0 != 0
    }

    /// 声明的类别，按 [`PermissionKind::GRANTABLE`] 的顺序。
    pub fn iter(self) -> impl Iterator<Item = PermissionKind> {
        PermissionKind::GRANTABLE
            .into_iter()
            .filter(move |kind| self.contains(*kind))
    }
}

fn plugin_disabled(config: &RuntimeConfig, action_id: &str) -> bool {
    let plugin = action_id.split('.').next().unwrap_or(action_id);
    config
        .plugins
        .disabled
        .iter()
        .any(|d| d == plugin || d == action_id)
}

fn action_disabled(config: &RuntimeConfig, action_id: &str) -> bool {
    config
        .plugins
        .disabled_actions
        .iter()
        .any(|d| d == action_id)
}

/// daemon Invoke 与 `corex ui` 探测共用的门禁：
/// `plugins.disabled` → `plugins.disabled_actions` → `strict_permissions`。
///
/// 每条分支都报 [`ActionError::PermissionDenied`]，而不是通用的
/// 执行失败：这些是策略拒绝，而引擎刻意只让 `on_error: continue` / `skip`
/// 吞掉普通失败。把它们报成 `execution`，曾使得选择退出错误传播的
/// 步骤能绕过 `strict_permissions`。
pub fn check_runtime_allowed(
    config: &RuntimeConfig,
    store: &dyn ActionStore,
    action_id: &str,
) -> Result<(), ActionError> {
    if plugin_disabled(config, action_id) {
        return Err(ActionError::PermissionDenied(format!(
            "runtime_denied: 插件/动作 {action_id} 已被 plugins.disabled 禁用"
        )));
    }
    if action_disabled(config, action_id) {
        return Err(ActionError::PermissionDenied(format!(
            "runtime_denied: 动作 {action_id} 已被 disabled_actions 禁用"
        )));
    }
    if config.strict_permissions && !store.permissions_of(action_id).is_empty() {
        return Err(ActionError::PermissionDenied(format!(
            "runtime_denied: strict_permissions 不允许执行需权限的动作 {action_id}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, ActionMeta, Bucket, HashMapStore};
    use crate::context::{ExecutionContext, PluginConfig};
    use crate::value::Value;
    use std::sync::Arc;

    /// 唯一有意思的属性就是它声明了什么的动作。
    struct Fake {
        id: &'static str,
        permissions: PermissionSet,
    }

    #[async_trait::async_trait]
    impl Action for Fake {
        fn permissions(&self) -> PermissionSet {
            self.permissions
        }

        fn meta(&self) -> ActionMeta {
            ActionMeta::new(self.id, self.id, "", Bucket::System)
        }

        async fn execute(
            &self,
            _params: Value,
            _ctx: &mut ExecutionContext,
        ) -> Result<Value, ActionError> {
            Ok(Value::Null)
        }
    }

    fn store(entries: &[(&'static str, PermissionSet)]) -> HashMapStore {
        let mut store = HashMapStore::new();
        for (id, permissions) in entries {
            store.register(Arc::new(Fake {
                id,
                permissions: *permissions,
            }));
        }
        store
    }

    #[test]
    fn set_union_covers_both_categories() {
        let both = PermissionSet::CAPTURE.union(PermissionSet::FILESYSTEM);
        assert!(both.contains(PermissionKind::Capture));
        assert!(both.contains(PermissionKind::Filesystem));
        assert!(!both.contains(PermissionKind::Shell));
        assert_eq!(
            both.iter().collect::<Vec<_>>(),
            vec![PermissionKind::Filesystem, PermissionKind::Capture]
        );
        assert!(PermissionSet::NONE.is_empty());
    }

    #[test]
    fn lookups_resolve_through_the_store() {
        let store = store(&[
            ("file.write", PermissionSet::FILESYSTEM),
            ("template.render", PermissionSet::NONE),
        ]);
        assert_eq!(
            store.permissions_of("file.write"),
            PermissionSet::FILESYSTEM
        );
        assert_eq!(store.permissions_of("template.render"), PermissionSet::NONE);
        // 未知 id 什么都不声明；缺查找这件事在别处暴露。
        assert_eq!(store.permissions_of("unknown.action"), PermissionSet::NONE);
    }

    #[test]
    fn runtime_denied_when_action_disabled() {
        let store = store(&[
            ("ui.element.list", PermissionSet::UI),
            ("ui.window.list", PermissionSet::UI),
        ]);
        let cfg = RuntimeConfig {
            plugins: PluginConfig {
                disabled_actions: vec!["ui.element.list".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(check_runtime_allowed(&cfg, &store, "ui.element.list").is_err());
        assert!(check_runtime_allowed(&cfg, &store, "ui.window.list").is_ok());
    }

    #[test]
    fn runtime_denied_when_plugin_disabled() {
        let store = store(&[("ui.window.desktop", PermissionSet::UI)]);
        let cfg = RuntimeConfig {
            plugins: PluginConfig {
                disabled: vec!["ui".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(check_runtime_allowed(&cfg, &store, "ui.window.desktop").is_err());
    }

    #[test]
    fn runtime_denied_under_strict() {
        let store = store(&[
            ("file.write", PermissionSet::FILESYSTEM),
            ("template.render", PermissionSet::NONE),
        ]);
        let cfg = RuntimeConfig {
            strict_permissions: true,
            ..Default::default()
        };
        assert!(check_runtime_allowed(&cfg, &store, "file.write").is_err());
        assert!(check_runtime_allowed(&cfg, &store, "template.render").is_ok());
    }
}
