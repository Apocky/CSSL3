//! § CompanionPerspectivePass — Stage-8 orchestrator.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The single public entry-point for Stage-8 of the canonical render-
//!   pipeline. Composes :
//!
//!     1. consent-gate     — refuse-if-closed (zero-cost path)
//!     2. salience-eval    — per-cell SemanticSalienceEvaluator pass
//!     3. mutual-witness   — apply AURA-overlap shimmer if applicable
//!     4. visualization    — map salience → glow/fade/warmth
//!     5. CompanionView    — populate the per-eye output buffer
//!     6. RenderCostReport — record audit-envelope info
//!
//! § INPUT-OUTPUT CONTRACT
//!   Inputs :
//!     - [`crate::consent_gate::CompanionConsentToken`] (proof of consent)
//!     - [`crate::companion_context::CompanionContext`] (companion's state)
//!     - cell-positions slice : `&[[f32; 3]]` (world-space)
//!     - per-cell Σ-mask slice : `&[SigmaMaskPacked]` (consent gates)
//!     - optional [`crate::mutual_witness::AuraOverlap`] (player↔companion overlap)
//!
//!   Output :
//!     - [`CompanionView`] containing per-eye, per-cell, per-band intensity
//!     - [`RenderCostReport`] for audit + budget tracking
//!
//! § PER-CELL CONSENT-GATE
//!   For each cell, the pass consults the cell's Σ-mask :
//!     - if companion holds Sovereignty over the cell : full salience read
//!     - else if mask permits Observe : read but do not modify
//!     - else : output `salience = 0.0` (privacy-preserving zero)
//!
//!   This is the per-cell privacy-preservation discipline — the companion
//!   cannot "see through" body-region cells the player has Σ-marked private.
//!
//! § COST CONTRACT
//!   - gate-OFF       : O(0) — return empty CompanionView
//!   - gate-ON, K cells : O(K) salience-evaluations
//!   - mutual-witness  : O(K) shimmer modulations (optional)
//!
//! § DETERMINISM
//!   The pass is a PURE function of inputs. Calling `execute` twice with
//!   identical inputs produces byte-identical outputs.
//!
//! § PRIME-DIRECTIVE
//!   - ATTESTATION : every entry into `execute` records the pass-level
//!     ATTESTATION constant in the cost-report. The text mirrors the
//!     workspace's canonical attestation.
//!   - The salience-tensor never escapes `CompanionView`. It is consumed
//!     by Stage-10 (ToneMap) and discarded ; no telemetry path exists.

use crate::budget::Stage8Budget;
use crate::companion_context::CompanionContext;
use crate::consent_gate::{
    CompanionConsentDecision, CompanionConsentGate, CompanionConsentToken, ConsentGateError,
    PlayerToggleState,
};
use crate::mutual_witness::{AuraOverlap, MutualWitnessMode, MutualWitnessReport};
use crate::salience_evaluator::{SalienceScore, SemanticSalienceEvaluator, SALIENCE_AXES};
use crate::salience_visualization::{SalienceVisualization, VisualizationParams};
use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked};

/// Number of eyes in a stereo VR rig. Spec § Stage-8 effect-row :
/// `MultiView<2>`.
pub const MULTIVIEW_EYES: usize = 2;

/// Number of hyperspectral bands. Spec § Stage-8 effect-row :
/// `Hyperspectral<16>`. The CompanionView buffer carries the per-eye,
/// per-cell intensity at each band.
pub const HYPERSPECTRAL_BANDS: usize = 16;

/// The canonical attestation constant. Mirrors the workspace's pattern :
/// every harm-relevant fn carries this string. Recorded in every
/// [`RenderCostReport`].
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// The per-eye spectral semantic-render output of Stage-8. Consumed by
/// Stage-10 ToneMap during composition.
///
/// § STORAGE
///   `cells_per_eye[eye_idx]` : `Vec<CompanionViewCell>` parallel to the
///   input cell-positions slice.
///   `is_empty()` ⇒ Stage-8 was skipped (gate closed) ; downstream
///   compositor treats this as zero contribution.
#[derive(Debug, Clone, Default)]
pub struct CompanionView {
    /// Per-eye per-cell visualization parameters.
    pub cells_per_eye: [Vec<CompanionViewCell>; MULTIVIEW_EYES],
    /// True iff the gate refused this frame (no salience-evaluation took
    /// place). Distinct from "empty due to no cells" — see
    /// [`Self::is_skipped`].
    pub skipped: bool,
    /// True iff the companion declined this frame. Set together with
    /// `skipped = true` so the orchestrator can label the empty buffer
    /// for the player's UI.
    pub companion_declined: bool,
}

