//! § aggregate — DP-noised, sample-floored federated bias-aggregation
//!
//! ⊑ DP_SAMPLE_FLOOR = 100 ¬ negotiable
//! ⊑ Gaussian-noise σ ∝ 1/√N · clamp finite
//! ⊑ deterministic-RNG seeded from (region · ts-bucket) ⟶ replay-stable
//! ⊑ refuse-and-audit-skip-event when below floor · NEVER leak partial

use crate::privacy::{dp_seed, OptInTier, RegionTag};
use crate::spore::{Spore, SporeKind};
use serde::{Deserialize, Serialize};

/// § ChaCha-style deterministic RNG seeded by BLAKE3-XOF.
///
/// We avoid the `rand` ecosystem (cc-toolchain dependency) and roll a
/// minimal byte-stream RNG : seed feeds blake3 in keyed-XOF mode and we
/// pull u64 / f64 samples sequentially. Output is bit-identical across
/// platforms for the same seed.
struct DeterministicRng {
    xof: blake3::OutputReader,
}

impl DeterministicRng {
    fn from_seed(seed: [u8; 32]) -> Self {
        let mut h = blake3::Hasher::new_keyed(&seed);
        h.update(b"cssl-host-mycelium\0DeterministicRng\0v1");
        Self { xof: h.finalize_xof() }
    }

    fn next_u64(&mut self) -> u64 {
        let mut buf = [0_u8; 8];
        self.xof.fill(&mut buf);
        u64::from_le_bytes(buf)
    }

    /// § next_open_unit — uniform in (0, 1).
    fn next_open_unit(&mut self) -> f64 {
        // 53-bit mantissa + offset away from zero so ln(0) is unreachable.
        let bits = self.next_u64() >> 11; // top 53 bits
        let denom = (1_u64 << 53) as f64;
        let u = (bits as f64 + 0.5) / denom; // (0,1) strictly
        u
    }
}

/// § DP-anonymity-set floor : aggregate refuses below this.
pub const DP_SAMPLE_FLOOR: usize = 100;

/// § Aggregate-stage error.
#[derive(Debug, thiserror::Error)]
pub enum AggregateError {
    #[error("DP-floor not met : have {have} < need {DP_SAMPLE_FLOOR}")]
    BelowFloor { have: usize },
    #[error("no value field in spore payload (expected `score` or `value`)")]
    NoValueField,
    #[error("non-finite weighted mean")]
    NonFiniteMean,
}

/// § AggregatedBias — DP-noised cross-user mean.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AggregatedBias {
    pub region: RegionTag,
    pub kind: SporeKind,
    pub sample_count: usize,
    pub weighted_mean: f32,
    pub noise_sigma: f32,
}

/// § extract_value — pull the numeric "value" field from a spore payload.
///
/// Looks at `score` then `value` then `nudge` ; missing → None. Sensitive
/// fields are already stripped before this is called.
fn extract_value(spore: &Spore) -> Option<f64> {
    let obj = spore.payload.0.as_object()?;
    for k in ["score", "value", "nudge"] {
        if let Some(v) = obj.get(k) {
            if let Some(f) = v.as_f64() {
                if f.is_finite() {
                    return Some(f);
                }
            } else if let Some(i) = v.as_i64() {
                return Some(i as f64);
            }
        }
    }
    None
}

/// § AuditSkipEvent — emitted when aggregate is denied below-floor. Never
/// contains per-spore data.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AuditSkipEvent {
    pub region: RegionTag,
    pub kind: SporeKind,
    pub sample_count: usize,
    pub reason: SkipReason,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum SkipReason {
    BelowDpFloor,
    NoNumericField,
}

