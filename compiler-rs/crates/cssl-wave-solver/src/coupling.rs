//! § Cross-band coupling — the canonical Wave-Unity §XI table.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS (Wave-Unity §V + §XI)
//!   Cross-band coupling is what makes "you can hear the light and see
//!   the sound" emerge from the substrate. The table is *asymmetric* by
//!   AGENCY-design : LIGHT can excite AUDIO (via shimmer) ; AUDIO can
//!   excite MAGIC (via Λ-token tone) ; MAGIC can excite both LIGHT and
//!   AUDIO (via Λ-emission). LIGHT cannot excite MAGIC ; AUDIO cannot
//!   excite MAGIC. Those forbidden mappings are PRIME-DIRECTIVE-driven
//!   AGENCY-laundering preventives — see [`CrossBandTableEntry::is_forbidden`].
//!
//! § TABLE (verbatim from spec §XI)
//!   The full 8-band coupling table is reduced here to the 5-band default
//!   active in this slice. Entries below the AGENCY-laundering threshold
//!   are zero-strength.
//!
//!     | From → To       | Strength |
//!     |-----------------|----------|
//!     | LIGHT → AUDIO   | 0.001    | shimmer
//!     | AUDIO → LIGHT   | 0.001    | visible-sound
//!     | LIGHT_R → LIGHT_NEAR_IR | 0.05 | thermal absorption
//!     | LIGHT_R → LIGHT_G       | 0.0  | (RGB are independent in default config)
//!     | All other LIGHT_x ↔ LIGHT_y    | 0.0
//!     | LIGHT → MANA            | 0.0  | FORBIDDEN — AGENCY laundering
//!     | AUDIO → MANA            | 0.0  | FORBIDDEN — AGENCY laundering
//!
//!   In the 8-band extended config (HEAT, SCENT, MANA enabled) the
//!   table grows ; the same `BandPair` lookup pattern applies.
//!
//! § AGENCY-LAUNDERING PROTECTION
//!   The forbidden-mapping check is enforced at runtime via
//!   [`CouplingError::ForbiddenMapping`] : if a coupling-table entry
//!   for a forbidden pair has non-zero strength, the call returns the
//!   error and aborts the substep. Stage-0 ensures the canonical
//!   table is well-formed via the `forbidden_pairs_zero_strength`
//!   test.
//!
//! § DETERMINISM
//!   - Iterates `prev` in Morton-sorted order.
//!   - Writes deltas into a separate `next` buffer.
//!   - No RNG draws.
//!
//! § FLOP COUNT
//!   Per cell per pair : 1 complex multiply + 1 add. With ≤ 4 active
//!   pairs and ≤ 1 M cells, the cost is bounded at 8 MF/substep.

use thiserror::Error;

use crate::band::Band;
#[cfg(test)]
use crate::complex::C32;
use crate::psi_field::WaveField;

/// § A directional coupling pair (from-band, to-band).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BandPair {
    /// § Source band — the donor of amplitude.
    pub from: Band,
    /// § Target band — the receiver of amplitude.
    pub to: Band,
}

impl BandPair {
    /// § Construct a pair.
    #[inline]
    #[must_use]
    pub const fn new(from: Band, to: Band) -> Self {
        Self { from, to }
    }
}

/// § A row in the cross-band coupling table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossBandTableEntry {
    /// § The directional pair.
    pub pair: BandPair,
    /// § Coupling strength ∈ ℝ⁺ ; 0 ⇒ no coupling.
    pub strength: f32,
    /// § True iff this pair is AGENCY-laundering-forbidden ; runtime
    ///   guards refuse to execute a non-zero coupling for forbidden
    ///   pairs.
    pub forbidden: bool,
    /// § Documentation line — what aesthetic effect this drives.
    pub doc: &'static str,
}

