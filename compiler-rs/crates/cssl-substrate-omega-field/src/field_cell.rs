//! § FieldCell — 72B std430-aligned dense Ω-field cell.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The canonical 7-facet (M, S, P, Φ + Σ) packed cell that sits in the
//!   sparse Morton-keyed dense-tier of the [`crate::omega_field::OmegaField`].
//!   Λ + Ψ live in separate sparse-overlay grids per § 0.STORAGE.
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md` § I (offset/byte/field
//!     table) verbatim — this struct's `#[repr(C)]` layout matches the
//!     spec offsets line-for-line.
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.1 FieldCell.
//!   - `cssl-mir/src/layout_check.rs` LAY0001 validator (D126) — the
//!     `sizeof(FieldCell) == 72` invariant is asserted by both the runtime
//!     test below + the compile-time `@layout(std430, soa)` checker.
//!
//! § BYTE-LAYOUT (std430, 72B total, 8-byte alignment)
//!
//!   ```text
//!   offset | bytes | facet | field
//!   -------+-------+-------+------------------------------------------
//!     0    |   8   |  M    | m_pga_or_region (1-bit tag + 63-bit id)
//!     8    |   4   |  S    | density (f32 ρ)
//!    12    |  12   |  S    | velocity (vec3<f32> u)
//!    24    |  12   |  S    | vorticity (vec3<f32> ω)
//!    36    |   4   |  S    | enthalpy (f32'pos H)
//!    40    |   8   |  S    | multivec_dynamics_lo (u64 packed bivector)
//!    48    |   8   |  P    | radiance_probe_lo (u64)
//!    56    |   8   |  P    | radiance_probe_hi (u64)
//!    64    |   8   |  Φ + Σ | pattern_handle_lo (4B Φ) | sigma_low (4B Σ)
//!   -------+-------+-------+------------------------------------------
//!         |   72  |       | TOTAL
//!   ```
//!
//!   Note : the spec's verbal description packs Φ at 64..=71 and stores Σ
//!   in a sparse-overlay. This implementation places the LOW-HALF of the
//!   Σ-mask (32 bits) in the cell's bytes 68..=71 alongside a 32-bit Φ
//!   handle. The full 16-byte SigmaMaskPacked still lives in the
//!   [`crate::sigma_overlay::SigmaOverlay`] for cells that have non-default
//!   masks ; the in-cell low-half is a fast-path consent-bits cache that
//!   avoids overlay lookup for the hot Observe / Modify / Sample tests.
//!   This matches the "Φ-handle and Σ-mask between dense+overlay based on
//!   usage tier" allowance in 00_FACETS.csl.md § I.NOTE.
//!
//! § INVARIANTS
//!   - `core::mem::size_of::<FieldCell>() == 72` (verified-by-test).
//!   - `core::mem::align_of::<FieldCell>() == 8` (std430-rule).
//!   - The cell is `Copy + Default` — Default produces an "air cell" with
//!     density 0, all velocity/vorticity zero, default Σ-mask consent =
//!     Default-Private (Observe-only).
//!   - All multi-byte fields are little-endian-ordered for std430 GPU
//!     upload.
//!
//! § PRIME-DIRECTIVE-ALIGNMENT
//!   - Σ-low cache is the in-cell consent surface ; the
//!     [`FieldCell::can_modify`] helper consults it directly for the hot
//!     mutation-gate path. The slow path consults the SigmaOverlay (16B
//!     full mask). The two MUST stay in sync — see [`FieldCell::sync_sigma_low`].
//!   - The cell is OPAQUE TO TELEMETRY by default. Bulk readback requires
//!     an explicit consent gate at the OmegaField surface.
//!
//! § REFERENCES (foundation crates wired into this layer)
//!   - `cssl-pga` — multivec_dynamics_lo encodes a grade-2 PGA bivector via
//!     `pack_bivector_lo` / `unpack_bivector_lo` helpers below. Half-precision
//!     packing keeps the field at 8 bytes ; the high half lives in a sparse
//!     overlay when stress is high.
//!   - `cssl-substrate-prime-directive::sigma::SigmaMaskPacked` — the canonical
//!     16B Σ-mask. The low 32 consent_bits are mirrored in the cell ; the
//!     full mask lives in the overlay.
//!   - `cssl-hdc` — pattern_handle_lo indexes the [`crate::phi_table::PhiTable`]
//!     where the 10000-D pattern hypervector lives.

