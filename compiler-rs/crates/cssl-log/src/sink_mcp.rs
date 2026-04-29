//! `McpSink` : cap-gated MCP IPC sink (Wave-Jθ wiring).
//!
//! § SPEC § 2.6 : `Cap<DebugMcp>` ; structured JSON over IPC.
//!
//! § STAGE-0 IMPLEMENTATION : we don't yet have the MCP IPC surface
//! (Wave-Jθ). Stage-0 buffers records into an in-memory `Vec<LogRecord>`
//! protected by a Mutex ; the MCP server (when it lands) drains via
//! [`McpSink::drain_records`].
//!
//! § INTEGRATION-POINT : when `cssl-mcp` lands, the in-memory drain is
//! replaced by a stream-write into a `unix-socket`/`stdio`/`ws-loop`
//! channel. The sink-side surface stays identical (records flow in via
//! `LogSink::write` ; they go out via the IPC protocol).

use std::sync::Mutex;

use crate::severity::Severity;
use crate::sink::{LogRecord, LogSink, SinkError};

/// Cap-token stand-in for `Cap<DebugMcp>` (spec § 2.6 + § 3.1).
///
/// § INTEGRATION-POINT : when `cssl-ifc::Cap<DebugMcp>` lands, this is
/// replaced by a re-export.
pub struct DebugMcpCap {
    sealed: u8,
}

impl DebugMcpCap {
    /// Construct a test-only cap. Replaced when canonical cap-system lands.
    #[must_use]
    pub const fn for_test() -> Self {
        Self { sealed: 0xDB }
    }

    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.sealed == 0xDB
    }
}

/// MCP IPC sink. Buffers records into an in-memory queue ; the MCP
/// server drains via [`Self::drain_records`].
pub struct McpSink {
    queue: Mutex<Vec<LogRecord>>,
    level_floor: Severity,
    /// Maximum buffered records ; oldest dropped on overflow.
    capacity: usize,
}

impl McpSink {
    /// Build an MCP sink with the given capacity. Default level-floor is
    /// `Severity::Info` per spec § 2.6.
    ///
    /// # Errors
    /// Returns [`SinkError::Mcp`] if the cap is fabricated.
    pub fn new_with_cap(
        capacity: usize,
        cap: &DebugMcpCap,
    ) -> Result<Self, SinkError> {
        if !cap.is_valid() {
            return Err(SinkError::Mcp(String::from(
                "DebugMcpCap fabrication detected",
            )));
        }
        Ok(Self {
            queue: Mutex::new(Vec::with_capacity(capacity)),
            level_floor: Severity::Info,
            capacity,
        })
    }

    #[must_use]
    pub fn with_level_floor(mut self, floor: Severity) -> Self {
        self.level_floor = floor;
        self
    }

    /// Drain all buffered records (consumer-side). Used by the MCP
    /// server (when it lands) at request-time.
    pub fn drain_records(&self) -> Vec<LogRecord> {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *q)
    }

    /// Number of currently-buffered records.
    pub fn buffered_len(&self) -> usize {
        self.queue
            .lock()
            .map(|q| q.len())
            .unwrap_or_default()
    }
}

impl LogSink for McpSink {
    fn write(&self, record: &LogRecord) -> Result<(), SinkError> {
        if record.severity < self.level_floor {
            return Ok(());
        }
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        // Capacity bound : oldest-drop-first if at capacity.
        if q.len() >= self.capacity {
            q.remove(0);
        }
        q.push(record.clone());
        Ok(())
    }

    fn name(&self) -> &'static str {
        "mcp"
    }
}

#[cfg(test)]
mod tests {
    use super::{DebugMcpCap, McpSink};
    use crate::field::FieldValue;
    use crate::path_hash_field::PathHashField;
    use crate::severity::{Severity, SourceLocation};
    use crate::sink::{LogRecord, LogSink};
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;

    fn fresh_record(severity: Severity, msg: &str) -> LogRecord {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test.rs");
        LogRecord {
            frame_n: 0,
            severity,
            subsystem: SubsystemTag::Render,
            source: SourceLocation::new(PathHashField::from_path_hash(h), 1, 1),
            message: msg.to_string(),
            fields: vec![("k", FieldValue::I64(1))],
        }
    }

