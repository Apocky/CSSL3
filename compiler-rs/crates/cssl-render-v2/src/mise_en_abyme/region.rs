//! § RegionBoundary — anti-surveillance gate for cross-region mirror reflections
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per spec § V.6.d (PRIME_DIRECTIVE alignment) :
//!     ```text
//!     anti-surveil  : ‼ mirror ¬ a-camera-loop-back to-other-region
//!                      R! reflections strictly-LOCAL ⊗ N! cross-region-spy-mirror
//!     ```
//!
//!   And per spec § XIII-bis (path-6 failure-modes) :
//!     ```text
//!     ⊘ FAIL : mirror leaks-Σ-private-region from-other-locale
//!        symptom : reflection contains-distant-Sovereign-not-present-locally
//!        remedy  : R! recursion-bounce honors-Σ-mask ⊗ compile-time-checked
//!     ```
//!
//!   This module implements the runtime predicate that decides whether a
//!   recursive bounce from region-A is allowed to "see into" region-B. The
//!   default policy is `RegionPolicy::SameRegionOnly` — strict locality.
//!   The `RegionPolicy::PolicyTable` variant allows a more permissive
//!   per-region-pair policy table, and is consulted in cases where the
//!   level-design allows e.g. two interior rooms to share visual context
//!   (think : a salon with two opposing mirrors that intentionally form a
//!   visual corridor).
//!
//! § INVARIANT
//!   The anti-surveillance check happens BEFORE the recursive ray is cast :
//!   if the destination region forbids surveillance from the source region,
//!   the bounce is blocked and a `Stage9Event::SurveillanceBlocked` is
//!   emitted. The recursion continues with `attenuation = 0` for that
//!   bounce — the visual effect is "the mirror shows nothing" rather than
//!   a hard panic.

/// § Region identifier — opaque tag that the level-design pipeline assigns
///   to spatial regions. Matches the `Σ-mask.region_id` field in the spec.
///
///   The level designer is responsible for choosing meaningful region IDs ;
///   this crate treats the value as opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct RegionId(pub u16);

impl RegionId {
    /// § Sentinel for "unclaimed / default-public" region. Reflections of
    ///   PUBLIC into PUBLIC always succeed.
    pub const PUBLIC: Self = Self(0);

    /// § Sentinel for "absent" — used in tests where the region is
    ///   intentionally unset. Reflections involving ABSENT always fail
    ///   (defensive default).
    pub const ABSENT: Self = Self(0xFFFF);
}

/// § Policy that decides whether a recursive bounce from `source_region`
///   into `target_region` is permitted.
#[derive(Debug, Clone)]
pub enum RegionPolicy {
    /// § Strict locality : a mirror in region-A may only reflect surfaces
    ///   that are in region-A. The default policy.
    SameRegionOnly,
    /// § Public-only : a mirror in region-A may reflect surfaces in
    ///   region-A OR in `RegionId::PUBLIC`. Used for outdoor scenes
    ///   where the sky and distant terrain are a single PUBLIC region
    ///   while interior nooks are private regions.
    SameRegionOrPublic,
    /// § Permissive policy table : an explicit allow-list of
    ///   `(source, target)` pairs. The pair `(A, A)` is implicit ; the
    ///   table lists ONLY cross-region permissions. If the pair is not
    ///   in the table, the bounce is blocked.
    PolicyTable(Vec<(RegionId, RegionId)>),
}

impl Default for RegionPolicy {
    fn default() -> Self {
        Self::SameRegionOnly
    }
}

impl RegionPolicy {
    /// § Predicate : true iff a bounce from `source_region` into
    ///   `target_region` is permitted under this policy.
    #[must_use]
    pub fn is_permitted(&self, source_region: RegionId, target_region: RegionId) -> bool {
        // § ABSENT regions are never permitted on either side — defensive.
        if source_region == RegionId::ABSENT || target_region == RegionId::ABSENT {
            return false;
        }
        match self {
            Self::SameRegionOnly => source_region == target_region,
            Self::SameRegionOrPublic => {
                source_region == target_region
                    || target_region == RegionId::PUBLIC
                    || source_region == RegionId::PUBLIC
            }
            Self::PolicyTable(t) => {
                if source_region == target_region {
                    return true;
                }
                t.iter()
                    .any(|&(s, d)| s == source_region && d == target_region)
            }
        }
    }
}

/// § Lightweight wrapper over `RegionPolicy` that the recursion calls into.
///   The wrapper exists to make the call-site readable :
///
///   ```text
///     if !boundary.permits(src, dst) { return Err(SurveillanceBlocked); }
///   ```
///
///   It also caches the most-recent block-reason so the compositor can
///   emit a richer telemetry event.
#[derive(Debug, Clone)]
pub struct RegionBoundary {
    /// § The active policy.
    pub policy: RegionPolicy,
    /// § Number of bounces blocked so far (frame-local counter).
    blocks: u32,
}

impl Default for RegionBoundary {
    fn default() -> Self {
        Self {
            policy: RegionPolicy::default(),
            blocks: 0,
        }
    }
}

impl RegionBoundary {
    /// § Construct from a policy.
    #[must_use]
    pub fn from_policy(policy: RegionPolicy) -> Self {
        Self { policy, blocks: 0 }
    }

