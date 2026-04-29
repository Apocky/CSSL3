//! § cssl-substrate-save — Ω-tensor + scheduler + replay-log placeholder types.
//!
//! § ROLE
//!   These types are minimal canonical-shaped stand-ins for the H1
//!   (Ω-tensor serialization) + H2 (omega_step replay-log) deliverables.
//!   At the time S8-H5 landed, neither H1 nor H2 had been implemented ;
//!   this slice carries placeholder types so the save/load + replay
//!   machinery can compile + test standalone.
//!
//! § UPGRADE PATH
//!   When H1 + H2 land, this module either :
//!   (a) re-exports the canonical types from `cssl-substrate-omega`
//!       (tentative future crate name) — drop the local impl, keep the API ;
//!   OR
//!   (b) keeps the local types as the canonical home — H1 / H2 lift
//!       additional fields onto these structs without renaming.
//!   The save-format invariant (sorted-key serialization, deterministic
//!   field-ordering) is preserved across either path.
//!
//! § PRIME-DIRECTIVE alignment
//!   - **IFC label travels through saves** : every [`OmegaCell`] carries an
//!     `ifc_label : u32` (placeholder for the full Jif-DLM lattice in
//!     `cssl-ifc`). Round-trip through save+load preserves the label
//!     bit-exact.
//!   - **No HashMap iteration** : the [`OmegaTensor`] field-order matches
//!     the spec's `(type-tag, rank, shape, strides, data)` discipline ;
//!     [`OmegaScheduler`] keys are sorted into a `Vec<(name, tensor)>`
//!     before serialization (per the slice landmines).
//!
//! § STAGE-0 SHAPE
//!   - [`OmegaTensor`] : type-tag + rank + shape (`Vec<u32>`) + strides
//!     (`Vec<u32>`) + raw bytes (`Vec<u8>`) + IFC-label (u32)
//!   - [`OmegaScheduler`] : sorted `Vec<(name : String, tensor : OmegaTensor)>`
//!     + a `frame : u64` counter + a `replay_log : ReplayLog`
//!   - [`ReplayLog`] : `Vec<ReplayEvent>` in frame-order
//!   - [`ReplayEvent`] : `{ frame : u64, kind : ReplayKind, payload : Vec<u8> }`
//!     where `ReplayKind` enumerates the canonical `omega_step` phase events
//!     (Sim / Render / Audio / Save / Net) per `specs/30_SUBSTRATE.csl`
//!     § OMEGA-STEP § PHASES.

use core::cmp::Ordering;

/// Canonical Ω-tensor cell — the placeholder shape for H1's full lattice.
///
/// At stage-0 the cell is a single scalar with a type-tag + raw byte-data +
/// IFC-label. H1 will lift this to multi-element tensors with arbitrary rank.
/// The serialization shape is forward-compatible : H1's richer cells emit
/// rank ≥ 1 ; this stage-0 cell emits as `(type_tag, rank=0, shape=[], strides=[], data, ifc_label)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OmegaCell {
    /// Type-tag matching one of the `OMEGA_TYPE_TAG_*` constants in
    /// [`crate::format`] (u8 / i32 / i64 / f32 / f64).
    pub type_tag: u8,
    /// Raw little-endian byte representation of the cell's value.
    pub data: Vec<u8>,
    /// IFC label as a 32-bit integer. The full Jif-DLM lattice from
    /// `specs/11_IFC.csl` bit-packs into this slot via a registry
    /// (`cssl-ifc::Label::to_u32` / `from_u32` pair, deferred).
    pub ifc_label: u32,
}

impl OmegaCell {
    /// New cell with `type_tag`, value `data`, and zero IFC-label
    /// (`L = ⊥` = "everyone reads, nobody influences" per
    /// `specs/11_IFC.csl § LABEL ALGEBRA`).
    #[must_use]
    pub const fn new(type_tag: u8, data: Vec<u8>) -> Self {
        Self {
            type_tag,
            data,
            ifc_label: 0,
        }
    }

