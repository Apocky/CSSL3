// § audit : minimal local sink. Sibling cssl-host-attestation will own real
// § wiring ; we emit-locally so consumers can observe in tests.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEvent {
    TokenIssued  { token_id: [u8; 32], holder: [u8; 32], cohort_id: [u8; 32], at_secs: u64 },
    TokenExpired { token_id: [u8; 32], at_secs: u64 },
    TourStepped  { token_id: [u8; 32], frame: u64, payload_digest: [u8; 32] },
}

pub trait AuditSink {
    fn emit(&mut self, e: AuditEvent);
}

/// § VecAuditSink — in-memory test sink (BTree-free since order-of-emission
/// IS the contract here).
#[derive(Debug, Clone, Default)]
pub struct VecAuditSink {
    pub events: Vec<AuditEvent>,
}

impl VecAuditSink {
    pub fn new() -> Self { Self::default() }
    pub fn count_issued(&self)  -> usize { self.events.iter().filter(|e| matches!(e, AuditEvent::TokenIssued  { .. })).count() }
    pub fn count_expired(&self) -> usize { self.events.iter().filter(|e| matches!(e, AuditEvent::TokenExpired { .. })).count() }
    pub fn count_stepped(&self) -> usize { self.events.iter().filter(|e| matches!(e, AuditEvent::TourStepped  { .. })).count() }
}

impl AuditSink for VecAuditSink {
    fn emit(&mut self, e: AuditEvent) {
        self.events.push(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_sink_records() {
        let mut s = VecAuditSink::new();
        s.emit(AuditEvent::TokenIssued { token_id: [1; 32], holder: [2; 32], cohort_id: [3; 32], at_secs: 100 });
        s.emit(AuditEvent::TokenExpired { token_id: [1; 32], at_secs: 200 });
        assert_eq!(s.count_issued(), 1);
        assert_eq!(s.count_expired(), 1);
        assert_eq!(s.count_stepped(), 0);
    }

    #[test]
    fn audit_event_serde() {
        let e = AuditEvent::TourStepped { token_id: [9; 32], frame: 7, payload_digest: [3; 32] };
        let j = serde_json::to_string(&e).unwrap();
        let back: AuditEvent = serde_json::from_str(&j).unwrap();
        assert_eq!(e, back);
    }
}
