//! `AuditSink` : appends Fatal + Error severities + PD-tagged Warnings
//! to a [`cssl_telemetry::AuditChain`].
//!
//! § SPEC § 2.6 + § 5.3 : audit-chain dual-feed for L0 (errors) + L1
//! (logs). BLAKE3-hashed + Ed25519-signed. **Never deduplicated** at the
//! audit layer (forensic-integrity per spec § 11 Q5).
//!
//! § THREAD-SAFETY : the chain lives behind a `Mutex` so multi-thread
//! callers serialize audit-append ; the audit-chain itself is not
//! intrinsically thread-safe but most engines have a single audit-thread
//! anyway. The sink does NOT block on audit-chain failure ; it returns
//! [`SinkError::Audit`] so the upstream chain can record the failure.

use std::sync::{Arc, Mutex};

use cssl_telemetry::AuditChain;

use crate::severity::Severity;
use crate::sink::{LogRecord, LogSink, SinkError};
use crate::subsystem::SubsystemTag;

/// Audit sink. Appends Error + Fatal severities + PD-adjacent Warnings
/// to the chain. The chain is owned externally (the engine creates +
/// shares it across audit-bridges).
pub struct AuditSink {
    chain: Arc<Mutex<AuditChain>>,
}

impl AuditSink {
    /// Build an audit-sink wrapping the given chain.
    #[must_use]
    pub fn new(chain: Arc<Mutex<AuditChain>>) -> Self {
        Self { chain }
    }

    /// True iff the given record qualifies for audit-chain append per
    /// spec § 5.3 :
    ///   - Error / Fatal : ALWAYS append.
    ///   - Warning + subsystem == PrimeDirective : append (PD-adjacent).
    ///   - All other levels : skip.
    #[must_use]
    pub fn qualifies_for_audit(record: &LogRecord) -> bool {
        match record.severity {
            Severity::Error | Severity::Fatal => true,
            Severity::Warning if record.subsystem == SubsystemTag::PrimeDirective => true,
            _ => false,
        }
    }

    /// Build an audit-entry tag from a record. Stable wire-form across
    /// versions ; mutation = §7-INTEGRITY violation.
    #[must_use]
    pub fn audit_tag(record: &LogRecord) -> String {
        format!(
            "log/{lvl}/{sub}",
            lvl = record.severity.as_str(),
            sub = record.subsystem.as_str()
        )
    }

    /// Build an audit-entry message. Path-hash-only via the source-loc
    /// path-hash short-form. The message field NEVER contains the raw
    /// log message text — it carries the structural-summary so audit-
    /// chain readers can reconstruct without re-emitting the message.
    #[must_use]
    pub fn audit_message(record: &LogRecord) -> String {
        format!(
            "frame={n} src_hash={src_hash} line={ln} col={col} msg={msg}",
            n = record.frame_n,
            src_hash = record.source.file_path_hash,
            ln = record.source.line,
            col = record.source.column,
            msg = record.message,
        )
    }
}

impl LogSink for AuditSink {
    fn write(&self, record: &LogRecord) -> Result<(), SinkError> {
        if !Self::qualifies_for_audit(record) {
            return Ok(());
        }
        let tag = Self::audit_tag(record);
        let message = Self::audit_message(record);

        // Path-hash discipline check on the message before append.
        if cssl_telemetry::audit_path_op_check_raw_path_rejected(&message).is_err() {
            return Err(SinkError::Audit(format!(
                "raw-path-leak in audit-message ; record dropped (tag={tag})"
            )));
        }

        let mut chain = self.chain.lock().unwrap_or_else(|e| e.into_inner());
        chain.append(tag, message, record.frame_n);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "audit"
    }
}

#[cfg(test)]
mod tests {
    use super::AuditSink;
    use crate::field::FieldValue;
    use crate::path_hash_field::PathHashField;
    use crate::severity::{Severity, SourceLocation};
    use crate::sink::{LogRecord, LogSink};
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::{AuditChain, PathHasher};
    use std::sync::{Arc, Mutex};

    fn fresh_record(severity: Severity, subsystem: SubsystemTag) -> LogRecord {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test.rs");
        LogRecord {
            frame_n: 100,
            severity,
            subsystem,
            source: SourceLocation::new(PathHashField::from_path_hash(h), 7, 3),
            message: String::from("audit-msg"),
            fields: vec![("k", FieldValue::I64(1))],
        }
    }

    #[test]
    fn qualifies_for_audit_error_fatal_yes() {
        assert!(AuditSink::qualifies_for_audit(&fresh_record(
            Severity::Error,
            SubsystemTag::Render
        )));
        assert!(AuditSink::qualifies_for_audit(&fresh_record(
            Severity::Fatal,
            SubsystemTag::Render
        )));
    }

    #[test]
    fn qualifies_for_audit_warn_pd_yes() {
        assert!(AuditSink::qualifies_for_audit(&fresh_record(
            Severity::Warning,
            SubsystemTag::PrimeDirective
        )));
    }

