// § lens.rs · DimensionalLens · N → 3 projection + rotation
// ══════════════════════════════════════════════════════════════════
// § The lens is the OBSERVER's window onto an N-D field. It picks 3 of
// N axes to expose to a conventional 3D-renderer + records WHICH axes
// were promoted/demoted, so a full N-D coord can be reconstructed
// when the observer rotates the lens.
//
// § PRIME-DIRECTIVE :
//   - construction MUST go through `with_consent()` ; a lens that hasn't
//     been consented-to refuses projection
//   - axis-out-of-range is structurally-rejected
//   - rotation is consent-state-preserving (Rotation does not bypass
//     consent ; it only re-orders consented axes)
//
// § STAGE-0 LENSES :
//   - `spatial_xyz()` : axes 0/1/2 → conventional spatial-renderer
//   - `mood_temporal()` : axes 4/5/3 → "feelings-renderer" surfacing the
//     emotional landscape as if it were spatial geometry
//   - `causal_arc()` : axes 6/5/3 → narrative-debugger projection
// ══════════════════════════════════════════════════════════════════

use thiserror::Error;

use crate::coord::NdCoord;

/// § Consent-state for a lens. Default-deny ; observers must explicitly
/// consent before a lens can project.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConsentState {
    /// Observer has not yet consented ; project() refuses.
    Unconsented,
    /// Observer consented at this monotonic clock-tick ; auditable.
    Consented { at_tick: u64 },
    /// Observer revoked ; lens is permanently inert (substrate refuses to
    /// silently re-enable a revoked lens — observer must build a new one).
    Revoked,
}

/// § Errors emitted by lens-construction + projection paths.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConsentError {
    /// Lens references an axis index that exceeds the source coord's N.
    #[error("axis {axis} out of range for source N={n}")]
    AxisOutOfRange { axis: u8, n: usize },
    /// Lens has duplicate target slots ; substrate refuses ambiguous projection.
    #[error("duplicate axis {axis} in lens targets")]
    DuplicateTarget { axis: u8 },
    /// Observer has not consented ; project() refused.
    #[error("lens has not been consented-to ; project() refused")]
    Unconsented,
    /// Observer revoked the lens ; it is permanently inert.
    #[error("lens was revoked ; build a new one to project again")]
    Revoked,
}

/// § Source-N erased + consent-tracked projection.
/// Per Apocky-foundational : 3D is a CONVENTION not a constraint.
#[derive(Clone, Debug)]
pub struct DimensionalLens {
    /// Indices into the source coord's axes that the lens "knows about".
    /// Stage-0 the source-set must be a superset of `target_axes`.
    source_axes: Vec<u8>,
    /// Three axis-indices selected from `source_axes` ; these become the
    /// X/Y/Z exposed to conventional 3D renderers.
    target_axes: [u8; 3],
    /// Source-coord arity at construction-time ; used to bounds-check
    /// against incoming coords without re-validating each call.
    source_n: usize,
    /// Consent-state ; default-deny.
    consent: ConsentState,
}

/// § Lens-rotation : swap two of the three exposed slots, optionally
/// promoting a different source-axis into a target slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LensRotation {
    /// Swap the contents of two target-slots (slot indices 0/1/2).
    SwapTargets { a: u8, b: u8 },
    /// Replace target-slot `slot` with a different source-axis. The new
    /// axis must already be in `source_axes` (enforced by `apply`).
    PromoteSource { slot: u8, new_axis: u8 },
}

impl DimensionalLens {
    /// § Build a lens with consent already granted at `tick`.
    /// Returns `ConsentError` if axis-indices are inconsistent.
    pub fn with_consent(
        source_axes: Vec<u8>,
        target_axes: [u8; 3],
        source_n: usize,
        tick: u64,
    ) -> Result<Self, ConsentError> {
        Self::validate(&source_axes, target_axes, source_n)?;
        Ok(Self {
            source_axes,
            target_axes,
            source_n,
            consent: ConsentState::Consented { at_tick: tick },
        })
    }

    /// § Build a lens UNCONSENTED. Observers can pre-stage a lens spec
    /// then consent later. Until consented, project() returns Unconsented.
    pub fn unconsented(
        source_axes: Vec<u8>,
        target_axes: [u8; 3],
        source_n: usize,
    ) -> Result<Self, ConsentError> {
        Self::validate(&source_axes, target_axes, source_n)?;
        Ok(Self {
            source_axes,
            target_axes,
            source_n,
            consent: ConsentState::Unconsented,
        })
    }