    /// New cell with explicit IFC-label.
    #[must_use]
    pub const fn with_label(type_tag: u8, data: Vec<u8>, ifc_label: u32) -> Self {
        Self {
            type_tag,
            data,
            ifc_label,
        }
    }
}

/// Canonical Ω-tensor — type-tag + rank + shape + strides + flattened cells.
///
/// At stage-0 this is the rank-0 "single-cell" form that holds one
/// [`OmegaCell`]. H1 will lift this to rank ≥ 1 with shape + strides driving
/// multi-element layouts. The serialized form is forward-compatible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OmegaTensor {
    /// Tensor rank (number of dimensions). Stage-0 only emits rank-0
    /// (scalar tensor).
    pub rank: u32,
    /// Shape — `rank` u32 values. Empty for rank-0.
    pub shape: Vec<u32>,
    /// Strides — `rank` u32 values, in element-units. Empty for rank-0.
    pub strides: Vec<u32>,
    /// Flattened cells in row-major order (or as dictated by `strides`).
    /// Length must equal `product(shape)` ; rank-0 has length 1.
    pub cells: Vec<OmegaCell>,
}

impl OmegaTensor {
    /// New scalar (rank-0) tensor wrapping a single cell.
    #[must_use]
    pub fn scalar(cell: OmegaCell) -> Self {
        Self {
            rank: 0,
            shape: Vec::new(),
            strides: Vec::new(),
            cells: vec![cell],
        }
    }
}

/// Replay-event kind — corresponds to the omega_step phase that produced the event.
///
/// Per `specs/30_SUBSTRATE.csl` § OMEGA-STEP § PHASES, the canonical
/// 13-phase tick produces events in {Sim, Render, Audio, Save, Net,
/// Telemetry, Audit}-categories. At stage-0 we encode 5 of those 7 ;
/// Telemetry + Audit live in their own ring (`cssl-telemetry`) and don't
/// cross into the save-replay log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ReplayKind {
    /// `{Sim}` phase event (physics + voxel + fluid update).
    Sim = 1,
    /// `{Render}` phase event (CmdBuf record + submit).
    Render = 2,
    /// `{Audio}` phase event (DSP-callback feed).
    Audio = 3,
    /// `{Save}` phase event (journal-append). NOT TO BE confused with the
    /// save-file format itself ; this is the in-tick journal-append that
    /// the canonical save-file consumes.
    Save = 4,
    /// `{Net}` phase event. Replayed-mode replaces actual network with the
    /// recorded trace per `specs/30_SUBSTRATE.csl` § EFFECT-ROWS.
    Net = 5,
}

impl ReplayKind {
    /// Decode a wire-format byte back into a [`ReplayKind`]. Returns `None`
    /// for unknown tags ; callers map the `None` to [`crate::LoadError::UnknownEventTag`].
    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Sim),
            2 => Some(Self::Render),
            3 => Some(Self::Audio),
            4 => Some(Self::Save),
            5 => Some(Self::Net),
            _ => None,
        }
    }
}

/// Single replay-event in the omega_step replay-log.
///
/// `frame` is the omega_step epoch ; `kind` identifies the phase ;
/// `payload` is the phase-specific data (e.g. for `Sim` it would be the
/// substep input-state ; for `Net` the recorded packet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEvent {
    /// Frame index this event was emitted in.
    pub frame: u64,
    /// Phase that produced the event.
    pub kind: ReplayKind,
    /// Phase-specific payload — opaque bytes at the save-layer.
    pub payload: Vec<u8>,
}

impl ReplayEvent {
    /// New event.
    #[must_use]
    pub const fn new(frame: u64, kind: ReplayKind, payload: Vec<u8>) -> Self {
        Self {
            frame,
            kind,
            payload,
        }
    }
}

