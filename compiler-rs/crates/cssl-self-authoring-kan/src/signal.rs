//! § signal.rs — QualitySignal taxonomy + Q14 weight-arithmetic.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § QualitySignal
//!   The single discriminant feeding the Self-Authoring-KAN-Loop. Each variant
//!   carries a small numeric payload bounded to fit in a 16-byte record together
//!   with the (template_id · archetype · player_handle · frame_offset) header.
//!
//!   The MAGNITUDE → Q14 mapping is variant-specific :
//!
//! ```text
//!   Variant                       | bias-delta sign | magnitude-source
//!   ──────────────────────────────┼─────────────────┼─────────────────
//!   SandboxPass                   | +               | const +Q14_DELTA_SMALL
//!   SandboxFail                   | -               | const -Q14_DELTA_LARGE
//!   GmAccept(score: u16 Q14)      | +               | scaled by score
//!   GmReject(reason: u8)          | -               | const -Q14_DELTA_MED
//!   PlayerLike(weight: u16 Q14)   | +               | scaled by weight
//!   PlayerDislike(weight: u16 Q14)| -               | scaled by weight
//!   RemixForked                   | +               | const +Q14_DELTA_MED
//!   RatingFiveStar                | +               | const +Q14_DELTA_MED
//!   RatingOneStar                 | -               | const -Q14_DELTA_MED
//!   PlaytestPass(score: u16 Q14)  | +               | scaled by score
//! ```

use crate::bias_map::{BIAS_Q14_MAX, BIAS_Q14_MIN};

// ───────────────────────────────────────────────────────────────────────────
// § Q14 fixed-point delta constants
// ───────────────────────────────────────────────────────────────────────────

/// Q14 small-delta : 0.05 ≈ 819. Used for sandbox-pass (cheap signal).
pub const Q14_DELTA_SMALL: i16 = 819;
/// Q14 medium-delta : 0.25 ≈ 4096. Used for {gm-reject · remix-forked ·
/// rating-five-star · rating-one-star}.
pub const Q14_DELTA_MED: i16 = 4096;
/// Q14 large-delta : 0.50 ≈ 8192. Used for sandbox-fail (heavy signal).
pub const Q14_DELTA_LARGE: i16 = 8192;

// ───────────────────────────────────────────────────────────────────────────
// § QualitySignal — the canonical discriminant.
// ───────────────────────────────────────────────────────────────────────────

/// Quality-signal kinds emitted by sibling-agents into the loop.
///
/// § INVARIANT : every variant maps deterministically to a Q14 bias-delta
/// via [`QualitySignal::to_q14_delta`]. The reservoir stores the variant
/// tag and payload separately so that reservoir-sampling is bit-pack-friendly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum QualitySignal {
    /// Sandbox-evaluation passed (e.g. compiled+ran cleanly under cssl-host-coder-runtime).
    SandboxPass = 0,
    /// Sandbox-evaluation failed (compile-error · runtime-panic · effect-violation).
    SandboxFail = 1,
    /// GM accepted with a rated score in Q14 ([0.0, 1.0]).
    GmAccept(u16) = 2,
    /// GM rejected with reason-code (0..=255 ; semantics in spec).
    GmReject(u8) = 3,
    /// Player liked with weight in Q14 (0.0..1.0 ; default = +0.5 for casual likes).
    PlayerLike(u16) = 4,
    /// Player disliked with weight in Q14 (0.0..1.0 ; default = +0.5 for casual dislikes).
    PlayerDislike(u16) = 5,
    /// Remix-forked the procgen-template (positive · player invested time).
    RemixForked = 6,
    /// Rated 5-star (strong endorsement).
    RatingFiveStar = 7,
    /// Rated 1-star (strong rejection).
    RatingOneStar = 8,
    /// Playtest-arena passed with score in Q14 (0.0..1.0).
    PlaytestPass(u16) = 9,
}

impl QualitySignal {
    /// Number of variants (excluding any future Unknown sentinel).
    pub const COUNT: usize = 10;

