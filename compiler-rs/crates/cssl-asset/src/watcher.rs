//! Hot-reload watcher scaffold.
//!
//! § DESIGN
//!   A real hot-reload watcher needs OS-specific filesystem-event APIs
//!   (Win32 `ReadDirectoryChangesW`, Linux inotify, macOS FSEvents) +
//!   a debouncer + a thread to dispatch events. None of that exists at
//!   stage-0.
//!
//!   What stage-0 provides is the SURFACE :
//!     - `AssetWatcher`         — opaque handle returned by `watch_path`.
//!     - `WatchEvent`           — sum-type of events the watcher can emit.
//!     - `AssetWatcher::poll()` — drain the event queue.
//!     - `AssetWatcher::push_event()` — test / synthetic-driver hook.
//!
//!   This means downstream code can be written in terms of the watcher
//!   today and switch to the real OS-backed implementation without an
//!   API break. Every `AssetWatcher` ever returned from `watch_path` at
//!   stage-0 starts empty and only fills if a test or synthetic driver
//!   pushes events into it.
//!
//! § PRIME-DIRECTIVE
//!   The watcher does NOT spy on filesystem activity. At stage-0 it is
//!   inert ; at stage-1+ it will only fire on paths the caller explicitly
//!   handed it. No directory traversal, no implicit subscriptions, no
//!   process-wide listeners. `Drop` releases all resources silently.

use crate::error::{AssetError, Result};

/// Hot-reload event surfaced by `AssetWatcher`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// File at `path` was created.
    Created {
        /// Path that was created.
        path: String,
    },
    /// File at `path` was modified.
    Modified {
        /// Path that was modified.
        path: String,
    },
    /// File at `path` was deleted.
    Deleted {
        /// Path that was deleted.
        path: String,
    },
    /// File at `from` was renamed to `to`.
    Renamed {
        /// Original path.
        from: String,
        /// New path.
        to: String,
    },
    /// Watcher dropped events because the OS queue overflowed
    /// (real OS-backed implementation only ; never fires at stage-0).
    Overflow {
        /// Approximate count of dropped events.
        dropped: u64,
    },
}

impl WatchEvent {
    /// Path most-relevant to this event (for `Renamed` returns the new path).
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::Created { path } | Self::Modified { path } | Self::Deleted { path } => path,
            Self::Renamed { to, .. } => to,
            Self::Overflow { .. } => "",
        }
    }

    /// Is this event a "the file changed" signal worth a reload ?
    #[must_use]
    pub const fn is_reload_signal(&self) -> bool {
        matches!(
            self,
            Self::Created { .. } | Self::Modified { .. } | Self::Renamed { .. }
        )
    }
}

/// Opaque watcher handle. At stage-0 a thin in-memory queue ; the
/// real OS-backed implementation will hold platform-specific state
/// behind the same surface.
#[derive(Debug)]
pub struct AssetWatcher {
    /// Logical path the watcher was opened on (for diagnostics).
    path: String,
    /// In-memory event queue. Real implementations push from the OS
    /// thread ; stage-0 only accepts pushes from `push_event`.
    queue: Vec<WatchEvent>,
    /// Has the watcher been closed ? Polling a closed watcher returns
    /// an empty Vec.
    closed: bool,
    /// Total events delivered (debug + test diagnostic).
    events_delivered: u64,
}

impl AssetWatcher {
    /// Build a fresh stage-0 watcher rooted at `path`.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            queue: Vec::new(),
            closed: false,
            events_delivered: 0,
        }
    }

    /// Path the watcher was opened on.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Has the watcher been closed ?
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Total events delivered since creation.
    #[must_use]
    pub const fn events_delivered(&self) -> u64 {
        self.events_delivered
    }

    /// Pending events queued (not yet drained).
    #[must_use]
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Drain all currently-queued events.
    pub fn poll(&mut self) -> Vec<WatchEvent> {
        if self.closed {
            return Vec::new();
        }
        let drained = std::mem::take(&mut self.queue);
        self.events_delivered = self.events_delivered.saturating_add(drained.len() as u64);
        drained
    }

    /// Push an event into the watcher's queue. At stage-0 this is the
    /// canonical way to drive the watcher (e.g. tests, synthetic
    /// drivers, manual reload). Real OS-backed implementations will
    /// route this through their event thread.
    pub fn push_event(&mut self, event: WatchEvent) -> Result<()> {
        if self.closed {
            return Err(AssetError::watcher(
                "AssetWatcher::push_event",
                "watcher is closed",
            ));
        }
        self.queue.push(event);
        Ok(())
    }

    /// Close the watcher. After close, `push_event` errors and `poll`
    /// returns an empty Vec.
    pub fn close(&mut self) {
        self.closed = true;
        self.queue.clear();
    }
}

