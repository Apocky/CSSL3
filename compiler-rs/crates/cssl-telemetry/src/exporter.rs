//! Exporter trait + stage-0 implementations.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` § OPEN-TELEMETRY exporter + § CHROME-TRACE.

use core::fmt::Write as _;

use thiserror::Error;

use crate::ring::TelemetrySlot;
use crate::scope::{TelemetryKind, TelemetryScope};

/// Exporter failure modes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExportError {
    /// Endpoint (file / socket / URL) was unreachable.
    #[error("export endpoint `{endpoint}` unreachable")]
    EndpointUnreachable { endpoint: String },
    /// Serialization produced malformed output.
    #[error("serialization error : {0}")]
    Serialization(String),
    /// Phase-1 stub does not wire real network ; documented missing-wire.
    #[error(
        "exporter wire not implemented at stage-0 (T11-phase-2 delivers real OTLP + chrome-trace)"
    )]
    NotWired,
}

/// Trait common to every telemetry exporter.
pub trait Exporter {
    /// Human-readable exporter name.
    fn name(&self) -> &'static str;
    /// Export a batch of slots. Returns the number of slots successfully exported.
    ///
    /// # Errors
    /// Returns [`ExportError::EndpointUnreachable`] / [`ExportError::Serialization`]
    /// on failure ; [`ExportError::NotWired`] for the stage-0 stubs.
    fn export_batch(&self, slots: &[TelemetrySlot]) -> Result<usize, ExportError>;
}

/// Chrome-trace JSON-object-per-span exporter.
///
/// Stage-0 produces valid Chrome-tracing JSON (`[{"name": ..., "ph": "B", ...}, ...]`)
/// to an in-memory buffer. Phase-2 adds file-I/O + DevTools compatibility validation.
#[derive(Debug, Clone, Default)]
pub struct ChromeTraceExporter {
    /// Accumulated JSON-lines output.
    pub buffer: core::cell::RefCell<String>,
}

impl ChromeTraceExporter {
    /// New empty exporter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: core::cell::RefCell::new(String::new()),
        }
    }

    /// Drain + return the accumulated JSON.
    #[must_use]
    pub fn take_output(&self) -> String {
        self.buffer.replace(String::new())
    }
}

impl Exporter for ChromeTraceExporter {
    fn name(&self) -> &'static str {
        "chrome-trace"
    }

    fn export_batch(&self, slots: &[TelemetrySlot]) -> Result<usize, ExportError> {
        let mut buf = self.buffer.borrow_mut();
        if buf.is_empty() {
            buf.push_str("[\n");
        }
        let mut n = 0usize;
        for (i, slot) in slots.iter().enumerate() {
            if n > 0 || i > 0 {
                buf.push_str(",\n");
            }
            let ph = match slot.kind {
                x if x == TelemetryKind::SpanBegin.as_u16() => "B",
                x if x == TelemetryKind::SpanEnd.as_u16() => "E",
                x if x == TelemetryKind::Counter.as_u16() => "C",
                _ => "i",
            };
            let scope_name = scope_name_for_u16(slot.scope);
            write!(
                buf,
                r#"  {{ "name": "{scope_name}", "ph": "{ph}", "ts": {ts}, "pid": {pid}, "tid": {tid} }}"#,
                ts = slot.timestamp_ns / 1000,
                pid = slot.cpu_or_gpu_id,
                tid = slot.thread_id,
            )
            .map_err(|e| ExportError::Serialization(format!("{e}")))?;
            n += 1;
        }
        Ok(n)
    }
}

/// Generic JSON-lines exporter (each slot → one JSON-object on its own line).
#[derive(Debug, Clone, Default)]
pub struct JsonExporter {
    /// Accumulated newline-delimited JSON.
    pub buffer: core::cell::RefCell<String>,
}

impl JsonExporter {
    /// New empty exporter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drain output.
    #[must_use]
    pub fn take_output(&self) -> String {
        self.buffer.replace(String::new())
    }
}

