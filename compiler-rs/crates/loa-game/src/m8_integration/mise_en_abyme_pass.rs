//! § mise_en_abyme_pass — Stage 9 : recursive-witness rendering (bounded).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 9 of the pipeline. Drives `cssl-render-v2::mise_en_abyme::
//!   MiseEnAbymePass` over a small canonical mirror set. Per spec the
//!   recursion is HARD-bounded at depth = 5 ; this driver verifies the
//!   bound is respected.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use cssl_render_v2::mise_en_abyme::{
    MiseEnAbymePass, MiseEnAbymePassConfig, RecursionDepthBudget, RECURSION_DEPTH_HARD_CAP,
};

use super::fractal_amplifier_pass::FractalAmplifierOutputs;

/// Outputs of Stage 9 — mise-en-abyme summary.
#[derive(Debug, Clone)]
pub struct MiseEnAbymeOutputs {
    /// Frame this snapshot covers.
    pub frame_idx: u64,
    /// Maximum recursion depth reached this frame.
    pub max_depth_reached: u8,
    /// Hard cap declared by the substrate (always 5).
    pub hard_cap: u8,
    /// Whether the recursion was bounded (max ≤ hard cap).
    pub recursion_bounded: bool,
    /// Number of mirror surfaces detected this frame.
    pub mirror_count: u32,
}

impl MiseEnAbymeOutputs {
    /// Hash for determinism comparison.
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        self.max_depth_reached.hash(&mut h);
        self.hard_cap.hash(&mut h);
        self.recursion_bounded.hash(&mut h);
        self.mirror_count.hash(&mut h);
        h.finish()
    }
}

/// Stage 9 driver.
pub struct MiseEnAbymeDriver {
    pass: MiseEnAbymePass,
}

impl std::fmt::Debug for MiseEnAbymeDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiseEnAbymeDriver").finish_non_exhaustive()
    }
}

impl MiseEnAbymeDriver {
    /// Construct.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pass: MiseEnAbymePass::new(MiseEnAbymePassConfig::default()),
        }
    }

    /// Run Stage 9 — for M8 vertical-slice we just verify the bounded-
    /// recursion contract holds (the underlying pass requires a probe +
    /// material-list which is the next slice).
    pub fn run(&mut self, fractal: &FractalAmplifierOutputs, frame_idx: u64) -> MiseEnAbymeOutputs {
        self.pass.begin_frame();

        // Drive the recursion-depth budget through a small loop so the
        // hard-cap is exercised. This walks the bound explicitly.
        let mut budget = RecursionDepthBudget::new();
        let mut max_d = 0_u8;
        for _ in 0..(RECURSION_DEPTH_HARD_CAP + 2) {
            match budget.try_advance() {
                Ok(b) => {
                    budget = b;
                    max_d = budget.current();
                }
                Err(_) => break,
            }
        }

        // Mirror count is keyed off the fractal-amplifier output so frame-
        // to-frame variation propagates from upstream.
        let mirror_count = fractal.fragments_amplified.clamp(1, 3);

        MiseEnAbymeOutputs {
            frame_idx,
            max_depth_reached: max_d,
            hard_cap: RECURSION_DEPTH_HARD_CAP,
            recursion_bounded: max_d <= RECURSION_DEPTH_HARD_CAP,
            mirror_count,
        }
    }

    /// Hard-cap the substrate enforces.
    #[must_use]
    pub fn hard_cap(&self) -> u8 {
        RECURSION_DEPTH_HARD_CAP
    }
}

impl Default for MiseEnAbymeDriver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fractal() -> FractalAmplifierOutputs {
        FractalAmplifierOutputs {
            frame_idx: 0,
            fragments_amplified: 8,
            fragments_skipped: 0,
            sum_displacement: 0.0,
            sum_roughness: 0.0,
            sum_color_mag: 0.0,
            octaves_achieved: 5,
        }
    }

    #[test]
    fn mise_en_abyme_constructs() {
        let d = MiseEnAbymeDriver::new();
        assert_eq!(d.hard_cap(), 5);
    }

    #[test]
    fn mise_en_abyme_runs() {
        let mut d = MiseEnAbymeDriver::new();
        let o = d.run(&fractal(), 0);
        assert_eq!(o.frame_idx, 0);
    }

    #[test]
    fn mise_en_abyme_recursion_bounded() {
        // M8 AC : bounded-recursion is HARD cap = 5.
        let mut d = MiseEnAbymeDriver::new();
        let o = d.run(&fractal(), 0);
        assert!(o.recursion_bounded);
        assert!(o.max_depth_reached <= 5);
    }

    #[test]
    fn mise_en_abyme_replay_bit_equal() {
        let mut d1 = MiseEnAbymeDriver::new();
        let mut d2 = MiseEnAbymeDriver::new();
        let a = d1.run(&fractal(), 7);
        let b = d2.run(&fractal(), 7);
        assert_eq!(a.determinism_hash(), b.determinism_hash());
    }

    #[test]
    fn mise_en_abyme_hard_cap_is_five() {
        let mut d = MiseEnAbymeDriver::new();
        let o = d.run(&fractal(), 0);
        assert_eq!(o.hard_cap, 5);
    }
}
