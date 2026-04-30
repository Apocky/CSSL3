//! § FieldCellV3 — 88B std430-aligned Ω-field cell (ADCS extension).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The ADCS-tier 7-facet-v2 cell that extends [`crate::field_cell::FieldCell`]
//!   (72B v2) with three new facets per `specs/30_SUBSTRATE_v3.csl`
//!   § CELL-SHAPE-EXTENSION + `specs/35_DECAPLENOPTIC.csl` § STORAGE-LAYOUT :
//!
//!     - facet **E** (epoch + entropy band)        : 8 bytes  @ offset 72
//!     - facet **C** (capability-handle)           : 4 bytes  @ offset 80
//!     - facet **K** (KAN-band-handle)             : 4 bytes  @ offset 84
//!
//!   Total = 72 (v2 prefix) + 16 (E + C + K) = **88 bytes**, 8-byte aligned.
//!
//! § BYTE-LAYOUT (std430, 88B total, 8-byte alignment)
//!
//!   ```text
//!   offset | bytes | facet | field
//!   -------+-------+-------+---------------------------------------------
//!     0    |  72   | M-S-P-Φ-Σ | inline FieldCell-v2 prefix (verbatim)
//!    72    |   8   |  E    | epoch_and_entropy_band (u64 packed)
//!    80    |   4   |  C    | cap_handle (u32 → CapTable index)
//!    84    |   4   |  K    | kan_band_handle (u32 → KanBandTable index)
//!   -------+-------+-------+---------------------------------------------
//!         |   88  |       | TOTAL
//!   ```
//!
//!   The `epoch_and_entropy_band` u64 packs :
//!     bits  0..=47  : epoch_lo (48-bit monotone audit-clock low bits)
//!     bits 48..=55  : entropy_band (8-bit RG-flow band selector, 0..=255)
//!     bits 56..=63  : reserved (must be 0 — future RG-tier bits)
//!
//! § INVARIANTS
//!   - `core::mem::size_of::<FieldCellV3>() == 88` (verified-by-test).
//!   - `core::mem::align_of::<FieldCellV3>() == 8` (std430).
//!   - The cell is `Copy + Default` — Default produces an "air cell v3" with
//!     v2-AIR prefix + epoch=0 + entropy_band=0 + cap_handle=NULL +
//!     kan_band_handle=NULL.
//!   - All multi-byte fields are little-endian (std430 GPU upload).
//!   - The first 72 bytes are byte-identical to [`crate::field_cell::FieldCell`]
//!     so legacy v2 consumers can read a v3 cell as v2 via prefix-truncation.
//!
//! § BACKWARD-COMPAT SHIM
//!   - [`v2_to_v3`] : promotes a v2 cell to v3 with default E + NULL handles.
//!   - [`v3_to_v2`] : truncates a v3 cell to v2 (E + C + K facets dropped).
//!   - [`FieldCellV3::as_v2`] : zero-copy borrow of the v2-shaped prefix.
//!
//! § PRIME-DIRECTIVE-ALIGNMENT
//!   - The cap_handle indexes the [`CapTable`] which is the canonical
//!     ADCS-tier capability surface. A NULL handle ([`CAP_HANDLE_NULL`])
//!     means the cell has no capability claim ; consumers must consult
//!     the in-cell Σ-cache for consent decisions.
//!   - The kan_band_handle indexes the [`KanBandTable`] of KAN-substrate
//!     spline coefficients used by the runtime evaluator.
//!   - The epoch field provides per-cell audit ordering for ADCS replay.
//!
//! § SPEC REFERENCES
//!   - `specs/30_SUBSTRATE_v3.csl` § CELL-SHAPE-EXTENSION (88B layout).
//!   - `specs/35_DECAPLENOPTIC.csl` § STORAGE-LAYOUT (FieldCellV3 + tables).
//!   - `specs/30_SUBSTRATE_v2.csl` § FACETS (v2 prefix carried through).

use crate::field_cell::FieldCell;

// ───────────────────────────────────────────────────────────────────────
// § Bit-packing constants for the E facet.
// ───────────────────────────────────────────────────────────────────────

/// Width of the epoch field inside `epoch_and_entropy_band` (low 48 bits).
pub const EPOCH_BITS: u32 = 48;

/// Mask for the 48-bit epoch payload.
pub const EPOCH_MASK: u64 = (1u64 << EPOCH_BITS) - 1;

