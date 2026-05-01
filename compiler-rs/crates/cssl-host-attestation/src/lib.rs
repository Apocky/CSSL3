//! § cssl-host-attestation — PRIME-DIRECTIVE-aligned attestation aggregator.
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The LoA-v13 host emits structured JSONL audit streams via
//!   [`cssl_host_audit`] (asset-fetch · intent · spontaneous · audio ·
//!   multiplayer · companion-relay · runtime · telemetry · render). Per
//!   Apocky's PRIME_DIRECTIVE.md every wave must produce a per-session
//!   attestation that answers :
//!
//! ```text
//!     • What did the engine do during this session ?
//!     • Was anything done WITHOUT a sovereign cap-grant ?
//!     • Was every sovereign-cap-use recorded ?
//!     • Are there any error / denied / harm events ?
//!     • Is the session safe to ship as a session-summary block ?
//! ```
//!
//!   This crate aggregates the normalized [`cssl_host_audit::AuditRow`]
//!   stream and produces :
//!
//!   - [`attestation::SessionAttestation`] — counts per directive-axis,
//!     errors, sovereign-cap uses, harm-flags, and an overall verdict.
//!   - [`report::render_text`] / [`report::render_json_pretty`] /
//!     [`report::render_csl_native`] / [`report::render_attestation_block`]
//!     — four target formats : human · machine-stable JSON ·
//!     CSLv3-glyph-native · short ATTESTATION block matching the
//!     `ATTESTATION ¬ harm` / `ATTESTATION ⚠ <flag-summary>` convention
//!     used on agent reports.
//!   - [`store::AttestationStore`] — disk-backed persistence with
//!     deterministic JSON / .csl / .txt artifacts per session.
//!
//! § DESIGN
//!   - Pure aggregator. Zero outbound side-effects in the library
//!     (apart from explicit `store::*` save calls). No FS-watch, no
//!     async, no panic.
//!   - Determinism : all `HashMap`-shaped fields exposed via serde are
//!     [`std::collections::BTreeMap`] so JSON output is stable and
//!     diffable across runs.
//!   - Classifier in [`axis`] is keyword-driven against `AuditRow.kind`
//!     so new event-kinds added downstream classify themselves into
//!     existing axes by substring match without code changes.
//!
//! § PRIME-DIRECTIVE binding
//!   - [`axis::DirectiveAxis`] enumerates the nine canonical axes from
//!     `~/source/repos/CSLv3/PRIME_DIRECTIVE.md`. The aggregator NEVER
//!     synthesizes additional axes ; if a new directive is added,
//!     extend this enum, do not work around it.
//!   - [`attestation::AttestationVerdict`] is derived purely from the
//!     numbers ; no heuristics, no policy. A `BlockingViolation` means
//!     a `HarmSeverity::Critical` flag was found and the session SHOULD
//!     NOT be shipped without human review.
//!
//! § DEPENDENCIES
//!   `serde` + `serde_json` + `cssl-host-audit` only. The cssl-host-audit
//!   dep is a direct path-dep on the sibling crate ; no version pin,
//!   no workspace-dep declaration.

#![forbid(unsafe_code)]

pub mod attestation;
pub mod axis;
pub mod report;
pub mod store;

pub use attestation::{
    aggregate, AttestationVerdict, HarmFlag, HarmSeverity, SessionAttestation,
};
pub use axis::{classify_event, DirectiveAxis};
pub use report::{
    render_attestation_block, render_csl_native, render_json_pretty, render_text,
};
pub use store::AttestationStore;
