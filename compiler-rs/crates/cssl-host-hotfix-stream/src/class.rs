//! § class — hotfix taxonomy : id · 8 classes · tier · state · payload.
//!
//! Per `specs/grand-vision/16_MYCELIAL_NETWORK.csl` § "LIVE HOTFIXES &
//! IMPROVEMENTS" the substrate recognizes exactly 8 hotfix classes,
//! grouped into three policy-tiers.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ────────────────────────────────────────────────────────────────
// § fixed-array serde helpers
// ────────────────────────────────────────────────────────────────
//
// `serde` only auto-derives `Serialize`/`Deserialize` for arrays up
// to length 32 (and not at all for `[u8; 64]` in some versions).
// Since pulling in `serde-big-array` would breach the "no new
// external Cargo deps" hard-cap, we hand-roll byte-slice round-trip
// for the two fixed sizes used by `Hotfix` :  `[u8; 32]` (pubkey)
// and `[u8; 64]` (signature).
//
// Encoding : lower-case hex string. This is stable, human-readable
// in the audit JSON, and avoids any structural ambiguity.

mod hex_arr32 {
    use super::{from_hex_n, to_hex, Deserialize, Deserializer, Serializer};
    pub(super) fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&to_hex(bytes))
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        from_hex_n::<32, D>(&s)
    }
}

mod hex_arr64 {
    use super::{from_hex_n, to_hex, Deserialize, Deserializer, Serializer};
    pub(super) fn serialize<S: Serializer>(bytes: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&to_hex(bytes))
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let s = String::deserialize(d)?;
        from_hex_n::<64, D>(&s)
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn from_hex_n<'de, const N: usize, D: Deserializer<'de>>(s: &str) -> Result<[u8; N], D::Error> {
    use serde::de::Error;
    if s.len() != N * 2 {
        return Err(D::Error::custom(format!(
            "expected {} hex chars, got {}",
            N * 2,
            s.len()
        )));
    }
    let mut out = [0u8; N];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hex = std::str::from_utf8(chunk).map_err(D::Error::custom)?;
        out[i] = u8::from_str_radix(hex, 16).map_err(D::Error::custom)?;
    }
    Ok(out)
}

/// § Stable string-id for a hotfix payload. Σ-Chain assigns these.
///
/// Wraps `String` rather than `Uuid` so we keep the dep-tree
/// minimal ; format is opaque to this crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HotfixId(pub String);

impl HotfixId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// § The 8 hotfix classes ENUMERATED.
///
/// `repr(u8)` for stable wire encoding of the discriminant ; serde
/// uses the named variants so the .json output is human-readable.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum HotfixClass {
    /// HF-1 — retrained KAN classifier weights ; cosmetic-tier.
    KanWeightUpdate = 1,
    /// HF-2 — aggregate-feedback shifts material/biome distribution ;
    /// balance-tier (prompt + 30s revert).
    ProcgenBiasNudge = 2,
    /// HF-3 — gear-stat / mana-cost / status-effect-duration tuning ;
    /// balance-tier (prompt + 30s revert).
    BalanceConstantAdjust = 3,
    /// HF-4 — community-discovered alchemy recipes promoted-to-canon ;
    /// cosmetic-tier.
    NewRecipeUnlock = 4,
    /// HF-5 — Nemesis archetype evolves cohort-wide from
    /// aggregate-defeat-strategy ; cosmetic-tier.
    NemesisArchetypeEvolve = 5,
    /// HF-6 — sovereign-cap policy fix ; SECURITY-tier ;
    /// requires sovereign-cap before apply.
    SovereignCapPolicyFix = 6,
    /// HF-7 — new GM-narrative-storylet fragments ; cosmetic-tier.
    NarrativeStoryletAdd = 7,
    /// HF-8 — shader-uniform / render-pipeline-param tuning ;
    /// cosmetic-tier.
    RenderPipelineParam = 8,
}

impl HotfixClass {
    /// All 8 variants in stable order. Used by tests and registries.
    #[must_use]
    pub const fn all() -> [HotfixClass; 8] {
        [
            Self::KanWeightUpdate,
            Self::ProcgenBiasNudge,
            Self::BalanceConstantAdjust,
            Self::NewRecipeUnlock,
            Self::NemesisArchetypeEvolve,
            Self::SovereignCapPolicyFix,
            Self::NarrativeStoryletAdd,
            Self::RenderPipelineParam,
        ]
    }

