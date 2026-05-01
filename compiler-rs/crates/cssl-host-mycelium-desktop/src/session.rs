//! § session — turn-history retention + snapshot.
//!
//! § Stage-0 surface
//!   In-memory ring-buffer of completed turns. Bounded by `max_turns` ;
//!   oldest evicted on overflow. Per spec § AGENT-LOOP-DURATION + §
//!   PER-ACTION-AUDIT.
//!
//! § Persistence
//!   Wave-C2 wires SQLite-backed persistence via cssl-edge mirror ; for
//!   now the buffer lives in-process. The `Session::snapshot` shape is
//!   stable so the cssl-edge port can be added without breaking the IPC
//!   surface.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::now_unix;

/// In-memory session record. Holds a bounded ring-buffer of turns.
#[derive(Debug, Clone)]
pub struct Session {
    /// Opaque session identifier — stage-0 derives from `started_unix` ;
    /// later waves wire UUIDv7.
    pub id: String,
    /// Wall-clock unix-seconds at session-start.
    pub started_unix: u64,
    /// Bounded ring-buffer of completed turns.
    pub turns: VecDeque<StoredTurn>,
    /// Maximum number of turns retained ; oldest evicted on overflow.
    pub max_turns: usize,
}

/// A completed turn record retained on the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredTurn {
    /// Monotonic turn-id assigned by the agent-loop.
    pub turn_id: u64,
    /// Verbatim user input.
    pub user_input: String,
    /// Final reply from the LLM bridge.
    pub reply: String,
    /// Symbolic names of every tool the loop dispatched (in order).
    pub tool_calls: Vec<String>,
    /// Wall-clock duration of the turn in milliseconds.
    pub elapsed_ms: u64,
    /// Wall-clock unix-seconds at turn-completion.
    pub timestamp_unix: u64,
}

/// Immutable snapshot of the session for IPC + UI rendering. Cloned out of
/// the `Mutex<Session>` so the UI never holds the lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    /// Session identifier.
    pub id: String,
    /// Wall-clock unix-seconds at session-start.
    pub started_unix: u64,
    /// Number of turns currently retained.
    pub turn_count: usize,
    /// Snapshot of the turn-buffer (oldest-first).
    pub turns: Vec<StoredTurn>,
}

impl Session {
    /// Construct a fresh session with the given retention bound. The id is
    /// deterministically derived from the start-time so tests can assert
    /// on it without uuid-flake.
    #[must_use]
    pub fn new(max_turns: usize) -> Self {
        let started_unix = now_unix();
        Self {
            id: format!("session-{started_unix}"),
            started_unix,
            turns: VecDeque::with_capacity(max_turns.max(1)),
            max_turns: max_turns.max(1),
        }
    }

    /// Record a completed turn. Evicts the oldest if at capacity.
    pub fn record(&mut self, turn: StoredTurn) {
        while self.turns.len() >= self.max_turns {
            self.turns.pop_front();
        }
        self.turns.push_back(turn);
    }

    /// Take a clone-snapshot for IPC + UI consumption.
    #[must_use]
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            id: self.id.clone(),
            started_unix: self.started_unix,
            turn_count: self.turns.len(),
            turns: self.turns.iter().cloned().collect(),
        }
    }

    /// Borrow the last `n` completed turns (oldest-first within the slice).
    /// `n = 0` returns empty ; `n` > buffer-len returns all turns.
    #[must_use]
    pub fn replay_last_n(&self, n: usize) -> Vec<&StoredTurn> {
        if n == 0 || self.turns.is_empty() {
            return Vec::new();
        }
        let start = self.turns.len().saturating_sub(n);
        self.turns.iter().skip(start).collect()
    }

    /// Number of turns currently retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.turns.len()
    }

    /// True iff zero turns have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new(100)
    }
}