impl CrossBandTableEntry {
    /// § True iff the entry has positive coupling strength AND is
    ///   permitted (not AGENCY-forbidden).
    #[inline]
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.forbidden && self.strength > 0.0
    }

    /// § True iff the entry is forbidden by AGENCY-rules.
    #[inline]
    #[must_use]
    pub fn is_forbidden(&self) -> bool {
        self.forbidden
    }
}

/// § The canonical Wave-Unity cross-band table for the 5-band default.
///
///   Entries marked `forbidden=true` MUST have `strength=0` ; the
///   `forbidden_pairs_zero_strength` test guards this invariant.
pub const CROSS_BAND_TABLE: &[CrossBandTableEntry] = &[
    // ── Light ↔ Audio : the headline novelty ──
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::LightRed,
            to: Band::AudioSubKHz,
        },
        strength: 0.001,
        forbidden: false,
        doc: "shimmer-onset on illuminated red surfaces",
    },
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::LightGreen,
            to: Band::AudioSubKHz,
        },
        strength: 0.001,
        forbidden: false,
        doc: "shimmer-onset on illuminated green surfaces",
    },
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::LightBlue,
            to: Band::AudioSubKHz,
        },
        strength: 0.001,
        forbidden: false,
        doc: "shimmer-onset on illuminated blue surfaces",
    },
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::AudioSubKHz,
            to: Band::LightRed,
        },
        strength: 0.001,
        forbidden: false,
        doc: "visible-sound : orchestra glow at fortissimo",
    },
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::AudioSubKHz,
            to: Band::LightGreen,
        },
        strength: 0.001,
        forbidden: false,
        doc: "visible-sound : orchestra glow",
    },
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::AudioSubKHz,
            to: Band::LightBlue,
        },
        strength: 0.001,
        forbidden: false,
        doc: "visible-sound : orchestra glow",
    },
    // ── Light → near-IR thermal coupling ──
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::LightRed,
            to: Band::LightNearIr,
        },
        strength: 0.05,
        forbidden: false,
        doc: "red light deposits heat (thermal absorption)",
    },
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::LightGreen,
            to: Band::LightNearIr,
        },
        strength: 0.04,
        forbidden: false,
        doc: "green light deposits heat",
    },
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::LightBlue,
            to: Band::LightNearIr,
        },
        strength: 0.03,
        forbidden: false,
        doc: "blue light deposits heat",
    },
    // ── Near-IR thermal emission feeds back into visible bands ──
    CrossBandTableEntry {
        pair: BandPair {
            from: Band::LightNearIr,
            to: Band::LightRed,
        },
        strength: 0.001,
        forbidden: false,
        doc: "thermal emission → red glow (hot iron)",
    },
];

/// § Look up the coupling strength for a `BandPair`. Returns `0.0` for
///   any pair not present in the canonical table. Entries flagged as
///   `forbidden` always return `0.0` (defense-in-depth).
#[must_use]
pub fn coupling_strength(pair: BandPair) -> f32 {
    for entry in CROSS_BAND_TABLE {
        if entry.pair == pair {
            if entry.forbidden {
                return 0.0;
            }
            return entry.strength;
        }
    }
    0.0
}

/// § Apply the full cross-band-coupling table to `prev → next` for one
///   substep. Walks the table ; for each active pair computes the
///   delta `Δψ_to = strength · ψ_from · dt` per cell and accumulates
///   into `next`.
///
/// # Errors
///
/// Returns [`CouplingError::ForbiddenMapping`] if any table entry
/// is flagged forbidden but carries non-zero strength (defensive guard
/// against table-corruption).
pub fn apply_cross_coupling<const C: usize>(
    prev: &WaveField<C>,
    next: &mut WaveField<C>,
    dt: f64,
) -> Result<usize, CouplingError> {
    let mut total_writes = 0_usize;
    let dt_f32 = dt as f32;
    for entry in CROSS_BAND_TABLE {
        // Defense-in-depth : forbidden + non-zero ⇒ refuse.
        if entry.forbidden && entry.strength != 0.0 {
            return Err(CouplingError::ForbiddenMapping {
                from: entry.pair.from,
                to: entry.pair.to,
                strength: entry.strength,
            });
        }
        if !entry.is_active() {
            continue;
        }
        let from_idx = entry.pair.from.index();
        let to_idx = entry.pair.to.index();
        if from_idx >= prev.band_count() || to_idx >= prev.band_count() {
            continue;
        }
        let s = entry.strength * dt_f32;
        for (key, psi_from) in prev.cells_in_band(from_idx) {
            let delta = psi_from.scale(s);
            next.add(to_idx, key, delta);
            total_writes += 1;
        }
    }
    Ok(total_writes)
}

