//! § state_machine
//! ════════════════════════════════════════════════════════════════
//! Bounded handoff state-machine. Tracks current-role, ring-buffered
//! history (FIFO eviction at capacity), sovereign-cap mask. Emits
//! JSON-line audit events for replay-friendly logging.
//!
//! Cap-bit-bleed defence : the state-machine is the policy hook-point.
//! Calls to [`HandoffStateMachine::handoff`] reject any handoff whose
//! `from` does not match `current_role` — this prevents a malicious
//! caller from minting handoff records on behalf of a role they do not
//! hold.

use serde::{Deserialize, Serialize};

use crate::handoff::{Handoff, HandoffErr};
use crate::role::Role;

/// Bounded inter-role handoff state-machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffStateMachine {
    current_role: Role,
    history: Vec<Handoff>,
    max_history: usize,
    sovereign_caps: u32,
}

impl HandoffStateMachine {
    /// Construct a fresh state-machine starting at `initial` with bounded
    /// history of size `max_history`. `max_history == 0` is treated as 1
    /// (always retain at least the most recent handoff for revert).
    #[must_use]
    pub fn new(initial: Role, max_history: usize) -> Self {
        Self {
            current_role: initial,
            history: Vec::new(),
            max_history: max_history.max(1),
            sovereign_caps: 0,
        }
    }

    /// Currently-active role.
    #[must_use]
    pub fn current(&self) -> Role {
        self.current_role
    }

    /// Read-only view of bounded history (oldest-first).
    #[must_use]
    pub fn history(&self) -> &[Handoff] {
        &self.history
    }

    /// Sovereign cap bitfield (u32, application-defined semantics).
    #[must_use]
    pub fn sovereign_caps(&self) -> u32 {
        self.sovereign_caps
    }

    /// Set sovereign cap bits (consent-gate application sets these on
    /// authenticated user-attested action).
    pub fn set_sovereign_caps(&mut self, bits: u32) {
        self.sovereign_caps = bits;
    }

    /// Record a handoff `current_role → to`. Validates the topology +
    /// caller-role-match, advances `current_role`, FIFO-evicts from
    /// history if full, and returns a borrow of the just-recorded entry.
    pub fn handoff(
        &mut self,
        to: Role,
        reason: String,
        payload: Vec<u8>,
        ts_micros: u64,
        sovereign: bool,
    ) -> Result<&Handoff, HandoffErr> {
        let candidate = Handoff::new(self.current_role, to, reason, payload, ts_micros, sovereign);
        candidate.validate()?;

        // FIFO evict if at cap (drain oldest until len < max_history).
        while self.history.len() >= self.max_history {
            self.history.remove(0);
        }
        self.history.push(candidate);
        self.current_role = to;
        // SAFETY-FREE: just-pushed item is the last index.
        Ok(self.history.last().expect("just pushed"))
    }

    /// Emit a single-line JSON audit entry for `h`. Shape :
    ///   `{"from":"DM","to":"GM","ts":123,"reason":"...","sovereign":false,"payload_len":N}`
    /// Payload bytes are NOT logged — audit captures intent + size only.
    #[must_use]
    pub fn audit_event_for(&self, h: &Handoff) -> String {
        let v = serde_json::json!({
            "from": h.from,
            "to": h.to,
            "ts": h.ts_micros,
            "reason": h.reason,
            "sovereign": h.sovereign_used,
            "payload_len": h.payload.len(),
            "current_after": self.current_role,
        });
        v.to_string()
    }

