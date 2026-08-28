//! Notify event kind classification for watch triggers.

use notify::event::{EventKind, EventKindMask};
use notify::Event;
use std::path::{Path, PathBuf};

/// What to do with a debounced filesystem event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventAction {
    Trigger,
    Remount,
    RemountRoot(PathBuf),
    Skip,
}

/// Which notify event kinds may trigger a pipeline run.
#[derive(Debug, Clone)]
pub struct EventFilter {
    mask: EventKindMask,
}

impl EventFilter {
    /// Empty `events` uses create + modify + remove (no access).
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

/// Classify a notify event before path glob filtering.
pub fn classify_event(
    event: &Event,
    mount_roots: &[PathBuf],
    filter: &EventFilter,
) -> EventAction {
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
        assert!(!filter.matches(&EventKind::Access(
            notify::event::AccessKind::Read,
        )));
        assert!(filter.matches(&EventKind::Create(CreateKind::Any)));
        assert!(filter.matches(&EventKind::Modify(ModifyKind::Data(
            DataChange::Any
        ))));
    }

    #[test]
    fn custom_events_create_only() {
        let filter = EventFilter::from_events(&["create".into()]);
        assert!(filter.matches(&EventKind::Create(CreateKind::File)));
        assert!(!filter.matches(&EventKind::Modify(ModifyKind::Data(
            DataChange::Any
        ))));
    }
}
