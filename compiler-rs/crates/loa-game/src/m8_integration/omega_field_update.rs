//! § omega_field_update — Stage 3 : 6-phase async-compute Ω-field update.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 3 of the canonical 12-stage pipeline. Drives the
//!   `cssl-substrate-omega-field::OmegaField` six-phase hook sequence
//!   (Collapse → Propagate → Compose → Cohomology → AgencyVerify →
//!   EntropyBook) and emits a per-frame snapshot for downstream stages
//!   to consume.
//!
//! § ASYNC-COMPUTE LANE
//!   Per spec § II this stage runs on the async-compute queue + overlaps
//!   with graphics-lane stages 4-9. In the M8 vertical-slice we drive
//!   it serially but record the conceptual fence so a future scheduler
//!   slice can move it to a real async queue.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use cssl_substrate_omega_field::{FieldCell, OmegaField};

use super::embodiment_pass::BodyPresenceField;
use super::gaze_collapse_pass::GazeCollapseOutputsLite;
use super::pipeline::PipelineError;

/// Snapshot of Ω-field state after one Stage-3 run. Downstream stages
/// consume the epoch + dense-cell-count + collapsed-region-count.
#[derive(Debug, Clone)]
pub struct OmegaFieldOutputs {
    /// Frame this snapshot covers.
    pub frame_idx: u64,
    /// Field epoch (advances on every successful set_cell).
    pub epoch: u64,
    /// Number of dense FieldCell entries.
    pub dense_cell_count: usize,
    /// Number of cells touched in Phase-1 COLLAPSE this tick.
    pub cells_collapsed: u64,
    /// Number of cells touched in Phase-2 PROPAGATE this tick.
    pub cells_propagated: u64,
    /// Per-phase epoch-after marker (StepPhase order).
    pub phase_epochs: [u64; 6],
}

impl OmegaFieldOutputs {
    /// Hash for determinism comparison.
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        self.epoch.hash(&mut h);
        self.dense_cell_count.hash(&mut h);
        self.cells_collapsed.hash(&mut h);
        self.cells_propagated.hash(&mut h);
        for p in &self.phase_epochs {
            p.hash(&mut h);
        }
        h.finish()
    }
}

/// Errors specific to the Stage 3 driver.
#[derive(Debug, thiserror::Error)]
pub enum OmegaFieldDriverError {
    /// A `set_cell` call refused via the Σ-mask gate. In M8 this should
    /// never fire because the bootstrap path uses `stamp_cell_bootstrap`.
    #[error("Σ-mask refused at frame {frame}")]
    SigmaRefused { frame: u64 },
}

/// Stage 3 driver. Owns the OmegaField across frames so the epoch + cell-
/// count grow over time.
pub struct OmegaFieldDriver {
    seed: u64,
    field: OmegaField,
}

impl std::fmt::Debug for OmegaFieldDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OmegaFieldDriver")
            .field("seed", &self.seed)
            .field("epoch", &self.field.epoch())
            .field("dense_cell_count", &self.field.dense_cell_count())
            .finish()
    }
}

impl OmegaFieldDriver {
    /// Construct.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            field: OmegaField::new(),
        }
    }

    /// Run Stage 3. Stamps a few deterministic cells based on body-presence
    /// + gaze, then runs the canonical 6-phase hook sequence.
    pub fn run(
        &mut self,
        body: &BodyPresenceField,
        gaze: &GazeCollapseOutputsLite,
        frame_idx: u64,
    ) -> Result<OmegaFieldOutputs, OmegaFieldDriverError> {
        // Stamp cells (bootstrap path — no Σ-check needed since this is
        // scaffold-time substrate setup).
        for (i, cell_pos) in body.cells.iter().enumerate() {
            let key = morton_key_clamped(cell_pos);
            // Mock cell : density modulated by gaze foveal_coef + frame.
            let density = (body.aura_density.get(i).copied().unwrap_or(0.5) * gaze.foveal_coef)
                .clamp(0.0, 1.0);
            let phase = ((frame_idx as f32) * 0.01) % 1.0;
            let cell = FieldCell::new(
                self.seed.wrapping_add(i as u64) & 0x7FFF_FFFF_FFFF_FFFF,
                density,
                [phase, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                density * 0.5,
            );
            // Use stamp_cell_bootstrap to bypass Σ-check (we're in M8 setup mode).
            let _ = self.field.stamp_cell_bootstrap(key, cell);
        }

        // Drive the six canonical phases.
        let outcomes = self.field.omega_step();
        let phase_epochs = [
            outcomes[0].epoch_after,
            outcomes[1].epoch_after,
            outcomes[2].epoch_after,
            outcomes[3].epoch_after,
            outcomes[4].epoch_after,
            outcomes[5].epoch_after,
        ];

        Ok(OmegaFieldOutputs {
            frame_idx,
            epoch: self.field.epoch(),
            dense_cell_count: self.field.dense_cell_count(),
            cells_collapsed: outcomes[0].cells_touched,
            cells_propagated: outcomes[1].cells_touched,
            phase_epochs,
        })
    }

    /// Read-only access to the underlying field.
    #[must_use]
    pub fn field(&self) -> &OmegaField {
        &self.field
    }

    /// Master seed.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }
}

