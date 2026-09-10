//! Windows UI 探测集成测试（在 Windows CI / 开发机上跑）。

#[cfg(windows)]
mod windows {
    use corex_core::Value;
    use corex_registry::ui_probe::{self, TreeFormat};
    use std::collections::BTreeMap;

    /// 纯门禁检查——不需要桌面会话。
    #[tokio::test]
    async fn probe_scope_required_without_hwnd() {
        let ctx = ui_probe::probe_context(Default::default());
        let err = ui_probe::probe_element_tree(&ctx, BTreeMap::new(), TreeFormat::Flat)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("ui_scope_required"));
    }

    #[tokio::test]
    #[ignore = "requires Windows desktop session"]
    async fn probe_desktop_returns_icons_field() {
        let v = ui_probe::probe_desktop_icons()
            .await
            .expect("desktop icons");
        if let Value::Map(m) = v {
            assert!(m.contains_key("icons"));
        }
    }
}
