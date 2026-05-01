//! Decoration subsystem : place / remove / list of asset-refs at slot-keyed transforms.
//!
//! Decorations are the player's "fingerprint" on their Home — placed gear,
//! cosmetics, spore-trophies, ambient-music, etc (spec/16 § Home-features).
//! Each decoration occupies a `u32` slot-id ; placement is cap-gated by
//! `HM_CAP_DECORATE`. Iteration is `BTreeMap`-deterministic.

use crate::asset_ref::OpaqueAsset;
use serde::{Deserialize, Serialize};

/// Affine-ish transform for a decoration slot.
///
/// Kept as a flat `[f32; 10]` (3 pos + 4 quat-rot + 3 scale) — purposely simple
/// so this crate does **not** drag in glam / nalgebra as dependencies. Higher
/// fidelity transforms live in the consuming render-pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct SlotTransform {
    /// Position xyz.
    pub pos: [f32; 3],
    /// Rotation quaternion xyzw.
    pub rot: [f32; 4],
    /// Per-axis scale xyz.
    pub scale: [f32; 3],
}

impl SlotTransform {
    /// Identity transform : zero pos, identity quat (0,0,0,1), unit scale.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            pos: [0.0; 3],
            rot: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}

/// One decoration occupying a numeric slot.
///
/// `asset` is an [`OpaqueAsset`] — to use a different asset-handle, wrap it
/// in your own `AssetRef` impl and store it in a parallel `BTreeMap`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecorationSlot {
    /// Slot identifier (caller-allocated ; map key in the Home state).
    pub slot_id: u32,
    /// Asset reference for the decoration.
    pub asset: OpaqueAsset,
    /// Transform of the decoration relative to the Home origin.
    pub transform: SlotTransform,
}

impl DecorationSlot {
    /// Build a fresh decoration slot.
    #[must_use]
    pub fn new(slot_id: u32, asset: OpaqueAsset, transform: SlotTransform) -> Self {
        Self {
            slot_id,
            asset,
            transform,
        }
    }
}