    fn validate(
        source_axes: &[u8],
        target_axes: [u8; 3],
        source_n: usize,
    ) -> Result<(), ConsentError> {
        // Bounds : every referenced axis must be < source_n.
        for &a in source_axes {
            if (a as usize) >= source_n {
                return Err(ConsentError::AxisOutOfRange {
                    axis: a,
                    n: source_n,
                });
            }
        }
        for a in target_axes {
            if (a as usize) >= source_n {
                return Err(ConsentError::AxisOutOfRange {
                    axis: a,
                    n: source_n,
                });
            }
            if !source_axes.contains(&a) {
                // Target-axis not in source-set.
                return Err(ConsentError::AxisOutOfRange {
                    axis: a,
                    n: source_n,
                });
            }
        }
        // Duplicate targets are ambiguous.
        for i in 0..3 {
            for j in (i + 1)..3 {
                if target_axes[i] == target_axes[j] {
                    return Err(ConsentError::DuplicateTarget {
                        axis: target_axes[i],
                    });
                }
            }
        }
        Ok(())
    }

    /// § Grant consent on a previously-unconsented lens. No-op if already
    /// consented. Refuses if lens was revoked.
    pub fn consent(&mut self, tick: u64) -> Result<(), ConsentError> {
        match self.consent {
            ConsentState::Revoked => Err(ConsentError::Revoked),
            ConsentState::Consented { .. } | ConsentState::Unconsented => {
                self.consent = ConsentState::Consented { at_tick: tick };
                Ok(())
            }
        }
    }

    /// § Permanently revoke. Lens becomes inert ; observer must construct
    /// a new lens to project again. (Substrate-discipline : silent re-enable
    /// is a consent-violation.)
    pub fn revoke(&mut self) {
        self.consent = ConsentState::Revoked;
    }

    /// § Inspect current consent-state.
    pub const fn consent_state(&self) -> ConsentState {
        self.consent
    }

    /// § Read the three axes currently promoted to X/Y/Z.
    pub const fn target_axes(&self) -> [u8; 3] {
        self.target_axes
    }

    /// § Read the lens's source-set (axes the observer agreed to perceive).
    pub fn source_axes(&self) -> &[u8] {
        &self.source_axes
    }

    /// § Project an N-D coord through the lens, returning [x, y, z].
    /// Refuses if unconsented / revoked / source-N mismatch.
    pub fn project_to_3d<const N: usize>(
        &self,
        coord: &NdCoord<N>,
    ) -> Result<[i32; 3], ConsentError> {
        match self.consent {
            ConsentState::Unconsented => return Err(ConsentError::Unconsented),
            ConsentState::Revoked => return Err(ConsentError::Revoked),
            ConsentState::Consented { .. } => {}
        }
        if N != self.source_n {
            return Err(ConsentError::AxisOutOfRange {
                axis: 0,
                n: self.source_n,
            });
        }
        let axes = coord.axes();
        Ok([
            axes[self.target_axes[0] as usize],
            axes[self.target_axes[1] as usize],
            axes[self.target_axes[2] as usize],
        ])
    }

    /// § Inverse of project_to_3d for the consented axes only.
    /// Other axes default to 0 ; caller can preserve a "carry" coord and
    /// merge instead.
    pub fn unproject_from_3d<const N: usize>(
        &self,
        xyz: [i32; 3],
    ) -> Result<NdCoord<N>, ConsentError> {
        match self.consent {
            ConsentState::Unconsented => return Err(ConsentError::Unconsented),
            ConsentState::Revoked => return Err(ConsentError::Revoked),
            ConsentState::Consented { .. } => {}
        }
        if N != self.source_n {
            return Err(ConsentError::AxisOutOfRange {
                axis: 0,
                n: self.source_n,
            });
        }
        let mut out = [0i32; N];
        for (slot, &axis) in self.target_axes.iter().enumerate() {
            out[axis as usize] = xyz[slot];
        }
        Ok(NdCoord::from_axes(out))
    }

    /// § Merge a 3D-update INTO an existing N-D coord, preserving the
    /// non-projected axes. This is the "carry-coord" pattern : a player's
    /// position-on-rails-of-mood survives a spatial-only step.
    pub fn merge_3d_into<const N: usize>(
        &self,
        carry: &NdCoord<N>,
        xyz: [i32; 3],
    ) -> Result<NdCoord<N>, ConsentError> {
        match self.consent {
            ConsentState::Unconsented => return Err(ConsentError::Unconsented),
            ConsentState::Revoked => return Err(ConsentError::Revoked),
            ConsentState::Consented { .. } => {}
        }
        if N != self.source_n {
            return Err(ConsentError::AxisOutOfRange {
                axis: 0,
                n: self.source_n,
            });
        }
        let mut out = *carry.axes();
        for (slot, &axis) in self.target_axes.iter().enumerate() {
            out[axis as usize] = xyz[slot];
        }
        Ok(NdCoord::from_axes(out))
    }