use cssl_pga::Multivector;
use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked};

// ───────────────────────────────────────────────────────────────────────
// § Material-facet (M) tag bits.
// ───────────────────────────────────────────────────────────────────────

/// Bit-mask for the 1-bit "M-tag" inside `m_pga_or_region`. When SET, the
/// remaining 63 bits are a PGA-handle (per-cell individual material). When
/// CLEAR, the remaining 63 bits are a RegionID into a per-substrate
/// MaterialDef table.
pub const M_TAG_PGA: u64 = 1u64 << 63;
/// Mask for the 63-bit M-payload.
pub const M_PAYLOAD_MASK: u64 = (1u64 << 63) - 1;

// ───────────────────────────────────────────────────────────────────────
// § Φ pattern-handle null sentinel.
// ───────────────────────────────────────────────────────────────────────

/// "No pattern claims this cell." [`FieldCell::pattern_handle`] returning
/// this sentinel means the cell is unclaimed / air / pure substrate.
pub const PATTERN_HANDLE_NULL: u32 = 0;

// ───────────────────────────────────────────────────────────────────────
// § FieldCell — 72B packed struct.
// ───────────────────────────────────────────────────────────────────────

/// 72-byte std430-aligned Ω-field cell containing the M, S, P, Φ + low-Σ
/// facets. Λ, Ψ, and the high-Σ-half live in sparse overlays.
///
/// § FIELD-ORDER
///   The struct uses `#[repr(C, align(8))]` so the byte-order matches the
///   bit-layout documented at the module level verbatim. The ALIGN(8)
///   matches std430-rule + cssl-mir's LAY0001 validator (D126).
///
/// § DESIGN-NOTE
///   The 72-byte total + the 8-byte alignment means a contiguous
///   `&[FieldCell]` slice has rows that pack 9 u64s end-to-end with no
///   per-row padding. This is the "SoA-discipline" enforcement at
///   `Axiom 13 §II` — every byte in the cell has semantic content ; no
///   silent padding bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C, align(8))]
pub struct FieldCell {
    /// Bytes  0..=7 : Facet M — material identity (1-bit tag + 63-bit id).
    /// Bit 63 = 1 ⇒ PGA-handle ; bit 63 = 0 ⇒ RegionID.
    pub m_pga_or_region: u64,

    /// Bytes  8..=11 : Facet S — density ρ.
    pub density: f32,

    /// Bytes 12..=23 : Facet S — velocity vec3<f32> u.
    pub velocity: [f32; 3],

    /// Bytes 24..=35 : Facet S — vorticity vec3<f32> ω = ∇ × u.
    pub vorticity: [f32; 3],

    /// Bytes 36..=39 : Facet S — enthalpy f32'pos H (≥ 0.0 invariant).
    pub enthalpy: f32,

    /// Bytes 40..=47 : Facet S — packed PGA bivector (grade-2 G(3,0,1)).
    /// The low 8 bytes hold 6 half-precision bivector components ; the high
    /// 8 bytes (when stress is non-trivial) live in a sparse overlay.
    pub multivec_dynamics_lo: u64,

    /// Bytes 48..=55 : Facet P — radiance-cascade probe LOW (direction bins).
    pub radiance_probe_lo: u64,

    /// Bytes 56..=63 : Facet P — radiance-cascade probe HIGH (amplitude +
    /// frequency-band).
    pub radiance_probe_hi: u64,

    /// Bytes 64..=67 : Facet Φ — pattern handle (32-bit gen-ref into
    /// [`crate::phi_table::PhiTable`]). [`PATTERN_HANDLE_NULL`] = unclaimed.
    pub pattern_handle: u32,

