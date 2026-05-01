//! § privacy — opt-in tier ordering + sensitive-field strip + DP RNG
//!
//! ⊑ OptInTier total-order : LocalOnly < Anonymized < Pseudonymous < Public
//! ⊑ Sensitive<biometric|gaze|face|body> ¬ ever-egress @ emit
//! ⊑ DP-noise via deterministic seeded-RNG (replay-stable per (region, ts-bucket))

use serde::{Deserialize, Serialize};

/// § OptInTier — caller's consent surface for cross-user federation.
///
/// Total-order ascending : `LocalOnly` (most-restrictive) → `Public`. A
/// nutrient-poll at tier `T` returns spores whose author opted-in at any
/// tier ≤ `T`. Escalation (poller at `LocalOnly` receiving a `Public` spore)
/// is forbidden by `OptInTier::permits`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum OptInTier {
    /// § LocalOnly — never leave this user's sovereign boundary.
    LocalOnly = 0,
    /// § Anonymized — strip identity ; aggregate-only consumption.
    Anonymized = 1,
    /// § Pseudonymous — stable opaque-handle ; per-user query allowed.
    Pseudonymous = 2,
    /// § Public — fully attributable.
    Public = 3,
}

impl OptInTier {
    /// § Returns `true` iff a poller at tier `self` may receive a spore
    /// emitted at tier `spore_tier`. Escalation is forbidden.
    #[inline]
    #[must_use]
    pub fn permits(self, spore_tier: OptInTier) -> bool {
        // A poll at level T sees spores at level ≤ T. A LocalOnly poll
        // sees only LocalOnly spores ; a Public poll sees all.
        spore_tier as u8 <= self as u8
    }

    /// § Promote to the more-restrictive of two tiers (intersection).
    #[inline]
    #[must_use]
    pub fn most_restrictive(a: OptInTier, b: OptInTier) -> OptInTier {
        if (a as u8) <= (b as u8) {
            a
        } else {
            b
        }
    }
}

/// § RegionTag — coarse partition for cross-user queries.
///
/// 16-bit opaque. Mapped externally to `(world-shard, biome, season-epoch)`
/// or any other partitioning the host crate chooses. Determinism is
/// preserved across replay because the tag is part of the DP-RNG seed.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct RegionTag(pub u16);

impl RegionTag {
    pub const fn new(v: u16) -> Self {
        Self(v)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// § SensitiveField — discrim for fields that are structurally banned at
/// egress under PRIME_DIRECTIVE §§ 2 + 4.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum SensitiveField {
    Biometric,
    Gaze,
    Face,
    Body,
}

impl SensitiveField {
    /// § str-tag used for substring-strip in JSON payloads.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            SensitiveField::Biometric => "biometric",
            SensitiveField::Gaze => "gaze",
            SensitiveField::Face => "face",
            SensitiveField::Body => "body",
        }
    }

    /// § canonical list — used by [`strip_sensitive`] to enforce the
    /// PRIME_DIRECTIVE structural-ban.
    pub const ALL: [SensitiveField; 4] = [
        SensitiveField::Biometric,
        SensitiveField::Gaze,
        SensitiveField::Face,
        SensitiveField::Body,
    ];
}

/// § strip_sensitive — remove Sensitive<*> keys from a serde_json::Value
/// in-place. Called at emit so sensitive fields *cannot* leave the
/// sovereign boundary, even if a caller forgot to filter them upstream.
///
/// Strip rules:
/// - Object keys whose lowercased name contains any [`SensitiveField`] tag
///   are removed.
/// - Recurses into nested objects + arrays.
/// - Returns the count of stripped fields (for audit).
pub fn strip_sensitive(value: &mut serde_json::Value) -> usize {
    let mut stripped = 0_usize;
    strip_sensitive_recursive(value, &mut stripped);
    stripped
}

fn strip_sensitive_recursive(value: &mut serde_json::Value, count: &mut usize) {
    match value {
        serde_json::Value::Object(map) => {
            // Two-pass : collect doomed keys, then remove ; iterating-while-
            // mutating an object is not stable.
            let doomed: Vec<String> = map
                .keys()
                .filter(|k| is_sensitive_key(k))
                .cloned()
                .collect();
            for k in doomed {
                map.remove(&k);
                *count += 1;
            }
            for (_, child) in map.iter_mut() {
                strip_sensitive_recursive(child, count);
            }
        }
        serde_json::Value::Array(arr) => {
            for child in arr.iter_mut() {
                strip_sensitive_recursive(child, count);
            }
        }
        _ => {}
    }
}

#[inline]
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SensitiveField::ALL
        .iter()
        .any(|s| lower.contains(s.tag()))
}

