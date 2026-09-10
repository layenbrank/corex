//! 面向 watch 触发器的 notify 事件类型分类。

use notify::Event;
use notify::event::{EventKind, EventKindMask};
use std::path::{Path, PathBuf};

/// 一个去抖后的文件系统事件该怎么处理。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventAction {
    Trigger,
    Remount,
    RemountRoot(PathBuf),
    Skip,
}

/// 哪些 notify 事件类型可以触发流水线运行。
#[derive(Debug, Clone)]
pub struct EventFilter {
    mask: EventKindMask,
}

impl EventFilter {
    /// `events` 为空时使用 create + modify + remove（不含 access）。
    pub fn from_events(events: &[String]) -> Self {
        if events.is_empty() {
            return Self {
                mask: EventKindMask::CORE,
            };
        }
        let mut mask = EventKindMask::empty();
        for name in events {
            match name.to_ascii_lowercase().as_str() {
                "create" => mask |= EventKindMask::CREATE,
                "modify" => mask |= EventKindMask::ALL_MODIFY,
                "remove" => mask |= EventKindMask::REMOVE,
                "access" => mask |= EventKindMask::ALL_ACCESS,
                _ => {}
            }
        }
        if mask.is_empty() {
            mask = EventKindMask::CORE;
        }
        Self { mask }
    }

    pub fn matches(&self, kind: &EventKind) -> bool {
        self.mask.matches(kind)
    }
}

/// 在路径 glob 过滤之前，先对 notify 事件分类。
pub fn classify_event(event: &Event, mount_roots: &[PathBuf], filter: &EventFilter) -> EventAction {
    if event.need_rescan() {
        return EventAction::Remount;
    }

    if event.kind.is_remove() {
        for path in &event.paths {
            if mount_roots.iter().any(|root| paths_equal(root, path)) {
                return EventAction::RemountRoot(path.clone());
            }
        }
    }

    if !filter.matches(&event.kind) {
        return EventAction::Skip;
    }

    EventAction::Trigger
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    a.components().eq(b.components())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, ModifyKind};

    #[test]
    fn default_filter_skips_access() {
        let filter = EventFilter::from_events(&[]);
        assert!(!filter.matches(&EventKind::Access(notify::event::AccessKind::Read,)));
        assert!(filter.matches(&EventKind::Create(CreateKind::Any)));
        assert!(filter.matches(&EventKind::Modify(ModifyKind::Data(DataChange::Any))));
    }

    #[test]
    fn custom_events_create_only() {
        let filter = EventFilter::from_events(&["create".into()]);
        assert!(filter.matches(&EventKind::Create(CreateKind::File)));
        assert!(!filter.matches(&EventKind::Modify(ModifyKind::Data(DataChange::Any))));
    }
}