    /// Map class → tier. Single source of truth for tier-policy.
    #[must_use]
    pub const fn tier(self) -> HotfixTier {
        match self {
            Self::KanWeightUpdate
            | Self::NewRecipeUnlock
            | Self::NemesisArchetypeEvolve
            | Self::NarrativeStoryletAdd
            | Self::RenderPipelineParam => HotfixTier::Cosmetic,

            Self::ProcgenBiasNudge | Self::BalanceConstantAdjust => HotfixTier::Balance,

            Self::SovereignCapPolicyFix => HotfixTier::Security,
        }
    }

    /// Stable string code (HF-1 .. HF-8) per spec § 16.
    #[must_use]
    pub const fn spec_code(self) -> &'static str {
        match self {
            Self::KanWeightUpdate => "HF-1",
            Self::ProcgenBiasNudge => "HF-2",
            Self::BalanceConstantAdjust => "HF-3",
            Self::NewRecipeUnlock => "HF-4",
            Self::NemesisArchetypeEvolve => "HF-5",
            Self::SovereignCapPolicyFix => "HF-6",
            Self::NarrativeStoryletAdd => "HF-7",
            Self::RenderPipelineParam => "HF-8",
        }
    }
}

/// § Policy tier — drives `policy::decide_apply`.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum HotfixTier {
    /// Auto-apply silently (HF-1, HF-4, HF-5, HF-7, HF-8).
    Cosmetic,
    /// Prompt user before apply ; armed 30-second revert window.
    Balance,
    /// Apply only if sovereign-cap `SOV_HOTFIX_APPLY` is set.
    Security,
}

/// § Lifecycle state of a hotfix in the local stream.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum HotfixState {
    /// Just polled from Σ-Chain ; sig + payload-hash unchecked.
    Pending,
    /// Ed25519-signature verified + payload BLAKE3 matches claim.
    Verified,
    /// Payload bytes copied into staging area ; not yet applied.
    Staged,
    /// Applied to runtime state ; revert window may still be open.
    Applied,
    /// Reverted via rollback (manual or 30s-window expiry).
    Reverted,
    /// Refused : sig-fail · hash-mismatch · policy-veto · cap-missing.
    Rejected,
}

/// § The hotfix message itself, as observed on the Σ-Chain feed.
///
/// `payload_blake3` is the *claimed* digest signed alongside the
/// `id + class + ts` envelope ; verify-pipeline checks the actual
/// `payload` bytes hash to that value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotfix {
    pub id: HotfixId,
    pub class: HotfixClass,
    /// Raw payload bytes (class-specific shape).
    pub payload: Vec<u8>,
    /// Claimed BLAKE3-256 of `payload`. Hex-encoded, lowercase.
    pub payload_blake3: String,
    /// Ed25519 signature over the canonical message `envelope_bytes`.
    #[serde(with = "hex_arr64")]
    pub ed25519_sig: [u8; 64],
    /// Public key claimed to be the issuer (Apocky-master-key).
    #[serde(with = "hex_arr32")]
    pub issuer_pubkey: [u8; 32],
    /// Σ-Chain timestamp (epoch nanoseconds).
    pub ts: u64,
    /// Tier, encoded by issuer for transparency. Verifier
    /// re-derives from `class.tier()` and rejects on mismatch.
    pub class_tier: HotfixTier,
}

impl Hotfix {
    /// Canonical signing-envelope : `id || class_byte || ts_le ||
    /// payload_blake3_hex_bytes`.
    ///
    /// Determinism rule : NEVER include floating-point or
    /// `HashMap`-ordered fields. All bytes are derived from stable
    /// integer / fixed-byte / static-string sources.
    #[must_use]
    pub fn envelope_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(
            self.id.0.len() + 1 + 8 + self.payload_blake3.len() + 4,
        );
        buf.extend_from_slice(self.id.0.as_bytes());
        buf.push(0); // separator
        buf.push(self.class as u8);
        buf.extend_from_slice(&self.ts.to_le_bytes());
        buf.push(0);
        buf.extend_from_slice(self.payload_blake3.as_bytes());
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(class: HotfixClass) -> Hotfix {
        Hotfix {
            id: HotfixId::new(format!("hf-{}", class as u8)),
            class,
            payload: vec![0xAA, 0xBB, 0xCC],
            payload_blake3: blake3::hash(&[0xAA, 0xBB, 0xCC]).to_hex().to_string(),
            ed25519_sig: [0u8; 64],
            issuer_pubkey: [0u8; 32],
            ts: 1_700_000_000_000_000_000,
            class_tier: class.tier(),
        }
    }

