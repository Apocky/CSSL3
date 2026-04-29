//! § OmegaField — the canonical 7-facet substrate container.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The single struct that holds the entire Ω-field for one CrossSection :
//!   the dense FieldCell grid + every overlay (Λ, Ψ, Σ-overlay) + the Φ
//!   pattern table + the MERA pyramid + the substrate-evolution book-keeping
//!   counters.
//!
//! § SPEC
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.3 OmegaField
//!     (canonical container).
//!   - `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md` § I dense-tier table.
//!   - `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md` § III VRAM
//!     budget table.
//!
//! § INVARIANTS
//!   - `cells` carries iso-ownership of the dense FieldCell grid.
//!   - `phi_table` is shared via [`std::sync::Arc`] (read-only at the field
//!     boundary ; writes go through [`OmegaField::append_pattern`] which
//!     forwards to the inner [`crate::phi_table::PhiTable::append`]).
//!   - Σ-check is non-optional on every [`OmegaField::set_cell`] call : the
//!     full 16-byte Σ-mask is consulted via the overlay (default-mask if
//!     absent), and modification is REFUSED if `can_modify()` is false.
//!   - The audit-chain `epoch` counter advances on every successful
//!     mutation. Replay-determinism : same epoch sequence ⇒ same on-disk
//!     bytes.

use std::sync::Arc;

use crate::field_cell::FieldCell;
use crate::lambda::LambdaSimpleOverlay;
use crate::mera::MeraPyramid;
use crate::morton::{CellTier, MortonKey};
use crate::phi_table::{Pattern, PhiHandle, PhiTable, PHI_HANDLE_NULL};
use crate::psi::PsiOverlay;
use crate::sigma_overlay::SigmaOverlay;
use crate::sparse_grid::{GridError, SparseMortonGrid};
use cssl_substrate_prime_directive::sigma::SigmaMaskPacked;

/// The canonical 7-facet Ω-field container. ASSEMBLES the foundations.
#[derive(Debug, Clone)]
pub struct OmegaField {
    /// Dense FieldCell tier — the active 7-facet grid (M, S, P, Φ, Σ-low).
    cells: SparseMortonGrid<FieldCell>,

    /// Λ overlay (Symbol Lattice) — sparse Morton-keyed token buckets.
    lambda: LambdaSimpleOverlay,

    /// Ψ overlay (Wigner-negativity / "magic") — sparse f32 scalars.
    psi: PsiOverlay,

    /// Σ overlay — full 16B SigmaMaskPacked for cells with non-default masks.
    sigma: SigmaOverlay,

    /// Φ-table — append-only Pattern records, shared.
    phi_table: Arc<PhiTable>,

    /// MERA pyramid — 4-tier cascade summaries.
    mera: MeraPyramid,

    /// Epoch counter — advances on every successful mutation. R18 audit-
    /// chain link.
    epoch: u64,
}

impl OmegaField {
    /// Construct a new empty Ω-field with a fresh PhiTable.
    #[must_use]
    pub fn new() -> Self {
        OmegaField {
            cells: SparseMortonGrid::with_capacity(256),
            lambda: LambdaSimpleOverlay::new(),
            psi: PsiOverlay::new(),
            sigma: SigmaOverlay::new(),
            phi_table: Arc::new(PhiTable::new()),
            mera: MeraPyramid::new(),
            epoch: 0,
        }
    }

    /// Construct with an explicit shared PhiTable (e.g. when joining a
    /// federated worldline).
    #[must_use]
    pub fn with_shared_phi_table(phi_table: Arc<PhiTable>) -> Self {
        OmegaField {
            cells: SparseMortonGrid::with_capacity(256),
            lambda: LambdaSimpleOverlay::new(),
            psi: PsiOverlay::new(),
            sigma: SigmaOverlay::new(),
            phi_table,
            mera: MeraPyramid::new(),
            epoch: 0,
        }
    }