    /// Variant index (the `#[repr(u8)]` discriminant).
    pub const fn tag(self) -> u8 {
        match self {
            Self::SandboxPass => 0,
            Self::SandboxFail => 1,
            Self::GmAccept(_) => 2,
            Self::GmReject(_) => 3,
            Self::PlayerLike(_) => 4,
            Self::PlayerDislike(_) => 5,
            Self::RemixForked => 6,
            Self::RatingFiveStar => 7,
            Self::RatingOneStar => 8,
            Self::PlaytestPass(_) => 9,
        }
    }

    /// Const-time variant name (LUT-style ; no allocation).
    pub const fn name(self) -> &'static str {
        match self {
            Self::SandboxPass => "sandbox.pass",
            Self::SandboxFail => "sandbox.fail",
            Self::GmAccept(_) => "gm.accept",
            Self::GmReject(_) => "gm.reject",
            Self::PlayerLike(_) => "player.like",
            Self::PlayerDislike(_) => "player.dislike",
            Self::RemixForked => "remix.forked",
            Self::RatingFiveStar => "rating.five_star",
            Self::RatingOneStar => "rating.one_star",
            Self::PlaytestPass(_) => "playtest.pass",
        }
    }

    /// Map signal → Q14 bias-delta (saturating-clamped to ±BIAS_Q14_MAX).
    ///
    /// § INVARIANT : the returned delta is always in `[BIAS_Q14_MIN, BIAS_Q14_MAX]`.
    pub fn to_q14_delta(self) -> i16 {
        let raw = match self {
            Self::SandboxPass => i32::from(Q14_DELTA_SMALL),
            Self::SandboxFail => -i32::from(Q14_DELTA_LARGE),
            Self::GmAccept(score) => {
                // Q14 score scaled by med-delta : (score * MED) / Q14_ONE.
                ((i32::from(score) * i32::from(Q14_DELTA_MED)) / 16384).clamp(0, i32::from(Q14_DELTA_LARGE))
            }
            Self::GmReject(_) => -i32::from(Q14_DELTA_MED),
            Self::PlayerLike(weight) => {
                ((i32::from(weight) * i32::from(Q14_DELTA_MED)) / 16384).clamp(0, i32::from(Q14_DELTA_LARGE))
            }
            Self::PlayerDislike(weight) => {
                -((i32::from(weight) * i32::from(Q14_DELTA_MED)) / 16384).clamp(0, i32::from(Q14_DELTA_LARGE))
            }
            Self::RemixForked => i32::from(Q14_DELTA_MED),
            Self::RatingFiveStar => i32::from(Q14_DELTA_MED),
            Self::RatingOneStar => -i32::from(Q14_DELTA_MED),
            Self::PlaytestPass(score) => {
                ((i32::from(score) * i32::from(Q14_DELTA_MED)) / 16384).clamp(0, i32::from(Q14_DELTA_LARGE))
            }
        };
        raw.clamp(i32::from(BIAS_Q14_MIN), i32::from(BIAS_Q14_MAX)) as i16
    }

    /// Bit-pack the signal into a 4-byte (tag · payload-hi · payload-lo · reserved) representation.
    ///
    /// § LAYOUT
    ///   - byte 0 : tag (0..=9 ; Self::COUNT)
    ///   - byte 1..=2 : little-endian u16 payload (0 for nullary variants)
    ///   - byte 3 : reserved (zero)
    pub fn pack4(self) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[0] = self.tag();
        let payload_u16: u16 = match self {
            Self::GmAccept(s) | Self::PlaytestPass(s) | Self::PlayerLike(s) | Self::PlayerDislike(s) => s,
            Self::GmReject(reason) => u16::from(reason),
            _ => 0,
        };
        out[1..3].copy_from_slice(&payload_u16.to_le_bytes());
        out
    }

    /// Inverse of [`Self::pack4`]. Returns None for unknown tags.
    pub fn unpack4(bytes: [u8; 4]) -> Option<Self> {
        let tag = bytes[0];
        let payload = u16::from_le_bytes([bytes[1], bytes[2]]);
        match tag {
            0 => Some(Self::SandboxPass),
            1 => Some(Self::SandboxFail),
            2 => Some(Self::GmAccept(payload)),
            3 => Some(Self::GmReject(payload as u8)),
            4 => Some(Self::PlayerLike(payload)),
            5 => Some(Self::PlayerDislike(payload)),
            6 => Some(Self::RemixForked),
            7 => Some(Self::RatingFiveStar),
            8 => Some(Self::RatingOneStar),
            9 => Some(Self::PlaytestPass(payload)),
            _ => None,
        }
    }

    /// Sign of this signal's bias-delta (true = positive, false = negative).
    pub const fn is_positive(self) -> bool {
        matches!(
            self,
            Self::SandboxPass
                | Self::GmAccept(_)
                | Self::PlayerLike(_)
                | Self::RemixForked
                | Self::RatingFiveStar
                | Self::PlaytestPass(_)
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_tag_distinct_for_each_variant() {
        let all = [
            QualitySignal::SandboxPass,
            QualitySignal::SandboxFail,
            QualitySignal::GmAccept(0),
            QualitySignal::GmReject(0),
            QualitySignal::PlayerLike(0),
            QualitySignal::PlayerDislike(0),
            QualitySignal::RemixForked,
            QualitySignal::RatingFiveStar,
            QualitySignal::RatingOneStar,
            QualitySignal::PlaytestPass(0),
        ];
        let mut tags: Vec<u8> = all.iter().map(|s| s.tag()).collect();
        tags.sort_unstable();
        let prev_len = tags.len();
        tags.dedup();
        assert_eq!(tags.len(), prev_len, "tags must be unique");
        assert_eq!(tags.len(), QualitySignal::COUNT);
    }

    #[test]
    fn q14_delta_sign_matches_polarity() {
        assert!(QualitySignal::SandboxPass.to_q14_delta() > 0);
        assert!(QualitySignal::SandboxFail.to_q14_delta() < 0);
        assert!(QualitySignal::GmAccept(8192).to_q14_delta() > 0);
        assert!(QualitySignal::GmReject(7).to_q14_delta() < 0);
        assert!(QualitySignal::RemixForked.to_q14_delta() > 0);
        assert!(QualitySignal::RatingOneStar.to_q14_delta() < 0);
        assert!(QualitySignal::RatingFiveStar.to_q14_delta() > 0);
    }

    #[test]
    fn q14_delta_clamped_within_range() {
        for s in [
            QualitySignal::SandboxPass,
            QualitySignal::SandboxFail,
            QualitySignal::GmAccept(u16::MAX),
            QualitySignal::PlayerDislike(u16::MAX),
            QualitySignal::PlaytestPass(u16::MAX),
        ] {
            let d = s.to_q14_delta();
            assert!(
                (BIAS_Q14_MIN..=BIAS_Q14_MAX).contains(&d),
                "{:?} delta {} out of [{}..{}]",
                s,
                d,
                BIAS_Q14_MIN,
                BIAS_Q14_MAX
            );
        }
    }

    #[test]
    fn pack_unpack_roundtrip() {
        for s in [
            QualitySignal::SandboxPass,
            QualitySignal::SandboxFail,
            QualitySignal::GmAccept(12345),
            QualitySignal::GmReject(42),
            QualitySignal::PlayerLike(8192),
            QualitySignal::PlayerDislike(16383),
            QualitySignal::RemixForked,
            QualitySignal::RatingFiveStar,
            QualitySignal::RatingOneStar,
            QualitySignal::PlaytestPass(7777),
        ] {
            let bytes = s.pack4();
            let back = QualitySignal::unpack4(bytes).expect("roundtrip");
            assert_eq!(s, back, "pack/unpack roundtrip for {:?}", s);
        }
    }

    #[test]
    fn names_unique_and_well_formed() {
        let all = [
            QualitySignal::SandboxPass,
            QualitySignal::SandboxFail,
            QualitySignal::GmAccept(0),
            QualitySignal::GmReject(0),
            QualitySignal::PlayerLike(0),
            QualitySignal::PlayerDislike(0),
            QualitySignal::RemixForked,
            QualitySignal::RatingFiveStar,
            QualitySignal::RatingOneStar,
            QualitySignal::PlaytestPass(0),
        ];
        let mut names: Vec<&str> = all.iter().map(|s| s.name()).collect();
        names.sort_unstable();
        let prev_len = names.len();
        names.dedup();
        assert_eq!(names.len(), prev_len, "names must be unique");
        for n in names {
            assert!(n.contains('.'), "name '{}' should be dotted-namespace", n);
        }
    }
}
