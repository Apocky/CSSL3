// § run-share ← GDDs/ROGUELIKE_LOOP.csl §RUN-SHARING (gift-economy)
// ════════════════════════════════════════════════════════════════════
// § I> completed-run → snapshot ; serializer for cssl-edge async-MP push
// § I> NO leaderboards · NO PvP · NO ranked — gift-economy axiom
// § I> player-consent-gated upload ; revocable-per-share
// ════════════════════════════════════════════════════════════════════

use crate::biome_dag::Biome;
use serde::{Deserialize, Serialize};

/// § Screenshot-asset handle (opaque ; resolved by cssl-host-asset-bundle).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotHandle {
    /// FNV-1a-128 fingerprint hex-string of the screenshot bytes.
    pub fingerprint_hex: String,
    /// Asset-bundle handle (URI or asset-id) ; opaque to this crate.
    pub asset_uri: String,
}

/// § Scoring shape — gift-economy ; ¬ leaderboards.
///
/// Score is creator-self-reported · "personal-best" oriented · consumer
/// receives this AS GIFT — not for ranked-comparison. The friend who attempts
/// the seed sees the creator's score for context only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunShareScoring {
    /// Boss-arenas cleared this run.
    pub boss_clears: u32,
    /// Total Echoes earned in-run (pre-soft-perma split).
    pub echoes_earned: u64,
    /// Run duration in milliseconds.
    pub duration_ms: u32,
    /// Creator-attested style-tag (e.g. "speed", "completionist", "pacifist").
    pub style_tag: String,
}

/// § Run-share receipt ← serializable for cssl-edge async-MP endpoint.
///
/// Sealed by the creator's signing-key (handled by cssl-host-attestation) ;
/// this crate produces the unsigned payload only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunShareReceipt {
    /// Pinned u128 seed — friend-attempt replays this seed.
    pub seed: u128,
    /// Biome-path traversed (DAG-walk).
    pub biome_path: Vec<Biome>,
    /// Floor-count reached.
    pub floor_count: u8,
    /// Final scoring (gift-economy shape).
    pub scoring: RunShareScoring,
    /// Optional creator-screenshot handle.
    pub screenshot: Option<ScreenshotHandle>,
    /// Run-id-counter at creation (monotonic per-player).
    pub run_id: u64,
    /// Creator-self-attested ConsentToken-key for gift-share. Empty = ¬ shareable.
    pub consent_token_key: String,
    /// Spec-anchor for forward-compat decoders.
    pub spec_anchor: String,
}

impl RunShareReceipt {
    /// Construct a fresh receipt from minimal inputs ; caller fills extras.
    pub fn new(
        seed: u128,
        biome_path: Vec<Biome>,
        floor_count: u8,
        scoring: RunShareScoring,
        run_id: u64,
        consent_token_key: impl Into<String>,
    ) -> Self {
        Self {
            seed,
            biome_path,
            floor_count,
            scoring,
            screenshot: None,
            run_id,
            consent_token_key: consent_token_key.into(),
            spec_anchor: crate::SPEC_ANCHOR.to_string(),
        }
    }

    /// JSON-serialize for cssl-edge upload.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Is this receipt-payload eligible for upload ? (consent-gated).
    pub fn is_shareable(&self) -> bool {
        !self.consent_token_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unshareable_when_consent_empty() {
        let r = RunShareReceipt::new(
            0x1234,
            vec![Biome::Crypt],
            5,
            RunShareScoring {
                boss_clears: 1,
                echoes_earned: 100,
                duration_ms: 10000,
                style_tag: String::new(),
            },
            1,
            "",
        );
        assert!(!r.is_shareable());
    }

    #[test]
    fn json_roundtrips() {
        let r = RunShareReceipt::new(
            0xDEAD,
            vec![Biome::Crypt, Biome::Citadel],
            7,
            RunShareScoring {
                boss_clears: 2,
                echoes_earned: 500,
                duration_ms: 25000,
                style_tag: "speed".into(),
            },
            12,
            "consent-token-abc",
        );
        let s = r.to_json().unwrap();
        let r2: RunShareReceipt = serde_json::from_str(&s).unwrap();
        assert_eq!(r, r2);
    }
}