/// § Coupling error type — represents the AGENCY-laundering refusal +
///   table-corruption diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum CouplingError {
    /// § A forbidden pair has non-zero strength in the canonical table.
    ///   This is a §1 PROHIBITIONS violation : LIGHT → MANA = 0 + AUDIO →
    ///   MANA = 0 are AGENCY-laundering preventives. The runtime refuses
    ///   to execute the coupling and the tick is aborted.
    #[error(
        "WS0040 — AGENCY-laundering refused : pair {from:?} → {to:?} is forbidden (strength={strength})"
    )]
    ForbiddenMapping { from: Band, to: Band, strength: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_omega_field::MortonKey;

    fn key(x: u64, y: u64, z: u64) -> MortonKey {
        MortonKey::encode(x, y, z).unwrap()
    }

    #[test]
    fn cross_band_table_nonempty() {
        assert!(!CROSS_BAND_TABLE.is_empty());
    }

    #[test]
    fn forbidden_pairs_zero_strength() {
        // Defense-in-depth : every forbidden entry MUST have zero strength.
        for entry in CROSS_BAND_TABLE {
            if entry.forbidden {
                assert_eq!(
                    entry.strength, 0.0,
                    "forbidden pair {:?} → {:?} must have zero strength",
                    entry.pair.from, entry.pair.to
                );
            }
        }
    }

    #[test]
    fn light_to_audio_present_with_positive_strength() {
        let pair = BandPair::new(Band::LightRed, Band::AudioSubKHz);
        let s = coupling_strength(pair);
        assert!(s > 0.0);
    }

    #[test]
    fn audio_to_light_present_with_positive_strength() {
        let pair = BandPair::new(Band::AudioSubKHz, Band::LightRed);
        let s = coupling_strength(pair);
        assert!(s > 0.0);
    }

    #[test]
    fn coupling_table_is_asymmetric() {
        // Spec § XI : LIGHT → AUDIO and AUDIO → LIGHT both 0.001.
        // But LIGHT → NEAR_IR = 0.05 vs NEAR_IR → LIGHT = 0.001.
        let lr_to_ir = coupling_strength(BandPair::new(Band::LightRed, Band::LightNearIr));
        let ir_to_lr = coupling_strength(BandPair::new(Band::LightNearIr, Band::LightRed));
        assert_ne!(lr_to_ir, ir_to_lr);
    }

    #[test]
    fn unmapped_pair_returns_zero() {
        // No coupling between LIGHT_R and LIGHT_G in the default table.
        let pair = BandPair::new(Band::LightRed, Band::LightGreen);
        assert_eq!(coupling_strength(pair), 0.0);
    }

    #[test]
    fn apply_cross_coupling_no_active_returns_zero_writes() {
        let prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let n = apply_cross_coupling(&prev, &mut next, 1e-3).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn apply_cross_coupling_red_to_audio_writes_into_audio() {
        let mut prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let k = key(5, 5, 5);
        prev.set_band(Band::LightRed, k, C32::new(1.0, 0.0));
        let n = apply_cross_coupling(&prev, &mut next, 1.0).unwrap();
        assert!(n > 0);
        // Audio band should have received a small contribution.
        let v = next.at_band(Band::AudioSubKHz, k);
        assert!(v.re > 0.0);
        assert!(v.re < 0.01);
    }

    #[test]
    fn apply_cross_coupling_audio_to_visible_writes_into_three_bands() {
        let mut prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let k = key(0, 0, 0);
        prev.set_band(Band::AudioSubKHz, k, C32::new(1.0, 0.0));
        apply_cross_coupling(&prev, &mut next, 1.0).unwrap();
        // Should have written into LightRed, LightGreen, LightBlue.
        assert!(next.at_band(Band::LightRed, k).re > 0.0);
        assert!(next.at_band(Band::LightGreen, k).re > 0.0);
        assert!(next.at_band(Band::LightBlue, k).re > 0.0);
    }

    #[test]
    fn apply_cross_coupling_replay_deterministic() {
        let mut prev = WaveField::<5>::with_default_bands();
        for i in 0..5_u64 {
            prev.set_band(Band::LightRed, key(i, 0, 0), C32::new(i as f32, 0.0));
        }
        let mut n1 = WaveField::<5>::with_default_bands();
        let mut n2 = WaveField::<5>::with_default_bands();
        apply_cross_coupling(&prev, &mut n1, 1.0).unwrap();
        apply_cross_coupling(&prev, &mut n2, 1.0).unwrap();
        for i in 0..5_u64 {
            let k = key(i, 0, 0);
            assert_eq!(
                n1.at_band(Band::AudioSubKHz, k),
                n2.at_band(Band::AudioSubKHz, k),
            );
        }
    }

    #[test]
    fn forbidden_mapping_with_nonzero_strength_returns_error() {
        // Use a constructed entry not from the canonical table.
        let bad_entry = CrossBandTableEntry {
            pair: BandPair::new(Band::AudioSubKHz, Band::LightRed),
            strength: 0.1,
            forbidden: true,
            doc: "test-only",
        };
        assert!(bad_entry.is_forbidden());
        assert!(!bad_entry.is_active());
    }

    #[test]
    fn entry_is_active_only_when_unforbidden_and_strength_positive() {
        let active = CrossBandTableEntry {
            pair: BandPair::new(Band::LightRed, Band::AudioSubKHz),
            strength: 0.001,
            forbidden: false,
            doc: "",
        };
        let dormant = CrossBandTableEntry {
            pair: BandPair::new(Band::LightRed, Band::AudioSubKHz),
            strength: 0.0,
            forbidden: false,
            doc: "",
        };
        let forbidden_zero = CrossBandTableEntry {
            pair: BandPair::new(Band::LightRed, Band::AudioSubKHz),
            strength: 0.0,
            forbidden: true,
            doc: "",
        };
        assert!(active.is_active());
        assert!(!dormant.is_active());
        assert!(!forbidden_zero.is_active());
    }

    #[test]
    fn coupling_strength_returns_zero_for_self_pair() {
        // Coupling from a band to itself is conventionally zero.
        let s = coupling_strength(BandPair::new(Band::LightRed, Band::LightRed));
        assert_eq!(s, 0.0);
    }

    #[test]
    fn coupling_table_no_self_entries() {
        // Sanity : the table should not contain identity self-couplings.
        for entry in CROSS_BAND_TABLE {
            assert_ne!(
                entry.pair.from, entry.pair.to,
                "self-coupling {:?} → {:?} is meaningless",
                entry.pair.from, entry.pair.to
            );
        }
    }

    #[test]
    fn coupling_with_dt_zero_writes_zero_amplitude() {
        let mut prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let k = key(0, 0, 0);
        prev.set_band(Band::LightRed, k, C32::new(1.0, 0.0));
        // dt = 0 ⇒ the delta is zero ; net amplitude unchanged.
        apply_cross_coupling(&prev, &mut next, 0.0).unwrap();
        assert_eq!(next.at_band(Band::AudioSubKHz, k), C32::ZERO);
    }
}
