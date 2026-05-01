//! § fidelity — graphical-fidelity report (catalog + runtime).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-FID-MAINSTREAM (W-LOA-fidelity-mainstream)
//!
//! § ROLE
//!   Catalog-buildable companion to `gpu::FidelityConfig`. The wgpu-typed
//!   `FidelityConfig` only exists under `--features runtime` (its fields
//!   are wgpu enums) ; this module provides a string-rendered mirror that
//!   any build can read — including the MCP tool registry which is
//!   catalog-mode by design (so unit-tests cover it without spinning up
//!   a GPU).
//!
//! § FLOW
//!   1. `gpu::GpuContext::new` finishes adapter probe → publishes a
//!      `FidelityConfig` to a global OnceLock + ALSO calls
//!      `fidelity::set_report(...)` with string-rendered fields.
//!   2. `fidelity::current_report()` returns the most-recently-published
//!      `FidelityReport`, or a default "not_initialized" stub.
//!   3. MCP `render.fidelity` reads `current_report()` → returns JSON.
//!
//! § PRIME-DIRECTIVE
//!   No surveillance ; the report is local-process, query-on-demand,
//!   never-pushed.

#![allow(clippy::module_name_repetitions)]

use std::sync::OnceLock;
use std::sync::RwLock;

/// String-rendered fidelity report (catalog-buildable mirror of
/// `gpu::FidelityConfig`).
#[derive(Debug, Clone)]
pub struct FidelityReport {
    /// MSAA sample count (1, 2, 4, 8).
    pub msaa_samples: u32,
    /// `Debug`-rendered HDR intermediate format ("Rgba16Float" · "Bgra8UnormSrgb" · …).
    pub hdr_format: String,
    /// `Debug`-rendered present mode ("Mailbox" · "Fifo" · "AutoVsync" · …).
    pub present_mode: String,
    /// Anisotropy clamp (1, 2, 4, 8, 16).
    pub aniso_max: u16,
    /// Whether a separate tonemap pass is wired (HDR → surface).
    pub tonemap_path: bool,
    /// Whether the report was populated by GPU init (true) or is a default
    /// "not_initialized" fallback (false).
    pub initialized: bool,
}

impl Default for FidelityReport {
    fn default() -> Self {
        // § Default = "not_initialized" stub. Catalog-mode tests + offline
        // MCP introspection get a stable shape with `initialized: false`.
        Self {
            msaa_samples: 1,
            hdr_format: "Unknown".to_string(),
            present_mode: "Unknown".to_string(),
            aniso_max: 1,
            tonemap_path: false,
            initialized: false,
        }
    }
}

impl FidelityReport {
    /// Render as a JSON object string (used by the `render.fidelity` MCP tool).
    #[must_use]
    pub fn to_json_string(&self) -> String {
        format!(
            "{{\"msaa_samples\":{},\"hdr_format\":\"{}\",\
             \"present_mode\":\"{}\",\"aniso_max\":{},\
             \"tonemap_path\":{},\"initialized\":{}}}",
            self.msaa_samples,
            json_escape(&self.hdr_format),
            json_escape(&self.present_mode),
            self.aniso_max,
            self.tonemap_path,
            self.initialized,
        )
    }
}

// § Process-global slot — published once by the runtime gpu module.
static REPORT_SLOT: OnceLock<RwLock<FidelityReport>> = OnceLock::new();

fn slot() -> &'static RwLock<FidelityReport> {
    REPORT_SLOT.get_or_init(|| RwLock::new(FidelityReport::default()))
}

/// Publish a fresh fidelity report. Called by `gpu::GpuContext::new` after
/// adapter probe completes.
pub fn set_report(report: FidelityReport) {
    if let Ok(mut w) = slot().write() {
        *w = report;
    }
}

/// Return the most-recently-published fidelity report (or default stub).
#[must_use]
pub fn current_report() -> FidelityReport {
    slot().read().map(|r| r.clone()).unwrap_or_default()
}

/// Minimal JSON-string escape — same shape as `telemetry::json_escape` but
/// reimplemented here to avoid pulling that module into catalog-mode.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────
// § Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_report_is_not_initialized() {
        let r = FidelityReport::default();
        assert!(!r.initialized);
        assert_eq!(r.msaa_samples, 1);
        assert!(!r.tonemap_path);
    }

    #[test]
    fn report_round_trips_through_json() {
        let r = FidelityReport {
            msaa_samples: 4,
            hdr_format: "Rgba16Float".to_string(),
            present_mode: "Mailbox".to_string(),
            aniso_max: 16,
            tonemap_path: true,
            initialized: true,
        };
        let s = r.to_json_string();
        assert!(s.contains("\"msaa_samples\":4"));
        assert!(s.contains("\"hdr_format\":\"Rgba16Float\""));
        assert!(s.contains("\"present_mode\":\"Mailbox\""));
        assert!(s.contains("\"aniso_max\":16"));
        assert!(s.contains("\"tonemap_path\":true"));
        assert!(s.contains("\"initialized\":true"));
    }

    #[test]
    fn set_report_then_current_report_returns_published() {
        let r = FidelityReport {
            msaa_samples: 4,
            hdr_format: "Rgba16Float".to_string(),
            present_mode: "Mailbox".to_string(),
            aniso_max: 16,
            tonemap_path: true,
            initialized: true,
        };
        set_report(r.clone());
        let got = current_report();
        assert_eq!(got.msaa_samples, 4);
        assert_eq!(got.hdr_format, "Rgba16Float");
        assert!(got.tonemap_path);
        assert!(got.initialized);
    }

    #[test]
    fn json_escape_handles_quotes_and_newlines() {
        assert_eq!(json_escape("a\"b\nc\\d"), "a\\\"b\\nc\\\\d");
    }

    #[test]
    fn report_struct_size_is_reasonable() {
        // Sanity bound : the struct is mostly String + small ints.
        // `RwLock<FidelityReport>` lives in the static, this doesn't need
        // tight packing — but let's bound it so a future expansion can't
        // accidentally balloon to a bizarre shape.
        let sz = core::mem::size_of::<FidelityReport>();
        assert!(sz < 256, "report unexpectedly large : {sz} bytes");
    }
}