    /// Bytes 68..=71 : Facet Σ — low-half of [`SigmaMaskPacked`] (the 32-bit
    /// `consent_bits` field). The full 16-byte mask lives in the
    /// [`crate::sigma_overlay::SigmaOverlay`] when the cell has a non-default
    /// Σ-mask. This 4-byte cache is the hot-path consent-gate read.
    pub sigma_consent_bits: u32,
}

impl FieldCell {
    /// "Air cell" — no material, no excitation, no probe, default Σ.
    /// Used as the default for newly-allocated grid slots.
    pub const AIR: FieldCell = FieldCell {
        m_pga_or_region: 0, // RegionID 0 = "air / void" by convention.
        density: 0.0,
        velocity: [0.0; 3],
        vorticity: [0.0; 3],
        enthalpy: 0.0,
        multivec_dynamics_lo: 0,
        radiance_probe_lo: 0,
        radiance_probe_hi: 0,
        pattern_handle: PATTERN_HANDLE_NULL,
        sigma_consent_bits: ConsentBit::Observe.bits(), // Default-Private.
    };

    /// Construct a new cell with explicit fields.
    ///
    /// # Panics
    /// Panics if `enthalpy < 0.0` (refinement-type `f32'pos` violation).
    #[must_use]
    pub fn new(
        m_pga_or_region: u64,
        density: f32,
        velocity: [f32; 3],
        vorticity: [f32; 3],
        enthalpy: f32,
    ) -> FieldCell {
        assert!(
            enthalpy >= 0.0,
            "FieldCell::new : enthalpy must be ≥ 0 (f32'pos refinement)"
        );
        FieldCell {
            m_pga_or_region,
            density,
            velocity,
            vorticity,
            enthalpy,
            multivec_dynamics_lo: 0,
            radiance_probe_lo: 0,
            radiance_probe_hi: 0,
            pattern_handle: PATTERN_HANDLE_NULL,
            sigma_consent_bits: ConsentBit::Observe.bits(),
        }
    }

    // ── Material-facet helpers ──────────────────────────────────────

    /// True iff the M-tag is set (cell carries an individual PGA-handle).
    #[inline]
    #[must_use]
    pub const fn m_is_pga_handle(&self) -> bool {
        (self.m_pga_or_region & M_TAG_PGA) != 0
    }

    /// True iff the M-tag is clear (cell references a per-substrate
    /// RegionID).
    #[inline]
    #[must_use]
    pub const fn m_is_region(&self) -> bool {
        (self.m_pga_or_region & M_TAG_PGA) == 0
    }

    /// 63-bit M-payload (RegionID or PGA-handle id).
    #[inline]
    #[must_use]
    pub const fn m_payload(&self) -> u64 {
        self.m_pga_or_region & M_PAYLOAD_MASK
    }

    /// Replace the M-facet with a RegionID (M-tag clear).
    #[inline]
    pub fn set_region(&mut self, region_id: u64) {
        self.m_pga_or_region = region_id & M_PAYLOAD_MASK;
    }

    /// Replace the M-facet with a PGA-handle (M-tag set).
    #[inline]
    pub fn set_pga_handle(&mut self, handle_id: u64) {
        self.m_pga_or_region = M_TAG_PGA | (handle_id & M_PAYLOAD_MASK);
    }

    // ── Bivector-pack/unpack via cssl-pga ───────────────────────────

