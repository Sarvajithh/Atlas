//! Debouncing logic for the Folder Watcher (§21: "debounces rapid
//! changes"). Deliberately pure and I/O-free so it can be unit tested
//! without touching the filesystem or real timers -- the real-time
//! wrapper around this lives in `watcher.rs`.

use std::collections::HashMap;
use std::path::PathBuf;

/// The kind of raw filesystem change observed for a path, before
/// debouncing collapses a burst of these into one decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawChangeKind {
    Created,
    Modified,
    Removed,
}

/// A single raw event with the (logical) time it was observed. Time is
/// represented as milliseconds since some arbitrary epoch chosen by the
/// caller, so tests can drive it deterministically instead of sleeping.
#[derive(Debug, Clone)]
pub struct RawChange {
    pub path: PathBuf,
    pub kind: RawChangeKind,
    pub observed_at_ms: u64,
}

/// The debounced, final decision for a path once its debounce window has
/// elapsed (§21, §34.2: maps to `FileAdded` / `FileUpdated` / `FileDeleted`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebouncedChangeKind {
    Added,
    Updated,
    Deleted,
}

/// Coalesces rapid-fire raw events per path into a single pending change,
/// collapsing e.g. Created+Modified+Modified into one `Added`, and
/// Created+Removed within the same window into nothing at all (§21:
/// "debounces rapid changes").
#[derive(Default)]
pub struct Debouncer {
    window_ms: u64,
    pending: HashMap<PathBuf, (DebouncedChangeKind, u64)>,
}

impl Debouncer {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            pending: HashMap::new(),
        }
    }

    /// Feed one raw change into the debouncer, updating (or clearing) the
    /// pending decision for its path.
    pub fn observe(&mut self, change: RawChange) {
        let entry = self.pending.get(&change.path).map(|(kind, _)| *kind);

        let next_kind = match (entry, change.kind) {
            // A brand-new path being created: Added, regardless of what
            // (if anything) was pending before -- a fresh Created event
            // always means the path exists now.
            (_, RawChangeKind::Created) => Some(DebouncedChangeKind::Added),
            // A path we haven't seen pending yet being modified: Updated.
            (None, RawChangeKind::Modified) => Some(DebouncedChangeKind::Updated),
            // Already pending as Added and then modified again: stays
            // Added (still "new" from the watcher's perspective).
            (Some(DebouncedChangeKind::Added), RawChangeKind::Modified) => {
                Some(DebouncedChangeKind::Added)
            }
            // Already pending as Updated/Deleted and modified again:
            // Updated.
            (Some(_), RawChangeKind::Modified) => Some(DebouncedChangeKind::Updated),
            // Removed while a Created/Updated was still pending within the
            // same window: net effect is nothing happened -- drop it
            // entirely rather than emitting a spurious Delete for a file
            // whose net lifetime never left this debounce window.
            (Some(DebouncedChangeKind::Added), RawChangeKind::Removed) => None,
            // Otherwise, Removed always wins.
            (_, RawChangeKind::Removed) => Some(DebouncedChangeKind::Deleted),
        };

        match next_kind {
            Some(kind) => {
                self.pending.insert(change.path, (kind, change.observed_at_ms));
            }
            None => {
                self.pending.remove(&change.path);
            }
        }
    }

    /// Drain every pending change whose debounce window has elapsed as of
    /// `now_ms`, returning them and removing them from the pending set.
    pub fn drain_ready(&mut self, now_ms: u64) -> Vec<(PathBuf, DebouncedChangeKind)> {
        let ready: Vec<PathBuf> = self
            .pending
            .iter()
            .filter(|(_, (_, last_seen))| now_ms.saturating_sub(*last_seen) >= self.window_ms)
            .map(|(path, _)| path.clone())
            .collect();

        ready
            .into_iter()
            .map(|path| {
                let (kind, _) = self.pending.remove(&path).expect("path was just observed as ready");
                (path, kind)
            })
            .collect()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: RawChangeKind, at: u64) -> RawChange {
        RawChange {
            path: PathBuf::from(path),
            kind,
            observed_at_ms: at,
        }
    }

    #[test]
    fn single_created_event_debounces_to_added_after_window() {
        let mut debouncer = Debouncer::new(100);
        debouncer.observe(change("a.pdf", RawChangeKind::Created, 0));

        assert!(debouncer.drain_ready(50).is_empty());
        let ready = debouncer.drain_ready(100);
        assert_eq!(ready, vec![(PathBuf::from("a.pdf"), DebouncedChangeKind::Added)]);
    }

    #[test]
    fn repeated_modifies_within_window_collapse_to_one_updated() {
        let mut debouncer = Debouncer::new(100);
        debouncer.observe(change("a.pdf", RawChangeKind::Modified, 0));
        debouncer.observe(change("a.pdf", RawChangeKind::Modified, 10));
        debouncer.observe(change("a.pdf", RawChangeKind::Modified, 20));

        let ready = debouncer.drain_ready(120);
        assert_eq!(ready, vec![(PathBuf::from("a.pdf"), DebouncedChangeKind::Updated)]);
    }

    #[test]
    fn created_then_removed_within_window_cancels_out() {
        let mut debouncer = Debouncer::new(100);
        debouncer.observe(change("a.pdf", RawChangeKind::Created, 0));
        debouncer.observe(change("a.pdf", RawChangeKind::Removed, 10));

        assert_eq!(debouncer.pending_count(), 0);
        assert!(debouncer.drain_ready(200).is_empty());
    }

    #[test]
    fn removed_event_always_wins_for_an_existing_file() {
        let mut debouncer = Debouncer::new(100);
        debouncer.observe(change("a.pdf", RawChangeKind::Modified, 0));
        debouncer.observe(change("a.pdf", RawChangeKind::Removed, 10));

        let ready = debouncer.drain_ready(200);
        assert_eq!(ready, vec![(PathBuf::from("a.pdf"), DebouncedChangeKind::Deleted)]);
    }

    #[test]
    fn different_paths_are_tracked_independently() {
        let mut debouncer = Debouncer::new(50);
        debouncer.observe(change("a.pdf", RawChangeKind::Created, 0));
        debouncer.observe(change("b.pdf", RawChangeKind::Created, 0));

        let mut ready = debouncer.drain_ready(50);
        ready.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            ready,
            vec![
                (PathBuf::from("a.pdf"), DebouncedChangeKind::Added),
                (PathBuf::from("b.pdf"), DebouncedChangeKind::Added),
            ]
        );
    }

    #[test]
    fn not_yet_ready_events_stay_pending() {
        let mut debouncer = Debouncer::new(1000);
        debouncer.observe(change("a.pdf", RawChangeKind::Created, 0));
        assert_eq!(debouncer.pending_count(), 1);
        assert!(debouncer.drain_ready(500).is_empty());
        assert_eq!(debouncer.pending_count(), 1);
    }
}