impl CompanionView {
    /// Construct an empty view (used when the gate refuses).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            cells_per_eye: Default::default(),
            skipped: true,
            companion_declined: false,
        }
    }

    /// Construct an empty view labeled "companion declined". Distinct
    /// from `empty()` so the orchestrator can render a small UI hint :
    /// "Companion is keeping their thoughts private."
    #[must_use]
    pub fn companion_declined() -> Self {
        Self {
            cells_per_eye: Default::default(),
            skipped: true,
            companion_declined: true,
        }
    }

    /// True iff Stage-8 took the zero-cost path this frame.
    #[must_use]
    pub fn is_skipped(&self) -> bool {
        self.skipped
    }

    /// Total number of cells across both eyes.
    #[must_use]
    pub fn total_cells(&self) -> usize {
        self.cells_per_eye[0].len() + self.cells_per_eye[1].len()
    }

    /// The number of cells per eye. Both eyes carry the same slice-shape
    /// per the MultiView<2> contract.
    #[must_use]
    pub fn cells_per_eye_count(&self) -> usize {
        self.cells_per_eye[0].len().max(self.cells_per_eye[1].len())
    }
}

/// One cell's worth of CompanionView output. Carries both the visualization
/// parameters (host shader input) AND the raw salience score (audit-bus
/// input). The host shader reads `viz` ; the audit-bus reads `score`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CompanionViewCell {
    /// Per-cell visualization parameters.
    pub viz: VisualizationParams,
    /// Raw salience score that produced `viz`. Kept for audit-replay.
    pub score: SalienceScore,
    /// Per-band intensity samples ∈ [0, 1]^16. Used by the host shader's
    /// hyperspectral compositor in Stage-10. Default = zeros.
    pub bands: [f32; HYPERSPECTRAL_BANDS],
    /// True iff this cell's Σ-mask blocked the salience read (consent
    /// refused). Recorded for audit-replay ; the cell is rendered with
    /// faded() viz but counted separately so the report can show
    /// "K cells suppressed by Σ-mask".
    pub consent_refused: bool,
}

/// Per-frame cost report. Every entry into `execute` produces one of
/// these even on gate-refused frames (with `skipped = true`).
#[derive(Debug, Clone, Default)]
pub struct RenderCostReport {
    /// Number of cells the pass evaluated.
    pub cells_evaluated: u32,
    /// Number of cells that were SKIPPED at the per-cell consent-gate.
    /// (Gate said "no read") — these are NOT cells_evaluated.
    pub cells_consent_refused: u32,
    /// Number of cells that fell below the glow-edge threshold and were
    /// faded. These ARE part of cells_evaluated.
    pub cells_faded: u32,
    /// Mutual-witness shimmer report. None if witness did not fire.
    pub mutual_witness: Option<MutualWitnessReport>,
    /// True iff the consent-gate was closed (player off OR companion
    /// declined). Stage-8 emitted an empty CompanionView.
    pub skipped: bool,
    /// True iff the companion explicitly declined or revoked.
    pub companion_declined: bool,
    /// Frame counter at the moment of execution. Mirrors the
    /// CompanionConsentToken.issued_frame.
    pub frame: u64,
    /// Per-frame attestation text (recorded so a third-party auditor can
    /// confirm the canonical attestation was carried).
    pub attestation: &'static str,
}

impl RenderCostReport {
    /// Construct a "skipped" report (gate refused).
    #[must_use]
    pub fn skipped(frame: u64, companion_declined: bool) -> Self {
        Self {
            cells_evaluated: 0,
            cells_consent_refused: 0,
            cells_faded: 0,
            mutual_witness: None,
            skipped: true,
            companion_declined,
            frame,
            attestation: ATTESTATION,
        }
    }
}