    /// Current epoch counter.
    #[inline]
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Number of dense FieldCell entries.
    #[inline]
    #[must_use]
    pub fn dense_cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Read the cell at `key`. If absent, returns [`FieldCell::default()`]
    /// (the air-cell default).
    #[must_use]
    pub fn cell(&self, key: MortonKey) -> FieldCell {
        self.cells.at_const(key).unwrap_or_default()
    }

    /// Read the cell at `key` ; returns `None` if no cell is present.
    #[must_use]
    pub fn cell_opt(&self, key: MortonKey) -> Option<FieldCell> {
        self.cells.at_const(key)
    }

    /// Read-only view of the dense grid (for nightly-bench + iter paths).
    #[must_use]
    pub fn cells(&self) -> &SparseMortonGrid<FieldCell> {
        &self.cells
    }

    /// Read-only view of the MERA pyramid.
    #[must_use]
    pub fn mera(&self) -> &MeraPyramid {
        &self.mera
    }

    /// Mutable view of the MERA pyramid.
    pub fn mera_mut(&mut self) -> &mut MeraPyramid {
        &mut self.mera
    }

    /// Read-only view of the Σ overlay.
    #[must_use]
    pub fn sigma(&self) -> &SigmaOverlay {
        &self.sigma
    }

    /// Read-only view of the Λ overlay.
    #[must_use]
    pub fn lambda(&self) -> &LambdaSimpleOverlay {
        &self.lambda
    }

    /// Read-only view of the Ψ overlay.
    #[must_use]
    pub fn psi(&self) -> &PsiOverlay {
        &self.psi
    }

    /// Read-only view of the Φ-table.
    #[must_use]
    pub fn phi_table(&self) -> &Arc<PhiTable> {
        &self.phi_table
    }

    // ── Σ-checked mutation surface ──────────────────────────────────

    /// Set the cell at `key`. The Σ-check is non-optional :
    ///   1. The current Σ-mask (default if absent) must permit `Modify`.
    ///   2. If the check fails, the call is refused with [`MutationError::SigmaRefused`].
    ///
    /// This is the canonical mutation-gate per the load-bearing Σ-check
    /// landmine called out by T11-D144.
    pub fn set_cell(&mut self, key: MortonKey, cell: FieldCell) -> Result<(), MutationError> {
        // Σ-check the FULL mask (overlay-canonical).
        let mask = self.sigma.at(key);
        if !mask.can_modify() {
            return Err(MutationError::SigmaRefused {
                key,
                consent_bits: mask.consent_bits(),
            });
        }
        // Sync the cell's low-half Σ cache before insert so the hot-path
        // gate stays coherent.
        let mut new_cell = cell;
        new_cell.sync_sigma_low(mask);
        self.cells.insert(key, new_cell).map_err(|e| MutationError::Grid(e))?;
        self.epoch = self.epoch.wrapping_add(1);
        Ok(())
    }

    /// Set the cell at `key` BYPASSING the Σ-check. Used at scene-load when
    /// stamping the initial substrate before any consent contracts exist.
    /// After scene-load completes, callers MUST switch to [`Self::set_cell`].
    ///
    /// This method does NOT advance the epoch counter (it's the "boot" path,
    /// not a runtime mutation).
    pub fn stamp_cell_bootstrap(
        &mut self,
        key: MortonKey,
        cell: FieldCell,
    ) -> Result<(), MutationError> {
        self.cells
            .insert(key, cell)
            .map_err(MutationError::Grid)
            .map(|_| ())
    }

    /// Set the cell at `key`, GRANTING Modify-consent first. Combines a
    /// Σ-mask update + cell write into a single audited operation. Used
    /// by the Sovereign-action path when the Sovereign explicitly authorizes
    /// the modification.
    pub fn set_cell_with_consent_grant(
        &mut self,
        key: MortonKey,
        cell: FieldCell,
        sovereign: u16,
        new_mask: SigmaMaskPacked,
    ) -> Result<(), MutationError> {
        // The new mask MUST permit Modify or the request is incoherent.
        if !new_mask.can_modify() {
            return Err(MutationError::ConsentMaskNotModifiable);
        }
        // Sovereign-handle must match if the existing mask is claimed.
        let existing = self.sigma.at(key);
        if existing.is_sovereign() && existing.sovereign_handle() != sovereign {
            return Err(MutationError::SovereignMismatch {
                expected: existing.sovereign_handle(),
                got: sovereign,
            });
        }
        // Update Σ overlay first.
        self.sigma.set(key, new_mask);
        // Now apply the cell.
        let mut new_cell = cell;
        new_cell.sync_sigma_low(new_mask);
        self.cells.insert(key, new_cell).map_err(MutationError::Grid)?;
        self.epoch = self.epoch.wrapping_add(1);
        Ok(())
    }

