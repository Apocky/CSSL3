//! § seed_real — REAL stage-1 KAN seed-cell emitter.
//!
//! § PIPELINE
//!   (intent_kind, intent_args, optional ω-field-summary, optional
//!     sovereign-cap-table)
//!     → encode_args + encode_features + encode_existing → composite vec
//!     → KanRuntime<FEATURE_DIM, MAX_SEED_CELLS · CELL_DIM>::eval
//!     → decode into bounded `Vec<SeedCell>` (N ≤ MAX_SEED_CELLS)
//!     → audit-emit
//!
//! § OUTPUT SHAPE
//!   The KAN output is a flat `[f32; MAX_SEED_CELLS · CELL_DIM]` array.
//!   Per-cell decoding :
//!
//!     - cell.kind       = (output_o · 256.0).round() % 256  ; u8
//!     - cell.x / y / z  = ((output + 1.0) · 7.5).round() ; u8 ∈ 0..16
//!     - cell.charge     = output_c (already tanh-clamped to [-1, 1])
//!     - cell.color_hint = ((output_h + 1.0) · 127.5).round() ; u8
//!
//!   Cells with `output[gate]` < EMIT_GATE are dropped (sparsity-control)
//!   so the typical emission count is `[1..MAX_SEED_CELLS]`.
//!
//! § FALLBACK
//!   When the KAN runtime is `None` OR untrained ⇒ delegate to the
//!   stage-0 fallback (typically `Stage0KeywordSeedClassifier`).

use crate::adapter::KanRuntime;
use crate::audit::{audit_log, fnv1a_64, AuditEvent};
use crate::feature_encode::{encode_args, FeatureEncodeConfig, FEATURE_DIM};
use cssl_host_kan_substrate_bridge::seed_classifier::{SeedCell, SeedCellClassifier};

/// § Maximum number of seed-cells the classifier may emit per call.
///   Bounded for `I-5` latency + `I-6` never-refuse safety.
pub const MAX_SEED_CELLS: usize = 16;

/// § Per-cell output-dim : (kind, x, y, z, charge, color_hint, gate) = 7.
pub const CELL_DIM: usize = 7;

/// § Total KAN output dim.
pub const SEED_OUT_DIM: usize = MAX_SEED_CELLS * CELL_DIM;

/// § Emit-gate threshold. Cells with `gate < EMIT_GATE` are dropped.
const EMIT_GATE: f32 = 0.0;

/// § REAL stage-1 KAN seed-cell classifier.
pub struct RealSeedCellKanClassifier {
    /// § Optional KAN runtime. `None` ⇒ pure-fallback per I-4.
    pub runtime: Option<KanRuntime<FEATURE_DIM, SEED_OUT_DIM>>,
    /// § Stage-0 fallback.
    pub fallback: Box<dyn SeedCellClassifier>,
    /// § Feature-encoder config.
    pub encode_cfg: FeatureEncodeConfig,
    /// § Stable impl-id for audit events.
    pub impl_id: &'static str,
}

impl RealSeedCellKanClassifier {
    /// § Construct with an explicit runtime + fallback.
    #[must_use]
    pub fn new(
        runtime: KanRuntime<FEATURE_DIM, SEED_OUT_DIM>,
        fallback: Box<dyn SeedCellClassifier>,
    ) -> Self {
        Self {
            runtime: Some(runtime),
            fallback,
            encode_cfg: FeatureEncodeConfig::default(),
            impl_id: "real-kan",
        }
    }

    /// § Construct with NO runtime (pure-fallback).
    #[must_use]
    pub fn pure_fallback(fallback: Box<dyn SeedCellClassifier>) -> Self {
        Self {
            runtime: None,
            fallback,
            encode_cfg: FeatureEncodeConfig::default(),
            impl_id: "stage-0-fallback",
        }
    }

    /// § Construct with a baked runtime (deterministic seed) + fallback.
    #[must_use]
    pub fn with_baked_seed(seed: u64, fallback: Box<dyn SeedCellClassifier>) -> Self {
        let mut runtime = KanRuntime::<FEATURE_DIM, SEED_OUT_DIM>::new_untrained();
        runtime.bake_from_seed(seed);
        Self::new(runtime, fallback)
    }