/// Errors that can be returned by Stage-8.
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum PassError {
    /// Caller passed a cell-positions slice whose length does not match
    /// the Σ-mask slice length.
    #[error("cell positions and Σ-mask slices have different lengths : positions={positions} masks={masks}")]
    SliceLengthMismatch { positions: usize, masks: usize },
    /// Companion context's belief embedding contains non-finite values.
    #[error("companion context belief embedding has non-finite values")]
    NonFiniteBelief,
    /// Companion context's emotion is malformed.
    #[error("companion context emotion is malformed")]
    MalformedEmotion,
    /// The consent-gate refused but the caller tried to execute anyway.
    /// Surfaced when `execute_with_gate` is called with a closed gate.
    #[error("consent gate refused : {0}")]
    GateRefused(ConsentGateError),
}

/// The Stage-8 orchestrator. Aggregates the four core sub-systems and
/// drives them per-frame.
#[derive(Debug)]
pub struct CompanionPerspectivePass {
    /// The salience evaluator (one set of KANs across both eyes).
    pub evaluator: SemanticSalienceEvaluator,
    /// The visualization mapper.
    pub visualization: SalienceVisualization,
    /// The mutual-witness mode driver.
    pub mutual_witness: MutualWitnessMode,
    /// The budget tracker.
    pub budget: Stage8Budget,
}

