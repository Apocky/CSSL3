//! Tracks messages whose outcome the service has not yet confirmed.
//!
//! Ported from the Android client's `RequestJournal.java`. A turn that times
//! out may still be running on the server, so the client records it and
//! refuses to send again in that conversation until history resolves it.
//! Silently retrying would duplicate a person's message.

use std::collections::BTreeMap;

use crate::protocol::{self, ProtocolError, Result};

const MAX_PENDING: usize = 100;

#[derive(Debug, Default)]
pub struct RequestJournal {
    pending: BTreeMap<String, String>,
    order: Vec<String>,
}

fn reject(message: &str) -> ProtocolError {
    ProtocolError::Rejected(message.to_string())
}

impl RequestJournal {
    pub fn begin(&mut self, session: &str, request: &str) -> Result<()> {
        let session = protocol::id(session)?;
        let request = protocol::id(request)?;
        if self.pending.contains_key(&session) {
            return Err(reject("Check the previous reply before sending again in this conversation."));
        }
        if self.pending.len() >= MAX_PENDING {
            return Err(reject("Check unresolved conversations before starting another message."));
        }
        self.order.push(session.clone());
        self.pending.insert(session, request);
        Ok(())
    }

    pub fn request_for(&self, session: &str) -> String {
        self.pending.get(session).cloned().unwrap_or_default()
    }

    pub fn resolve(&mut self, session: &str, request: &str) -> bool {
        if self.pending.get(session).map(String::as_str) != Some(request) {
            return false;
        }
        self.pending.remove(session);
        self.order.retain(|item| item != session);
        true
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.order.clear();
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::Value::Object(
            self.order
                .iter()
                .filter_map(|session| {
                    self.pending
                        .get(session)
                        .map(|request| (session.clone(), serde_json::json!(request)))
                })
                .collect(),
        )
    }

    pub fn restore(&mut self, data: &serde_json::Value) -> Result<()> {
        let rows = data
            .as_object()
            .ok_or_else(|| reject("Your saved session is damaged. Sign out and reconnect."))?;
        if rows.len() > MAX_PENDING {
            return Err(reject("Too many unresolved conversations."));
        }
        let mut pending = BTreeMap::new();
        let mut order = Vec::with_capacity(rows.len());
        for (session, request) in rows {
            let session = protocol::id(session)?;
            let request = protocol::id(
                request
                    .as_str()
                    .ok_or_else(|| reject("Your saved session is damaged. Sign out and reconnect."))?,
            )?;
            order.push(session.clone());
            pending.insert(session, request);
        }
        self.pending = pending;
        self.order = order;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_holds_one_unconfirmed_message_at_a_time() {
        let mut journal = RequestJournal::default();
        let session = protocol::new_id();
        let first = protocol::new_id();
        journal.begin(&session, &first).unwrap();
        assert_eq!(journal.request_for(&session), first);
        let second = protocol::new_id();
        assert!(journal.begin(&session, &second).is_err(), "a second send must be refused while one is unresolved");
        assert!(journal.resolve(&session, &first));
        assert!(journal.request_for(&session).is_empty());
        journal.begin(&session, &second).unwrap();
    }

    #[test]
    fn resolving_a_different_request_does_nothing() {
        let mut journal = RequestJournal::default();
        let session = protocol::new_id();
        let request = protocol::new_id();
        journal.begin(&session, &request).unwrap();
        assert!(!journal.resolve(&session, &protocol::new_id()));
        assert_eq!(journal.request_for(&session), request);
    }

    #[test]
    fn the_journal_round_trips_through_storage() {
        let mut journal = RequestJournal::default();
        let sessions: Vec<(String, String)> = (0..3)
            .map(|_| (protocol::new_id(), protocol::new_id()))
            .collect();
        for (session, request) in &sessions {
            journal.begin(session, request).unwrap();
        }
        let saved = journal.json();
        let mut restored = RequestJournal::default();
        restored.restore(&saved).unwrap();
        for (session, request) in &sessions {
            assert_eq!(&restored.request_for(session), request);
        }
        assert_eq!(restored.json(), saved);
    }

    #[test]
    fn damaged_storage_is_refused_rather_than_half_loaded() {
        let mut journal = RequestJournal::default();
        let good = protocol::new_id();
        journal.begin(&good, &protocol::new_id()).unwrap();
        let before = journal.json();
        for bad in [
            serde_json::json!({ "not-a-uuid": "00000000-0000-4000-8000-000000000000" }),
            serde_json::json!({ "00000000-0000-4000-8000-000000000000": "not-a-uuid" }),
            serde_json::json!({ "00000000-0000-4000-8000-000000000000": 7 }),
            serde_json::json!([]),
        ] {
            assert!(journal.restore(&bad).is_err());
            assert_eq!(journal.json(), before, "a refused restore must leave the journal untouched");
        }
    }

    #[test]
    fn the_pending_set_is_bounded() {
        let mut journal = RequestJournal::default();
        for _ in 0..MAX_PENDING {
            journal.begin(&protocol::new_id(), &protocol::new_id()).unwrap();
        }
        assert!(journal.begin(&protocol::new_id(), &protocol::new_id()).is_err());
    }
}