    /// Set the Σ-mask at `key`. Subsequent cell mutations are gated against
    /// the new mask.
    pub fn set_sigma(&mut self, key: MortonKey, mask: SigmaMaskPacked) {
        self.sigma.set(key, mask);
        // Re-sync any existing dense cell's low-half cache.
        if let Some(c) = self.cells.at_mut(key) {
            c.sync_sigma_low(mask);
        }
    }

    // ── Λ / Ψ / Φ surface ──────────────────────────────────────────

    /// Push a Λ-token at `key`.
    pub fn push_lambda(&mut self, key: MortonKey, token: crate::lambda::LambdaToken) -> bool {
        self.lambda.push(key, token)
    }

    /// Set the Ψ-quasi-prob scalar at `key`.
    pub fn set_psi(&mut self, key: MortonKey, negativity: f32) -> Option<f32> {
        self.psi.set(key, negativity)
    }

    /// Append a new Pattern to the Φ-table. Mutates the shared Arc via
    /// [`Arc::make_mut`] (cheap when the Arc is uniquely held).
    pub fn append_pattern(&mut self, pattern: Pattern) -> PhiHandle {
        let table = Arc::make_mut(&mut self.phi_table);
        table.append(pattern)
    }

    /// Look up a Pattern by handle. Returns the canonical (post-tombstone-
    /// chain) pattern at this handle.
    #[must_use]
    pub fn lookup_pattern(&self, handle: PhiHandle) -> Option<&Pattern> {
        if handle == PHI_HANDLE_NULL {
            return None;
        }
        self.phi_table.get(handle)
    }

    /// Resolve the Pattern that claims `key` by following the cell's
    /// Φ-handle through the table.
    #[must_use]
    pub fn pattern_at(&self, key: MortonKey) -> Option<&Pattern> {
        let cell = self.cells.at_const(key)?;
        self.lookup_pattern(cell.pattern_handle)
    }

    // ── MERA cascade ───────────────────────────────────────────────

    /// Sample the MERA pyramid at `key`, returning the finest tier that
    /// holds the cell. Falls back to the dense T0 grid when the pyramid is
    /// empty for that key.
    #[must_use]
    pub fn sample_mera(&self, key: MortonKey) -> Option<(CellTier, FieldCell)> {
        if let Some((t, c)) = self.mera.sample(key) {
            return Some((t, c));
        }
        // Fall through to the canonical dense grid.
        self.cells
            .at_const(key)
            .map(|c| (CellTier::T0Fovea, c))
    }

    /// Run the full coarsen-cascade : T0 → T1 → T2 → T3.
    pub fn coarsen_cascade(&mut self) -> usize {
        // Seed the T0 of the MERA from the dense cells.
        for (k, c) in self.cells.iter() {
            // Only T0 cells get seeded (by tier discrimination).
            if k.tier() == CellTier::T0Fovea {
                let _ = self.mera.tier_mut(CellTier::T0Fovea).insert(k, *c);
            }
        }
        self.mera.coarsen_all()
    }

    // ── Step phase-hooks (for D113..D125) ──────────────────────────

    /// Phase-1 COLLAPSE hook. At this slice the hook is a stub — wires up at
    /// `T11-D113`. The contract : observation-driven collapse at unobserved
    /// regions, emitting fresh cells per Axiom-5.
    pub fn phase_collapse(&mut self) -> StepOutcome {
        StepOutcome {
            phase: StepPhase::Collapse,
            cells_touched: 0,
            epoch_after: self.epoch,
        }
    }