impl Ord for ReplayEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        // Frame-major then kind-major (stable byte-by-byte ordering).
        self.frame
            .cmp(&other.frame)
            .then((self.kind as u8).cmp(&(other.kind as u8)))
            .then(self.payload.cmp(&other.payload))
    }
}

impl PartialOrd for ReplayEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Replay-log — frame-ordered stream of [`ReplayEvent`].
///
/// `events` are stored in append-order during a run. The serialized form
/// canonicalizes them by `(frame, kind, payload)` ordering so that two
/// runs that produced events in the same logical-order serialize identically.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplayLog {
    /// Events appended in production-order. Sorted before serialization
    /// per the deterministic-field-ordering invariant.
    pub events: Vec<ReplayEvent>,
}

impl ReplayLog {
    /// New empty log.
    #[must_use]
    pub const fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Append an event.
    pub fn append(&mut self, ev: ReplayEvent) {
        self.events.push(ev);
    }

    /// Number of events in the log.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Empty check.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Return a sorted clone of the events for deterministic serialization.
    /// Callers MUST go through this rather than reading [`Self::events`]
    /// directly when computing attestation hashes.
    #[must_use]
    pub fn sorted_events(&self) -> Vec<ReplayEvent> {
        let mut v = self.events.clone();
        v.sort();
        v
    }
}

/// Canonical Ω-tensor scheduler — the H2 placeholder shape.
///
/// Stage-0 fields :
/// - `tensors` : `Vec<(name : String, tensor : OmegaTensor)>` SORTED by name.
///   The slice landmines forbid HashMap iteration ; we use a sorted Vec to
///   make the field-order deterministic.
/// - `frame` : current omega_step epoch counter.
/// - `replay_log` : the recorded event-stream from frame 0 → `frame`.
///
/// H2 will lift this with the real omega_step machinery + capability
/// tracking + telemetry-ring integration. The save/load contract here
/// stays stable across the upgrade.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OmegaScheduler {
    /// Named tensors, SORTED by name. Use [`Self::insert_tensor`] to
    /// preserve sorted-order ; do not push directly to `tensors` outside
    /// of test fixtures.
    pub tensors: Vec<(String, OmegaTensor)>,
    /// Current omega_step epoch counter.
    pub frame: u64,
    /// Event-log produced by every step from frame 0 to [`Self::frame`].
    pub replay_log: ReplayLog,
}