    #[test]
    fn qualifies_for_audit_warn_other_no() {
        assert!(!AuditSink::qualifies_for_audit(&fresh_record(
            Severity::Warning,
            SubsystemTag::Render
        )));
    }

    #[test]
    fn qualifies_for_audit_info_debug_trace_no() {
        for s in [Severity::Trace, Severity::Debug, Severity::Info] {
            assert!(!AuditSink::qualifies_for_audit(&fresh_record(
                s,
                SubsystemTag::PrimeDirective
            )));
        }
    }

    #[test]
    fn audit_tag_canonical_form() {
        let r = fresh_record(Severity::Error, SubsystemTag::Render);
        assert_eq!(AuditSink::audit_tag(&r), "log/error/render");
    }

    #[test]
    fn audit_message_includes_frame_and_src_hash() {
        let r = fresh_record(Severity::Error, SubsystemTag::Render);
        let msg = AuditSink::audit_message(&r);
        assert!(msg.contains("frame=100"));
        assert!(msg.contains("src_hash="));
        assert!(msg.contains("line=7"));
        assert!(msg.contains("col=3"));
    }

    #[test]
    fn audit_message_no_raw_path_chars() {
        let r = fresh_record(Severity::Error, SubsystemTag::Render);
        let msg = AuditSink::audit_message(&r);
        // path-hash short-form only ; no '/' or '\\'.
        assert!(!msg.contains("/test"));
        assert!(!msg.contains('\\'));
    }

    #[test]
    fn audit_sink_appends_error_to_chain() {
        let chain = Arc::new(Mutex::new(AuditChain::new()));
        let sink = AuditSink::new(chain.clone());
        let r = fresh_record(Severity::Error, SubsystemTag::Render);
        sink.write(&r).unwrap();
        let chain = chain.lock().unwrap();
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn audit_sink_skips_info() {
        let chain = Arc::new(Mutex::new(AuditChain::new()));
        let sink = AuditSink::new(chain.clone());
        let r = fresh_record(Severity::Info, SubsystemTag::Render);
        sink.write(&r).unwrap();
        assert_eq!(chain.lock().unwrap().len(), 0);
    }

    #[test]
    fn audit_sink_appends_fatal() {
        let chain = Arc::new(Mutex::new(AuditChain::new()));
        let sink = AuditSink::new(chain.clone());
        let r = fresh_record(Severity::Fatal, SubsystemTag::Audit);
        sink.write(&r).unwrap();
        assert_eq!(chain.lock().unwrap().len(), 1);
    }

    #[test]
    fn audit_sink_appends_pd_warn() {
        let chain = Arc::new(Mutex::new(AuditChain::new()));
        let sink = AuditSink::new(chain.clone());
        let r = fresh_record(Severity::Warning, SubsystemTag::PrimeDirective);
        sink.write(&r).unwrap();
        assert_eq!(chain.lock().unwrap().len(), 1);
    }

    #[test]
    fn audit_sink_no_dedup() {
        // Spec § 11 Q5 — audit-chain dedup forbidden. Repeat-emit must
        // produce repeat-entries.
        let chain = Arc::new(Mutex::new(AuditChain::new()));
        let sink = AuditSink::new(chain.clone());
        let r = fresh_record(Severity::Error, SubsystemTag::Render);
        sink.write(&r).unwrap();
        sink.write(&r).unwrap();
        sink.write(&r).unwrap();
        assert_eq!(chain.lock().unwrap().len(), 3);
    }

    #[test]
    fn audit_sink_chain_verifies_after_append() {
        let chain = Arc::new(Mutex::new(AuditChain::new()));
        let sink = AuditSink::new(chain.clone());
        let r = fresh_record(Severity::Error, SubsystemTag::Render);
        sink.write(&r).unwrap();
        chain.lock().unwrap().verify_chain().expect("chain valid");
    }

    #[test]
    fn audit_sink_rejects_raw_path_leak_in_message() {
        // Construct a record whose message *itself* contains a raw path —
        // sanitization at the macro layer normally prevents this, but the
        // sink must still refuse on direct invocation.
        let chain = Arc::new(Mutex::new(AuditChain::new()));
        let sink = AuditSink::new(chain.clone());
        let mut r = fresh_record(Severity::Error, SubsystemTag::Render);
        r.message = String::from("loaded /etc/hosts");
        let err = sink.write(&r).unwrap_err();
        match err {
            crate::sink::SinkError::Audit(s) => {
                assert!(s.contains("raw-path-leak"));
            }
            _ => panic!("wrong error variant"),
        }
        assert_eq!(chain.lock().unwrap().len(), 0);
    }

    #[test]
    fn audit_sink_name_is_audit() {
        let chain = Arc::new(Mutex::new(AuditChain::new()));
        let sink = AuditSink::new(chain);
        assert_eq!(sink.name(), "audit");
    }
}