/// Bit-offset of the entropy-band byte inside `epoch_and_entropy_band`.
pub const ENTROPY_BAND_SHIFT: u32 = 48;

/// Mask for the 8-bit entropy-band field after right-shift.
pub const ENTROPY_BAND_MASK: u64 = 0xFF;

/// Bit-offset of the reserved byte (high 8 bits ; must be zero).
pub const RESERVED_E_SHIFT: u32 = 56;

/// Mask for the high reserved byte (must remain zero in valid cells).
pub const RESERVED_E_MASK: u64 = 0xFF;

// ───────────────────────────────────────────────────────────────────────
// § C / K facet null sentinels.
// ───────────────────────────────────────────────────────────────────────

/// "No capability claim." Default cap_handle for fresh / air cells.
pub const CAP_HANDLE_NULL: u32 = 0;

/// "No KAN-band attached." Default kan_band_handle for fresh / air cells.
pub const KAN_BAND_HANDLE_NULL: u32 = 0;

// ───────────────────────────────────────────────────────────────────────
// § FieldCellV3 — 88B packed struct.
// ───────────────────────────────────────────────────────────────────────

/// 88-byte std430-aligned ADCS Ω-field cell : v2 prefix + E + C + K.
///
/// § FIELD-ORDER
///   The struct uses `#[repr(C, align(8))]` so the byte-order matches the
///   bit-layout documented at the module level verbatim. The first 72 bytes
///   are a byte-identical embedding of [`FieldCell`] (v2). The trailing 16
///   bytes carry the three ADCS facets E + C + K.
///
/// § DESIGN-NOTE
///   The cell is exactly 88 bytes — divisible by 8 — which means a contiguous
///   `&[FieldCellV3]` slice has zero per-row padding under std430. The 72→88
///   step is the minimal addition that lets ADCS (audit-clock + capabilities
///   + KAN-band binding) coexist in the dense-tier without bumping cells off
///   the natural 8-byte alignment boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C, align(8))]
pub struct FieldCellV3 {
    /// Bytes  0..=71 : verbatim v2 prefix (M, S, P, Φ + low-Σ).
    pub v2: FieldCell,

    /// Bytes 72..=79 : Facet **E** — epoch (low 48 bits) + entropy_band
    /// (next 8 bits) + reserved (high 8 bits, must be 0).
    pub epoch_and_entropy_band: u64,

    /// Bytes 80..=83 : Facet **C** — capability handle (gen-ref into
    /// [`CapTable`]). [`CAP_HANDLE_NULL`] = no capability claim.
    pub cap_handle: u32,

    /// Bytes 84..=87 : Facet **K** — KAN-band handle (gen-ref into
    /// [`KanBandTable`]). [`KAN_BAND_HANDLE_NULL`] = no KAN binding.
    pub kan_band_handle: u32,
}

impl FieldCellV3 {
    /// "Air cell v3" — v2-AIR prefix + zero ADCS extension fields.
    pub const AIR: FieldCellV3 = FieldCellV3 {
        v2: FieldCell::AIR,
        epoch_and_entropy_band: 0,
        cap_handle: CAP_HANDLE_NULL,
        kan_band_handle: KAN_BAND_HANDLE_NULL,
    };

    /// Construct a fresh v3 cell from a v2 cell with default ADCS extensions.
    #[inline]
    #[must_use]
    pub const fn from_v2(v2: FieldCell) -> Self {
        Self {
            v2,
            epoch_and_entropy_band: 0,
            cap_handle: CAP_HANDLE_NULL,
            kan_band_handle: KAN_BAND_HANDLE_NULL,
        }
    }

    /// Truncate the v3 cell back to a v2 cell (E + C + K dropped).
    #[inline]
    #[must_use]
    pub const fn to_v2(&self) -> FieldCell {
        self.v2
    }

    /// Zero-copy borrow of the v2-shaped prefix.
    #[inline]
    #[must_use]
    pub const fn as_v2(&self) -> &FieldCell {
        &self.v2
    }

    /// Mutable borrow of the v2-shaped prefix.
    #[inline]
    pub fn as_v2_mut(&mut self) -> &mut FieldCell {
        &mut self.v2
    }

    // ── E-facet accessors (epoch + entropy band) ────────────────────