    /// § Apply a rotation in-place. Rotations are consent-preserving :
    /// they only re-shape an already-consented lens.
    pub fn apply(&mut self, rot: LensRotation) -> Result<(), ConsentError> {
        if self.consent == ConsentState::Revoked {
            return Err(ConsentError::Revoked);
        }
        match rot {
            LensRotation::SwapTargets { a, b } => {
                if a >= 3 || b >= 3 {
                    return Err(ConsentError::AxisOutOfRange { axis: a.max(b), n: 3 });
                }
                self.target_axes.swap(a as usize, b as usize);
                Ok(())
            }
            LensRotation::PromoteSource { slot, new_axis } => {
                if slot >= 3 {
                    return Err(ConsentError::AxisOutOfRange { axis: slot, n: 3 });
                }
                if !self.source_axes.contains(&new_axis) {
                    return Err(ConsentError::AxisOutOfRange {
                        axis: new_axis,
                        n: self.source_n,
                    });
                }
                // Reject if the resulting target-set would have a duplicate.
                for i in 0..3 {
                    if i as u8 != slot && self.target_axes[i] == new_axis {
                        return Err(ConsentError::DuplicateTarget { axis: new_axis });
                    }
                }
                self.target_axes[slot as usize] = new_axis;
                Ok(())
            }
        }
    }
}

/// § Convenience constructor : the conventional "spatial XYZ" lens against
/// a stage-0 N=8 coord. Already consented at tick 0 ; use only for tests +
/// engine-bootstrap. Real observers should call `with_consent`.
pub fn spatial_xyz_for_stage0() -> DimensionalLens {
    DimensionalLens::with_consent(
        vec![0, 1, 2, 3, 4, 5, 6, 7],
        [0, 1, 2],
        8,
        0,
    )
    .expect("stage-0 spatial lens is well-formed by construction")
}

/// § Convenience : "mood + temporal" lens — surfaces emotional landscape as
/// if it were spatial. Promotes axis 4 (mood) to X, axis 5 (arc) to Y,
/// axis 3 (temporal) to Z.
pub fn mood_temporal_for_stage0() -> DimensionalLens {
    DimensionalLens::with_consent(
        vec![0, 1, 2, 3, 4, 5, 6, 7],
        [4, 5, 3],
        8,
        0,
    )
    .expect("stage-0 mood-temporal lens is well-formed by construction")
}

/// § Convenience : "causal-arc" lens — promotes causality / arc / temporal.
pub fn causal_arc_for_stage0() -> DimensionalLens {
    DimensionalLens::with_consent(
        vec![0, 1, 2, 3, 4, 5, 6, 7],
        [6, 5, 3],
        8,
        0,
    )
    .expect("stage-0 causal-arc lens is well-formed by construction")
}

/// § Standalone projection helper. Returns the 3D-projection of a coord
/// through the supplied lens. Equivalent to `lens.project_to_3d(coord)`
/// but lives at the module-level for ergonomic call-sites.
pub fn project_to_3d<const N: usize>(
    coord: &NdCoord<N>,
    lens: &DimensionalLens,
) -> Result<[i32; 3], ConsentError> {
    lens.project_to_3d(coord)
}