/// § dp_seed — deterministic 32-byte seed derived from region + time-bucket.
///
/// `bucket_seconds` should be ≥ 60 to avoid trivially leaking single-user
/// timing. Replay-stable : same (region, ts) → same seed → same noise.
#[must_use]
pub fn dp_seed(region: RegionTag, ts_bucketed: u64, salt: &[u8]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"cssl-host-mycelium\0dp-seed\0v1");
    h.update(&region.0.to_le_bytes());
    h.update(&ts_bucketed.to_le_bytes());
    h.update(salt);
    *h.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_in_tier_total_order() {
        assert!(OptInTier::LocalOnly < OptInTier::Anonymized);
        assert!(OptInTier::Anonymized < OptInTier::Pseudonymous);
        assert!(OptInTier::Pseudonymous < OptInTier::Public);
    }

    #[test]
    fn opt_in_tier_permits_descending() {
        // A Public-poller can see everything ≤ Public.
        assert!(OptInTier::Public.permits(OptInTier::LocalOnly));
        assert!(OptInTier::Public.permits(OptInTier::Public));
        // A LocalOnly-poller can see only LocalOnly.
        assert!(OptInTier::LocalOnly.permits(OptInTier::LocalOnly));
        assert!(!OptInTier::LocalOnly.permits(OptInTier::Public));
        assert!(!OptInTier::LocalOnly.permits(OptInTier::Anonymized));
        // Pseudonymous sees ≤ Pseudonymous, not Public.
        assert!(OptInTier::Pseudonymous.permits(OptInTier::Anonymized));
        assert!(!OptInTier::Pseudonymous.permits(OptInTier::Public));
    }

    #[test]
    fn strip_sensitive_top_level_keys() {
        let mut v = serde_json::json!({
            "biometric_pulse": 72,
            "gaze_dwell_ms": 12000,
            "face_landmarks": [1, 2, 3],
            "body_pose": {"hip": 1.0},
            "score": 42,
            "region": "alpha",
        });
        let n = strip_sensitive(&mut v);
        assert_eq!(n, 4, "should strip 4 sensitive top-level keys");
        let obj = v.as_object().unwrap();
        assert!(obj.contains_key("score"));
        assert!(obj.contains_key("region"));
        assert!(!obj.contains_key("biometric_pulse"));
        assert!(!obj.contains_key("gaze_dwell_ms"));
        assert!(!obj.contains_key("face_landmarks"));
        assert!(!obj.contains_key("body_pose"));
    }

    #[test]
    fn strip_sensitive_nested_object() {
        let mut v = serde_json::json!({
            "outer": {
                "biometric_hr": 60,
                "ok": "fine",
                "deeper": {"gaze_x": 0.1, "ok": 1}
            },
            "ok": true
        });
        let n = strip_sensitive(&mut v);
        assert_eq!(n, 2);
        // outer.ok survives ; outer.biometric_hr stripped.
        let outer = v.get("outer").unwrap().as_object().unwrap();
        assert!(outer.contains_key("ok"));
        assert!(!outer.contains_key("biometric_hr"));
        let deeper = outer.get("deeper").unwrap().as_object().unwrap();
        assert!(!deeper.contains_key("gaze_x"));
        assert!(deeper.contains_key("ok"));
    }

    #[test]
    fn strip_sensitive_array_of_objects() {
        let mut v = serde_json::json!([
            {"face_pts": 1, "x": 1},
            {"y": 2},
            {"body_kg": 70.0, "z": 3}
        ]);
        let n = strip_sensitive(&mut v);
        assert_eq!(n, 2);
        let arr = v.as_array().unwrap();
        assert!(!arr[0].as_object().unwrap().contains_key("face_pts"));
        assert!(arr[0].as_object().unwrap().contains_key("x"));
        assert!(arr[2].as_object().unwrap().contains_key("z"));
        assert!(!arr[2].as_object().unwrap().contains_key("body_kg"));
    }

    #[test]
    fn strip_sensitive_case_insensitive() {
        let mut v = serde_json::json!({
            "Biometric_Score": 1,
            "GAZE_HEATMAP": 2,
            "Face": "x",
            "Body": "y",
            "ok": 3,
        });
        let n = strip_sensitive(&mut v);
        assert_eq!(n, 4);
        assert!(v.get("ok").is_some());
    }

    #[test]
    fn dp_seed_deterministic() {
        let r = RegionTag::new(7);
        let s1 = dp_seed(r, 42, b"salt");
        let s2 = dp_seed(r, 42, b"salt");
        assert_eq!(s1, s2);
    }

    #[test]
    fn dp_seed_changes_with_region() {
        let s1 = dp_seed(RegionTag::new(1), 42, b"salt");
        let s2 = dp_seed(RegionTag::new(2), 42, b"salt");
        assert_ne!(s1, s2);
    }

    #[test]
    fn dp_seed_changes_with_ts() {
        let r = RegionTag::new(1);
        let s1 = dp_seed(r, 41, b"salt");
        let s2 = dp_seed(r, 42, b"salt");
        assert_ne!(s1, s2);
    }

    #[test]
    fn most_restrictive_picks_lower() {
        assert_eq!(
            OptInTier::most_restrictive(OptInTier::Public, OptInTier::LocalOnly),
            OptInTier::LocalOnly
        );
        assert_eq!(
            OptInTier::most_restrictive(OptInTier::Anonymized, OptInTier::Pseudonymous),
            OptInTier::Anonymized
        );
    }
}