impl CompanionPerspectivePass {
    /// Construct with canonical defaults (untrained KAN + canonical
    /// thresholds + Quest-3 budget).
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            evaluator: SemanticSalienceEvaluator::new_untrained(),
            visualization: SalienceVisualization::canonical(),
            mutual_witness: MutualWitnessMode::canonical(),
            budget: Stage8Budget::quest3(),
        }
    }

    /// The "skip" path : produce an empty CompanionView + a skipped report.
    /// Public so the orchestrator can call this directly on gate-closed
    /// frames without going through `execute`.
    #[must_use]
    pub fn skip(
        &mut self,
        frame: u64,
        companion_declined: bool,
    ) -> (CompanionView, RenderCostReport) {
        let view = if companion_declined {
            CompanionView::companion_declined()
        } else {
            CompanionView::empty()
        };
        let report = RenderCostReport::skipped(frame, companion_declined);
        // § Record a zero-cost sample so the rolling budget reflects gate-
        //   off frames as zero (and does not drift from older non-zero
        //   samples while the gate is off).
        self.budget.record_cost(0);
        (view, report)
    }

    /// Execute Stage-8 against a held [`CompanionConsentToken`]. The
    /// token is consumed.
    ///
    /// § PRECONDITIONS
    ///   - `cell_positions.len() == cell_masks.len()`
    ///   - `ctx.is_well_formed()` — the upstream inference engine emits
    ///     well-formed contexts ; we re-verify defensively.
    pub fn execute<'consent>(
        &mut self,
        token: CompanionConsentToken<'consent>,
        ctx: &CompanionContext,
        cell_positions: &[[f32; 3]],
        cell_masks: &[SigmaMaskPacked],
        aura_overlap: Option<AuraOverlap>,
    ) -> Result<(CompanionView, RenderCostReport), PassError> {
        if cell_positions.len() != cell_masks.len() {
            return Err(PassError::SliceLengthMismatch {
                positions: cell_positions.len(),
                masks: cell_masks.len(),
            });
        }
        if !ctx.belief_is_finite() {
            return Err(PassError::NonFiniteBelief);
        }
        if !ctx.emotion.is_well_formed() {
            return Err(PassError::MalformedEmotion);
        }

        let frame = token.issued_frame();
        // Drop the token NOW — its lifetime is tied to the gate, but the
        // pass-body only needs its provenance.
        drop(token);

        // ─── 1. salience eval + per-cell consent ─────────────────────
        let mut scores: Vec<SalienceScore> = Vec::with_capacity(cell_positions.len());
        let mut cell_consent_refused: Vec<bool> = Vec::with_capacity(cell_positions.len());
        let mut cells_consent_refused = 0_u32;
        for (pos, mask) in cell_positions.iter().zip(cell_masks.iter()) {
            let allow_read = if ctx.holds_sovereignty(mask) {
                true
            } else {
                mask.permits(ConsentBit::Observe)
            };
            if !allow_read {
                scores.push(SalienceScore::zero());
                cell_consent_refused.push(true);
                cells_consent_refused += 1;
                continue;
            }
            let s = self.evaluator.evaluate(pos, ctx);
            scores.push(s);
            cell_consent_refused.push(false);
        }
        let cells_evaluated = (cell_positions.len() as u32) - cells_consent_refused;

        // ─── 2. mutual-witness shimmer (optional) ────────────────────
        let mutual_witness_report = if let Some(overlap) = aura_overlap {
            // § The shimmer can only fire when the consent-gate was open
            //   (we're inside execute, so it was). Privacy-preservation
            //   means cells_consent_refused are NOT shimmer-eligible :
            //   their salience is still 0 and the dry-run will skip them.
            Some(self.mutual_witness.apply_shimmer(overlap, &mut scores))
        } else {
            None
        };

        // ─── 3. visualization mapping ────────────────────────────────
        let mut cells_left: Vec<CompanionViewCell> = Vec::with_capacity(cell_positions.len());
        let mut cells_right: Vec<CompanionViewCell> = Vec::with_capacity(cell_positions.len());
        let mut cells_faded = 0_u32;
        for (idx, score) in scores.iter().enumerate() {
            let viz = self.visualization.map(score, &ctx.emotion);
            if viz.fade_factor < 1.0 {
                cells_faded += 1;
            }
            // § Hyperspectral bands : per-band intensity is a sinusoidal
            //   weighting of the salience-score's dominant axis. This is
            //   the canonical synthetic mapping used until the spectral-
            //   render compositor in Stage-10 is wired up. Both eyes get
            //   the same band-vector for now (MultiView<2> means per-eye
            //   distinct VIEWS, but Stage-8 is a SHARED-cognition output —
            //   the companion's belief-state is one belief regardless of
            //   eye).
            let bands = synth_bands(score);
            let cell = CompanionViewCell {
                viz,
                score: *score,
                bands,
                consent_refused: cell_consent_refused[idx],
            };
            cells_left.push(cell);
            cells_right.push(cell);
        }

        let view = CompanionView {
            cells_per_eye: [cells_left, cells_right],
            skipped: false,
            companion_declined: false,
        };

        // ─── 4. cost report ──────────────────────────────────────────
        let report = RenderCostReport {
            cells_evaluated,
            cells_consent_refused,
            cells_faded,
            mutual_witness: mutual_witness_report,
            skipped: false,
            companion_declined: false,
            frame,
            attestation: ATTESTATION,
        };

        Ok((view, report))
    }

    /// Convenience entry-point : opens the gate, executes, advances frame.
    /// Recommended for orchestrators that don't need the token-passing
    /// pattern.
    pub fn execute_with_gate(
        &mut self,
        gate: &mut CompanionConsentGate,
        ctx: &CompanionContext,
        cell_positions: &[[f32; 3]],
        cell_masks: &[SigmaMaskPacked],
        aura_overlap: Option<AuraOverlap>,
    ) -> Result<(CompanionView, RenderCostReport), PassError> {
        // § Snapshot the frame counter before opening the gate so we can
        //   thread it through both branches without borrow-overlap.
        let frame_at_open = gate.frame_counter();
        match gate.open() {
            Ok(token) => {
                let r = self.execute(token, ctx, cell_positions, cell_masks, aura_overlap);
                gate.frame_complete();
                r.map(|(v, mut rep)| {
                    // Ensure frame is recorded from the gate (defense-in-depth).
                    rep.frame = frame_at_open;
                    // Record the per-frame cost — STAGE-0 places a synthetic
                    // cost sample proportional to evaluated cells. The
                    // production path replaces this with a real GPU-timer
                    // readback once cssl-host-* wires up. The synthetic
                    // accounting is documented in `crate::budget` rustdoc.
                    let synth_ns = (rep.cells_evaluated as u64) * 100;
                    self.budget.record_cost(synth_ns);
                    (v, rep)
                })
            }
            Err(e) => {
                let companion_declined = matches!(
                    e,
                    ConsentGateError::CompanionDeclined | ConsentGateError::CompanionRevoked
                );
                let (view, report) = self.skip(frame_at_open, companion_declined);
                gate.frame_complete();
                Ok((view, report))
            }
        }
    }

    /// Convenience entry-point that reads the gate state from explicit
    /// parameters. Used by tests + by orchestrators that don't carry a
    /// stateful gate. Equivalent to constructing a fresh gate.
    pub fn execute_one_shot(
        &mut self,
        player_toggle: PlayerToggleState,
        companion_decision: CompanionConsentDecision,
        ctx: &CompanionContext,
        cell_positions: &[[f32; 3]],
        cell_masks: &[SigmaMaskPacked],
        aura_overlap: Option<AuraOverlap>,
        frame: u64,
    ) -> Result<(CompanionView, RenderCostReport), PassError> {
        let mut gate = CompanionConsentGate::new();
        gate.set_player_toggle(player_toggle);
        gate.set_companion_decision(companion_decision);
        // Advance frame counter to the requested frame.
        for _ in 0..frame {
            gate.frame_complete();
        }
        self.execute_with_gate(&mut gate, ctx, cell_positions, cell_masks, aura_overlap)
    }
}

