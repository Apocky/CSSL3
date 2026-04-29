//! `TunableRegistry` — the core surface.
//!
//! Per `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 4.2 + § 4.4 the
//! registry is the single place where tweaks land. Reads are unsynchronized,
//! mutating calls are gated by [`Cap`]`<`[`Tweak`]`>` and routed through a
//! validate → stage → fence → apply pipeline :
//!
//! ```text
//!   set(id, value, cap)
//!     ↳ lookup spec       (UnknownTunable on miss)
//!     ↳ kind-check        (KindMismatch on type drift)
//!     ↳ range-check       (clamp + warn — or — HardReject)
//!     ↳ pending-buffer    (frame-boundary defer)
//!     ↳ tick_frame()      ← published as current value here
//!     ↳ audit + replay    (TweakAuditEntry + TweakEvent on apply)
//! ```
//!
//! The crate keeps the audit-sink and replay-log in-memory. When the real
//! `cssl-audit` and `cssl-replay` crates land (Wave-Jeta-1) the registry will
//! grow constructor parameters that take their `AuditSink` and
//! `ReplayHandle` directly ; the in-memory analogues become a fall-back used
//! by tests + tooling.

use std::collections::HashMap;

use crate::tunable::{
    BudgetMode, Stage, TunableId, TunableRange, TunableSpec, TunableValue, TweakError,
};

// ─── Cap stub ──────────────────────────────────────────────────────────────────

/// Stable identifier for a capability tag.
///
/// Real cap-tokens live in `cssl-substrate-prime-directive::cap` ; per the
/// T11-D164 prompt they are mocked here as a string-tag enum so the registry
/// can demand `Cap<Tweak>` without a cross-crate dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapTag(pub &'static str);

/// Marker carried by a capability token. The L4 surface only needs `Tweak` ;
/// a future `DevMode` variant will be added when the real cap-machinery
/// supplants this stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tweak;

impl Tweak {
    /// Stable tag returned in [`TweakError::CapDenied`].
    pub const TAG: CapTag = CapTag("Tweak");
}

/// A type-erased capability token. Construction is unrestricted in the stub —
/// the real implementation will require a witness from the substrate's
/// authority root. The `tag` field is checked at `set` time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cap<T> {
    /// Tag of the capability.
    pub tag: CapTag,
    /// Phantom marker for the cap variant.
    _marker: core::marker::PhantomData<T>,
}

impl<T> Cap<T> {
    /// Build a stub-token for `T`. Real builds will gate this constructor on
    /// the prime-directive authority root.
    #[must_use]
    pub const fn stub(tag: CapTag) -> Self {
        Self {
            tag,
            _marker: core::marker::PhantomData,
        }
    }
}

impl Cap<Tweak> {
    /// Convenience : the canonical `Tweak` cap-token.
    #[must_use]
    pub const fn tweak() -> Self {
        Self::stub(Tweak::TAG)
    }
}

// ─── Audit + Replay stubs ──────────────────────────────────────────────────────

/// Origin of a tweak event. Mirrors `TweakOrigin` from spec § 4.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweakOrigin {
    /// Manual operator action through the inspector.
    Manual,
    /// Inbound MCP call.
    Mcp,
    /// Replayed from a recorded session.
    Replay,
    /// Dev-time watcher (file change → tweak).
    Watcher,
    /// Default re-installation by `install_defaults`.
    Default,
}

/// Audit entry recorded for every applied tweak. Byte-stable so it can be
/// fed into a BLAKE3 audit-chain when `cssl-audit` lands.
#[derive(Debug, Clone, PartialEq)]
pub struct TweakAuditEntry {
    /// Logical frame at which the tweak was applied.
    pub frame_n: u64,
    /// Monotonic audit sequence number.
    pub audit_seq: u64,
    /// Tunable that changed.
    pub tunable_id: TunableId,
    /// Canonical name (helpful for human inspection).
    pub canonical_name: &'static str,
    /// Pre-apply value, byte-stable rendering.
    pub old_value: String,
    /// Post-apply value, byte-stable rendering.
    pub new_value: String,
    /// `true` when the registry clamped the input.
    pub was_clamped: bool,
    /// Cap-tags presented by the caller.
    pub cap_chain: Vec<CapTag>,
    /// Origin of the call.
    pub origin: TweakOrigin,
}

