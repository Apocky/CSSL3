//! cssl-tweak — runtime parameter adjustment for CSSLv3 substrates.
//!
//! § T11-D164 (Wave-Jeta-3) : implements the L4 *tweak* surface from
//! `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 4. The crate exposes a
//! type-erased tunable registry that is :
//!
//! - **Cap-gated** — every mutating call requires a `Cap<Tweak>` stub-token, so
//!   read-only consumers cannot accidentally write through the API.
//! - **Range-checked + budgeted** — every spec carries a [`TunableRange`] and a
//!   [`BudgetMode`] (`WarnAndClamp` or `HardReject`). Out-of-range writes are
//!   either clamped + warned or rejected with [`TweakError::BudgetExceeded`].
//! - **Frame-boundary deferred** — mutating calls land in a pending-buffer that
//!   the runtime drains via [`TunableRegistry::tick_frame`]. Mid-frame reads
//!   continue to observe the pre-tick value, preserving determinism.
//! - **Replay-logged** — every applied write emits a [`TweakEvent`] tagged with
//!   the logical frame and the audit-sink records a byte-stable
//!   [`TweakAuditEntry`] so `record(N) → replay(N)` is byte-equal.
//!
//! ## Crate layout
//!
//! ```text
//! tweak::tunable      — TunableValue / TunableKind / TunableRange / TunableSpec / pending-buffer
//! tweak::registry     — TunableRegistry + Cap<Tweak> gate + audit + replay
//! tweak::defaults     — 30 default tunables from spec § 4.3 (LOAD-BEARING table)
//! ```
//!
//! ## ATTESTATION
//! Per `PRIME_DIRECTIVE.md § 11` : there was no hurt nor harm in the making of
//! this, to anyone, anything, or anybody.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_precision_loss)]

mod defaults;
mod registry;
mod tunable;

pub use defaults::{default_tunable_specs, install_defaults, DEFAULT_TUNABLE_COUNT};
pub use registry::{
    AuditSink, Cap, CapTag, ReplayLog, TunableRegistry, Tweak, TweakAuditEntry, TweakEvent,
    TweakOrigin,
};
pub use tunable::{
    BudgetMode, Stage, TunableId, TunableKind, TunableRange, TunableSpec, TunableValue, TweakError,
};
