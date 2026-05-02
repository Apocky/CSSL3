//! § observer — per-frame observer-coordinate (where + when + who-perceives).

use cssl_host_crystallization::spectral::IlluminantBlend;

/// All the per-observer state needed to evaluate the pixel field for one
/// frame. Each component fits in a fixed-size primitive so the struct is
/// FFI-stable + cheaply Copy-able.
#[derive(Debug, Clone, Copy)]
pub struct ObserverCoord {
    pub x_mm: i32,
    pub y_mm: i32,
    pub z_mm: i32,
    pub yaw_milli: u32,
    pub pitch_milli: u32,
    pub frame_t_milli: u64,
    /// Σ-mask token (per-observer consent state). Bitmask :
    ///   bit 0 = silhouette permission  · bit 1 = surface
    ///   bit 2 = luminance              · bit 3 = motion
    ///   bit 4 = sound                  · bit 5 = aura
    ///   bit 6 = echo                   · bit 7 = bloom
    /// Bits 8..32 reserved for future filters (e.g., gore-overlay,
    /// audio-localization, accessibility-overlay-toggle).
    pub sigma_mask_token: u32,
    pub illuminant_blend: IlluminantBlend,
}

impl Default for ObserverCoord {
    fn default() -> Self {
        Self {
            x_mm: 0,
            y_mm: 0,
            z_mm: 0,
            yaw_milli: 0,
            pitch_milli: 0,
            frame_t_milli: 0,
            sigma_mask_token: 0xFFFF_FFFF,
            illuminant_blend: IlluminantBlend::day(),
        }
    }
}

impl ObserverCoord {
    /// True if observer's Σ-mask permits aspect `aspect_idx` (0..8).
    pub fn permits_aspect(&self, aspect_idx: u8) -> bool {
        if aspect_idx >= 8 {
            return false;
        }
        (self.sigma_mask_token & (1u32 << aspect_idx)) != 0
    }
}