    /// Read the 48-bit epoch (audit-clock low bits).
    #[inline]
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch_and_entropy_band & EPOCH_MASK
    }

    /// Set the 48-bit epoch. High bits beyond 48 are clipped silently.
    #[inline]
    pub fn set_epoch(&mut self, epoch: u64) {
        let masked = epoch & EPOCH_MASK;
        self.epoch_and_entropy_band = (self.epoch_and_entropy_band & !EPOCH_MASK) | masked;
    }

    /// Read the 8-bit entropy-band selector.
    #[inline]
    #[must_use]
    pub const fn entropy_band(&self) -> u8 {
        ((self.epoch_and_entropy_band >> ENTROPY_BAND_SHIFT) & ENTROPY_BAND_MASK) as u8
    }

    /// Set the 8-bit entropy-band selector.
    #[inline]
    pub fn set_entropy_band(&mut self, band: u8) {
        let cleared = self.epoch_and_entropy_band
            & !(ENTROPY_BAND_MASK << ENTROPY_BAND_SHIFT);
        let inserted = (u64::from(band) & ENTROPY_BAND_MASK) << ENTROPY_BAND_SHIFT;
        self.epoch_and_entropy_band = cleared | inserted;
    }

    /// Read the high-byte reserved bits (must be zero in valid cells).
    #[inline]
    #[must_use]
    pub const fn reserved_e(&self) -> u8 {
        ((self.epoch_and_entropy_band >> RESERVED_E_SHIFT) & RESERVED_E_MASK) as u8
    }

    /// Pack the E facet from raw components.
    #[inline]
    #[must_use]
    pub const fn pack_e(epoch: u64, entropy_band: u8) -> u64 {
        (epoch & EPOCH_MASK)
            | (((entropy_band as u64) & ENTROPY_BAND_MASK) << ENTROPY_BAND_SHIFT)
    }

    /// Unpack the E facet into (epoch, entropy_band) — drops reserved bits.
    #[inline]
    #[must_use]
    pub const fn unpack_e(packed: u64) -> (u64, u8) {
        let epoch = packed & EPOCH_MASK;
        let band = ((packed >> ENTROPY_BAND_SHIFT) & ENTROPY_BAND_MASK) as u8;
        (epoch, band)
    }

    /// Atomically advance the epoch counter by one (saturating at 48-bit max).
    #[inline]
    pub fn tick_epoch(&mut self) {
        let cur = self.epoch();
        if cur < EPOCH_MASK {
            self.set_epoch(cur + 1);
        }
    }

    // ── C-facet accessors (capability handle) ────────────────────

    /// True iff this cell has a non-null capability handle.
    #[inline]
    #[must_use]
    pub const fn has_cap(&self) -> bool {
        self.cap_handle != CAP_HANDLE_NULL
    }

    /// Set the capability handle.
    #[inline]
    pub fn set_cap_handle(&mut self, handle: u32) {
        self.cap_handle = handle;
    }

    /// Clear the capability handle (cell becomes uncapped).
    #[inline]
    pub fn clear_cap(&mut self) {
        self.cap_handle = CAP_HANDLE_NULL;
    }

    // ── K-facet accessors (KAN-band handle) ──────────────────────

    /// True iff this cell has a non-null KAN-band handle.
    #[inline]
    #[must_use]
    pub const fn has_kan_band(&self) -> bool {
        self.kan_band_handle != KAN_BAND_HANDLE_NULL
    }

    /// Set the KAN-band handle.
    #[inline]
    pub fn set_kan_band_handle(&mut self, handle: u32) {
        self.kan_band_handle = handle;
    }

    /// Clear the KAN-band handle (cell unbinds from any KAN-substrate).
    #[inline]
    pub fn clear_kan_band(&mut self) {
        self.kan_band_handle = KAN_BAND_HANDLE_NULL;
    }
}