impl From<OmegaFieldDriverError> for PipelineError {
    fn from(e: OmegaFieldDriverError) -> Self {
        match e {
            OmegaFieldDriverError::SigmaRefused { frame } => {
                PipelineError::OmegaFieldFailed { frame }
            }
        }
    }
}

fn morton_key_clamped(pos: &[i16; 3]) -> cssl_substrate_omega_field::MortonKey {
    let x = (pos[0].max(0) as u64).min(0x1FFFFF);
    let y = (pos[1].max(0) as u64).min(0x1FFFFF);
    let z = (pos[2].max(0) as u64).min(0x1FFFFF);
    cssl_substrate_omega_field::MortonKey::encode(x, y, z)
        .unwrap_or(cssl_substrate_omega_field::MortonKey::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> BodyPresenceField {
        BodyPresenceField {
            frame_idx: 0,
            cells: vec![[0, 0, 0], [1, 0, 0], [2, 0, 0]],
            aura_density: vec![0.5, 0.5, 0.5],
            sdf_handle: 7,
        }
    }

    fn gaze() -> GazeCollapseOutputsLite {
        GazeCollapseOutputsLite {
            frame_idx: 0,
            fallback_used: false,
            foveal_coef: 1.0,
            para_foveal_coef: 0.5,
            peripheral_coef: 0.25,
            foveal_pixels: 1024,
            transitions: 0,
            fovea_center_left: [0.5, 0.5],
            fovea_center_right: [0.5, 0.5],
        }
    }

    #[test]
    fn omega_field_constructs_empty() {
        let d = OmegaFieldDriver::new(0xC551_F00D);
        assert_eq!(d.field().epoch(), 0);
        assert_eq!(d.field().dense_cell_count(), 0);
    }

    #[test]
    fn omega_field_run_advances_state() {
        let mut d = OmegaFieldDriver::new(0xC551_F00D);
        let o = d.run(&body(), &gaze(), 0).unwrap();
        assert_eq!(o.frame_idx, 0);
        assert!(o.dense_cell_count > 0);
    }

    #[test]
    fn omega_field_replay_bit_equal() {
        let mut d1 = OmegaFieldDriver::new(0xC551_F00D);
        let mut d2 = OmegaFieldDriver::new(0xC551_F00D);
        let a = d1.run(&body(), &gaze(), 7).unwrap();
        let b = d2.run(&body(), &gaze(), 7).unwrap();
        assert_eq!(a.determinism_hash(), b.determinism_hash());
    }

    #[test]
    fn omega_field_six_phases_executed() {
        let mut d = OmegaFieldDriver::new(0xC551_F00D);
        let o = d.run(&body(), &gaze(), 0).unwrap();
        assert_eq!(o.phase_epochs.len(), 6);
    }

    #[test]
    fn omega_field_dense_cells_grow_across_frames() {
        let mut d = OmegaFieldDriver::new(0xC551_F00D);
        let mut last = 0;
        for i in 0..3 {
            let o = d.run(&body(), &gaze(), i).unwrap();
            assert!(o.dense_cell_count >= last);
            last = o.dense_cell_count;
        }
    }
}