    /// Phase-2 PROPAGATE hook. Stub — wires at `T11-D114` (LBM + KAN-ODE +
    /// RC + Λ-stream + Ψ-flow).
    pub fn phase_propagate(&mut self) -> StepOutcome {
        StepOutcome {
            phase: StepPhase::Propagate,
            cells_touched: 0,
            epoch_after: self.epoch,
        }
    }

    /// Phase-3 COMPOSE hook. Stub — wires at `T11-D116` (operadic sheaf-glue).
    pub fn phase_compose(&mut self) -> StepOutcome {
        StepOutcome {
            phase: StepPhase::Compose,
            cells_touched: 0,
            epoch_after: self.epoch,
        }
    }

    /// Phase-4 COHOMOLOGY hook. Stub — wires at `T11-D117` (persistent-
    /// homology incremental compute).
    pub fn phase_cohomology(&mut self) -> StepOutcome {
        StepOutcome {
            phase: StepPhase::Cohomology,
            cells_touched: 0,
            epoch_after: self.epoch,
        }
    }

    /// Phase-5 AGENCY-VERIFY hook. Stub — wires at `T11-D120`.
    pub fn phase_agency_verify(&mut self) -> StepOutcome {
        StepOutcome {
            phase: StepPhase::AgencyVerify,
            cells_touched: 0,
            epoch_after: self.epoch,
        }
    }

    /// Phase-6 ENTROPY-BOOK hook. Stub — wires at `T11-D125`.
    pub fn phase_entropy_book(&mut self) -> StepOutcome {
        StepOutcome {
            phase: StepPhase::EntropyBook,
            cells_touched: 0,
            epoch_after: self.epoch,
        }
    }

    /// Run all six phase-hooks in canonical order, returning per-phase outcomes.
    pub fn omega_step(&mut self) -> [StepOutcome; 6] {
        [
            self.phase_collapse(),
            self.phase_propagate(),
            self.phase_compose(),
            self.phase_cohomology(),
            self.phase_agency_verify(),
            self.phase_entropy_book(),
        ]
    }
}

impl Default for OmegaField {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of a single phase-hook run. Used by the runtime telemetry +
/// nightly-bench gating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepOutcome {
    pub phase: StepPhase,
    pub cells_touched: u64,
    pub epoch_after: u64,
}

/// Six canonical phases of `omega_step` per
/// `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION § III.update-rule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepPhase {
    Collapse,
    Propagate,
    Compose,
    Cohomology,
    AgencyVerify,
    EntropyBook,
}

impl StepPhase {
    /// All six phases in canonical execution order.
    #[must_use]
    pub const fn all() -> [StepPhase; 6] {
        [
            StepPhase::Collapse,
            StepPhase::Propagate,
            StepPhase::Compose,
            StepPhase::Cohomology,
            StepPhase::AgencyVerify,
            StepPhase::EntropyBook,
        ]
    }

    /// Stable canonical name (for telemetry).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Collapse => "phase_1_collapse",
            Self::Propagate => "phase_2_propagate",
            Self::Compose => "phase_3_compose",
            Self::Cohomology => "phase_4_cohomology",
            Self::AgencyVerify => "phase_5_agency_verify",
            Self::EntropyBook => "phase_6_entropy_book",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Errors.
// ───────────────────────────────────────────────────────────────────────

/// Failure modes for [`OmegaField`] mutations.
#[derive(Debug, thiserror::Error)]
pub enum MutationError {
    /// The Σ-check on `set_cell` failed : the cell's mask does not permit
    /// Modify. This is the canonical consent-refusal per the load-bearing
    /// landmine called out by T11-D144.
    #[error(
        "OF0020 — Σ-check refused mutation at MortonKey {} (consent_bits=0x{:08x})",
        .key.to_u64(),
        .consent_bits
    )]
    SigmaRefused {
        key: MortonKey,
        consent_bits: u32,
    },

    /// The new Σ-mask supplied to `set_cell_with_consent_grant` does not
    /// itself permit Modify — the request is incoherent.
    #[error("OF0021 — supplied Σ-mask does not permit Modify (incoherent grant)")]
    ConsentMaskNotModifiable,

    /// The Sovereign-handle on `set_cell_with_consent_grant` does not match
    /// the cell's existing claimant.
    #[error("OF0022 — Sovereign-handle mismatch : expected={expected}, got={got}")]
    SovereignMismatch { expected: u16, got: u16 },

    /// Underlying grid-error (e.g. saturated probe).
    #[error("OF0023 — sparse-grid error : {0}")]
    Grid(#[from] GridError),
}

