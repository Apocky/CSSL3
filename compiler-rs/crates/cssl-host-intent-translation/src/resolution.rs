//! § resolution — IntentResolution + fingerprints + crystal-spawn-params.
#![allow(clippy::module_name_repetitions)]

use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};

use crate::intent_category::{Classification, IntentCategory};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentResolution {
    pub category: IntentCategory,
    pub confidence_pct: u8,
    pub token_count: u32,
    pub seed: u64,
    pub text_fingerprint: u32,
    pub dispatch_fingerprint: u32,
}

impl IntentResolution {
    pub fn from_classification(
        classification: Classification,
        seed: u64,
        token_count: u32,
        text_fingerprint: u32,
    ) -> Self {
        let dispatch_fingerprint = compute_dispatch_fingerprint(
            classification.category,
            seed,
            token_count,
            text_fingerprint,
        );
        Self {
            category: classification.category,
            confidence_pct: classification.confidence_pct,
            token_count,
            seed,
            text_fingerprint,
            dispatch_fingerprint,
        }
    }

    /// Map intent → crystal-class for spawning categories. None for non-spawning.
    pub fn crystal_class(&self) -> Option<CrystalClass> {
        match self.category {
            IntentCategory::DescribeObject => Some(CrystalClass::Object),
            IntentCategory::DescribeEntity => Some(CrystalClass::Entity),
            IntentCategory::DescribeEnv => Some(CrystalClass::Environment),
            IntentCategory::DescribeBehavior => Some(CrystalClass::Behavior),
            IntentCategory::InvokeAction => Some(CrystalClass::Event),
            IntentCategory::NarrateAuthor => Some(CrystalClass::Recipe),
            _ => None,
        }
    }

    /// Spawn at a deterministic position derived from dispatch_fingerprint.
    pub fn spawn(&self) -> Option<Crystal> {
        let class = self.crystal_class()?;
        // Deterministic position from dispatch_fingerprint.
        let fp = self.dispatch_fingerprint;
        let x_mm = (((fp & 0xFFFF) as i32) - 32768).clamp(-2000, 2000);
        let z_mm = (((fp >> 16) & 0xFFFF) as i32 % 4000).abs() + 1000;
        Some(Crystal::allocate(class, self.seed, WorldPos::new(x_mm, 0, z_mm)))
    }

    /// Spawn at a caller-specified position (overrides deterministic placement).
    pub fn spawn_at(&self, world_pos: WorldPos) -> Option<Crystal> {
        let class = self.crystal_class()?;
        Some(Crystal::allocate(class, self.seed, world_pos))
    }
}

pub fn compute_text_fingerprint(text: &str) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"intent-text-fp-v1");
    h.update(text.as_bytes());
    let d: [u8; 32] = h.finalize().into();
    u32::from_le_bytes([d[28], d[29], d[30], d[31]])
}

pub fn compute_dispatch_fingerprint(
    category: IntentCategory,
    seed: u64,
    token_count: u32,
    text_fingerprint: u32,
) -> u32 {
    let mut h = blake3::Hasher::new();
    h.update(b"intent-dispatch-fp-v1");
    h.update(&category.as_u32().to_le_bytes());
    h.update(&seed.to_le_bytes());
    h.update(&token_count.to_le_bytes());
    h.update(&text_fingerprint.to_le_bytes());
    let d: [u8; 32] = h.finalize().into();
    u32::from_le_bytes([d[0], d[1], d[2], d[3]])
}