/// Replay event emitted on every applied tweak.
#[derive(Debug, Clone, PartialEq)]
pub struct TweakEvent {
    /// Logical frame at which the event was applied.
    pub frame_n: u64,
    /// Tunable that changed.
    pub tunable_id: TunableId,
    /// Canonical name.
    pub canonical_name: &'static str,
    /// Post-apply value (replay-determinism uses this on playback).
    pub new_value: TunableValue,
    /// Origin of the call.
    pub origin: TweakOrigin,
}

/// In-memory audit sink. Intentionally an `enum` so a real implementation can
/// add a `Bridged` variant that wires through `cssl-audit`.
#[derive(Debug, Default)]
pub struct AuditSink {
    entries: Vec<TweakAuditEntry>,
    next_seq: u64,
}

impl AuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a new entry, assigning a monotonic audit sequence.
    pub fn record(&mut self, mut entry: TweakAuditEntry) {
        entry.audit_seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(entry);
    }

    /// Read-only view of recorded entries.
    #[must_use]
    pub fn entries(&self) -> &[TweakAuditEntry] {
        &self.entries
    }

    /// Number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the sink is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// In-memory replay log. Per spec § 4.4 step 5 the registry must record one
/// event per applied tweak.
#[derive(Debug, Default)]
pub struct ReplayLog {
    events: Vec<TweakEvent>,
}

impl ReplayLog {
    /// Construct an empty log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event.
    pub fn push(&mut self, event: TweakEvent) {
        self.events.push(event);
    }

    /// Read-only view of recorded events.
    #[must_use]
    pub fn events(&self) -> &[TweakEvent] {
        &self.events
    }

    /// Number of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

// ─── Internals ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct PendingWrite {
    new_value: TunableValue,
    was_clamped: bool,
    origin: TweakOrigin,
    cap_chain: Vec<CapTag>,
}

#[derive(Debug)]
struct Entry {
    spec: TunableSpec,
    current: TunableValue,
    pending: Option<PendingWrite>,
    last_applied_stage: Stage,
}

// ─── TunableRegistry ───────────────────────────────────────────────────────────

/// Type-erased tunable registry.
///
/// The registry tracks *current* and *pending* values per tunable. Pending
/// writes are merged into the current value at [`TunableRegistry::tick_frame`],
/// emulating the spec's "frame-fence" discipline. Reads always observe the
/// current value, so mid-frame consumers see a consistent snapshot.
#[derive(Debug)]
pub struct TunableRegistry {
    entries: HashMap<TunableId, Entry>,
    /// Logical frame counter. Incremented by [`Self::tick_frame`].
    frame_n: u64,
    /// `true` when no further registrations are accepted.
    closed: bool,
    /// `true` while the registry is replaying a recorded session.
    replay_mode: bool,
    /// In-memory audit sink. Owned by the registry so callers don't have to
    /// pre-construct one (matches the spec's `AuditSink` substrate-side hand-off).
    audit: AuditSink,
    /// In-memory replay log.
    replay: ReplayLog,
}

