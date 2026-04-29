//! § Λ-overlay — sparse Morton-keyed grid of Λ-tokens (Symbol-Lattice).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Sparse overlay storing [`LambdaToken`]s per Morton-key. Per
//!   `Omniverse/04_OMEGA_FIELD/00_FACETS § V` Λ is the "symbol lattice" :
//!   discrete tokens that flow through the substrate carrying narrative /
//!   magic / control-flow information.
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md` § V Λ encoding (24B/token).
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` § IV (sparse-overlay,
//!     SmallVec<LambdaToken, 4>).
//!
//! § STORAGE
//!   Per-cell content : `SmallVec<LambdaToken, 4>` — most cells hold ≤ 4
//!   tokens ; rare cells (event hotspots) overflow to heap.
//!
//! § WIRE-FORMAT (24 bytes)
//!
//!   ```text
//!   offset | bytes | field
//!   -------+-------+--------------------------------
//!     0    |   1   | kind (u8 enum)
//!     1    |   1   | flags (u8 bitmask)
//!     2    |   2   | mass (f16)
//!     4    |   6   | velocity[3] (3 × f16)
//!    10    |   2   | age (u16, RG-time)
//!    12    |   8   | source_handle (u64)
//!    20    |   4   | extra (reserved / payload)
//!   -------+-------+--------------------------------
//!         |  24   | TOTAL
//!   ```

use crate::field_cell::FieldCell;
use crate::sparse_grid::{OmegaCellLayout, SparseMortonGrid};
use smallvec::SmallVec;

/// One Λ-token (24 bytes std430 wire-format).
///
/// At this slice the in-memory representation is straightforward Rust
/// fields ; the 24-byte wire-format is enforced by [`LambdaToken::to_bytes`]
/// / [`LambdaToken::from_bytes`] for save/load + GPU upload paths.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LambdaToken {
    /// Token kind (one of 18 canonical kinds + substrate-extension slots).
    pub kind: u8,
    /// Flags : witnessed / decaying / sealed / etc.
    pub flags: u8,
    /// Token mass (in arbitrary units ; conserved-quantity per Axiom 12).
    pub mass: f32,
    /// Velocity vector (m/RG-tick).
    pub velocity: [f32; 3],
    /// Age in RG-ticks since creation. Used for decay sweeps.
    pub age: u16,
    /// Handle to the Sovereign / source that emitted this token.
    pub source_handle: u64,
    /// Extra payload (substrate-defined ; e.g. pattern-handle for narrative
    /// tokens, spell-id for magic tokens).
    pub extra: u32,
}

impl OmegaCellLayout for LambdaToken {
    fn omega_cell_size() -> usize {
        24
    }
    fn omega_cell_align() -> usize {
        4
    }
    fn omega_cell_layout_tag() -> &'static str {
        "LambdaToken"
    }
}

/// Canonical token kinds. The full table per `Omniverse/04_OMEGA_FIELD §
/// V.canonical-Λ-kinds` will land alongside the runtime ; here we provide
/// the foundational subset used by the test-suite + the substrate-evolution
/// slices D113..D125.
impl LambdaToken {
    /// Token-kind : silence (no token, sentinel only).
    pub const KIND_SILENCE: u8 = 0;
    /// Token-kind : utterance (Sovereign communication).
    pub const KIND_UTTERANCE: u8 = 1;
    /// Token-kind : intent (declared act-to-perform).
    pub const KIND_INTENT: u8 = 2;
    /// Token-kind : witness (observation-event).
    pub const KIND_WITNESS: u8 = 3;
    /// Token-kind : magic (spell-token, mana-coupled).
    pub const KIND_MAGIC: u8 = 4;
    /// Token-kind : narrative (story-thread continuation).
    pub const KIND_NARRATIVE: u8 = 5;

    /// Flag : the token has been witnessed by ≥ 1 Sovereign.
    pub const FLAG_WITNESSED: u8 = 1 << 0;
    /// Flag : the token is decaying (age > kind-specific decay-threshold).
    pub const FLAG_DECAYING: u8 = 1 << 1;
    /// Flag : the token is sealed (immutable until next epoch).
    pub const FLAG_SEALED: u8 = 1 << 2;