impl Default for CompanionPerspectivePass {
    fn default() -> Self {
        Self::canonical()
    }
}

/// § Synthesize 16 hyperspectral bands from a salience score. The bands
///   are a sinusoidal weighting of the dominant axis ; this is the
///   canonical placeholder mapping until the spectral compositor lands.
///
/// § DESIGN
///   For each band b ∈ 0..16 :
///     bands[b] = magnitude * (0.5 + 0.5 * cos(b * φ_axis))
///   where φ_axis is a per-axis-distinct phase. The phases are deterministic
///   so this synthesis is bit-stable.
fn synth_bands(score: &SalienceScore) -> [f32; HYPERSPECTRAL_BANDS] {
    let mut bands = [0.0_f32; HYPERSPECTRAL_BANDS];
    let mag = score.magnitude();
    if mag <= 0.0 {
        return bands;
    }
    let phase_per_axis = [0.10_f32, 0.31_f32, 0.52_f32, 0.73_f32, 0.94_f32];
    for (axis_idx, axis_val) in score.axes.iter().enumerate() {
        let phi = phase_per_axis[axis_idx % SALIENCE_AXES];
        for b in 0..HYPERSPECTRAL_BANDS {
            let theta = (b as f32) * phi;
            let lobe = 0.5 + 0.5 * theta.cos();
            bands[b] += axis_val * lobe / (SALIENCE_AXES as f32);
        }
    }
    for b in &mut bands {
        *b = b.clamp(0.0, 1.0);
    }
    bands
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companion_context::{CompanionEmotion, CompanionId};
    use crate::consent_gate::CompanionConsentDecision;
    use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaPolicy};

    fn open_mask() -> SigmaMaskPacked {
        SigmaMaskPacked::from_policy(SigmaPolicy::DefaultPrivate)
            .with_consent(ConsentBit::Observe.bits())
    }

    fn closed_mask() -> SigmaMaskPacked {
        SigmaMaskPacked::from_policy(SigmaPolicy::DefaultPrivate).with_consent(0)
    }

    fn nonzero_ctx(handle: u16) -> CompanionContext {
        let mut c = CompanionContext::neutral();
        c.companion_id = CompanionId(7);
        c.companion_sovereign_handle = handle;
        for i in 0..crate::companion_context::BELIEF_DIM {
            c.belief_embedding[i] = 0.05 * (i as f32 + 1.0);
        }
        c.emotion = CompanionEmotion {
            curious: 0.3,
            anxious: 0.0,
            content: 0.4,
            alert: 0.0,
        };
        c
    }

    #[test]
    fn skip_returns_empty_view_when_companion_off() {
        let mut p = CompanionPerspectivePass::canonical();
        let (view, report) = p.skip(0, false);
        assert!(view.is_skipped());
        assert!(!view.companion_declined);
        assert!(report.skipped);
        assert_eq!(report.attestation, ATTESTATION);
    }

    #[test]
    fn skip_companion_declined_labels_view() {
        let mut p = CompanionPerspectivePass::canonical();
        let (view, _r) = p.skip(0, true);
        assert!(view.is_skipped());
        assert!(view.companion_declined);
    }

    #[test]
    fn execute_returns_empty_when_consent_closed() {
        let mut p = CompanionPerspectivePass::canonical();
        let ctx = nonzero_ctx(1);
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        let masks = vec![open_mask(); 2];
        let (view, report) = p
            .execute_one_shot(
                PlayerToggleState::Off,
                CompanionConsentDecision::Granted,
                &ctx,
                &positions,
                &masks,
                None,
                0,
            )
            .unwrap();
        assert!(view.is_skipped());
        assert_eq!(report.cells_evaluated, 0);
    }

    #[test]
    fn execute_renders_when_both_consent() {
        let mut p = CompanionPerspectivePass::canonical();
        let ctx = nonzero_ctx(1);
        let positions = vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        let masks = vec![open_mask(); 2];
        let (view, report) = p
            .execute_one_shot(
                PlayerToggleState::On,
                CompanionConsentDecision::Granted,
                &ctx,
                &positions,
                &masks,
                None,
                0,
            )
            .unwrap();
        assert!(!view.is_skipped());
        assert_eq!(view.cells_per_eye[0].len(), positions.len());
        assert_eq!(view.cells_per_eye[1].len(), positions.len());
        assert_eq!(report.cells_evaluated, positions.len() as u32);
    }

    #[test]
    fn execute_companion_declined_emits_labeled_view() {
        let mut p = CompanionPerspectivePass::canonical();
        let ctx = nonzero_ctx(1);
        let positions = vec![[0.0, 0.0, 0.0]];
        let masks = vec![open_mask()];
        let (view, report) = p
            .execute_one_shot(
                PlayerToggleState::On,
                CompanionConsentDecision::Declined,
                &ctx,
                &positions,
                &masks,
                None,
                0,
            )
            .unwrap();
        assert!(view.is_skipped());
        assert!(view.companion_declined);
        assert!(report.companion_declined);
    }

    #[test]
    fn slice_length_mismatch_errors() {
        let mut p = CompanionPerspectivePass::canonical();
        let ctx = nonzero_ctx(1);
        let positions = vec![[0.0; 3], [1.0; 3]];
        let masks = vec![open_mask()]; // wrong length
        let mut gate = CompanionConsentGate::new();
        gate.set_player_toggle(PlayerToggleState::On);
        gate.companion_grant();
        let token = gate.open().unwrap();
        let r = p.execute(token, &ctx, &positions, &masks, None);
        assert!(matches!(
            r.unwrap_err(),
            PassError::SliceLengthMismatch { .. }
        ));
    }

    #[test]
    fn closed_mask_zeros_salience() {
        let mut p = CompanionPerspectivePass::canonical();
        let ctx = nonzero_ctx(1);
        let positions = vec![[1.0, 2.0, 3.0]];
        let masks = vec![closed_mask()]; // does NOT permit Observe
        let (view, report) = p
            .execute_one_shot(
                PlayerToggleState::On,
                CompanionConsentDecision::Granted,
                &ctx,
                &positions,
                &masks,
                None,
                0,
            )
            .unwrap();
        assert_eq!(report.cells_consent_refused, 1);
        assert_eq!(report.cells_evaluated, 0);
        // The cell should be marked consent_refused in the view, with
        // a zero salience.
        assert!(view.cells_per_eye[0][0].consent_refused);
        assert_eq!(view.cells_per_eye[0][0].score.magnitude(), 0.0);
    }

    #[test]
    fn sovereign_handle_overrides_closed_mask() {
        // When the companion HOLDS sovereignty over a cell, the mask's
        // consent-bits no longer block the read (the companion's own
        // body-cells are theirs to inspect).
        let mut p = CompanionPerspectivePass::canonical();
        let companion_handle: u16 = 100;
        let ctx = nonzero_ctx(companion_handle);
        let positions = vec![[1.0, 2.0, 3.0]];
        let masks = vec![SigmaMaskPacked::default_mask()
            .with_sovereign(companion_handle)
            .with_consent(0)]; // no consent bits
        let (_view, report) = p
            .execute_one_shot(
                PlayerToggleState::On,
                CompanionConsentDecision::Granted,
                &ctx,
                &positions,
                &masks,
                None,
                0,
            )
            .unwrap();
        // Companion holds sovereignty ⇒ allowed to read despite no consent
        // bits set.
        assert_eq!(report.cells_consent_refused, 0);
        assert_eq!(report.cells_evaluated, 1);
    }

    #[test]
    fn frame_is_recorded_in_report() {
        let mut p = CompanionPerspectivePass::canonical();
        let ctx = nonzero_ctx(1);
        let positions = vec![[0.0; 3]];
        let masks = vec![open_mask()];
        let (_view, report) = p
            .execute_one_shot(
                PlayerToggleState::On,
                CompanionConsentDecision::Granted,
                &ctx,
                &positions,
                &masks,
                None,
                42,
            )
            .unwrap();
        assert_eq!(report.frame, 42);
    }

    #[test]
    fn nonfinite_belief_errors() {
        let mut p = CompanionPerspectivePass::canonical();
        let mut ctx = nonzero_ctx(1);
        ctx.belief_embedding[0] = f32::INFINITY;
        let positions = vec![[0.0; 3]];
        let masks = vec![open_mask()];
        let mut gate = CompanionConsentGate::new();
        gate.set_player_toggle(PlayerToggleState::On);
        gate.companion_grant();
        let token = gate.open().unwrap();
        let r = p.execute(token, &ctx, &positions, &masks, None);
        assert_eq!(r.unwrap_err(), PassError::NonFiniteBelief);
    }

    #[test]
    fn malformed_emotion_errors() {
        let mut p = CompanionPerspectivePass::canonical();
        let mut ctx = nonzero_ctx(1);
        ctx.emotion.curious = -0.1;
        let positions = vec![[0.0; 3]];
        let masks = vec![open_mask()];
        let mut gate = CompanionConsentGate::new();
        gate.set_player_toggle(PlayerToggleState::On);
        gate.companion_grant();
        let token = gate.open().unwrap();
        let r = p.execute(token, &ctx, &positions, &masks, None);
        assert_eq!(r.unwrap_err(), PassError::MalformedEmotion);
    }

    #[test]
    fn execute_with_aura_overlap_records_witness() {
        let mut p = CompanionPerspectivePass::canonical();
        let ctx = nonzero_ctx(1);
        let positions = vec![[0.0, 0.0, 0.0]; 4];
        let masks = vec![open_mask(); 4];
        let overlap = AuraOverlap::from_spheres(&[0.0; 3], 1.0, &[0.0; 3], 1.0);
        let (_view, report) = p
            .execute_one_shot(
                PlayerToggleState::On,
                CompanionConsentDecision::Granted,
                &ctx,
                &positions,
                &masks,
                Some(overlap),
                0,
            )
            .unwrap();
        assert!(report.mutual_witness.is_some());
    }

    #[test]
    fn synth_bands_are_within_unit_range() {
        let s = SalienceScore::new([0.5, 0.3, 0.2, 0.7, 0.4]);
        let bands = synth_bands(&s);
        for b in bands {
            assert!(b.is_finite());
            assert!((0.0..=1.0).contains(&b));
        }
    }

    #[test]
    fn synth_bands_zero_for_zero_score() {
        let s = SalienceScore::zero();
        let bands = synth_bands(&s);
        assert!(bands.iter().all(|b| *b == 0.0));
    }

    #[test]
    fn execute_with_gate_advances_frame() {
        let mut p = CompanionPerspectivePass::canonical();
        let mut gate = CompanionConsentGate::new();
        gate.set_player_toggle(PlayerToggleState::On);
        gate.companion_grant();
        let ctx = nonzero_ctx(1);
        let positions = vec![[0.0; 3]];
        let masks = vec![open_mask()];
        let _r1 = p
            .execute_with_gate(&mut gate, &ctx, &positions, &masks, None)
            .unwrap();
        let _r2 = p
            .execute_with_gate(&mut gate, &ctx, &positions, &masks, None)
            .unwrap();
        assert_eq!(gate.frame_counter(), 2);
    }

    #[test]
    fn budget_records_zero_for_skipped_frames() {
        let mut p = CompanionPerspectivePass::canonical();
        let _r = p.skip(0, false);
        assert_eq!(p.budget.sample_count(), 1);
        assert_eq!(p.budget.mean_cost_ns(), 0);
    }

    #[test]
    fn rendered_view_has_well_formed_visualization() {
        let mut p = CompanionPerspectivePass::canonical();
        let ctx = nonzero_ctx(1);
        let positions = vec![[0.5, 0.5, 0.5]; 4];
        let masks = vec![open_mask(); 4];
        let (view, _r) = p
            .execute_one_shot(
                PlayerToggleState::On,
                CompanionConsentDecision::Granted,
                &ctx,
                &positions,
                &masks,
                None,
                0,
            )
            .unwrap();
        for cell in &view.cells_per_eye[0] {
            assert!(cell.viz.is_well_formed());
        }
    }

    #[test]
    fn pipeline_attestation_is_canonical_text() {
        let report = RenderCostReport::skipped(0, false);
        assert_eq!(report.attestation, ATTESTATION);
        assert!(ATTESTATION.contains("hurt nor harm"));
    }
}