impl Default for TunableRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TunableRegistry {
    /// Construct an empty registry. Use [`crate::install_defaults`] to load the
    /// 30 default tunables from spec § 4.3.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            frame_n: 0,
            closed: false,
            replay_mode: false,
            audit: AuditSink::new(),
            replay: ReplayLog::new(),
        }
    }

    /// Current logical-frame counter.
    #[must_use]
    pub fn frame_n(&self) -> u64 {
        self.frame_n
    }

    /// Number of registered tunables.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry has any tunables.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns `true` when no further registrations are accepted.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Freeze the registry. After this no further `register` calls succeed.
    pub fn close(&mut self) {
        self.closed = true;
    }

    /// Toggle replay-mode. While `true`, all *Manual* / *Mcp* / *Watcher*
    /// `set` calls fail with [`TweakError::ReplayDeterminismHold`] (spec §
    /// 4.7 + AP-10). Replay-origin writes still pass through.
    pub fn set_replay_mode(&mut self, on: bool) {
        self.replay_mode = on;
    }

    /// Whether replay-mode is currently on.
    #[must_use]
    pub fn is_replay_mode(&self) -> bool {
        self.replay_mode
    }

    /// Read-only view of the audit-sink.
    #[must_use]
    pub fn audit(&self) -> &AuditSink {
        &self.audit
    }

    /// Read-only view of the replay-log.
    #[must_use]
    pub fn replay_log(&self) -> &ReplayLog {
        &self.replay
    }

    /// Register a new tunable. The spec's `default` is published as the current
    /// value at frame 0. Returns the deterministic [`TunableId`].
    pub fn register(&mut self, spec: TunableSpec) -> Result<TunableId, TweakError> {
        if self.closed {
            return Err(TweakError::RegistryClosed);
        }
        // Default-validation : the spec author has to honor the contract.
        if spec.default.kind() != spec.kind {
            return Err(TweakError::KindMismatch {
                expected: spec.kind.as_str(),
                got: spec.default.kind().as_str(),
            });
        }
        if spec.range.kind() != spec.kind {
            return Err(TweakError::KindMismatch {
                expected: spec.kind.as_str(),
                got: spec.range.kind().as_str(),
            });
        }
        if spec.range.check_in_range(&spec.default).is_err() {
            return Err(TweakError::DefaultOutOfRange {
                canonical_name: spec.canonical_name,
            });
        }
        let id = spec.id();
        if self.entries.contains_key(&id) {
            return Err(TweakError::AlreadyRegistered {
                canonical_name: spec.canonical_name,
            });
        }
        let current = spec.default.clone();
        self.entries.insert(
            id,
            Entry {
                spec,
                current,
                pending: None,
                last_applied_stage: Stage::Applied,
            },
        );
        Ok(id)
    }

    /// Look up the spec for an id.
    pub fn spec(&self, id: TunableId) -> Result<&TunableSpec, TweakError> {
        self.entries
            .get(&id)
            .map(|e| &e.spec)
            .ok_or(TweakError::UnknownTunable(id))
    }

    /// Read the current value of a tunable.
    pub fn read(&self, id: TunableId) -> Result<TunableValue, TweakError> {
        self.entries
            .get(&id)
            .map(|e| e.current.clone())
            .ok_or(TweakError::UnknownTunable(id))
    }

    /// Read the staged (pending) value for a tunable, if any. Returns `None`
    /// when there is no pending write.
    pub fn read_pending(&self, id: TunableId) -> Result<Option<TunableValue>, TweakError> {
        self.entries
            .get(&id)
            .map(|e| e.pending.as_ref().map(|p| p.new_value.clone()))
            .ok_or(TweakError::UnknownTunable(id))
    }

    /// Stage a write for the next frame. The new value lands in the
    /// pending-buffer ; reads continue to observe the previous current value
    /// until [`TunableRegistry::tick_frame`] is called.
    ///
    /// Internally :
    /// 1. `cap.tag` is checked against [`Tweak::TAG`].
    /// 2. The value's [`TunableKind`] is checked against the spec.
    /// 3. The range is checked. Out-of-range :
    ///    - `WarnAndClamp` ⇒ value is clamped, `was_clamped` is recorded.
    ///    - `HardReject`  ⇒ [`TweakError::BudgetExceeded`].
    /// 4. The `(value, origin, cap-chain)` triple is stored as pending.
    pub fn set(
        &mut self,
        id: TunableId,
        value: TunableValue,
        cap: Cap<Tweak>,
    ) -> Result<Stage, TweakError> {
        self.set_with_origin(id, value, cap, TweakOrigin::Manual)
    }

    /// Like [`Self::set`] but lets the caller stamp the origin. The MCP
    /// bridge uses `TweakOrigin::Mcp`, the file-watcher uses `Watcher`, and
    /// the replay player uses `Replay`. `Manual` is the default.
    pub fn set_with_origin(
        &mut self,
        id: TunableId,
        value: TunableValue,
        cap: Cap<Tweak>,
        origin: TweakOrigin,
    ) -> Result<Stage, TweakError> {
        // 1. Cap gate.
        if cap.tag != Tweak::TAG {
            return Err(TweakError::CapDenied { needed: "Tweak" });
        }
        // 2. Replay-mode gate. Manual/Mcp/Watcher writes are forbidden.
        if self.replay_mode && !matches!(origin, TweakOrigin::Replay | TweakOrigin::Default) {
            return Err(TweakError::ReplayDeterminismHold);
        }

        let entry = self
            .entries
            .get_mut(&id)
            .ok_or(TweakError::UnknownTunable(id))?;

        // 3. Kind-check.
        if value.kind() != entry.spec.kind {
            return Err(TweakError::KindMismatch {
                expected: entry.spec.kind.as_str(),
                got: value.kind().as_str(),
            });
        }

        // 4. Range-check + clamp / hard-reject.
        let (final_value, was_clamped) = match entry.spec.range.check_in_range(&value) {
            Ok(()) => (value, false),
            Err(()) => match (entry.spec.budget_mode, &entry.spec.range) {
                (BudgetMode::WarnAndClamp, _) => match entry.spec.range.clamp(&value) {
                    Some(clamped) => (clamped, true),
                    // Clamp not defined for Bool/StringEnum — fall through to reject.
                    None => match &entry.spec.range {
                        TunableRange::StringEnum(allowed) => {
                            return Err(TweakError::StringEnumInvalid {
                                spec_name: entry.spec.canonical_name,
                                allowed: allowed.clone(),
                                got: match value {
                                    TunableValue::StringEnum(s) => s,
                                    _ => String::new(),
                                },
                            });
                        }
                        _ => {
                            return Err(TweakError::BudgetExceeded {
                                spec_name: entry.spec.canonical_name,
                            });
                        }
                    },
                },
                (BudgetMode::HardReject, TunableRange::StringEnum(allowed)) => {
                    return Err(TweakError::StringEnumInvalid {
                        spec_name: entry.spec.canonical_name,
                        allowed: allowed.clone(),
                        got: match value {
                            TunableValue::StringEnum(s) => s,
                            _ => String::new(),
                        },
                    });
                }
                (BudgetMode::HardReject, _) => {
                    return Err(TweakError::BudgetExceeded {
                        spec_name: entry.spec.canonical_name,
                    });
                }
            },
        };

        // 5. Stage. Frame-boundary defer is the contract for the L4 surface ;
        //    if a future caller flips the flag off, the value applies inline.
        let cap_chain = vec![cap.tag];
        if entry.spec.frame_boundary_defer {
            entry.pending = Some(PendingWrite {
                new_value: final_value,
                was_clamped,
                origin,
                cap_chain,
            });
            entry.last_applied_stage = Stage::Pending;
            Ok(Stage::Pending)
        } else {
            // Inline path : update + audit + replay immediately.
            let old_render = entry.current.render();
            entry.current = final_value.clone();
            entry.last_applied_stage = Stage::Applied;
            let event_value = final_value.clone();
            self.audit.record(TweakAuditEntry {
                frame_n: self.frame_n,
                audit_seq: 0, // overwritten by sink
                tunable_id: id,
                canonical_name: entry.spec.canonical_name,
                old_value: old_render,
                new_value: final_value.render(),
                was_clamped,
                cap_chain,
                origin,
            });
            self.replay.push(TweakEvent {
                frame_n: self.frame_n,
                tunable_id: id,
                canonical_name: entry.spec.canonical_name,
                new_value: event_value,
                origin,
            });
            Ok(Stage::Applied)
        }
    }

    /// Reset a tunable to its spec default. Treated as a `Default`-origin
    /// `set` that bypasses replay-mode (the spec considers reset a registry
    /// operation, not a user mutation).
    pub fn reset(&mut self, id: TunableId, cap: Cap<Tweak>) -> Result<Stage, TweakError> {
        let default = self
            .entries
            .get(&id)
            .map(|e| e.spec.default.clone())
            .ok_or(TweakError::UnknownTunable(id))?;
        self.set_with_origin(id, default, cap, TweakOrigin::Default)
    }

    /// Advance the logical-frame counter and apply every pending write.
    ///
    /// On apply, each write produces :
    /// 1. A [`TweakAuditEntry`] with `frame_n = old + 1`.
    /// 2. A [`TweakEvent`] in the replay-log with the same frame.
    /// 3. Atomic publish of the new value to `current`.
    ///
    /// Returns the number of writes that were applied this tick.
    pub fn tick_frame(&mut self) -> usize {
        self.frame_n += 1;
        let mut applied = 0_usize;
        // Two-phase walk so we can record into self.audit / self.replay
        // without aliasing the entry borrow.
        let ids: Vec<TunableId> = self
            .entries
            .iter()
            .filter(|(_, e)| e.pending.is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let frame_n = self.frame_n;
            let (audit, event) = {
                let entry = self.entries.get_mut(&id).expect("id was just collected");
                let pending = entry.pending.take().expect("filtered to Some");
                let old_render = entry.current.render();
                entry.current = pending.new_value.clone();
                entry.last_applied_stage = Stage::Applied;
                let audit = TweakAuditEntry {
                    frame_n,
                    audit_seq: 0,
                    tunable_id: id,
                    canonical_name: entry.spec.canonical_name,
                    old_value: old_render,
                    new_value: pending.new_value.render(),
                    was_clamped: pending.was_clamped,
                    cap_chain: pending.cap_chain,
                    origin: pending.origin,
                };
                let event = TweakEvent {
                    frame_n,
                    tunable_id: id,
                    canonical_name: entry.spec.canonical_name,
                    new_value: pending.new_value,
                    origin: pending.origin,
                };
                (audit, event)
            };
            self.audit.record(audit);
            self.replay.push(event);
            applied += 1;
        }
        applied
    }

    /// Iterate over registered specs in `(id, spec)` pairs. The order is the
    /// hash-map's iteration order ; tests should not rely on it.
    pub fn iter(&self) -> impl Iterator<Item = (TunableId, &TunableSpec)> {
        self.entries.iter().map(|(id, entry)| (*id, &entry.spec))
    }

    /// Lifecycle stage of the most recent operation on `id`. Useful for
    /// inspector UIs that want to mark "pending" values.
    pub fn stage(&self, id: TunableId) -> Result<Stage, TweakError> {
        self.entries
            .get(&id)
            .map(|e| {
                if e.pending.is_some() {
                    Stage::Pending
                } else {
                    e.last_applied_stage
                }
            })
            .ok_or(TweakError::UnknownTunable(id))
    }
}