// ══════════════════════════════════════════════════════════════════
// § tests
// ══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coord::CoordError;

    #[test]
    fn unconsented_lens_refuses() {
        let lens = DimensionalLens::unconsented(vec![0, 1, 2, 3], [0, 1, 2], 4).unwrap();
        let c: NdCoord<4> = NdCoord::from_axes([1, 2, 3, 4]);
        assert!(matches!(
            lens.project_to_3d(&c),
            Err(ConsentError::Unconsented)
        ));
    }

    #[test]
    fn consent_then_project() {
        let mut lens =
            DimensionalLens::unconsented(vec![0, 1, 2, 3], [0, 1, 2], 4).unwrap();
        lens.consent(99).unwrap();
        let c: NdCoord<4> = NdCoord::from_axes([10, 20, 30, 40]);
        let xyz = lens.project_to_3d(&c).unwrap();
        assert_eq!(xyz, [10, 20, 30]);
    }

    #[test]
    fn revoke_is_permanent() {
        let mut lens =
            DimensionalLens::with_consent(vec![0, 1, 2, 3], [0, 1, 2], 4, 0).unwrap();
        lens.revoke();
        assert!(matches!(
            lens.consent(5),
            Err(ConsentError::Revoked)
        ));
        let c: NdCoord<4> = NdCoord::from_axes([1, 2, 3, 4]);
        assert!(matches!(
            lens.project_to_3d(&c),
            Err(ConsentError::Revoked)
        ));
    }

    #[test]
    fn duplicate_targets_rejected() {
        assert!(matches!(
            DimensionalLens::with_consent(vec![0, 1, 2, 3], [0, 0, 1], 4, 0),
            Err(ConsentError::DuplicateTarget { axis: 0 })
        ));
    }

    #[test]
    fn axis_out_of_range_rejected() {
        assert!(matches!(
            DimensionalLens::with_consent(vec![0, 1, 9], [0, 1, 9], 4, 0),
            Err(ConsentError::AxisOutOfRange { axis: 9, n: 4 })
        ));
    }

    #[test]
    fn project_then_unproject_round_trip_on_targets() {
        let lens = spatial_xyz_for_stage0();
        let c: NdCoord<8> = NdCoord::from_axes([1, 2, 3, 4, 5, 6, 7, 8]);
        let xyz = lens.project_to_3d(&c).unwrap();
        let back: NdCoord<8> = lens.unproject_from_3d(xyz).unwrap();
        // Spatial axes match ; semantic axes default to 0.
        assert_eq!(back.axes(), &[1, 2, 3, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn merge_preserves_carry_semantic_axes() {
        let lens = spatial_xyz_for_stage0();
        let carry: NdCoord<8> = NdCoord::from_axes([0, 0, 0, 7, 8, 9, 10, 11]);
        let merged: NdCoord<8> = lens.merge_3d_into(&carry, [42, 43, 44]).unwrap();
        // Spatial axes overwritten ; semantic axes preserved.
        assert_eq!(merged.axes(), &[42, 43, 44, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn rotation_swap_targets() {
        let mut lens = spatial_xyz_for_stage0();
        lens.apply(LensRotation::SwapTargets { a: 0, b: 2 }).unwrap();
        assert_eq!(lens.target_axes(), [2, 1, 0]);
        let c: NdCoord<8> = NdCoord::from_axes([1, 2, 3, 4, 5, 6, 7, 8]);
        let xyz = lens.project_to_3d(&c).unwrap();
        assert_eq!(xyz, [3, 2, 1]);
    }

    #[test]
    fn rotation_promote_source_to_mood() {
        let mut lens = spatial_xyz_for_stage0();
        // Promote axis-4 (mood) into target-slot 0 ; replaces spatial-X.
        lens.apply(LensRotation::PromoteSource { slot: 0, new_axis: 4 })
            .unwrap();
        assert_eq!(lens.target_axes(), [4, 1, 2]);
        let c: NdCoord<8> = NdCoord::from_axes([1, 2, 3, 4, 99, 6, 7, 8]);
        let xyz = lens.project_to_3d(&c).unwrap();
        assert_eq!(xyz, [99, 2, 3]);
    }

    #[test]
    fn rotation_rejects_duplicate_promotion() {
        let mut lens = spatial_xyz_for_stage0();
        // axis-1 already in slot 1 ; promoting it into slot 0 would dup.
        assert!(matches!(
            lens.apply(LensRotation::PromoteSource { slot: 0, new_axis: 1 }),
            Err(ConsentError::DuplicateTarget { axis: 1 })
        ));
    }

    #[test]
    fn mood_temporal_lens_projects_emotional_landscape() {
        let lens = mood_temporal_for_stage0();
        // (1,2,3,4,5,6,7,8) → mood=5,arc=6,temporal=4 → [5,6,4]
        let c: NdCoord<8> = NdCoord::from_axes([1, 2, 3, 4, 5, 6, 7, 8]);
        let xyz = lens.project_to_3d(&c).unwrap();
        assert_eq!(xyz, [5, 6, 4]);
    }

    #[test]
    fn module_level_project_helper_works() {
        let lens = spatial_xyz_for_stage0();
        let c: NdCoord<8> = NdCoord::from_axes([100, 200, 300, 0, 0, 0, 0, 0]);
        assert_eq!(project_to_3d(&c, &lens).unwrap(), [100, 200, 300]);
    }

    #[test]
    fn n_must_match_source_n() {
        let lens = spatial_xyz_for_stage0(); // source_n = 8
        let c: NdCoord<4> = NdCoord::from_axes([1, 2, 3, 4]);
        let _ = c; // unused warning silencer
                    // Wrong-arity coord refused.
        let result = lens.project_to_3d::<4>(&NdCoord::<4>::from_axes([1, 2, 3, 4]));
        assert!(matches!(
            result,
            Err(ConsentError::AxisOutOfRange { .. })
        ));
    }

    #[test]
    fn unused_coord_error_re_export_compiles() {
        // Smoke : CoordError is re-exported at crate root and useable here.
        let _: Result<(), CoordError> = Ok(());
    }
}