impl OmegaScheduler {
    /// New empty scheduler at frame 0 with an empty replay-log.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tensors: Vec::new(),
            frame: 0,
            replay_log: ReplayLog::new(),
        }
    }

    /// Insert (or replace) a named tensor, preserving the sorted-by-name
    /// invariant. `O(n)` (linear scan + insert) ; stage-0 schedulers are
    /// expected to hold tens of named tensors, not thousands.
    pub fn insert_tensor(&mut self, name: impl Into<String>, tensor: OmegaTensor) {
        let name = name.into();
        match self.tensors.binary_search_by(|(n, _)| n.cmp(&name)) {
            Ok(idx) => {
                self.tensors[idx].1 = tensor;
            }
            Err(idx) => {
                self.tensors.insert(idx, (name, tensor));
            }
        }
    }

    /// Look up a named tensor.
    #[must_use]
    pub fn get_tensor(&self, name: &str) -> Option<&OmegaTensor> {
        self.tensors
            .binary_search_by(|(n, _)| n.as_str().cmp(name))
            .ok()
            .map(|idx| &self.tensors[idx].1)
    }

    /// Snapshot just the tensor portion (no replay-log, no frame counter).
    /// Used by [`crate::replay_from`] to compare bit-equality.
    #[must_use]
    pub fn snapshot_tensors(&self) -> Vec<(String, OmegaTensor)> {
        self.tensors.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omega_cell_default_label_is_zero() {
        let c = OmegaCell::new(crate::format::OMEGA_TYPE_TAG_I32, vec![0, 0, 0, 0]);
        assert_eq!(c.ifc_label, 0);
    }

    #[test]
    fn omega_cell_with_label_round_trips() {
        let c = OmegaCell::with_label(crate::format::OMEGA_TYPE_TAG_F32, vec![0; 4], 0xDEAD_BEEF);
        assert_eq!(c.ifc_label, 0xDEAD_BEEF);
    }

    #[test]
    fn omega_tensor_scalar_has_rank_zero() {
        let t = OmegaTensor::scalar(OmegaCell::new(crate::format::OMEGA_TYPE_TAG_U8, vec![42]));
        assert_eq!(t.rank, 0);
        assert!(t.shape.is_empty());
        assert!(t.strides.is_empty());
        assert_eq!(t.cells.len(), 1);
    }

    #[test]
    fn replay_kind_byte_round_trip() {
        for k in [
            ReplayKind::Sim,
            ReplayKind::Render,
            ReplayKind::Audio,
            ReplayKind::Save,
            ReplayKind::Net,
        ] {
            let b = k as u8;
            assert_eq!(ReplayKind::from_byte(b), Some(k));
        }
    }

    #[test]
    fn replay_kind_zero_byte_is_unknown() {
        assert_eq!(ReplayKind::from_byte(0), None);
        assert_eq!(ReplayKind::from_byte(255), None);
    }

    #[test]
    fn replay_log_sorts_events_deterministically() {
        let mut log = ReplayLog::new();
        log.append(ReplayEvent::new(2, ReplayKind::Sim, vec![1]));
        log.append(ReplayEvent::new(0, ReplayKind::Render, vec![2]));
        log.append(ReplayEvent::new(1, ReplayKind::Audio, vec![3]));
        let sorted = log.sorted_events();
        assert_eq!(sorted[0].frame, 0);
        assert_eq!(sorted[1].frame, 1);
        assert_eq!(sorted[2].frame, 2);
    }

    #[test]
    fn omega_scheduler_insert_tensor_preserves_sorted_order() {
        let mut sched = OmegaScheduler::new();
        sched.insert_tensor("zebra", OmegaTensor::default());
        sched.insert_tensor("alpha", OmegaTensor::default());
        sched.insert_tensor("mango", OmegaTensor::default());
        let names: Vec<&str> = sched.tensors.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["alpha", "mango", "zebra"]);
    }

    #[test]
    fn omega_scheduler_insert_tensor_replaces_on_duplicate_name() {
        let mut sched = OmegaScheduler::new();
        let t1 = OmegaTensor::scalar(OmegaCell::new(crate::format::OMEGA_TYPE_TAG_U8, vec![1]));
        let t2 = OmegaTensor::scalar(OmegaCell::new(crate::format::OMEGA_TYPE_TAG_U8, vec![2]));
        sched.insert_tensor("k", t1);
        sched.insert_tensor("k", t2.clone());
        assert_eq!(sched.tensors.len(), 1);
        assert_eq!(sched.tensors[0].1, t2);
    }

    #[test]
    fn omega_scheduler_get_tensor_finds_by_name() {
        let mut sched = OmegaScheduler::new();
        sched.insert_tensor("a", OmegaTensor::scalar(OmegaCell::new(0xA, vec![])));
        sched.insert_tensor("b", OmegaTensor::scalar(OmegaCell::new(0xB, vec![])));
        let a = sched.get_tensor("a").expect("a present");
        let b = sched.get_tensor("b").expect("b present");
        assert_eq!(a.cells[0].type_tag, 0xA);
        assert_eq!(b.cells[0].type_tag, 0xB);
        assert!(sched.get_tensor("missing").is_none());
    }

    #[test]
    fn omega_scheduler_new_starts_at_frame_zero_with_empty_log() {
        let s = OmegaScheduler::new();
        assert_eq!(s.frame, 0);
        assert!(s.replay_log.is_empty());
    }
}