    /// Undo the most recent handoff : reverts `current_role` to the
    /// `from` of the popped entry. Returns the popped Handoff, or
    /// `None` if history is empty.
    pub fn revert_one(&mut self) -> Option<Handoff> {
        let last = self.history.pop()?;
        self.current_role = last.from;
        Some(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_with_role() {
        let sm = HandoffStateMachine::new(Role::Dm, 8);
        assert_eq!(sm.current(), Role::Dm);
        assert!(sm.history().is_empty());
        assert_eq!(sm.sovereign_caps(), 0);
    }

    #[test]
    fn handoff_advances() {
        let mut sm = HandoffStateMachine::new(Role::Dm, 8);
        let _ = sm
            .handoff(Role::Gm, "narrate".into(), vec![], 1, false)
            .expect("dm→gm");
        assert_eq!(sm.current(), Role::Gm);
        assert_eq!(sm.history().len(), 1);
        let _ = sm
            .handoff(Role::Dm, "back".into(), vec![], 2, false)
            .expect("gm→dm");
        assert_eq!(sm.current(), Role::Dm);
        assert_eq!(sm.history().len(), 2);
    }

    #[test]
    fn invalid_transition_rejected() {
        let mut sm = HandoffStateMachine::new(Role::Gm, 8);
        let r = sm.handoff(Role::Coder, "illegal".into(), vec![], 1, false);
        assert!(matches!(
            r,
            Err(HandoffErr::InvalidTransition(Role::Gm, Role::Coder))
        ));
        // current did not change
        assert_eq!(sm.current(), Role::Gm);
        assert!(sm.history().is_empty());
    }

    #[test]
    fn history_bounded_by_max() {
        let mut sm = HandoffStateMachine::new(Role::Dm, 3);
        for ts in 0..10 {
            let to = if sm.current() == Role::Dm { Role::Gm } else { Role::Dm };
            sm.handoff(to, format!("h{ts}"), vec![], ts, false).unwrap();
        }
        assert!(sm.history().len() <= 3);
        assert_eq!(sm.history().len(), 3);
    }

    #[test]
    fn history_fifo_eviction() {
        let mut sm = HandoffStateMachine::new(Role::Dm, 2);
        sm.handoff(Role::Gm, "a".into(), vec![], 1, false).unwrap();
        sm.handoff(Role::Dm, "b".into(), vec![], 2, false).unwrap();
        sm.handoff(Role::Gm, "c".into(), vec![], 3, false).unwrap();
        // oldest ("a") evicted ; remaining = [b, c]
        let hist = sm.history();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].reason, "b");
        assert_eq!(hist[1].reason, "c");
    }

    #[test]
    fn audit_line_shape() {
        let mut sm = HandoffStateMachine::new(Role::Dm, 4);
        let h = sm
            .handoff(Role::Coder, "schema".into(), vec![1, 2, 3], 42, true)
            .unwrap()
            .clone();
        let line = sm.audit_event_for(&h);
        // single-line JSON
        assert!(!line.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&line).expect("audit-json");
        assert_eq!(v["from"], "Dm");
        assert_eq!(v["to"], "Coder");
        assert_eq!(v["ts"], 42);
        assert_eq!(v["reason"], "schema");
        assert_eq!(v["sovereign"], true);
        assert_eq!(v["payload_len"], 3);
        assert_eq!(v["current_after"], "Coder");
    }

    #[test]
    fn revert_works() {
        let mut sm = HandoffStateMachine::new(Role::Dm, 8);
        sm.handoff(Role::Gm, "n".into(), vec![], 1, false).unwrap();
        sm.handoff(Role::Dm, "b".into(), vec![], 2, false).unwrap();
        assert_eq!(sm.current(), Role::Dm);
        let popped = sm.revert_one().expect("had history");
        assert_eq!(popped.reason, "b");
        assert_eq!(sm.current(), Role::Gm);
        let popped2 = sm.revert_one().expect("still had");
        assert_eq!(popped2.reason, "n");
        assert_eq!(sm.current(), Role::Dm);
    }

    #[test]
    fn revert_on_empty_returns_none() {
        let mut sm = HandoffStateMachine::new(Role::Coder, 4);
        assert!(sm.revert_one().is_none());
        assert_eq!(sm.current(), Role::Coder);
    }
}
