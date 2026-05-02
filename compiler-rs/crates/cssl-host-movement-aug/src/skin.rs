// § skin.rs — cosmetic-only boost-affixes.
// ════════════════════════════════════════════════════════════════════
// § I> COSMETIC-ONLY-AXIOM enforced BY-CONSTRUCTION : `BoostAffix` exposes
//      only render-channel fields. There is NO method that returns
//      `MovementParams` ; mechanical state is owned by `aug.rs` and never
//      reads `BoostAffix`.
// § I> ¬ pay-for-power : no skin gates a mechanical advantage.
// § I> Battle-pass / gacha sources may UNLOCK affixes ; ¬ affect mechanics.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Stable identifier for a cosmetic boost-skin. Uses `u32` so it survives
/// network sync without per-instance interning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoostSkinId(pub u32);

impl BoostSkinId {
    /// The default skin everyone has unlocked. No affix data — pure-baseline.
    pub const DEFAULT: Self = Self(0);
}

/// Visual + audio only. Critically : NO mechanical fields here.
///
/// The cosmetic-only-axiom is enforced by-CONSTRUCTION : the only fields
/// are render-channel ; no method on this struct returns or mutates a
/// `MovementParams`. Tests assert distance-traveled is invariant across skins.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoostAffix {
    /// Skin identity — used for telemetry + render-bundle lookup ONLY.
    pub skin: BoostSkinId,
    /// Hue rotation applied to the sprint-trail VFX (radians, 0..τ).
    pub trail_hue: f32,
    /// Audio-pack identifier ; render-side maps to footstep / jet sample-bank.
    pub audio_pack_id: u16,
    /// VFX particle-density multiplier (0.5 = sparse · 1.0 = baseline · 2.0 = lush).
    /// Caps at 2.0 to avoid render-perf issues.
    pub vfx_density: f32,
    /// Slide-spark color (RGB ; render-only).
    pub slide_spark_rgb: [u8; 3],
    /// Wall-run footstep particles (boolean — render-only ; no gameplay).
    pub emit_wall_run_particles: bool,
}

impl BoostAffix {
    /// Default baseline skin — no cosmetic deviation.
    pub const fn baseline() -> Self {
        Self {
            skin: BoostSkinId::DEFAULT,
            trail_hue: 0.0,
            audio_pack_id: 0,
            vfx_density: 1.0,
            slide_spark_rgb: [255, 200, 80],
            emit_wall_run_particles: true,
        }
    }

    /// Validate cosmetic ranges. Returns an audit-friendly error if any
    /// field exceeds the safe-render envelope. Reject path : caller falls
    /// back to `baseline()`.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.trail_hue.is_finite() {
            return Err("trail_hue must be finite");
        }
        if !self.vfx_density.is_finite() || self.vfx_density < 0.0 || self.vfx_density > 2.0 {
            return Err("vfx_density must be in [0, 2]");
        }
        Ok(())
    }
}

impl Default for BoostAffix {
    fn default() -> Self {
        Self::baseline()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_validates() {
        assert!(BoostAffix::baseline().validate().is_ok());
    }

    #[test]
    fn out_of_range_density_rejects() {
        let mut a = BoostAffix::baseline();
        a.vfx_density = 3.5;
        assert!(a.validate().is_err());
    }

    #[test]
    fn nan_hue_rejects() {
        let mut a = BoostAffix::baseline();
        a.trail_hue = f32::NAN;
        assert!(a.validate().is_err());
    }

    #[test]
    fn skin_id_default_is_zero() {
        assert_eq!(BoostSkinId::DEFAULT, BoostSkinId(0));
    }
}