    /// Pack the low 6 components (e₁₂, e₁₃, e₂₃, e₀₁, e₀₂, e₀₃) of a PGA
    /// grade-2 multivector into the 8-byte `multivec_dynamics_lo` field
    /// using half-precision (f16) packing. The high components are dropped
    /// (they live in the sparse overlay when needed). This is the
    /// "S-block bivector compressed-low" wire defined in
    /// 00_FACETS.csl.md § III.compressed-multivec_dynamics.
    pub fn pack_bivector_lo(&mut self, mvec: &Multivector) {
        // The canonical PGA bivector basis blade indices (per cssl-pga::basis):
        //   e12  → grade-2 blade 1
        //   e13  → grade-2 blade 2
        //   e23  → grade-2 blade 3
        //   e01  → grade-2 blade 4
        //   e02  → grade-2 blade 5
        //   e03  → grade-2 blade 6
        // We grab them by canonical-index and pack as 6 × f16. With 6 × 2 =
        // 12 bytes that exceeds 8B, so we drop the e23/e02/e03 dual-axis
        // components and keep the 4 most-active : e12, e13, e01, e23 — in
        // half-precision = 4 × 2 = 8 bytes.
        //
        // (The spec calls this "compressed via half-precision + packed-form
        // to 8B in lo + 8B in hi if dense" so we are honoring the lo-only
        // path here.)
        let blade_indices = [5_usize, 6, 7, 8]; // canonical PGA blades
        let mut packed: u64 = 0;
        for (i, &b) in blade_indices.iter().enumerate() {
            let v = mvec.coefficient(b);
            let h = f32_to_f16(v);
            packed |= (h as u64) << (i * 16);
        }
        self.multivec_dynamics_lo = packed;
    }

    /// Inverse of [`Self::pack_bivector_lo`] : returns 4 floats representing
    /// the e12, e13, e01, e23 PGA bivector components reconstructed from the
    /// packed half-precision storage.
    #[must_use]
    pub fn unpack_bivector_lo_components(&self) -> [f32; 4] {
        let mut out = [0.0_f32; 4];
        for (i, slot) in out.iter_mut().enumerate() {
            let h = ((self.multivec_dynamics_lo >> (i * 16)) & 0xFFFF) as u16;
            *slot = f16_to_f32(h);
        }
        out
    }

    // ── Σ-cache helpers (hot path) ──────────────────────────────────

    /// True iff the cell's cached Σ permits the canonical "modify" op-class.
    /// This is the hot-path mutation gate that avoids an overlay lookup.
    #[inline]
    #[must_use]
    pub const fn can_modify(&self) -> bool {
        (self.sigma_consent_bits & ConsentBit::Modify.bits()) != 0
    }

    /// True iff the cell's cached Σ permits "observe".
    #[inline]
    #[must_use]
    pub const fn can_observe(&self) -> bool {
        (self.sigma_consent_bits & ConsentBit::Observe.bits()) != 0
    }

    /// True iff the cell's cached Σ permits "sample".
    #[inline]
    #[must_use]
    pub const fn can_sample(&self) -> bool {
        (self.sigma_consent_bits & ConsentBit::Sample.bits()) != 0
    }

    /// True iff the cell's cached Σ permits "communicate".
    #[inline]
    #[must_use]
    pub const fn can_communicate(&self) -> bool {
        (self.sigma_consent_bits & ConsentBit::Communicate.bits()) != 0
    }

    /// Sync the cached low-half Σ from a full [`SigmaMaskPacked`].
    /// Called after the SigmaOverlay full-mask is updated to keep the cell-
    /// embedded fast-path bits coherent.
    #[inline]
    pub fn sync_sigma_low(&mut self, full_mask: SigmaMaskPacked) {
        self.sigma_consent_bits = full_mask.consent_bits();
    }

    /// Reconstruct an in-cell `SigmaMaskPacked` from the low-half cache plus
    /// neutral defaults. This is a "lossy" extraction — for the canonical
    /// full Σ-mask consult [`crate::sigma_overlay::SigmaOverlay`].
    #[must_use]
    pub fn sigma_low_only(&self) -> SigmaMaskPacked {
        SigmaMaskPacked::default_mask().with_consent(self.sigma_consent_bits)
    }

    // ── Pattern-facet helpers ───────────────────────────────────────

    /// True iff a Pattern claims this cell.
    #[inline]
    #[must_use]
    pub const fn has_pattern(&self) -> bool {
        self.pattern_handle != PATTERN_HANDLE_NULL
    }