    #[test]
    fn mcp_sink_constructor_with_valid_cap() {
        let cap = DebugMcpCap::for_test();
        let sink = McpSink::new_with_cap(8, &cap).unwrap();
        assert_eq!(sink.buffered_len(), 0);
    }

    #[test]
    fn mcp_sink_writes_buffer_records() {
        let cap = DebugMcpCap::for_test();
        let sink = McpSink::new_with_cap(8, &cap).unwrap();
        sink.write(&fresh_record(Severity::Info, "a")).unwrap();
        sink.write(&fresh_record(Severity::Info, "b")).unwrap();
        assert_eq!(sink.buffered_len(), 2);
    }

    #[test]
    fn mcp_sink_drain_returns_records_in_order() {
        let cap = DebugMcpCap::for_test();
        let sink = McpSink::new_with_cap(8, &cap).unwrap();
        sink.write(&fresh_record(Severity::Info, "first")).unwrap();
        sink.write(&fresh_record(Severity::Info, "second")).unwrap();
        let records = sink.drain_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].message, "first");
        assert_eq!(records[1].message, "second");
    }

    #[test]
    fn mcp_sink_drain_empties_buffer() {
        let cap = DebugMcpCap::for_test();
        let sink = McpSink::new_with_cap(8, &cap).unwrap();
        sink.write(&fresh_record(Severity::Info, "a")).unwrap();
        let _ = sink.drain_records();
        assert_eq!(sink.buffered_len(), 0);
    }

    #[test]
    fn mcp_sink_drops_oldest_on_overflow() {
        let cap = DebugMcpCap::for_test();
        let sink = McpSink::new_with_cap(2, &cap).unwrap();
        sink.write(&fresh_record(Severity::Info, "old")).unwrap();
        sink.write(&fresh_record(Severity::Info, "mid")).unwrap();
        sink.write(&fresh_record(Severity::Info, "new")).unwrap();
        let records = sink.drain_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].message, "mid");
        assert_eq!(records[1].message, "new");
    }

    #[test]
    fn mcp_sink_default_floor_drops_trace_debug() {
        let cap = DebugMcpCap::for_test();
        let sink = McpSink::new_with_cap(8, &cap).unwrap();
        sink.write(&fresh_record(Severity::Trace, "no")).unwrap();
        sink.write(&fresh_record(Severity::Debug, "no")).unwrap();
        assert_eq!(sink.buffered_len(), 0);
    }

    #[test]
    fn mcp_sink_with_level_floor_overrides() {
        let cap = DebugMcpCap::for_test();
        let sink =
            McpSink::new_with_cap(8, &cap).unwrap().with_level_floor(Severity::Warning);
        sink.write(&fresh_record(Severity::Info, "no")).unwrap();
        sink.write(&fresh_record(Severity::Warning, "yes")).unwrap();
        assert_eq!(sink.buffered_len(), 1);
    }

    #[test]
    fn mcp_sink_name_is_mcp() {
        let cap = DebugMcpCap::for_test();
        let sink = McpSink::new_with_cap(8, &cap).unwrap();
        assert_eq!(sink.name(), "mcp");
    }

    #[test]
    fn mcp_sink_concurrent_writes_serialize() {
        use std::sync::Arc;
        use std::thread;
        let cap = DebugMcpCap::for_test();
        let sink = Arc::new(McpSink::new_with_cap(10_000, &cap).unwrap());
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let s = sink.clone();
                thread::spawn(move || {
                    for _ in 0..100 {
                        s.write(&fresh_record(Severity::Info, &format!("t{i}")))
                            .unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(sink.buffered_len(), 800);
    }

    #[test]
    fn mcp_sink_records_include_full_fields() {
        let cap = DebugMcpCap::for_test();
        let sink = McpSink::new_with_cap(8, &cap).unwrap();
        sink.write(&fresh_record(Severity::Info, "msg")).unwrap();
        let r = &sink.drain_records()[0];
        assert_eq!(r.message, "msg");
        assert_eq!(r.fields.len(), 1);
    }

    #[test]
    fn debug_mcp_cap_validates() {
        let cap = DebugMcpCap::for_test();
        assert!(cap.is_valid());
    }
}