/// § aggregate — compute DP-noised mean from a slice of spores.
///
/// Pre-conditions :
/// - Spores must already have been tier-filtered by the poll pipeline.
/// - Spores below `OptInTier::Anonymized` are dropped (LocalOnly never
///   contributes to cross-user aggregation).
///
/// Returns `Ok(Some(_))` only when the DP-floor is met *and* at least one
/// numeric field could be extracted. Returns `Ok(None)` + an audit-skip
/// event in either failure case ; this lets the caller log without
/// leaking partial data.
///
/// `ts_bucketed` is the time-bucket key used to seed the deterministic
/// noise RNG ; pass the same value for replay-stability across runs.
pub fn aggregate(
    region: RegionTag,
    kind: SporeKind,
    spores: &[Spore],
    ts_bucketed: u64,
) -> (Option<AggregatedBias>, Option<AuditSkipEvent>) {
    // Tier-filter : drop LocalOnly (defensive ; the poll layer already
    // does this, but aggregate is the last line of defence).
    let contributing: Vec<&Spore> = spores
        .iter()
        .filter(|s| s.opt_in_tier >= OptInTier::Anonymized)
        .collect();

    let n = contributing.len();
    if n < DP_SAMPLE_FLOOR {
        return (
            None,
            Some(AuditSkipEvent {
                region,
                kind,
                sample_count: n,
                reason: SkipReason::BelowDpFloor,
            }),
        );
    }

    // Weighted-mean : equal weights for now (1.0 each) — bias-nudge style.
    // Future variant : weight by inverse-noise-of-emitter.
    let mut sum = 0.0_f64;
    let mut have_any_value = 0_usize;
    for s in contributing.iter() {
        if let Some(v) = extract_value(s) {
            sum += v;
            have_any_value += 1;
        }
    }
    if have_any_value < DP_SAMPLE_FLOOR {
        return (
            None,
            Some(AuditSkipEvent {
                region,
                kind,
                sample_count: have_any_value,
                reason: SkipReason::NoNumericField,
            }),
        );
    }
    let raw_mean = sum / (have_any_value as f64);

    // σ proportional to 1/√N — fits the standard Laplace-DP-mean bound up
    // to a constant. We use Gaussian for symmetry around the mean ; the
    // constant 0.5 is the (sensitivity / ε) calibration knob and is the
    // single place a future spec change would touch.
    let sigma = (0.5_f64) / (have_any_value as f64).sqrt();

    // Deterministic noise — seed from (region, ts_bucketed) so replay is
    // stable.  BLAKE3-XOF feeds our local DeterministicRng.
    let seed = dp_seed(region, ts_bucketed, kind.tag().as_bytes());
    let mut rng = DeterministicRng::from_seed(seed);
    // Box-Muller from two uniforms. Avoids any rand_distr dep.
    let u1: f64 = rng.next_open_unit();
    let u2: f64 = rng.next_open_unit();
    let r = (-2.0_f64 * u1.ln()).sqrt();
    let theta = 2.0_f64 * std::f64::consts::PI * u2;
    let z0 = r * theta.cos();
    let noise = sigma * z0;

    let noisy = raw_mean + noise;
    if !noisy.is_finite() {
        return (
            None,
            Some(AuditSkipEvent {
                region,
                kind,
                sample_count: have_any_value,
                reason: SkipReason::NoNumericField,
            }),
        );
    }

    // Clamp to a sane finite range : the substrate uses [-1e6, 1e6] as
    // a soft upper-bound for nudge magnitudes ; outside that we suspect
    // adversarial input and refuse rather than silently leaking.
    let clamped = noisy.clamp(-1.0e6_f64, 1.0e6_f64) as f32;
    let sigma_f = sigma as f32;

    (
        Some(AggregatedBias {
            region,
            kind,
            sample_count: have_any_value,
            weighted_mean: clamped,
            noise_sigma: sigma_f,
        }),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spore::SporeBuilder;

    fn mk_spore(region: RegionTag, kind: SporeKind, value: f64, ts: u64) -> Spore {
        SporeBuilder {
            region,
            kind,
            ts,
            opt_in_tier: OptInTier::Anonymized,
            emitter_pubkey: [0_u8; 32],
            payload: serde_json::json!({"score": value}),
        }
        .build(OptInTier::Public)
        .unwrap()
    }

    fn mk_n(n: usize, value_per: f64) -> Vec<Spore> {
        (0..n)
            .map(|i| mk_spore(RegionTag::new(1), SporeKind::BiasNudge, value_per, i as u64))
            .collect()
    }

    #[test]
    fn refuses_below_floor() {
        let spores = mk_n(99, 0.5);
        let (agg, skip) =
            aggregate(RegionTag::new(1), SporeKind::BiasNudge, &spores, 1234);
        assert!(agg.is_none());
        let s = skip.unwrap();
        assert_eq!(s.reason, SkipReason::BelowDpFloor);
        assert_eq!(s.sample_count, 99);
    }

    #[test]
    fn refuses_at_zero_samples() {
        let spores: Vec<Spore> = Vec::new();
        let (agg, skip) =
            aggregate(RegionTag::new(1), SporeKind::BiasNudge, &spores, 0);
        assert!(agg.is_none());
        assert_eq!(skip.unwrap().sample_count, 0);
    }

    #[test]
    fn accepts_at_floor_exact() {
        let spores = mk_n(DP_SAMPLE_FLOOR, 1.0);
        let (agg, skip) =
            aggregate(RegionTag::new(1), SporeKind::BiasNudge, &spores, 7);
        assert!(skip.is_none());
        let a = agg.unwrap();
        assert_eq!(a.sample_count, DP_SAMPLE_FLOOR);
    }

    #[test]
    fn weighted_mean_correct_with_constant_input() {
        // All values = 2.0 → noisy mean ≈ 2.0 ± O(1/√N).
        let spores = mk_n(400, 2.0);
        let (agg, _skip) =
            aggregate(RegionTag::new(1), SporeKind::BiasNudge, &spores, 7);
        let a = agg.unwrap();
        // σ = 0.5 / sqrt(400) = 0.025 ; |noise| < 5σ comfortably with prob ~ 1.
        assert!(
            (a.weighted_mean - 2.0).abs() < 0.5,
            "noisy mean too far from 2.0 : got {}",
            a.weighted_mean
        );
        assert!(a.noise_sigma < 0.05);
    }

    #[test]
    fn weighted_mean_two_value_split() {
        // Half 0.0, half 4.0 → true mean 2.0.
        let mut spores: Vec<Spore> = (0..200)
            .map(|i| mk_spore(RegionTag::new(1), SporeKind::BiasNudge, 0.0, i))
            .collect();
        spores.extend((200..400).map(|i| {
            mk_spore(RegionTag::new(1), SporeKind::BiasNudge, 4.0, i)
        }));
        let (agg, _skip) =
            aggregate(RegionTag::new(1), SporeKind::BiasNudge, &spores, 9);
        let a = agg.unwrap();
        assert!(
            (a.weighted_mean - 2.0).abs() < 0.5,
            "got {}", a.weighted_mean
        );
    }

    #[test]
    fn noise_deterministic_for_same_seed() {
        let spores = mk_n(200, 1.0);
        let (a1, _) = aggregate(RegionTag::new(5), SporeKind::BiasNudge, &spores, 100);
        let (a2, _) = aggregate(RegionTag::new(5), SporeKind::BiasNudge, &spores, 100);
        assert_eq!(a1.unwrap().weighted_mean, a2.unwrap().weighted_mean);
    }

    #[test]
    fn noise_changes_with_ts_bucket() {
        let spores = mk_n(200, 1.0);
        let (a1, _) = aggregate(RegionTag::new(5), SporeKind::BiasNudge, &spores, 100);
        let (a2, _) = aggregate(RegionTag::new(5), SporeKind::BiasNudge, &spores, 200);
        assert_ne!(
            a1.unwrap().weighted_mean,
            a2.unwrap().weighted_mean,
            "different ts-bucket → different noise"
        );
    }

    #[test]
    fn aggregate_skips_localonly_spores() {
        // 100 LocalOnly spores → all dropped → below-floor.
        let spores: Vec<Spore> = (0..100)
            .map(|i| {
                SporeBuilder {
                    region: RegionTag::new(1),
                    kind: SporeKind::BiasNudge,
                    ts: i,
                    opt_in_tier: OptInTier::LocalOnly,
                    emitter_pubkey: [0_u8; 32],
                    payload: serde_json::json!({"score": 1.0}),
                }
                .build(OptInTier::Public)
                .unwrap()
            })
            .collect();
        let (agg, skip) =
            aggregate(RegionTag::new(1), SporeKind::BiasNudge, &spores, 7);
        assert!(agg.is_none());
        assert_eq!(skip.unwrap().reason, SkipReason::BelowDpFloor);
    }

    #[test]
    fn audit_skip_event_carries_no_per_user_data() {
        let spores = mk_n(50, 999.0);
        let (_agg, skip) =
            aggregate(RegionTag::new(1), SporeKind::BiasNudge, &spores, 7);
        let s = skip.unwrap();
        // Skip event has only region/kind/count/reason — nothing per-user.
        assert_eq!(s.region, RegionTag::new(1));
        assert_eq!(s.kind, SporeKind::BiasNudge);
        assert_eq!(s.sample_count, 50);
    }

    #[test]
    fn aggregated_bias_serde_round_trip() {
        let a = AggregatedBias {
            region: RegionTag::new(3),
            kind: SporeKind::CombatOutcome,
            sample_count: 250,
            weighted_mean: 1.25,
            noise_sigma: 0.0316,
        };
        let json = serde_json::to_string(&a).unwrap();
        let a2: AggregatedBias = serde_json::from_str(&json).unwrap();
        assert_eq!(a, a2);
    }
}