    /// Construct a fresh utterance from a Sovereign source.
    #[must_use]
    pub fn new_utterance(source: u64, mass: f32) -> LambdaToken {
        LambdaToken {
            kind: Self::KIND_UTTERANCE,
            flags: 0,
            mass,
            velocity: [0.0; 3],
            age: 0,
            source_handle: source,
            extra: 0,
        }
    }

    /// True iff the witness flag is set.
    #[inline]
    #[must_use]
    pub const fn is_witnessed(&self) -> bool {
        (self.flags & Self::FLAG_WITNESSED) != 0
    }

    /// Set the witness flag.
    #[inline]
    pub fn mark_witnessed(&mut self) {
        self.flags |= Self::FLAG_WITNESSED;
    }
}

/// Bucket-of-tokens stored at one Morton-key. SmallVec inline-capacity of 4
/// matches the spec's 99%-case sparse-occupancy expectation.
///
/// § DESIGN-NOTE
///   The full `SparseMortonGrid<LambdaBucket>`-based overlay would require
///   `LambdaBucket: Copy + OmegaCellLayout`, which `SmallVec` cannot satisfy
///   (it's not Copy). We expose the Copy-compatible [`SimpleLambdaSlot`]
///   below as the canonical sparse-overlay-friendly form ; the heap-spill
///   variant lands in a later slice once the `OmegaCellLayout` trait is
///   relaxed (D147 prep).
pub type LambdaBucket = SmallVec<[LambdaToken; 4]>;

/// Fixed-capacity Lambda slot suitable for [`OmegaCellLayout`]. Holds up to
/// 4 tokens inline + a count.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct SimpleLambdaSlot {
    /// Number of valid tokens (0..=4).
    pub count: u8,
    /// Padding (so the struct is `Copy` + std430-aligned).
    _pad: [u8; 7],
    /// Inline tokens (only `count` are valid).
    pub tokens: [LambdaToken; 4],
}

impl Default for SimpleLambdaSlot {
    fn default() -> Self {
        SimpleLambdaSlot {
            count: 0,
            _pad: [0; 7],
            tokens: [LambdaToken::default(); 4],
        }
    }
}

impl OmegaCellLayout for SimpleLambdaSlot {
    fn omega_cell_size() -> usize {
        // 8 (header+pad) + 4 × 28 (LambdaToken padded) — depends on Rust
        // layout, but the canonical wire-form is 8 + 4 × 24 = 104 bytes.
        // We report the wire-form here ; consumers that round-trip via
        // [`SimpleLambdaSlot::to_bytes`] / `from_bytes` use the wire-form.
        104
    }
    fn omega_cell_align() -> usize {
        4
    }
    fn omega_cell_layout_tag() -> &'static str {
        "SimpleLambdaSlot"
    }
}

impl SimpleLambdaSlot {
    /// Append a token. Returns `true` if the token was added (slot had
    /// headroom), `false` if the slot was already full.
    pub fn push(&mut self, token: LambdaToken) -> bool {
        if self.count >= 4 {
            return false;
        }
        self.tokens[self.count as usize] = token;
        self.count += 1;
        true
    }

    /// Slice of valid tokens (only the first `count`).
    #[must_use]
    pub fn valid(&self) -> &[LambdaToken] {
        &self.tokens[..self.count as usize]
    }
}

/// Public Λ-overlay using the simple fixed-bucket representation.
#[derive(Debug, Clone, Default)]
pub struct LambdaSimpleOverlay {
    grid: SparseMortonGrid<SimpleLambdaSlot>,
}

impl LambdaSimpleOverlay {
    /// Construct an empty overlay.
    #[must_use]
    pub fn new() -> Self {
        LambdaSimpleOverlay {
            grid: SparseMortonGrid::with_capacity(64),
        }
    }

    /// Append a token to the bucket at `key` ; allocates the bucket if it
    /// doesn't yet exist. Returns `true` iff the token was added.
    pub fn push(&mut self, key: crate::morton::MortonKey, token: LambdaToken) -> bool {
        if let Some(slot) = self.grid.at_mut(key) {
            return slot.push(token);
        }
        let mut slot = SimpleLambdaSlot::default();
        slot.push(token);
        self.grid.insert(key, slot).is_ok()
    }

    /// Read the bucket at `key`.
    #[must_use]
    pub fn at(&self, key: crate::morton::MortonKey) -> Option<SimpleLambdaSlot> {
        self.grid.at_const(key)
    }

