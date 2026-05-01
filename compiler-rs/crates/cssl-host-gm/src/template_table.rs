//! § template_table.rs — stage-0 GM prose template registry.
//!
//! § DESIGN
//!   The table indexes prose-template-pools by the tuple
//!   `(zone × event-class × tone-bucket)`. A `tone-bucket` is a 2-bit
//!   discretization of the `(warm, terse, poetic)` axes — stage-0 picks
//!   the axis with the largest deviation from neutral and uses its
//!   bucket as the third coordinate.
//!
//!   Templates are simple strings with `{tag}` placeholders. The GM
//!   slot-fills `{tag}` with a Φ-tag-name lookup from a caller-supplied
//!   tag-name registry ; if no Φ-tag is available, the GM degrades to a
//!   generic prose fallback (`"you see something here"`) and records
//!   `gm.degrade.no_phi_tag` to the audit-sink.
//!
//! § DETERMINISM
//!   Template-pool selection is keyed by `(zone, class, tone-bucket)` ;
//!   within-pool selection uses a deterministic seed combining the
//!   template-pool index with the Φ-tags hash + bias-vec hash supplied
//!   by the caller. No RNG ; replay-bit-equal.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::types::ToneAxis;

/// Stable identifier for a prose template inside a pool.
///
/// `(pool_key, index_in_pool)` — both `u32` to keep serde-stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TemplateId {
    pub pool_key: u32,
    pub index_in_pool: u32,
}

/// Coarse event-class for the GM narrator.
///
/// Stage-0 covers the four primary scene-events the DM emits :
/// `Arrive` (player enters the zone), `Examine` (player inspects
/// something), `Companion` (companion takes initiative), and
/// `Tension` (a beat-rise / fall mark). Additional classes can extend
/// without breaking template lookups (missing-class falls back to
/// `Arrive`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventClass {
    Arrive,
    Examine,
    Companion,
    Tension,
}

impl EventClass {
    /// Stable u32 wire-tag for the event-class.
    #[must_use]
    pub fn wire_tag(self) -> u32 {
        match self {
            Self::Arrive => 0,
            Self::Examine => 1,
            Self::Companion => 2,
            Self::Tension => 3,
        }
    }
}

/// A small registry mapping Φ-tag ids → human-readable names.
///
/// Used by the slot-filler to resolve `{tag}` placeholders.
pub type PhiTagNameRegistry = BTreeMap<u32, String>;

/// Stage-0 template registry.
///
/// `pools` keys are packed `(zone, event-class.wire_tag, tone-bucket)`
/// triples ; each value is an ordered `Vec<String>` of templates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateTable {
    pub pools: BTreeMap<u32, Vec<String>>,
    pub tag_names: PhiTagNameRegistry,
}

impl TemplateTable {
    /// Pack a `(zone, event-class, tone-bucket)` triple into a single
    /// `u32` pool-key. Layout : `[zone:24][class:4][bucket:4]`.
    ///
    /// Zone bits intentionally truncate to 24 — the DM-zone-namespace
    /// fits comfortably ; the upper byte is reserved for future tone-
    /// bucket extension.
    #[must_use]
    pub fn pack_pool_key(zone_id: u32, class: EventClass, tone_bucket: u8) -> u32 {
        let z = zone_id & 0x00FF_FFFF;
        let c = class.wire_tag() & 0xF;
        let b = u32::from(tone_bucket) & 0xF;
        (z << 8) | (c << 4) | b
    }

    /// Pick a tone-bucket from a `ToneAxis`.
    ///
    /// 0 = warm-leaning ; 1 = terse-leaning ; 2 = poetic-leaning ; 3 =
    /// neutral / mixed. The axis-with-largest-deviation-from-0.5 wins.
    #[must_use]
    pub fn tone_bucket(tone: ToneAxis) -> u8 {
        let dw = (tone.warm - 0.5).abs();
        let dt = (tone.terse - 0.5).abs();
        let dp = (tone.poetic - 0.5).abs();
        let max = dw.max(dt).max(dp);
        if max < 0.05 {
            3 // neutral
        } else if dw == max {
            0
        } else if dt == max {
            1
        } else {
            2
        }
    }

    /// Build the canonical stage-0 default table.
    ///
    /// Populates a small set of pools that cover the test-room +
    /// generic-zone + companion-event combinations. Real content lives
    /// in `.csl` source ; this is bootstrap-scaffolding only.
    #[must_use]
    pub fn default_stage0() -> Self {
        let mut pools: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        let mut tag_names: PhiTagNameRegistry = BTreeMap::new();
        // Φ-tag id → name examples. Real registry lives in DM zone-data.
        tag_names.insert(101, String::from("altar"));
        tag_names.insert(102, String::from("fountain"));
        tag_names.insert(103, String::from("lantern"));
        tag_names.insert(104, String::from("companion"));

        // zone 0 (test-room) · Arrive · neutral bucket
        pools.insert(
            Self::pack_pool_key(0, EventClass::Arrive, 3),
            vec![
                String::from("you arrive ; the {tag} waits"),
                String::from("the threshold opens onto a {tag}"),
            ],
        );
        // zone 0 · Examine · poetic bucket
        pools.insert(
            Self::pack_pool_key(0, EventClass::Examine, 2),
            vec![
                String::from("the {tag} stirs as your gaze lingers"),
                String::from("light bends around the {tag}"),
            ],
        );
        // zone 0 · Examine · terse bucket
        pools.insert(
            Self::pack_pool_key(0, EventClass::Examine, 1),
            vec![
                String::from("you see {tag}"),
                String::from("a {tag}. nothing more."),
            ],
        );
        // zone 0 · Companion · warm bucket
        pools.insert(
            Self::pack_pool_key(0, EventClass::Companion, 0),
            vec![
                String::from("your companion smiles and gestures at the {tag}"),
                String::from("the {tag} draws your companion's attention"),
            ],
        );
        // zone 0 · Tension · poetic bucket
        pools.insert(
            Self::pack_pool_key(0, EventClass::Tension, 2),
            vec![
                String::from("the air around the {tag} thickens"),
                String::from("you feel the {tag} grow heavy"),
            ],
        );

        Self { pools, tag_names }
    }