#[cfg(test)]
mod tests {
    use super::{MutationError, OmegaField, StepPhase};
    use crate::field_cell::FieldCell;
    use crate::morton::MortonKey;
    use crate::phi_table::Pattern;
    use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked, SigmaPolicy};

    // ── Construction ───────────────────────────────────────────

    #[test]
    fn new_field_is_empty() {
        let f = OmegaField::new();
        assert_eq!(f.dense_cell_count(), 0);
        assert_eq!(f.epoch(), 0);
    }

    // ── Σ-checked set_cell ────────────────────────────────────

    #[test]
    fn set_cell_default_mask_refuses_modify() {
        let mut f = OmegaField::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        let mut c = FieldCell::default();
        c.density = 1.0;
        // Default Σ-mask = Default-Private = Observe-only ; Modify refused.
        let err = f.set_cell(k, c).unwrap_err();
        assert!(matches!(err, MutationError::SigmaRefused { .. }));
        assert_eq!(f.dense_cell_count(), 0);
        assert_eq!(f.epoch(), 0);
    }

    #[test]
    fn set_cell_after_modify_grant_succeeds() {
        let mut f = OmegaField::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        // Grant Modify via Σ-overlay update.
        let mask = SigmaMaskPacked::default_mask().with_consent(
            ConsentBit::Modify.bits() | ConsentBit::Observe.bits(),
        );
        f.set_sigma(k, mask);
        let mut c = FieldCell::default();
        c.density = 2.0;
        f.set_cell(k, c).unwrap();
        assert_eq!(f.dense_cell_count(), 1);
        assert_eq!(f.epoch(), 1);
        assert!((f.cell(k).density - 2.0).abs() < 1e-6);
    }

    #[test]
    fn set_cell_with_consent_grant_packages_both_steps() {
        let mut f = OmegaField::new();
        let k = MortonKey::encode(7, 8, 9).unwrap();
        let mut c = FieldCell::default();
        c.density = 5.0;
        let mask =
            SigmaMaskPacked::from_policy(SigmaPolicy::SovereignOnly).with_sovereign(99);
        f.set_cell_with_consent_grant(k, c, 99, mask).unwrap();
        assert_eq!(f.dense_cell_count(), 1);
        assert_eq!(f.epoch(), 1);
        // Sovereign mismatch on subsequent grant is rejected.
        let other_mask =
            SigmaMaskPacked::from_policy(SigmaPolicy::SovereignOnly).with_sovereign(7);
        let err = f.set_cell_with_consent_grant(k, c, 7, other_mask).unwrap_err();
        assert!(matches!(err, MutationError::SovereignMismatch { .. }));
    }

    #[test]
    fn set_cell_with_non_modifiable_grant_rejected() {
        let mut f = OmegaField::new();
        let k = MortonKey::encode(1, 1, 1).unwrap();
        let c = FieldCell::default();
        // Mask permits only Observe — should be rejected as incoherent.
        let mask = SigmaMaskPacked::default_mask().with_consent(ConsentBit::Observe.bits());
        let err = f.set_cell_with_consent_grant(k, c, 0, mask).unwrap_err();
        assert!(matches!(err, MutationError::ConsentMaskNotModifiable));
    }

    #[test]
    fn stamp_cell_bootstrap_bypasses_sigma_check() {
        let mut f = OmegaField::new();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        let c = FieldCell::default();
        f.stamp_cell_bootstrap(k, c).unwrap();
        assert_eq!(f.dense_cell_count(), 1);
        // Bootstrap path does NOT advance the epoch.
        assert_eq!(f.epoch(), 0);
    }

    // ── Pattern table integration ─────────────────────────────

    #[test]
    fn append_pattern_returns_handle_and_resolves() {
        let mut f = OmegaField::new();
        let h = f.append_pattern(Pattern::new("hero", 64));
        assert_eq!(h, 1);
        let p = f.lookup_pattern(h).unwrap();
        assert_eq!(p.name, "hero");
    }

    #[test]
    fn pattern_at_key_resolves_via_cell_handle() {
        let mut f = OmegaField::new();
        let h = f.append_pattern(Pattern::new("npc", 64));
        let k = MortonKey::encode(2, 0, 0).unwrap();
        let mask = SigmaMaskPacked::default_mask().with_consent(
            ConsentBit::Modify.bits() | ConsentBit::Observe.bits(),
        );
        f.set_sigma(k, mask);
        let mut c = FieldCell::default();
        c.set_pattern_handle(h);
        f.set_cell(k, c).unwrap();
        let p = f.pattern_at(k).unwrap();
        assert_eq!(p.name, "npc");
    }

    // ── Λ + Ψ + Σ overlays ────────────────────────────────────

    #[test]
    fn push_lambda_creates_overlay_entry() {
        let mut f = OmegaField::new();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        let t = crate::lambda::LambdaToken::new_utterance(1, 1.0);
        assert!(f.push_lambda(k, t));
        assert_eq!(f.lambda().bucket_count(), 1);
    }

    #[test]
    fn set_psi_records_negativity() {
        let mut f = OmegaField::new();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        f.set_psi(k, 1.5);
        assert!((f.psi().at(k) - 1.5).abs() < 1e-6);
    }

    #[test]
    fn set_sigma_replaces_overlay_value() {
        let mut f = OmegaField::new();
        let k = MortonKey::encode(0, 0, 0).unwrap();
        let m1 = SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead);
        let m2 = SigmaMaskPacked::from_policy(SigmaPolicy::SovereignOnly);
        f.set_sigma(k, m1);
        f.set_sigma(k, m2);
        let read = f.sigma().at(k);
        assert!(read.can_modify()); // SovereignOnly permits Modify
    }

    // ── Step phase-hooks ──────────────────────────────────────

    #[test]
    fn omega_step_runs_six_phases_in_order() {
        let mut f = OmegaField::new();
        let outcomes = f.omega_step();
        assert_eq!(outcomes.len(), 6);
        for (i, &phase) in StepPhase::all().iter().enumerate() {
            assert_eq!(outcomes[i].phase, phase);
        }
    }

    #[test]
    fn step_phase_canonical_names_unique() {
        let names: Vec<_> = StepPhase::all()
            .iter()
            .map(|p| p.canonical_name())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        let original_len = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), original_len);
    }

    // ── MERA cascade ──────────────────────────────────────────

    #[test]
    fn coarsen_cascade_produces_pyramid_levels() {
        let mut f = OmegaField::new();
        // Stamp 8 fine cells in the (0, 0, 0) coarse-block. Bootstrap path
        // bypasses Σ-check so we can populate cleanly.
        for x in 0..2_u64 {
            for y in 0..2_u64 {
                for z in 0..2_u64 {
                    let mut c = FieldCell::default();
                    c.density = 3.0;
                    f.stamp_cell_bootstrap(MortonKey::encode(x, y, z).unwrap(), c)
                        .unwrap();
                }
            }
        }
        let n = f.coarsen_cascade();
        // T0→T1 produces 1, T1→T2 produces 1, T2→T3 produces 1.
        assert_eq!(n, 3);
        let coarse = f
            .mera()
            .tier(crate::morton::CellTier::T1Mid)
            .at_const(MortonKey::encode(0, 0, 0).unwrap())
            .unwrap();
        assert!((coarse.density - 3.0).abs() < 1e-6);
    }

    // ── Cell read on missing key ──────────────────────────────

    #[test]
    fn cell_on_missing_key_returns_default() {
        let f = OmegaField::new();
        let k = MortonKey::encode(99, 99, 99).unwrap();
        assert_eq!(f.cell(k), FieldCell::default());
        assert!(f.cell_opt(k).is_none());
    }
}