    /// § Internal : decode a flat KAN output into a bounded
    ///   `Vec<SeedCell>` per the cell-decoding rule above.
    fn decode_cells(out: &[f32; SEED_OUT_DIM]) -> Vec<SeedCell> {
        let mut cells = Vec::with_capacity(MAX_SEED_CELLS);
        for cell_i in 0..MAX_SEED_CELLS {
            let base = cell_i * CELL_DIM;
            // Slot ordering : kind, x, y, z, charge, color_hint, gate.
            let kind_raw = out[base];
            let x_raw = out[base + 1];
            let y_raw = out[base + 2];
            let z_raw = out[base + 3];
            let charge_raw = out[base + 4];
            let color_raw = out[base + 5];
            let gate_raw = out[base + 6];

            // I-2 : NaN-defense per slot.
            let safe = |v: f32| if v.is_finite() { v } else { 0.0 };
            let gate = safe(gate_raw);
            if gate < EMIT_GATE {
                continue;
            }
            // Map [-1, 1] → [0, 255] for kind / color.
            let kind_u8 = (((safe(kind_raw) + 1.0) * 127.5).round().clamp(0.0, 255.0)) as u8;
            // Map [-1, 1] → [0, 15] for x/y/z (16-cell cube).
            let x_u8 = (((safe(x_raw) + 1.0) * 7.5).round().clamp(0.0, 15.0)) as u8;
            let y_u8 = (((safe(y_raw) + 1.0) * 7.5).round().clamp(0.0, 15.0)) as u8;
            let z_u8 = (((safe(z_raw) + 1.0) * 7.5).round().clamp(0.0, 15.0)) as u8;
            // Charge passes through (SeedCell::new clamps to [-1, 1]).
            let charge = safe(charge_raw);
            // Color in [0, 255].
            let color_u8 =
                (((safe(color_raw) + 1.0) * 127.5).round().clamp(0.0, 255.0)) as u8;

            cells.push(SeedCell::new(kind_u8, x_u8, y_u8, z_u8, charge, color_u8));
        }
        // I-6 : never-refuse — guarantee at least 1 cell on every call,
        //   even if the gate dropped all candidates. This deterministic
        //   "centroid" cell is decoded from the first slot regardless of
        //   gate. Stage-0 fallback returns empty for unknown intents,
        //   but THIS path always runs the KAN forward-pass and so always
        //   has a non-zero output.
        if cells.is_empty() {
            let safe = |v: f32| if v.is_finite() { v } else { 0.0 };
            let kind_u8 = (((safe(out[0]) + 1.0) * 127.5).round().clamp(0.0, 255.0)) as u8;
            cells.push(SeedCell::new(kind_u8, 8, 8, 8, safe(out[4]), 128));
        }
        cells.truncate(MAX_SEED_CELLS);
        cells
    }
}

impl SeedCellClassifier for RealSeedCellKanClassifier {
    fn name(&self) -> &'static str {
        "stage1-kan-real-seed"
    }

    fn intent_to_seed_cells(
        &self,
        intent_kind: &str,
        intent_args: &[(String, String)],
    ) -> Vec<SeedCell> {
        // I-1 : deterministic encoding.
        // Compose : encode the args together with the intent_kind by
        // synthesizing a `("intent_kind", intent_kind)` pair at the head.
        let mut composed: Vec<(String, String)> =
            Vec::with_capacity(intent_args.len() + 1);
        composed.push(("intent_kind".to_string(), intent_kind.to_string()));
        composed.extend_from_slice(intent_args);
        let features = encode_args(&composed, self.encode_cfg);

        let in_hash = fnv1a_64(intent_kind.as_bytes())
            ^ fnv1a_64(format!("{intent_args:?}").as_bytes());

        let cells = match self.runtime.as_ref() {
            Some(r) if r.is_trained() => {
                let out = r.eval(&features);
                Self::decode_cells(&out)
            }
            _ => self.fallback.intent_to_seed_cells(intent_kind, intent_args),
        };

        // I-3 : audit-emit.
        let out_bytes = format!("n={}", cells.len());
        audit_log(AuditEvent {
            sp_id: "spontaneous_seed",
            impl_id: self.impl_id,
            in_hash,
            out_hash: fnv1a_64(out_bytes.as_bytes()),
        });

        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_kan_substrate_bridge::seed_classifier::Stage0KeywordSeedClassifier;

    #[test]
    fn real_kan_emits_bounded_cells() {
        let fb = Box::new(Stage0KeywordSeedClassifier::default_table());
        let c = RealSeedCellKanClassifier::with_baked_seed(42, fb);
        let cells = c.intent_to_seed_cells("examine", &[]);
        assert!(!cells.is_empty());
        assert!(cells.len() <= MAX_SEED_CELLS);
    }

    #[test]
    fn real_kan_cells_well_formed() {
        let fb = Box::new(Stage0KeywordSeedClassifier::default_table());
        let c = RealSeedCellKanClassifier::with_baked_seed(7, fb);
        let cells = c.intent_to_seed_cells("cocreate", &[]);
        for cell in &cells {
            // Charge clamped to [-1, 1] by SeedCell::new.
            assert!(cell.charge.is_finite());
            assert!(cell.charge >= -1.0 && cell.charge <= 1.0);
            // Coords are u8 ; validity is type-enforced.
            // We additionally require x/y/z ≤ 15 from our decoding rule.
            assert!(cell.x <= 15);
            assert!(cell.y <= 15);
            assert!(cell.z <= 15);
        }
    }

    #[test]
    fn real_kan_deterministic() {
        let fb1 = Box::new(Stage0KeywordSeedClassifier::default_table());
        let c1 = RealSeedCellKanClassifier::with_baked_seed(42, fb1);
        let fb2 = Box::new(Stage0KeywordSeedClassifier::default_table());
        let c2 = RealSeedCellKanClassifier::with_baked_seed(42, fb2);
        let a = c1.intent_to_seed_cells("examine", &[]);
        let b = c2.intent_to_seed_cells("examine", &[]);
        assert_eq!(a, b);
    }

    #[test]
    fn pure_fallback_yields_stage0_output() {
        let fb = Box::new(Stage0KeywordSeedClassifier::default_table());
        let c = RealSeedCellKanClassifier::pure_fallback(fb);
        let cells = c.intent_to_seed_cells("move", &[]);
        // Stage-0 emits 1 cell with kind=1 for "move".
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, 1);
    }

    #[test]
    fn name_is_stable() {
        let fb = Box::new(Stage0KeywordSeedClassifier::default_table());
        let c = RealSeedCellKanClassifier::with_baked_seed(0, fb);
        assert_eq!(c.name(), "stage1-kan-real-seed");
    }
}