impl Exporter for JsonExporter {
    fn name(&self) -> &'static str {
        "json-lines"
    }

    fn export_batch(&self, slots: &[TelemetrySlot]) -> Result<usize, ExportError> {
        let mut buf = self.buffer.borrow_mut();
        for slot in slots {
            writeln!(
                buf,
                r#"{{"ts_ns":{ts},"scope":"{scope}","kind":"{kind}","tid":{tid},"pid":{pid}}}"#,
                ts = slot.timestamp_ns,
                scope = scope_name_for_u16(slot.scope),
                kind = kind_name_for_u16(slot.kind),
                tid = slot.thread_id,
                pid = slot.cpu_or_gpu_id,
            )
            .map_err(|e| ExportError::Serialization(format!("{e}")))?;
        }
        Ok(slots.len())
    }
}

/// OTLP (OpenTelemetry Protocol) exporter — phase-1 stub returning `NotWired`.
#[derive(Debug, Clone)]
pub struct OtlpExporter {
    /// Endpoint URL (e.g., `"http://localhost:4317"`).
    pub endpoint: String,
}

impl OtlpExporter {
    /// New exporter with the given endpoint.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }
}

impl Exporter for OtlpExporter {
    fn name(&self) -> &'static str {
        "otlp"
    }

    fn export_batch(&self, _slots: &[TelemetrySlot]) -> Result<usize, ExportError> {
        // Phase-1 : no real network transport ; phase-2 wires `prost` / `reqwest`.
        Err(ExportError::NotWired)
    }
}

fn scope_name_for_u16(u: u16) -> &'static str {
    TelemetryScope::ALL_SCOPES
        .iter()
        .find(|s| s.as_u16() == u)
        .map_or("unknown", |s| s.as_str())
}

fn kind_name_for_u16(u: u16) -> &'static str {
    for k in [
        TelemetryKind::Sample,
        TelemetryKind::SpanBegin,
        TelemetryKind::SpanEnd,
        TelemetryKind::Counter,
        TelemetryKind::Audit,
    ] {
        if k.as_u16() == u {
            return k.as_str();
        }
    }
    "unknown"
}

#[cfg(test)]
mod tests {
    use super::{ChromeTraceExporter, ExportError, Exporter, JsonExporter, OtlpExporter};
    use crate::ring::TelemetrySlot;
    use crate::scope::{TelemetryKind, TelemetryScope};

    #[test]
    fn chrome_trace_exports_slots() {
        let ex = ChromeTraceExporter::new();
        let slots = [
            TelemetrySlot::new(1000, TelemetryScope::Spans, TelemetryKind::SpanBegin),
            TelemetrySlot::new(2000, TelemetryScope::Spans, TelemetryKind::SpanEnd),
        ];
        let n = ex.export_batch(&slots).unwrap();
        assert_eq!(n, 2);
        let out = ex.take_output();
        assert!(out.contains("\"name\": \"spans\""));
        assert!(out.contains("\"ph\": \"B\""));
        assert!(out.contains("\"ph\": \"E\""));
    }

    #[test]
    fn json_exporter_emits_lines() {
        let ex = JsonExporter::new();
        let slots = [
            TelemetrySlot::new(1000, TelemetryScope::Power, TelemetryKind::Sample),
            TelemetrySlot::new(2000, TelemetryScope::Thermal, TelemetryKind::Sample),
        ];
        let n = ex.export_batch(&slots).unwrap();
        assert_eq!(n, 2);
        let out = ex.take_output();
        assert!(out.contains("\"scope\":\"power\""));
        assert!(out.contains("\"scope\":\"thermal\""));
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn otlp_exporter_returns_not_wired() {
        let ex = OtlpExporter::new("http://localhost:4317");
        let err = ex.export_batch(&[]).unwrap_err();
        assert_eq!(err, ExportError::NotWired);
    }

    #[test]
    fn exporter_names() {
        assert_eq!(ChromeTraceExporter::new().name(), "chrome-trace");
        assert_eq!(JsonExporter::new().name(), "json-lines");
        assert_eq!(OtlpExporter::new("x").name(), "otlp");
    }

    #[test]
    fn export_empty_batch_is_ok() {
        let ex = JsonExporter::new();
        let n = ex.export_batch(&[]).unwrap();
        assert_eq!(n, 0);
    }
}