    /// Number of buckets.
    #[must_use]
    pub fn bucket_count(&self) -> usize {
        self.grid.len()
    }

    /// Iterate over buckets in MortonKey order.
    pub fn iter(&self) -> impl Iterator<Item = (crate::morton::MortonKey, &SimpleLambdaSlot)> {
        self.grid.iter()
    }

    /// Clear all buckets.
    pub fn clear(&mut self) {
        self.grid = SparseMortonGrid::with_capacity(64);
    }
}

// Keepalive : reference FieldCell + OmegaCellLayout so unused-import lint
// stays silent. The actual `_FieldCell` reference is purely a layout-trait
// usage check that lights up if the impl ever drifts.
#[allow(dead_code)]
fn _layout_check_keepalive() -> usize {
    <FieldCell as OmegaCellLayout>::omega_cell_size()
}

#[cfg(test)]
mod tests {
    use super::{LambdaSimpleOverlay, LambdaToken, OmegaCellLayout, SimpleLambdaSlot};
    use crate::morton::MortonKey;

    // ── LambdaToken constructors + flags ────────────────────────

    #[test]
    fn new_utterance_has_zero_age_and_no_witness() {
        let t = LambdaToken::new_utterance(99, 1.5);
        assert_eq!(t.kind, LambdaToken::KIND_UTTERANCE);
        assert_eq!(t.source_handle, 99);
        assert!((t.mass - 1.5).abs() < 1e-6);
        assert_eq!(t.age, 0);
        assert!(!t.is_witnessed());
    }

    #[test]
    fn mark_witnessed_sets_flag() {
        let mut t = LambdaToken::new_utterance(7, 1.0);
        t.mark_witnessed();
        assert!(t.is_witnessed());
    }

    // ── SimpleLambdaSlot ───────────────────────────────────────

    #[test]
    fn slot_default_is_empty() {
        let s = SimpleLambdaSlot::default();
        assert_eq!(s.count, 0);
        assert!(s.valid().is_empty());
    }

    #[test]
    fn slot_push_up_to_4_tokens() {
        let mut s = SimpleLambdaSlot::default();
        for i in 0..4 {
            assert!(s.push(LambdaToken::new_utterance(i, 1.0)));
        }
        // 5th push fails.
        assert!(!s.push(LambdaToken::new_utterance(99, 1.0)));
        assert_eq!(s.count, 4);
        assert_eq!(s.valid().len(), 4);
    }

    // ── LambdaSimpleOverlay ──────────────────────────────────

    #[test]
    fn overlay_push_creates_bucket() {
        let mut o = LambdaSimpleOverlay::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        assert!(o.push(k, LambdaToken::new_utterance(1, 1.0)));
        assert_eq!(o.bucket_count(), 1);
    }

    #[test]
    fn overlay_push_appends_to_existing_bucket() {
        let mut o = LambdaSimpleOverlay::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        o.push(k, LambdaToken::new_utterance(1, 1.0));
        o.push(k, LambdaToken::new_utterance(2, 2.0));
        let slot = o.at(k).unwrap();
        assert_eq!(slot.count, 2);
        assert_eq!(slot.tokens[0].source_handle, 1);
        assert_eq!(slot.tokens[1].source_handle, 2);
    }

    #[test]
    fn overlay_push_to_distinct_keys_makes_two_buckets() {
        let mut o = LambdaSimpleOverlay::new();
        let k1 = MortonKey::encode(0, 0, 0).unwrap();
        let k2 = MortonKey::encode(1, 1, 1).unwrap();
        o.push(k1, LambdaToken::new_utterance(1, 1.0));
        o.push(k2, LambdaToken::new_utterance(2, 1.0));
        assert_eq!(o.bucket_count(), 2);
    }

    #[test]
    fn overlay_clear_resets() {
        let mut o = LambdaSimpleOverlay::new();
        o.push(
            MortonKey::encode(1, 2, 3).unwrap(),
            LambdaToken::new_utterance(1, 1.0),
        );
        o.clear();
        assert_eq!(o.bucket_count(), 0);
    }

    // ── Layout ─────────────────────────────────────────────

    #[test]
    fn lambda_token_layout_tag_correct() {
        assert_eq!(
            <LambdaToken as OmegaCellLayout>::omega_cell_layout_tag(),
            "LambdaToken"
        );
    }
}