    /// Resolve a Φ-tag id to its name, defaulting to `"something"`.
    #[must_use]
    pub fn resolve_tag(&self, tag: u32) -> String {
        self.tag_names
            .get(&tag)
            .cloned()
            .unwrap_or_else(|| String::from("something"))
    }

    /// Look up a pool, falling back to `Arrive`-class within the same
    /// zone-and-tone-bucket if the requested class has no pool. If
    /// nothing is found, returns `None` and the caller emits a
    /// generic-prose degrade.
    #[must_use]
    pub fn lookup_pool(
        &self,
        zone_id: u32,
        class: EventClass,
        tone_bucket: u8,
    ) -> Option<(TemplateId, &str)> {
        let key = Self::pack_pool_key(zone_id, class, tone_bucket);
        if let Some(pool) = self.pools.get(&key) {
            if !pool.is_empty() {
                return Some((
                    TemplateId {
                        pool_key: key,
                        index_in_pool: 0,
                    },
                    pool[0].as_str(),
                ));
            }
        }
        // fall through to Arrive-class same zone same bucket.
        let fallback_key = Self::pack_pool_key(zone_id, EventClass::Arrive, tone_bucket);
        if let Some(pool) = self.pools.get(&fallback_key) {
            if !pool.is_empty() {
                return Some((
                    TemplateId {
                        pool_key: fallback_key,
                        index_in_pool: 0,
                    },
                    pool[0].as_str(),
                ));
            }
        }
        None
    }

    /// Pick a deterministic template from a pool given a seed.
    ///
    /// Returns the template-id + the resolved (slot-filled) prose, or
    /// `None` if no pool exists for the requested coordinates.
    pub fn pick(
        &self,
        zone_id: u32,
        class: EventClass,
        tone_bucket: u8,
        primary_phi_tag: Option<u32>,
        seed: u64,
    ) -> Option<(TemplateId, String)> {
        let key = Self::pack_pool_key(zone_id, class, tone_bucket);
        let (effective_key, pool) = if let Some(pool) = self.pools.get(&key) {
            (key, pool)
        } else {
            let fb = Self::pack_pool_key(zone_id, EventClass::Arrive, tone_bucket);
            self.pools.get(&fb).map(|p| (fb, p))?
        };
        if pool.is_empty() {
            return None;
        }
        // Deterministic in-pool selection : seed % pool.len() — pool
        // lengths are O(few), the modulo is bias-free in practice.
        let idx = (seed % pool.len() as u64) as u32;
        let template_str = pool[idx as usize].clone();
        let tag_name = primary_phi_tag.map_or_else(
            || String::from("something"),
            |t| self.resolve_tag(t),
        );
        let filled = template_str.replace("{tag}", &tag_name);
        Some((
            TemplateId {
                pool_key: effective_key,
                index_in_pool: idx,
            },
            filled,
        ))
    }
}

impl Default for TemplateTable {
    fn default() -> Self {
        Self::default_stage0()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_pool_key_round_trip_zone() {
        let k = TemplateTable::pack_pool_key(7, EventClass::Examine, 1);
        // zone 7 << 8 | class 1 << 4 | bucket 1 = 0x00000711
        assert_eq!(k, (7u32 << 8) | (1u32 << 4) | 1);
    }

    #[test]
    fn tone_bucket_neutral_for_balanced() {
        let b = TemplateTable::tone_bucket(ToneAxis::neutral());
        assert_eq!(b, 3);
    }

    #[test]
    fn tone_bucket_picks_dominant_axis() {
        let warm = ToneAxis::clamped(0.95, 0.5, 0.5);
        let terse = ToneAxis::clamped(0.5, 0.95, 0.5);
        let poetic = ToneAxis::clamped(0.5, 0.5, 0.95);
        assert_eq!(TemplateTable::tone_bucket(warm), 0);
        assert_eq!(TemplateTable::tone_bucket(terse), 1);
        assert_eq!(TemplateTable::tone_bucket(poetic), 2);
    }

    #[test]
    fn default_table_has_test_room_pools() {
        let t = TemplateTable::default_stage0();
        assert!(t.lookup_pool(0, EventClass::Arrive, 3).is_some());
        assert!(t.lookup_pool(0, EventClass::Examine, 2).is_some());
        assert!(t.lookup_pool(0, EventClass::Companion, 0).is_some());
    }

    #[test]
    fn pick_slot_fills_phi_tag() {
        let t = TemplateTable::default_stage0();
        let (_id, prose) = t.pick(0, EventClass::Examine, 1, Some(101), 0).unwrap();
        // prose drawn from terse-bucket pool, slot-filled with "altar"
        assert!(prose.contains("altar"));
        assert!(!prose.contains("{tag}"));
    }
}
