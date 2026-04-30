//! § multigrid — V-cycle multigrid acceleration for CFER convergence.
//!
//! ## Role
//! Per spec § 36 § Multigrid acceleration the V-cycle solves the field on a
//! coarse grid (fewer cells) first, prolongates the result to fine, then
//! refines. Captures low-frequency light (sky, ambient) on coarse and
//! high-frequency (specular) on fine in O(N) total work.
//!
//! ## V-cycle (per spec)
//!   coarse-solve → prolongate → fine-relax → restrict → coarse-correct →
//!   V-cycle iterate
//!
//! ## Convergence improvement
//! Per spec : 10–100× for low-frequency scenes (skydome, ambient).

use crate::kan_stub::{kan_update, MaterialBag};
use crate::light_stub::LightState;
use thiserror::Error;

/// Error class for multigrid failures.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum MultigridError {
    /// Number of levels exceeds the per-cycle window (max 8).
    #[error("multigrid level-count {0} exceeds max 8")]
    LevelOverflow(usize),
    /// Level count is zero (no work to do).
    #[error("multigrid requires at least 1 level")]
    NoLevels,
    /// Underlying KAN-update error.
    #[error("kan update failed: {0}")]
    KanUpdate(#[from] crate::kan_stub::KanUpdateError),
}

/// V-cycle config : level count + relaxation iterations per level.
#[derive(Debug, Clone, PartialEq)]
pub struct MultigridConfig {
    /// Number of grid levels (coarse → fine). Cell-count halves per level.
    pub levels: usize,
    /// Pre-relaxation iterations on each level.
    pub pre_relax: u32,
    /// Post-relaxation iterations on each level.
    pub post_relax: u32,
    /// Coarse-solve iterations.
    pub coarse_iterations: u32,
    /// Per-level material (uniform default ; production passes a material map).
    pub level_material: MaterialBag,
}

impl Default for MultigridConfig {
    fn default() -> Self {
        Self {
            levels: 4,
            pre_relax: 2,
            post_relax: 2,
            coarse_iterations: 16,
            level_material: MaterialBag::DIFFUSE_MID,
        }
    }
}

impl MultigridConfig {
    /// Validate ; returns Ok(()) when ready.
    pub fn validate(&self) -> Result<(), MultigridError> {
        if self.levels == 0 {
            return Err(MultigridError::NoLevels);
        }
        if self.levels > 8 {
            return Err(MultigridError::LevelOverflow(self.levels));
        }
        Ok(())
    }
}

/// Per-frame V-cycle activity report.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MultigridReport {
    /// Levels actually visited.
    pub levels_visited: u32,
    /// Total relaxation iterations across all levels.
    pub total_relax_iterations: u32,
    /// Final residual after the cycle (L1).
    pub final_residual_l1: f32,
}

/// V-cycle multigrid driver.
#[derive(Debug, Clone)]
pub struct VCycle {
    pub config: MultigridConfig,
}

impl VCycle {
    /// Construct.
    pub fn new(config: MultigridConfig) -> Result<Self, MultigridError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Restriction (fine → coarse) : average pairs of cells.
    pub fn restrict(&self, fine: &[LightState]) -> Vec<LightState> {
        let n_coarse = (fine.len() + 1) / 2;
        let mut coarse = vec![LightState::zero(); n_coarse];
        for ci in 0..n_coarse {
            let i = ci * 2;
            let a = fine[i];
            let b = if i + 1 < fine.len() { fine[i + 1] } else { a };
            coarse[ci] = (a + b).scale(0.5);
        }
        coarse
    }

    /// Prolongation (coarse → fine) : duplicate each coarse cell to two fine
    /// cells (nearest-neighbor ; production uses tri-linear).
    pub fn prolongate(&self, coarse: &[LightState], fine_len: usize) -> Vec<LightState> {
        let mut fine = vec![LightState::zero(); fine_len];
        for fi in 0..fine_len {
            let ci = fi / 2;
            fine[fi] = if ci < coarse.len() {
                coarse[ci]
            } else {
                LightState::zero()
            };
        }
        fine
    }

    /// Relax once : apply the KAN-update with each cell's two-cell stencil
    /// (left + right) for `n_iters` steps.
    pub fn relax(
        &self,
        states: &[LightState],
        n_iters: u32,
    ) -> Result<Vec<LightState>, MultigridError> {
        let mut buf = states.to_vec();
        let n = buf.len();
        if n == 0 {
            return Ok(buf);
        }
        let mut next = vec![LightState::zero(); n];
        for _ in 0..n_iters {
            for i in 0..n {
                let left = if i > 0 { buf[i - 1] } else { buf[i] };
                let right = if i + 1 < n { buf[i + 1] } else { buf[i] };
                next[i] = kan_update(buf[i], &[left, right], self.config.level_material)?;
            }
            buf.clone_from_slice(&next);
        }
        Ok(buf)
    }