    /// 8-class-construction (counts as 8 distinct cases).
    #[test]
    fn class_kan_weight_constructs() {
        let h = fixture(HotfixClass::KanWeightUpdate);
        assert_eq!(h.class, HotfixClass::KanWeightUpdate);
        assert_eq!(h.class.tier(), HotfixTier::Cosmetic);
        assert_eq!(h.class.spec_code(), "HF-1");
    }

    #[test]
    fn class_procgen_bias_constructs() {
        let h = fixture(HotfixClass::ProcgenBiasNudge);
        assert_eq!(h.class.tier(), HotfixTier::Balance);
        assert_eq!(h.class.spec_code(), "HF-2");
    }

    #[test]
    fn class_balance_constant_constructs() {
        let h = fixture(HotfixClass::BalanceConstantAdjust);
        assert_eq!(h.class.tier(), HotfixTier::Balance);
        assert_eq!(h.class.spec_code(), "HF-3");
    }

    #[test]
    fn class_new_recipe_constructs() {
        let h = fixture(HotfixClass::NewRecipeUnlock);
        assert_eq!(h.class.tier(), HotfixTier::Cosmetic);
        assert_eq!(h.class.spec_code(), "HF-4");
    }

    #[test]
    fn class_nemesis_evolve_constructs() {
        let h = fixture(HotfixClass::NemesisArchetypeEvolve);
        assert_eq!(h.class.tier(), HotfixTier::Cosmetic);
        assert_eq!(h.class.spec_code(), "HF-5");
    }

    #[test]
    fn class_sovereign_cap_fix_constructs() {
        let h = fixture(HotfixClass::SovereignCapPolicyFix);
        assert_eq!(h.class.tier(), HotfixTier::Security);
        assert_eq!(h.class.spec_code(), "HF-6");
    }

    #[test]
    fn class_storylet_add_constructs() {
        let h = fixture(HotfixClass::NarrativeStoryletAdd);
        assert_eq!(h.class.tier(), HotfixTier::Cosmetic);
        assert_eq!(h.class.spec_code(), "HF-7");
    }

    #[test]
    fn class_render_param_constructs() {
        let h = fixture(HotfixClass::RenderPipelineParam);
        assert_eq!(h.class.tier(), HotfixTier::Cosmetic);
        assert_eq!(h.class.spec_code(), "HF-8");
    }

    #[test]
    fn all_returns_eight_distinct_classes_in_stable_order() {
        let all = HotfixClass::all();
        assert_eq!(all.len(), 8);
        // Discriminants strictly increasing 1..=8.
        for (i, c) in all.iter().enumerate() {
            assert_eq!(*c as u8, (i as u8) + 1);
        }
    }

    #[test]
    fn envelope_bytes_is_deterministic() {
        let h = fixture(HotfixClass::KanWeightUpdate);
        let a = h.envelope_bytes();
        let b = h.envelope_bytes();
        assert_eq!(a, b);
        // Mutating ts must change envelope.
        let mut h2 = h.clone();
        h2.ts = h.ts + 1;
        assert_ne!(a, h2.envelope_bytes());
    }

    /// serde round-trip (#1) : class enum json.
    #[test]
    fn class_serde_roundtrip() {
        for c in HotfixClass::all() {
            let s = serde_json::to_string(&c).unwrap();
            let back: HotfixClass = serde_json::from_str(&s).unwrap();
            assert_eq!(c, back);
        }
    }

    /// serde round-trip (#2) : full Hotfix struct.
    #[test]
    fn hotfix_serde_roundtrip() {
        let h = fixture(HotfixClass::BalanceConstantAdjust);
        let s = serde_json::to_string(&h).unwrap();
        let back: Hotfix = serde_json::from_str(&s).unwrap();
        assert_eq!(h, back);
    }
}
