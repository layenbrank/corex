//! Runtime permission kinds and gate helpers shared by engine, daemon, and UI probe.

use crate::context::RuntimeConfig;
use crate::error::ActionError;

/// Permission category required by an action id.
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

/// Map action id → required permission kind.
pub fn permission_kind_for(action_id: &str) -> PermissionKind {
    match action_id {
        "shell.run" | "exec.run" | "bootstrap.env" | "bootstrap.inspect" | "bootstrap.force" => {
            PermissionKind::Shell
        }
        "http.send" => PermissionKind::Network,
        "clipboard.get" | "clipboard.set" => PermissionKind::Clipboard,
        "notify.send" => PermissionKind::Notifications,
        id if id.starts_with("ui.") => PermissionKind::Ui,
        "capture.screenshot" | "capture.monitors" | "capture.ocr" => PermissionKind::Capture,
        id if id.starts_with("keyring.") => PermissionKind::Secret,
        "codec.json.parse" => PermissionKind::None,
        id if id.starts_with("file.")
            || id.starts_with("copy.")
            || id.starts_with("scrub.")
            || id.starts_with("shade.")
            || id.starts_with("compression.")
            || id.starts_with("morph.")
            || id.starts_with("generate.path")
            || id.starts_with("codec.")
            || id == "capture.crop" =>
        {
            PermissionKind::Filesystem
        }
        _ => PermissionKind::None,
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

/// Shared gate for daemon Invoke and `corex ui` probe:
/// `plugins.disabled`, `plugins.disabled_actions`, then `strict_permissions`.
pub fn check_runtime_allowed(config: &RuntimeConfig, action_id: &str) -> Result<(), ActionError> {
    if plugin_disabled(config, action_id) {
        return Err(ActionError::execution(format!(
            "runtime_denied: 插件/动作 {action_id} 已被 plugins.disabled 禁用"
        )));
    }
    if action_disabled(config, action_id) {
        return Err(ActionError::execution(format!(
            "runtime_denied: 动作 {action_id} 已被 disabled_actions 禁用"
        )));
    }
    if config.strict_permissions && permission_kind_for(action_id) != PermissionKind::None {
        return Err(ActionError::execution(format!(
            "runtime_denied: strict_permissions 不允许执行需权限的动作 {action_id}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::PluginConfig;

    #[test]
    fn maps_common_kinds() {
        assert_eq!(permission_kind_for("shell.run"), PermissionKind::Shell);
        assert_eq!(permission_kind_for("ui.element.pick"), PermissionKind::Ui);
        assert_eq!(
            permission_kind_for("capture.monitors"),
            PermissionKind::Capture
        );
        assert_eq!(permission_kind_for("keyring.get"), PermissionKind::Secret);
        assert_eq!(
            permission_kind_for("template.render"),
            PermissionKind::None
        );
        assert_eq!(permission_kind_for("file.write"), PermissionKind::Filesystem);
    }

    #[test]
    fn runtime_denied_when_action_disabled() {
        let mut cfg = RuntimeConfig::default();
        cfg.plugins = PluginConfig {
            disabled_actions: vec!["ui.element.list".into()],
            ..Default::default()
        };
        assert!(check_runtime_allowed(&cfg, "ui.element.list").is_err());
        assert!(check_runtime_allowed(&cfg, "ui.window.list").is_ok());
    }

    #[test]
    fn runtime_denied_when_plugin_disabled() {
        let mut cfg = RuntimeConfig::default();
        cfg.plugins = PluginConfig {
            disabled: vec!["ui".into()],
            ..Default::default()
        };
        assert!(check_runtime_allowed(&cfg, "ui.window.desktop").is_err());
    }

    #[test]
    fn runtime_denied_under_strict() {
        let mut cfg = RuntimeConfig::default();
        cfg.strict_permissions = true;
        assert!(check_runtime_allowed(&cfg, "file.write").is_err());
        assert!(check_runtime_allowed(&cfg, "template.render").is_ok());
    }
}