    /// Run a single V-cycle : descend (restrict + pre-relax), coarse-solve,
    /// ascend (prolongate + post-relax). Returns the relaxed fine-level
    /// states + a [`MultigridReport`].
    pub fn v_cycle(
        &self,
        fine: &[LightState],
    ) -> Result<(Vec<LightState>, MultigridReport), MultigridError> {
        let mut report = MultigridReport::default();
        if fine.is_empty() {
            return Ok((Vec::new(), report));
        }

        // Build pyramid : pyramid[0] = fine, pyramid[L-1] = coarsest.
        let mut pyramid: Vec<Vec<LightState>> = Vec::with_capacity(self.config.levels);
        pyramid.push(fine.to_vec());
        for _ in 1..self.config.levels {
            let coarse = self.restrict(pyramid.last().unwrap());
            pyramid.push(coarse);
        }

        // Descend : pre-relax on each finer level, restrict to coarser.
        for lvl in 0..pyramid.len() {
            let relaxed = self.relax(&pyramid[lvl], self.config.pre_relax)?;
            pyramid[lvl] = relaxed;
            report.total_relax_iterations += self.config.pre_relax;
        }

        // Coarse solve : extra iterations on the coarsest level.
        if let Some(last) = pyramid.last_mut() {
            let solved = self.relax(last, self.config.coarse_iterations)?;
            *last = solved;
            report.total_relax_iterations += self.config.coarse_iterations;
        }

        // Ascend : prolongate + correct + post-relax.
        for lvl in (0..pyramid.len() - 1).rev() {
            let coarse = pyramid[lvl + 1].clone();
            let fine_len = pyramid[lvl].len();
            let prolongated = self.prolongate(&coarse, fine_len);
            // Additive correction : fine += prolongated (simplified ; production
            // uses residual-correction).
            let corrected: Vec<LightState> = pyramid[lvl]
                .iter()
                .zip(prolongated.iter())
                .map(|(a, b)| (*a + *b).scale(0.5))
                .collect();
            let post = self.relax(&corrected, self.config.post_relax)?;
            pyramid[lvl] = post;
            report.total_relax_iterations += self.config.post_relax;
        }

        report.levels_visited = pyramid.len() as u32;
        let result = pyramid.into_iter().next().unwrap_or_default();

        // Final residual : compare to input.
        let mut residual = 0.0_f32;
        for (a, b) in fine.iter().zip(result.iter()) {
            residual += a.norm_diff_l1(*b);
        }
        report.final_residual_l1 = residual;

        Ok((result, report))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_validates() {
        let c = MultigridConfig::default();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn config_zero_levels_errors() {
        let c = MultigridConfig {
            levels: 0,
            ..MultigridConfig::default()
        };
        assert!(matches!(c.validate(), Err(MultigridError::NoLevels)));
    }

    #[test]
    fn config_too_many_levels_errors() {
        let c = MultigridConfig {
            levels: 9,
            ..MultigridConfig::default()
        };
        assert!(matches!(
            c.validate(),
            Err(MultigridError::LevelOverflow(9))
        ));
    }

    #[test]
    fn restrict_halves_count() {
        let v = VCycle::new(MultigridConfig::default()).unwrap();
        let fine = vec![LightState::from_coefs([1.0; 8]); 8];
        let coarse = v.restrict(&fine);
        assert_eq!(coarse.len(), 4);
    }

    #[test]
    fn restrict_averages_pairs() {
        let v = VCycle::new(MultigridConfig::default()).unwrap();
        let fine = vec![
            LightState::from_coefs([1.0; 8]),
            LightState::from_coefs([3.0; 8]),
        ];
        let coarse = v.restrict(&fine);
        assert_eq!(coarse.len(), 1);
        for i in 0..8 {
            assert_eq!(coarse[0].coefs[i], 2.0);
        }
    }

    #[test]
    fn prolongate_doubles_count() {
        let v = VCycle::new(MultigridConfig::default()).unwrap();
        let coarse = vec![LightState::from_coefs([1.0; 8]); 4];
        let fine = v.prolongate(&coarse, 8);
        assert_eq!(fine.len(), 8);
        for f in &fine {
            for i in 0..8 {
                assert_eq!(f.coefs[i], 1.0);
            }
        }
    }

    #[test]
    fn relax_runs_n_iters() {
        let v = VCycle::new(MultigridConfig::default()).unwrap();
        let states = vec![LightState::zero(); 4];
        let out = v.relax(&states, 3).unwrap();
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn v_cycle_runs_to_completion() {
        let v = VCycle::new(MultigridConfig::default()).unwrap();
        let fine = vec![LightState::from_coefs([0.5; 8]); 16];
        let (out, report) = v.v_cycle(&fine).unwrap();
        assert_eq!(out.len(), 16);
        assert!(report.levels_visited > 0);
        assert!(report.total_relax_iterations > 0);
    }

    #[test]
    fn v_cycle_empty_returns_empty() {
        let v = VCycle::new(MultigridConfig::default()).unwrap();
        let (out, report) = v.v_cycle(&[]).unwrap();
        assert!(out.is_empty());
        assert_eq!(report.levels_visited, 0);
    }
}