// ─── unit-tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tunable::{TunableKind, TunableRange, TunableSpec, TunableValue};

    fn float_spec(name: &'static str, mode: BudgetMode) -> TunableSpec {
        TunableSpec {
            canonical_name: name,
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..1.0),
            default: TunableValue::F32(0.5),
            budget_mode: mode,
            description: "test",
            units: None,
            frame_boundary_defer: true,
        }
    }

    #[test]
    fn register_and_read_default() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        match reg.read(id).unwrap() {
            TunableValue::F32(v) => assert!((v - 0.5).abs() < f32::EPSILON),
            _ => panic!(),
        }
    }

    #[test]
    fn register_duplicate_rejected() {
        let mut reg = TunableRegistry::new();
        reg.register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        let err = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap_err();
        assert!(matches!(err, TweakError::AlreadyRegistered { .. }));
    }

    #[test]
    fn closed_registry_refuses_new_registrations() {
        let mut reg = TunableRegistry::new();
        reg.close();
        let err = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap_err();
        assert_eq!(err, TweakError::RegistryClosed);
    }

    #[test]
    fn set_defers_until_tick() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        let stage = reg.set(id, TunableValue::F32(0.75), Cap::tweak()).unwrap();
        assert_eq!(stage, Stage::Pending);
        // Pre-tick : current is still default.
        match reg.read(id).unwrap() {
            TunableValue::F32(v) => assert!((v - 0.5).abs() < f32::EPSILON),
            _ => panic!(),
        }
        let applied = reg.tick_frame();
        assert_eq!(applied, 1);
        match reg.read(id).unwrap() {
            TunableValue::F32(v) => assert!((v - 0.75).abs() < f32::EPSILON),
            _ => panic!(),
        }
        assert_eq!(reg.frame_n(), 1);
        assert_eq!(reg.audit().len(), 1);
        assert_eq!(reg.replay_log().len(), 1);
    }

    #[test]
    fn set_without_cap_denied() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        let bad = Cap::<Tweak>::stub(CapTag("Imposter"));
        let err = reg.set(id, TunableValue::F32(0.75), bad).unwrap_err();
        assert!(matches!(err, TweakError::CapDenied { .. }));
    }

    #[test]
    fn set_kind_mismatch_rejected() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        let err = reg.set(id, TunableValue::U32(2), Cap::tweak()).unwrap_err();
        assert!(matches!(err, TweakError::KindMismatch { .. }));
    }

    #[test]
    fn warn_and_clamp_path() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        let stage = reg.set(id, TunableValue::F32(2.0), Cap::tweak()).unwrap();
        assert_eq!(stage, Stage::Pending);
        reg.tick_frame();
        match reg.read(id).unwrap() {
            TunableValue::F32(v) => {
                assert!(v < 1.0);
                assert!(v > 0.99);
            }
            _ => panic!(),
        }
        let entry = &reg.audit().entries()[0];
        assert!(entry.was_clamped);
    }

    #[test]
    fn hard_reject_path() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::HardReject))
            .unwrap();
        let err = reg
            .set(id, TunableValue::F32(2.0), Cap::tweak())
            .unwrap_err();
        assert!(matches!(err, TweakError::BudgetExceeded { .. }));
    }

    #[test]
    fn replay_mode_blocks_manual_writes() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        reg.set_replay_mode(true);
        let err = reg
            .set(id, TunableValue::F32(0.75), Cap::tweak())
            .unwrap_err();
        assert_eq!(err, TweakError::ReplayDeterminismHold);
    }

    #[test]
    fn replay_origin_passes_in_replay_mode() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        reg.set_replay_mode(true);
        let stage = reg
            .set_with_origin(
                id,
                TunableValue::F32(0.75),
                Cap::tweak(),
                TweakOrigin::Replay,
            )
            .unwrap();
        assert_eq!(stage, Stage::Pending);
        reg.tick_frame();
    }

    #[test]
    fn reset_returns_to_default() {
        let mut reg = TunableRegistry::new();
        let id = reg
            .register(float_spec("a.b", BudgetMode::WarnAndClamp))
            .unwrap();
        reg.set(id, TunableValue::F32(0.9), Cap::tweak()).unwrap();
        reg.tick_frame();
        reg.reset(id, Cap::tweak()).unwrap();
        reg.tick_frame();
        match reg.read(id).unwrap() {
            TunableValue::F32(v) => assert!((v - 0.5).abs() < f32::EPSILON),
            _ => panic!(),
        }
    }

    #[test]
    fn unknown_tunable() {
        let reg = TunableRegistry::new();
        let err = reg.read(TunableId::of("nope")).unwrap_err();
        assert!(matches!(err, TweakError::UnknownTunable(_)));
    }

    #[test]
    fn frame_counter_increments() {
        let mut reg = TunableRegistry::new();
        assert_eq!(reg.frame_n(), 0);
        reg.tick_frame();
        reg.tick_frame();
        reg.tick_frame();
        assert_eq!(reg.frame_n(), 3);
    }
}