/// Open a stage-0 watcher on `path`. At stage-0 the watcher is a
/// passive in-memory queue ; real OS-backed implementations replace
/// this fn body without an API break.
pub fn watch_path(path: impl Into<String>) -> Result<AssetWatcher> {
    let path = path.into();
    if path.is_empty() {
        return Err(AssetError::watcher("watch_path", "path must be non-empty"));
    }
    Ok(AssetWatcher::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_path_returns_open_watcher() {
        let w = watch_path("assets/levels/").unwrap();
        assert_eq!(w.path(), "assets/levels/");
        assert!(!w.is_closed());
        assert_eq!(w.queue_len(), 0);
    }

    #[test]
    fn watch_path_rejects_empty() {
        let r = watch_path("");
        assert!(matches!(r, Err(AssetError::Watcher { .. })));
    }

    #[test]
    fn push_then_poll_drains_queue() {
        let mut w = watch_path("a/").unwrap();
        w.push_event(WatchEvent::Modified {
            path: "a/x.png".into(),
        })
        .unwrap();
        w.push_event(WatchEvent::Created {
            path: "a/y.png".into(),
        })
        .unwrap();
        let drained = w.poll();
        assert_eq!(drained.len(), 2);
        assert_eq!(w.queue_len(), 0);
        assert_eq!(w.events_delivered(), 2);
    }

    #[test]
    fn poll_on_empty_returns_empty_vec() {
        let mut w = watch_path("a/").unwrap();
        let drained = w.poll();
        assert!(drained.is_empty());
        assert_eq!(w.events_delivered(), 0);
    }

    #[test]
    fn close_drops_pending_events() {
        let mut w = watch_path("a/").unwrap();
        w.push_event(WatchEvent::Modified { path: "a/x".into() })
            .unwrap();
        w.close();
        assert!(w.is_closed());
        assert_eq!(w.queue_len(), 0);
        let drained = w.poll();
        assert!(drained.is_empty());
    }

    #[test]
    fn push_after_close_errors() {
        let mut w = watch_path("a/").unwrap();
        w.close();
        let r = w.push_event(WatchEvent::Modified { path: "a/x".into() });
        assert!(matches!(r, Err(AssetError::Watcher { .. })));
    }

    #[test]
    fn watch_event_path_accessor() {
        let e = WatchEvent::Created {
            path: "foo.png".into(),
        };
        assert_eq!(e.path(), "foo.png");
        let e = WatchEvent::Renamed {
            from: "old.png".into(),
            to: "new.png".into(),
        };
        assert_eq!(e.path(), "new.png");
        let e = WatchEvent::Overflow { dropped: 3 };
        assert_eq!(e.path(), "");
    }

    #[test]
    fn watch_event_reload_signal_classification() {
        assert!(WatchEvent::Created { path: "x".into() }.is_reload_signal());
        assert!(WatchEvent::Modified { path: "x".into() }.is_reload_signal());
        assert!(WatchEvent::Renamed {
            from: "a".into(),
            to: "b".into()
        }
        .is_reload_signal());
        assert!(!WatchEvent::Deleted { path: "x".into() }.is_reload_signal());
        assert!(!WatchEvent::Overflow { dropped: 0 }.is_reload_signal());
    }

    #[test]
    fn events_delivered_increments() {
        let mut w = watch_path("a/").unwrap();
        for i in 0..5 {
            w.push_event(WatchEvent::Created {
                path: format!("a/{i}"),
            })
            .unwrap();
        }
        let _ = w.poll();
        assert_eq!(w.events_delivered(), 5);
        for i in 5..7 {
            w.push_event(WatchEvent::Created {
                path: format!("a/{i}"),
            })
            .unwrap();
        }
        let _ = w.poll();
        assert_eq!(w.events_delivered(), 7);
    }
}