    /// § Predicate : true iff the bounce is permitted. Internally
    ///   increments the `blocks` counter when blocked.
    #[must_use]
    pub fn permits(&mut self, source_region: RegionId, target_region: RegionId) -> bool {
        let ok = self.policy.is_permitted(source_region, target_region);
        if !ok {
            self.blocks = self.blocks.saturating_add(1);
        }
        ok
    }

    /// § Read the per-frame blocks counter.
    #[must_use]
    pub fn blocks(&self) -> u32 {
        self.blocks
    }

    /// § Reset the per-frame blocks counter (called by the compositor at
    ///   frame boundary).
    pub fn reset_blocks(&mut self) {
        self.blocks = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § PUBLIC sentinel is 0.
    #[test]
    fn public_is_zero() {
        assert_eq!(RegionId::PUBLIC.0, 0);
    }

    /// § ABSENT sentinel is 0xFFFF.
    #[test]
    fn absent_is_max() {
        assert_eq!(RegionId::ABSENT.0, 0xFFFF);
    }

    /// § SameRegionOnly permits same-region.
    #[test]
    fn same_region_only_permits_same() {
        let p = RegionPolicy::SameRegionOnly;
        assert!(p.is_permitted(RegionId(7), RegionId(7)));
    }

    /// § SameRegionOnly forbids cross-region.
    #[test]
    fn same_region_only_forbids_cross() {
        let p = RegionPolicy::SameRegionOnly;
        assert!(!p.is_permitted(RegionId(7), RegionId(8)));
    }

    /// § SameRegionOrPublic permits same-region.
    #[test]
    fn same_or_public_permits_same() {
        let p = RegionPolicy::SameRegionOrPublic;
        assert!(p.is_permitted(RegionId(7), RegionId(7)));
    }

    /// § SameRegionOrPublic permits any-to-public.
    #[test]
    fn same_or_public_permits_to_public() {
        let p = RegionPolicy::SameRegionOrPublic;
        assert!(p.is_permitted(RegionId(7), RegionId::PUBLIC));
    }

    /// § SameRegionOrPublic permits public-to-any.
    #[test]
    fn same_or_public_permits_from_public() {
        let p = RegionPolicy::SameRegionOrPublic;
        assert!(p.is_permitted(RegionId::PUBLIC, RegionId(7)));
    }

    /// § SameRegionOrPublic forbids cross-region private.
    #[test]
    fn same_or_public_forbids_cross_private() {
        let p = RegionPolicy::SameRegionOrPublic;
        assert!(!p.is_permitted(RegionId(7), RegionId(8)));
    }

    /// § PolicyTable allows whitelisted pairs.
    #[test]
    fn policy_table_allows_whitelisted() {
        let p = RegionPolicy::PolicyTable(vec![(RegionId(7), RegionId(8))]);
        assert!(p.is_permitted(RegionId(7), RegionId(8)));
    }

    /// § PolicyTable forbids non-whitelisted pairs.
    #[test]
    fn policy_table_forbids_non_whitelisted() {
        let p = RegionPolicy::PolicyTable(vec![(RegionId(7), RegionId(8))]);
        assert!(!p.is_permitted(RegionId(8), RegionId(9)));
    }

    /// § PolicyTable always permits same-region (regardless of whitelist).
    #[test]
    fn policy_table_implicit_same_region() {
        let p = RegionPolicy::PolicyTable(vec![]);
        assert!(p.is_permitted(RegionId(7), RegionId(7)));
    }

    /// § PolicyTable is asymmetric — `(A,B)` does NOT imply `(B,A)`.
    #[test]
    fn policy_table_asymmetric() {
        let p = RegionPolicy::PolicyTable(vec![(RegionId(7), RegionId(8))]);
        assert!(p.is_permitted(RegionId(7), RegionId(8)));
        assert!(!p.is_permitted(RegionId(8), RegionId(7)));
    }

    /// § ABSENT region is rejected on either side.
    #[test]
    fn absent_region_always_forbidden() {
        let p = RegionPolicy::SameRegionOnly;
        assert!(!p.is_permitted(RegionId::ABSENT, RegionId::ABSENT));
        assert!(!p.is_permitted(RegionId::ABSENT, RegionId(7)));
        assert!(!p.is_permitted(RegionId(7), RegionId::ABSENT));
    }

    /// § RegionBoundary tracks blocks count.
    #[test]
    fn boundary_counts_blocks() {
        let mut b = RegionBoundary::default();
        assert_eq!(b.blocks(), 0);
        let _ = b.permits(RegionId(7), RegionId(8)); // blocked under SameRegionOnly
        assert_eq!(b.blocks(), 1);
        let _ = b.permits(RegionId(7), RegionId(7)); // permitted
        assert_eq!(b.blocks(), 1);
    }

    /// § RegionBoundary reset_blocks zeros the counter.
    #[test]
    fn boundary_reset_blocks() {
        let mut b = RegionBoundary::default();
        let _ = b.permits(RegionId(7), RegionId(8));
        assert_eq!(b.blocks(), 1);
        b.reset_blocks();
        assert_eq!(b.blocks(), 0);
    }

    /// § Default policy is SameRegionOnly.
    #[test]
    fn default_policy_is_strict() {
        let p = RegionPolicy::default();
        assert!(matches!(p, RegionPolicy::SameRegionOnly));
    }
}