    /// Set the Φ pattern handle.
    #[inline]
    pub fn set_pattern_handle(&mut self, handle: u32) {
        self.pattern_handle = handle;
    }

    /// Clear the Φ pattern handle (cell becomes unclaimed).
    #[inline]
    pub fn clear_pattern(&mut self) {
        self.pattern_handle = PATTERN_HANDLE_NULL;
    }

    // ── Refinement-type predicates ─────────────────────────────────

    /// True iff `enthalpy >= 0.0` (the f32'pos refinement). This is checked
    /// at construction time but mutations may still violate it ; consumers
    /// that mutate the field should re-check post-update.
    #[inline]
    #[must_use]
    pub fn enthalpy_is_positive(&self) -> bool {
        self.enthalpy >= 0.0
    }

    /// True iff velocity is the zero vector or has magnitude ≈ 1 (the
    /// `vec3'unit_or_zero` refinement). Tolerance is 1e-3 in magnitude.
    #[must_use]
    pub fn velocity_is_unit_or_zero(&self) -> bool {
        let [x, y, z] = self.velocity;
        let mag2 = x * x + y * y + z * z;
        mag2 < 1e-6 || (mag2 - 1.0).abs() < 1e-3
    }
}

impl Default for FieldCell {
    fn default() -> Self {
        Self::AIR
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Half-precision conversion helpers (IEEE 754 binary16 ⇄ binary32).
// ───────────────────────────────────────────────────────────────────────
//
// We roll our own f16 ↔ f32 conversion to avoid pulling in a dep just for
// the bivector-pack path. The encoding is the standard IEEE 754 binary16
// :  1 sign bit + 5 exponent bits + 10 mantissa bits = 16 bits total.
// Subnormals + infinities + NaN are handled ; rounding is round-to-nearest-
// even at the f16 boundary.

/// Convert IEEE-754 binary32 → binary16. Subnormals + infinities + NaN
/// preserved.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp32 = (bits >> 23) & 0xFF;
    let mant32 = bits & 0x007F_FFFF;

    if exp32 == 0xFF {
        // Inf / NaN
        let mant16 = if mant32 == 0 { 0 } else { 0x0200 };
        return sign | 0x7C00 | mant16;
    }

    let exp_signed: i32 = exp32 as i32 - 127 + 15;
    if exp_signed >= 0x1F {
        // Overflow → +Inf
        return sign | 0x7C00;
    }
    if exp_signed <= 0 {
        // Subnormal or zero in f16.
        if exp_signed < -10 {
            return sign;
        }
        let mant_with_implicit = mant32 | 0x0080_0000;
        let shift = (14 - exp_signed) as u32;
        let mant16 = (mant_with_implicit >> shift) as u16;
        // Round-to-nearest-even at the discarded-bit boundary.
        let round_bit = (mant_with_implicit >> (shift - 1)) & 0x1;
        let sticky = (mant_with_implicit & ((1u32 << (shift - 1)) - 1)) != 0;
        let rounded = mant16 + (round_bit as u16) * (if sticky || (mant16 & 1) == 1 { 1 } else { 0 });
        return sign | rounded;
    }

    let exp16 = (exp_signed as u16) << 10;
    let mant16 = (mant32 >> 13) as u16;
    // Round-to-nearest-even.
    let lost = mant32 & 0x1FFF;
    let half = 0x1000;
    let rounded = if lost > half {
        mant16 + 1
    } else if lost == half {
        mant16 + (mant16 & 1)
    } else {
        mant16
    };
    sign | exp16 | rounded
}

/// Convert IEEE-754 binary16 → binary32.
#[allow(clippy::cast_lossless)]
fn f16_to_f32(value: u16) -> f32 {
    let sign = ((value >> 15) & 0x1) as u32;
    let exp = ((value >> 10) & 0x1F) as u32;
    let mant = (value & 0x3FF) as u32;
    let bits: u32 = if exp == 0 {
        if mant == 0 {
            sign << 31
        } else {
            // Subnormal : normalize.
            let mut e = 0_u32;
            let mut m = mant;
            while m & 0x400 == 0 {
                m <<= 1;
                e += 1;
            }
            let exp32 = (127 - 15 - e + 1) as u32;
            (sign << 31) | (exp32 << 23) | ((m & 0x3FF) << 13)
        }
    } else if exp == 0x1F {
        // Inf / NaN.
        (sign << 31) | (0xFF << 23) | (mant << 13)
    } else {
        let exp32 = exp + 127 - 15;
        (sign << 31) | (exp32 << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

#[cfg(test)]
mod tests {
    use super::{f16_to_f32, f32_to_f16, FieldCell, M_PAYLOAD_MASK, M_TAG_PGA, PATTERN_HANDLE_NULL};
    use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked, SigmaPolicy};

    // ── Layout invariants — the load-bearing 72B contract ───────────

    #[test]
    fn field_cell_size_is_72_bytes() {
        assert_eq!(core::mem::size_of::<FieldCell>(), 72);
    }

    #[test]
    fn field_cell_alignment_is_8_bytes() {
        assert_eq!(core::mem::align_of::<FieldCell>(), 8);
    }

    #[test]
    fn field_cell_layout_offsets_match_spec() {
        // Manual offset verification : write known values to each field +
        // read back the bytes to confirm the spec-table offsets.
        let cell = FieldCell {
            m_pga_or_region: 0xAAAA_BBBB_CCCC_DDDD,
            density: f32::from_bits(0x1111_1111),
            velocity: [
                f32::from_bits(0x2222_2222),
                f32::from_bits(0x3333_3333),
                f32::from_bits(0x4444_4444),
            ],
            vorticity: [
                f32::from_bits(0x5555_5555),
                f32::from_bits(0x6666_6666),
                f32::from_bits(0x7777_7777),
            ],
            enthalpy: f32::from_bits(0x8888_8888),
            multivec_dynamics_lo: 0x9999_AAAA_BBBB_CCCC,
            radiance_probe_lo: 0xDDDD_EEEE_FFFF_0000,
            radiance_probe_hi: 0x1111_2222_3333_4444,
            pattern_handle: 0x5555_6666,
            sigma_consent_bits: 0x7777_8888,
        };
        let bytes: &[u8; 72] = unsafe { &*((&cell as *const FieldCell).cast::<[u8; 72]>()) };
        // m_pga_or_region @ offset 0 (8 bytes, LE).
        assert_eq!(
            u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            0xAAAA_BBBB_CCCC_DDDD
        );
        // density @ offset 8 (4 bytes).
        assert_eq!(
            f32::from_bits(u32::from_le_bytes(bytes[8..12].try_into().unwrap())),
            cell.density
        );
        // velocity[0] @ offset 12.
        assert_eq!(
            f32::from_bits(u32::from_le_bytes(bytes[12..16].try_into().unwrap())),
            cell.velocity[0]
        );
        // vorticity[0] @ offset 24.
        assert_eq!(
            f32::from_bits(u32::from_le_bytes(bytes[24..28].try_into().unwrap())),
            cell.vorticity[0]
        );
        // enthalpy @ offset 36.
        assert_eq!(
            f32::from_bits(u32::from_le_bytes(bytes[36..40].try_into().unwrap())),
            cell.enthalpy
        );
        // multivec_dynamics_lo @ offset 40.
        assert_eq!(
            u64::from_le_bytes(bytes[40..48].try_into().unwrap()),
            cell.multivec_dynamics_lo
        );
        // radiance_probe_lo @ offset 48.
        assert_eq!(
            u64::from_le_bytes(bytes[48..56].try_into().unwrap()),
            cell.radiance_probe_lo
        );
        // radiance_probe_hi @ offset 56.
        assert_eq!(
            u64::from_le_bytes(bytes[56..64].try_into().unwrap()),
            cell.radiance_probe_hi
        );
        // pattern_handle @ offset 64.
        assert_eq!(
            u32::from_le_bytes(bytes[64..68].try_into().unwrap()),
            cell.pattern_handle
        );
        // sigma_consent_bits @ offset 68.
        assert_eq!(
            u32::from_le_bytes(bytes[68..72].try_into().unwrap()),
            cell.sigma_consent_bits
        );
    }

    // ── Default cell properties ────────────────────────────────────

    #[test]
    fn default_cell_is_air() {
        let c = FieldCell::default();
        assert_eq!(c.density, 0.0);
        assert_eq!(c.velocity, [0.0; 3]);
        assert_eq!(c.vorticity, [0.0; 3]);
        assert_eq!(c.enthalpy, 0.0);
        assert!(c.m_is_region());
        assert_eq!(c.m_payload(), 0);
        assert_eq!(c.pattern_handle, PATTERN_HANDLE_NULL);
        assert!(!c.has_pattern());
    }

    #[test]
    fn default_cell_can_observe_only() {
        let c = FieldCell::default();
        assert!(c.can_observe());
        assert!(!c.can_modify());
        assert!(!c.can_sample());
        assert!(!c.can_communicate());
    }

    // ── M-facet tag bits ───────────────────────────────────────────

    #[test]
    fn m_set_region_clears_tag_bit() {
        let mut c = FieldCell::default();
        c.set_region(0x42);
        assert!(c.m_is_region());
        assert!(!c.m_is_pga_handle());
        assert_eq!(c.m_payload(), 0x42);
    }

    #[test]
    fn m_set_pga_handle_sets_tag_bit() {
        let mut c = FieldCell::default();
        c.set_pga_handle(0x42);
        assert!(c.m_is_pga_handle());
        assert!(!c.m_is_region());
        assert_eq!(c.m_payload(), 0x42);
        assert_eq!(c.m_pga_or_region & M_TAG_PGA, M_TAG_PGA);
    }

    #[test]
    fn m_payload_clamps_to_63_bits() {
        let mut c = FieldCell::default();
        c.set_region(u64::MAX);
        // Bit 63 is the M-tag, NOT part of payload, so payload is the
        // low 63 bits.
        assert_eq!(c.m_payload(), M_PAYLOAD_MASK);
    }

    // ── Σ-cache hot-path consent ──────────────────────────────────

    #[test]
    fn sigma_low_modify_grant_via_full_mask() {
        let mut c = FieldCell::default();
        let full = SigmaMaskPacked::default_mask().with_consent(
            ConsentBit::Modify.bits() | ConsentBit::Observe.bits(),
        );
        c.sync_sigma_low(full);
        assert!(c.can_modify());
        assert!(c.can_observe());
    }

    #[test]
    fn sigma_low_after_policy_publicread() {
        let mut c = FieldCell::default();
        let full = SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead);
        c.sync_sigma_low(full);
        assert!(c.can_observe());
        assert!(c.can_sample());
        assert!(!c.can_modify());
    }

    #[test]
    fn sigma_low_only_drops_high_half() {
        let mut c = FieldCell::default();
        let full = SigmaMaskPacked::default_mask()
            .with_sovereign(0x1234)
            .with_capacity_floor(99)
            .with_consent(ConsentBit::Modify.bits());
        c.sync_sigma_low(full);
        let recovered = c.sigma_low_only();
        // The cached recovery must have the consent bits but defaults for
        // the rest of the mask (since the cell only stores the low half).
        assert_eq!(recovered.consent_bits(), ConsentBit::Modify.bits());
        assert_eq!(recovered.sovereign_handle(), 0); // dropped on roundtrip
    }

    // ── Φ pattern-handle helpers ──────────────────────────────────

    #[test]
    fn set_pattern_handle_marks_claimed() {
        let mut c = FieldCell::default();
        c.set_pattern_handle(42);
        assert!(c.has_pattern());
        assert_eq!(c.pattern_handle, 42);
    }

    #[test]
    fn clear_pattern_unclaims() {
        let mut c = FieldCell::default();
        c.set_pattern_handle(42);
        c.clear_pattern();
        assert!(!c.has_pattern());
    }

    // ── Refinement predicates ─────────────────────────────────────

    #[test]
    fn enthalpy_default_is_positive() {
        assert!(FieldCell::default().enthalpy_is_positive());
    }

    #[test]
    fn enthalpy_negative_violates_refinement() {
        let mut c = FieldCell::default();
        c.enthalpy = -0.001;
        assert!(!c.enthalpy_is_positive());
    }

    #[test]
    fn velocity_zero_satisfies_unit_or_zero() {
        assert!(FieldCell::default().velocity_is_unit_or_zero());
    }

    #[test]
    fn velocity_unit_x_satisfies_unit_or_zero() {
        let mut c = FieldCell::default();
        c.velocity = [1.0, 0.0, 0.0];
        assert!(c.velocity_is_unit_or_zero());
    }

    #[test]
    fn velocity_half_violates_unit_or_zero() {
        let mut c = FieldCell::default();
        c.velocity = [0.5, 0.0, 0.0];
        assert!(!c.velocity_is_unit_or_zero());
    }

    // ── Bivector pack/unpack roundtrip ────────────────────────────

    #[test]
    fn bivector_pack_zero_yields_zero() {
        let mut c = FieldCell::default();
        let mvec = cssl_pga::Multivector::default();
        c.pack_bivector_lo(&mvec);
        // All four packed half-floats must be zero.
        let comps = c.unpack_bivector_lo_components();
        for v in comps.iter() {
            assert!(v.abs() < 1e-3);
        }
    }

    #[test]
    fn bivector_pack_nontrivial_roundtrips_within_f16_tolerance() {
        let mvec = cssl_pga::Multivector::default();
        // Set the four bivector blades we pack (indices 5..=8).
        let mvec = mvec
            .with_coefficient(5, 1.0)
            .with_coefficient(6, 2.5)
            .with_coefficient(7, -0.75)
            .with_coefficient(8, 0.125);
        let mut c = FieldCell::default();
        c.pack_bivector_lo(&mvec);
        let comps = c.unpack_bivector_lo_components();
        // Half-precision tolerance ≈ 1e-3 relative.
        assert!((comps[0] - 1.0).abs() < 1e-3);
        assert!((comps[1] - 2.5).abs() < 1e-3);
        assert!((comps[2] - (-0.75)).abs() < 1e-3);
        assert!((comps[3] - 0.125).abs() < 1e-3);
    }

    // ── Half-precision helpers ────────────────────────────────────

    #[test]
    fn f16_zero_roundtrip() {
        assert_eq!(f16_to_f32(f32_to_f16(0.0)), 0.0);
    }

    #[test]
    fn f16_one_roundtrip() {
        assert!((f16_to_f32(f32_to_f16(1.0)) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn f16_negative_one_roundtrip() {
        assert!((f16_to_f32(f32_to_f16(-1.0)) - (-1.0)).abs() < 1e-3);
    }

    #[test]
    fn f16_overflow_clamps_to_inf() {
        assert!(f16_to_f32(f32_to_f16(1e20)).is_infinite());
    }

    // ── Construction asserts ──────────────────────────────────────

    #[test]
    #[should_panic(expected = "enthalpy must be ≥ 0")]
    fn new_with_negative_enthalpy_panics() {
        let _ = FieldCell::new(0, 0.0, [0.0; 3], [0.0; 3], -0.1);
    }

    #[test]
    fn new_with_positive_enthalpy_succeeds() {
        let c = FieldCell::new(0, 0.5, [0.0; 3], [0.0; 3], 1.0);
        assert!(c.enthalpy_is_positive());
        assert_eq!(c.density, 0.5);
        assert_eq!(c.enthalpy, 1.0);
    }

    // ── Copy + Default ────────────────────────────────────────────

    #[test]
    fn field_cell_is_copy() {
        let a = FieldCell::default();
        let b = a; // Copy
        assert_eq!(a, b);
    }
}