impl Default for FieldCellV3 {
    fn default() -> Self {
        Self::AIR
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Migration shims : v2 ⇄ v3 round-trip helpers.
// ───────────────────────────────────────────────────────────────────────

/// Promote a v2 cell to v3 with default ADCS-extension fields.
///
/// This is the canonical reader-side promotion path : a legacy v2 cell read
/// from an older substrate file is widened to v3 with epoch=0 + band=0 +
/// NULL cap-handle + NULL kan-band-handle. Re-attaching capabilities or KAN
/// bands is the responsibility of the migration driver.
#[inline]
#[must_use]
pub const fn v2_to_v3(cell_v2: FieldCell) -> FieldCellV3 {
    FieldCellV3::from_v2(cell_v2)
}

/// Truncate a v3 cell to v2 (LOSSY — drops E + C + K facets).
///
/// This is the canonical writer-side downgrade path used by legacy consumers
/// that only understand the 72B v2 layout. The ADCS-extension fields are
/// silently dropped ; consumers that need them must read the v3 cell directly.
#[inline]
#[must_use]
pub const fn v3_to_v2(cell_v3: FieldCellV3) -> FieldCell {
    cell_v3.v2
}

// ───────────────────────────────────────────────────────────────────────
// § CapTable — capability handle table (struct shell ; impls in W-S-CORE-3).
// ───────────────────────────────────────────────────────────────────────

/// One row of the [`CapTable`] : the 16-byte ADCS-tier capability descriptor.
///
/// § ROLE
///   Each row is referenced by a [`FieldCellV3::cap_handle`] u32 generation-
///   reference. The full impl (allocation + revocation + audit hooks) lands
///   in the W-S-CORE-3 slice ; this shell defines the struct + trivial
///   accessors so callers can compile against the surface today.
///
/// § BYTE-LAYOUT (std430, 16B total, 8-byte alignment)
///
///   ```text
///   offset | bytes | field
///   -------+-------+-------------------------------------------------
///     0    |   8   | sovereign_id : u64 (canonical Sovereign-handle)
///     8    |   4   | rights_mask  : u32 (bit-set of granted op-classes)
///    12    |   4   | gen_counter  : u32 (generation-counter ; ABA defeat)
///   -------+-------+-------------------------------------------------
///         |  16   | TOTAL
///   ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C, align(8))]
pub struct CapTableRow {
    /// Bytes 0..=7 : Canonical Sovereign-handle (u64).
    pub sovereign_id: u64,
    /// Bytes 8..=11 : Rights-mask bit-set (u32).
    pub rights_mask: u32,
    /// Bytes 12..=15 : Generation-counter for ABA defense (u32).
    pub gen_counter: u32,
}

/// CapTable struct shell : storage for capability rows referenced by
/// [`FieldCellV3::cap_handle`] handles.
///
/// § ROLE
///   The full impl (allocation + revocation + audit) lands in W-S-CORE-3.
///   This shell provides the storage vector + a `len` accessor so dependents
///   can compile-check against the public surface.
#[derive(Debug, Clone, Default)]
pub struct CapTable {
    /// Backing storage for capability rows. Index 0 is reserved as the NULL
    /// handle sentinel ; actual rows start at index 1.
    pub rows: Vec<CapTableRow>,
}

impl CapTable {
    /// Construct an empty cap-table with the index-0 NULL sentinel pre-set.
    #[must_use]
    pub fn new() -> Self {
        let mut t = Self { rows: Vec::new() };
        t.rows.push(CapTableRow::default()); // index 0 = NULL sentinel
        t
    }

    /// Number of populated rows (including the NULL sentinel).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True iff only the NULL sentinel is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.len() <= 1
    }

    /// Read a row by handle. `handle == CAP_HANDLE_NULL` returns `None`.
    #[must_use]
    pub fn get(&self, handle: u32) -> Option<&CapTableRow> {
        if handle == CAP_HANDLE_NULL {
            None
        } else {
            self.rows.get(handle as usize)
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § KanBandTable — KAN-band handle table (struct shell).
// ───────────────────────────────────────────────────────────────────────

/// One row of the [`KanBandTable`] : 32 bytes describing a KAN-substrate
/// spline-band binding referenced by [`FieldCellV3::kan_band_handle`].
///
/// § BYTE-LAYOUT (std430, 32B total, 8-byte alignment)
///
///   ```text
///   offset | bytes | field
///   -------+-------+-------------------------------------------------
///     0    |   4   | band_id          : u32 (which KAN band)
///     4    |   4   | spline_degree    : u32 (degree of B-spline basis)
///     8    |   8   | coeff_handle_lo  : u64 (low half of coeff-storage ref)
///    16    |   8   | coeff_handle_hi  : u64 (high half of coeff-storage ref)
///    24    |   4   | gen_counter      : u32 (generation, ABA defeat)
///    28    |   4   | flags            : u32 (KAN runtime flags)
///   -------+-------+-------------------------------------------------
///         |  32   | TOTAL
///   ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C, align(8))]
pub struct KanBandRow {
    /// Bytes 0..=3 : Band identifier (u32).
    pub band_id: u32,
    /// Bytes 4..=7 : B-spline basis degree (u32).
    pub spline_degree: u32,
    /// Bytes 8..=15 : Coefficient-storage handle low half (u64).
    pub coeff_handle_lo: u64,
    /// Bytes 16..=23 : Coefficient-storage handle high half (u64).
    pub coeff_handle_hi: u64,
    /// Bytes 24..=27 : Generation-counter (u32).
    pub gen_counter: u32,
    /// Bytes 28..=31 : KAN runtime flags (u32).
    pub flags: u32,
}

/// KanBandTable struct shell : storage for KAN-band rows referenced by
/// [`FieldCellV3::kan_band_handle`].
///
/// § ROLE
///   The full impl (band allocation + coefficient binding + KAN-runtime
///   evaluation) lands in W-S-CORE-3. This shell provides the storage
///   vector + accessors.
#[derive(Debug, Clone, Default)]
pub struct KanBandTable {
    /// Backing storage. Index 0 is the NULL sentinel.
    pub rows: Vec<KanBandRow>,
}

impl KanBandTable {
    /// Construct an empty KAN-band table with the NULL sentinel pre-set.
    #[must_use]
    pub fn new() -> Self {
        let mut t = Self { rows: Vec::new() };
        t.rows.push(KanBandRow::default()); // index 0 = NULL sentinel
        t
    }

    /// Number of populated rows (including NULL sentinel).
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True iff only the NULL sentinel is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.len() <= 1
    }

    /// Read a row by handle. `handle == KAN_BAND_HANDLE_NULL` returns `None`.
    #[must_use]
    pub fn get(&self, handle: u32) -> Option<&KanBandRow> {
        if handle == KAN_BAND_HANDLE_NULL {
            None
        } else {
            self.rows.get(handle as usize)
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Tests : ≥ 8 covering layout, packing, and migration shims.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        v2_to_v3, v3_to_v2, CapTable, CapTableRow, FieldCellV3, KanBandRow, KanBandTable,
        CAP_HANDLE_NULL, ENTROPY_BAND_MASK, ENTROPY_BAND_SHIFT, EPOCH_BITS, EPOCH_MASK,
        KAN_BAND_HANDLE_NULL,
    };
    use crate::field_cell::FieldCell;

    // ── Layout invariants — the load-bearing 88B contract ───────────

    #[test]
    fn v3_size_is_88_bytes() {
        assert_eq!(core::mem::size_of::<FieldCellV3>(), 88);
    }

    #[test]
    fn v3_alignment_is_8_bytes() {
        assert_eq!(core::mem::align_of::<FieldCellV3>(), 8);
    }

    #[test]
    fn v3_layout_offsets_match_spec() {
        // Manual offset verification : write known values, re-read bytes.
        let mut cell = FieldCellV3::default();
        cell.v2.m_pga_or_region = 0xAAAA_BBBB_CCCC_DDDD;
        cell.v2.pattern_handle = 0xDEAD_BEEF;
        cell.v2.sigma_consent_bits = 0xFEED_FACE;
        cell.epoch_and_entropy_band = 0x1122_3344_5566_7788;
        cell.cap_handle = 0x1357_9BDF;
        cell.kan_band_handle = 0x2468_ACE0;

        let bytes: &[u8; 88] =
            unsafe { &*((&cell as *const FieldCellV3).cast::<[u8; 88]>()) };

        // v2 prefix : m_pga_or_region @ offset 0.
        assert_eq!(
            u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            0xAAAA_BBBB_CCCC_DDDD
        );
        // v2 prefix : pattern_handle @ offset 64.
        assert_eq!(
            u32::from_le_bytes(bytes[64..68].try_into().unwrap()),
            0xDEAD_BEEF
        );
        // v2 prefix : sigma_consent_bits @ offset 68.
        assert_eq!(
            u32::from_le_bytes(bytes[68..72].try_into().unwrap()),
            0xFEED_FACE
        );
        // E facet : epoch_and_entropy_band @ offset 72.
        assert_eq!(
            u64::from_le_bytes(bytes[72..80].try_into().unwrap()),
            0x1122_3344_5566_7788
        );
        // C facet : cap_handle @ offset 80.
        assert_eq!(
            u32::from_le_bytes(bytes[80..84].try_into().unwrap()),
            0x1357_9BDF
        );
        // K facet : kan_band_handle @ offset 84.
        assert_eq!(
            u32::from_le_bytes(bytes[84..88].try_into().unwrap()),
            0x2468_ACE0
        );
    }

    // ── Default values ──────────────────────────────────────────────

    #[test]
    fn v3_default_is_air_with_zero_extensions() {
        let c = FieldCellV3::default();
        // v2 prefix is AIR.
        assert_eq!(c.v2, FieldCell::AIR);
        // ADCS extensions are zero.
        assert_eq!(c.epoch_and_entropy_band, 0);
        assert_eq!(c.cap_handle, CAP_HANDLE_NULL);
        assert_eq!(c.kan_band_handle, KAN_BAND_HANDLE_NULL);
        assert!(!c.has_cap());
        assert!(!c.has_kan_band());
        assert_eq!(c.epoch(), 0);
        assert_eq!(c.entropy_band(), 0);
    }

    // ── v2 ⇄ v3 migration shims ──────────────────────────────────────

    #[test]
    fn v2_to_v3_preserves_v2_prefix() {
        let mut v2 = FieldCell::default();
        v2.density = 1.5;
        v2.velocity = [1.0, 0.0, 0.0];
        v2.pattern_handle = 99;
        v2.sigma_consent_bits = 0xCAFE;

        let v3 = v2_to_v3(v2);
        assert_eq!(v3.v2, v2);
        // ADCS extensions are zeroed on promotion.
        assert_eq!(v3.epoch(), 0);
        assert_eq!(v3.cap_handle, CAP_HANDLE_NULL);
        assert_eq!(v3.kan_band_handle, KAN_BAND_HANDLE_NULL);
    }

    #[test]
    fn v3_to_v2_truncates_extensions() {
        let mut v3 = FieldCellV3::default();
        v3.v2.density = 2.5;
        v3.v2.pattern_handle = 7;
        v3.set_epoch(12345);
        v3.set_entropy_band(42);
        v3.cap_handle = 100;
        v3.kan_band_handle = 200;

        let v2 = v3_to_v2(v3);
        // v2 prefix preserved ; ADCS extensions silently dropped.
        assert_eq!(v2.density, 2.5);
        assert_eq!(v2.pattern_handle, 7);
        assert_eq!(v2, v3.v2);
    }

    #[test]
    fn v2_to_v3_to_v2_roundtrip_is_identity() {
        let mut v2 = FieldCell::default();
        v2.density = 3.14;
        v2.velocity = [0.0, 1.0, 0.0];
        v2.vorticity = [0.5, 0.5, 0.5];
        v2.enthalpy = 9.8;
        v2.multivec_dynamics_lo = 0xDEAD_BEEF_CAFE_BABE;
        v2.radiance_probe_lo = 0x1111_2222_3333_4444;
        v2.radiance_probe_hi = 0x5555_6666_7777_8888;
        v2.pattern_handle = 42;
        v2.sigma_consent_bits = 0xBEEF;

        let v3 = v2_to_v3(v2);
        let v2_back = v3_to_v2(v3);
        assert_eq!(v2, v2_back);
    }

    #[test]
    fn v3_as_v2_is_zero_copy_borrow() {
        let mut v3 = FieldCellV3::default();
        v3.v2.density = 1.0;
        let borrowed: &FieldCell = v3.as_v2();
        assert_eq!(borrowed.density, 1.0);
    }

    // ── E-facet packing ──────────────────────────────────────────────

    #[test]
    fn e_facet_pack_unpack_roundtrip() {
        let epoch = 0x0000_DEAD_BEEF_CAFE & EPOCH_MASK;
        let band: u8 = 0x37;
        let packed = FieldCellV3::pack_e(epoch, band);
        let (e_back, b_back) = FieldCellV3::unpack_e(packed);
        assert_eq!(e_back, epoch);
        assert_eq!(b_back, band);
    }

    #[test]
    fn e_facet_set_epoch_clips_to_48_bits() {
        let mut c = FieldCellV3::default();
        c.set_epoch(u64::MAX);
        assert_eq!(c.epoch(), EPOCH_MASK);
        // Other bits in the u64 must remain at their default (0).
        let high_byte = (c.epoch_and_entropy_band >> 56) & 0xFF;
        assert_eq!(high_byte, 0);
    }

    #[test]
    fn e_facet_set_entropy_band_preserves_epoch() {
        let mut c = FieldCellV3::default();
        c.set_epoch(0xCAFE_BABE);
        c.set_entropy_band(0xAB);
        assert_eq!(c.epoch(), 0xCAFE_BABE);
        assert_eq!(c.entropy_band(), 0xAB);
    }

    #[test]
    fn e_facet_set_epoch_preserves_entropy_band() {
        let mut c = FieldCellV3::default();
        c.set_entropy_band(0x5A);
        c.set_epoch(0x1234_5678);
        assert_eq!(c.entropy_band(), 0x5A);
        assert_eq!(c.epoch(), 0x1234_5678);
    }

    #[test]
    fn e_facet_tick_epoch_increments() {
        let mut c = FieldCellV3::default();
        c.set_epoch(99);
        c.tick_epoch();
        assert_eq!(c.epoch(), 100);
    }

    #[test]
    fn e_facet_tick_epoch_saturates_at_max() {
        let mut c = FieldCellV3::default();
        c.set_epoch(EPOCH_MASK);
        c.tick_epoch();
        // Saturates ; does not overflow into entropy_band byte.
        assert_eq!(c.epoch(), EPOCH_MASK);
        assert_eq!(c.entropy_band(), 0);
    }

    #[test]
    fn epoch_bits_constant_is_48() {
        assert_eq!(EPOCH_BITS, 48);
        assert_eq!(EPOCH_MASK, (1u64 << 48) - 1);
    }

    #[test]
    fn entropy_band_shift_constant_is_48() {
        assert_eq!(ENTROPY_BAND_SHIFT, 48);
        assert_eq!(ENTROPY_BAND_MASK, 0xFF);
    }

    // ── C-facet (cap-handle) helpers ────────────────────────────────

    #[test]
    fn cap_handle_set_and_clear() {
        let mut c = FieldCellV3::default();
        assert!(!c.has_cap());
        c.set_cap_handle(42);
        assert!(c.has_cap());
        assert_eq!(c.cap_handle, 42);
        c.clear_cap();
        assert!(!c.has_cap());
        assert_eq!(c.cap_handle, CAP_HANDLE_NULL);
    }

    // ── K-facet (kan-band-handle) helpers ───────────────────────────

    #[test]
    fn kan_band_handle_set_and_clear() {
        let mut c = FieldCellV3::default();
        assert!(!c.has_kan_band());
        c.set_kan_band_handle(7);
        assert!(c.has_kan_band());
        assert_eq!(c.kan_band_handle, 7);
        c.clear_kan_band();
        assert!(!c.has_kan_band());
        assert_eq!(c.kan_band_handle, KAN_BAND_HANDLE_NULL);
    }

    // ── Backward-compat : v2 readers see v3's first 72 bytes as v2 ──

    #[test]
    fn v3_first_72_bytes_match_v2_layout() {
        let mut v3 = FieldCellV3::default();
        v3.v2.m_pga_or_region = 0xAAAA_5555_AAAA_5555;
        v3.v2.density = 7.5;
        v3.v2.velocity = [1.0, 2.0, 3.0];
        v3.v2.pattern_handle = 88;
        v3.v2.sigma_consent_bits = 0xCAFE;
        // Set the ADCS extensions to non-zero too — they must not bleed back.
        v3.set_epoch(0x1234);
        v3.cap_handle = 99;
        v3.kan_band_handle = 100;

        // Reinterpret the first 72 bytes as a FieldCell.
        let v3_bytes: &[u8; 88] =
            unsafe { &*((&v3 as *const FieldCellV3).cast::<[u8; 88]>()) };

        // A direct byte-compare against a freshly-constructed v2 with the
        // same prefix must match the first 72 bytes.
        let v2_check = v3.v2;
        let v2_bytes: &[u8; 72] =
            unsafe { &*((&v2_check as *const FieldCell).cast::<[u8; 72]>()) };
        assert_eq!(&v3_bytes[..72], v2_bytes);
    }

    // ── LE byte-shape : verify entire 88B is LE-encoded ─────────────

    #[test]
    fn v3_le_byte_shape_for_extensions() {
        let mut c = FieldCellV3::default();
        c.epoch_and_entropy_band = 0x0123_4567_89AB_CDEF;
        c.cap_handle = 0x1234_5678;
        c.kan_band_handle = 0x90AB_CDEF;

        let bytes: &[u8; 88] =
            unsafe { &*((&c as *const FieldCellV3).cast::<[u8; 88]>()) };

        // E facet @ 72..80 : little-endian u64.
        assert_eq!(bytes[72], 0xEF);
        assert_eq!(bytes[73], 0xCD);
        assert_eq!(bytes[74], 0xAB);
        assert_eq!(bytes[75], 0x89);
        assert_eq!(bytes[76], 0x67);
        assert_eq!(bytes[77], 0x45);
        assert_eq!(bytes[78], 0x23);
        assert_eq!(bytes[79], 0x01);
        // C facet @ 80..84 : little-endian u32.
        assert_eq!(bytes[80], 0x78);
        assert_eq!(bytes[81], 0x56);
        assert_eq!(bytes[82], 0x34);
        assert_eq!(bytes[83], 0x12);
        // K facet @ 84..88 : little-endian u32.
        assert_eq!(bytes[84], 0xEF);
        assert_eq!(bytes[85], 0xCD);
        assert_eq!(bytes[86], 0xAB);
        assert_eq!(bytes[87], 0x90);
    }

    // ── Copy + Default ──────────────────────────────────────────────

    #[test]
    fn v3_is_copy() {
        let a = FieldCellV3::default();
        let b = a; // Copy
        assert_eq!(a, b);
    }

    #[test]
    fn v3_air_constant_matches_default() {
        assert_eq!(FieldCellV3::AIR, FieldCellV3::default());
    }

    // ── CapTable shell ──────────────────────────────────────────────

    #[test]
    fn cap_table_new_has_null_sentinel() {
        let t = CapTable::new();
        assert_eq!(t.len(), 1); // NULL sentinel at index 0
        assert!(t.is_empty()); // is_empty == only sentinel present
        assert!(t.get(CAP_HANDLE_NULL).is_none());
    }

    #[test]
    fn cap_table_row_size_is_16() {
        assert_eq!(core::mem::size_of::<CapTableRow>(), 16);
        assert_eq!(core::mem::align_of::<CapTableRow>(), 8);
    }

    #[test]
    fn cap_table_get_real_row() {
        let mut t = CapTable::new();
        t.rows.push(CapTableRow {
            sovereign_id: 0xABCD,
            rights_mask: 0b1111,
            gen_counter: 1,
        });
        let r = t.get(1).unwrap();
        assert_eq!(r.sovereign_id, 0xABCD);
        assert_eq!(r.rights_mask, 0b1111);
    }

    // ── KanBandTable shell ──────────────────────────────────────────

    #[test]
    fn kan_band_table_new_has_null_sentinel() {
        let t = KanBandTable::new();
        assert_eq!(t.len(), 1);
        assert!(t.is_empty());
        assert!(t.get(KAN_BAND_HANDLE_NULL).is_none());
    }

    #[test]
    fn kan_band_row_size_is_32() {
        assert_eq!(core::mem::size_of::<KanBandRow>(), 32);
        assert_eq!(core::mem::align_of::<KanBandRow>(), 8);
    }

    #[test]
    fn kan_band_table_get_real_row() {
        let mut t = KanBandTable::new();
        t.rows.push(KanBandRow {
            band_id: 5,
            spline_degree: 3,
            coeff_handle_lo: 0xAA,
            coeff_handle_hi: 0xBB,
            gen_counter: 1,
            flags: 0,
        });
        let r = t.get(1).unwrap();
        assert_eq!(r.band_id, 5);
        assert_eq!(r.spline_degree, 3);
    }

    // ── from_v2 / to_v2 const-fn paths ───────────────────────────────

    #[test]
    fn from_v2_const_fn_works() {
        const C: FieldCellV3 = FieldCellV3::from_v2(FieldCell::AIR);
        assert_eq!(C.v2, FieldCell::AIR);
        assert_eq!(C.epoch_and_entropy_band, 0);
    }

    #[test]
    fn air_constant_const_fn_works() {
        const A: FieldCellV3 = FieldCellV3::AIR;
        assert_eq!(A, FieldCellV3::default());
    }
}
