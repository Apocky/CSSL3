//! § Upgrade-paths per GDD § UPGRADE-PATH.
//!
//!   level-up  : XP → item-level + tier-bias-bump
//!   transmute : 5×N → 1×(N+1) ; lossy ; affixes re-rolled
//!                Legendary→Mythic FORBIDDEN (Mythic = drop-only-or-bond)
//!   bond      : Legendary+ binds-to-character ; revocable-pre-bond ; immutable-post
//!   reroll    : 1× per-affix-slot ; cost = 1×mat-tier-N + 1×Echo
//!
//! All four emit audit-events via the optional `AuditSink`.

use serde::{Deserialize, Serialize};

use crate::audit::{AuditEvent, AuditSink};
use crate::base::{BaseItem, BaseMat};
use crate::gear::Gear;
use crate::rarity::Rarity;
use crate::stat_rolling::{prefix_for_descriptor, roll_affix, suffix_for_descriptor, DetRng};

// ───────────────────────────────────────────────────────────────────────
// § level_up
// ───────────────────────────────────────────────────────────────────────

/// XP-cap = char-level × 100. We track a simpler invariant : every 100 XP →
/// item-level +1 ; tier-bias unchanged here (tier stored on RolledAffix).
/// Returns the leveled `Gear` ; mutates in-place.
pub fn level_up(g: &mut Gear, xp: u32, sink: &dyn AuditSink) {
    let levels = xp / 100;
    if levels == 0 {
        return;
    }
    g.item_level = g.item_level.saturating_add(levels);
    sink.emit(
        AuditEvent::bare("gear.leveled")
            .with("rarity", g.rarity.name())
            .with("xp", xp.to_string())
            .with("new_level", g.item_level.to_string()),
    );
}

// ───────────────────────────────────────────────────────────────────────
// § TransmuteResult
// ───────────────────────────────────────────────────────────────────────

/// Transmute outcome. Success → upgraded gear ; rejected → reason string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransmuteResult {
    /// Successful tier-shift ; new Gear returned.
    Ok(Gear),
    /// Forbidden : Legendary → Mythic blocked per GDD.
    ForbiddenMythicTransmute,
    /// Rarity already at Mythic ceiling.
    AlreadyMaxRarity,
    /// Mat-cost insufficient (caller responsibility ; we report).
    InsufficientMaterial,
}

/// Attempt a rarity-shift transmute. `mat_cost` ≥ required-tier per GDD :
///   Common→Uncommon : 1 Silver
///   Uncommon→Rare   : 1 Mithril
///   Rare→Epic       : 1 Adamant
///   Epic→Legendary  : 1 Voidsteel  (+ 1 Soul-essence — caller-tracked)
///   Legendary→Mythic: FORBIDDEN
///
/// On success : new-seed = old-seed XOR rarity-ordinal-shift ; affixes re-rolled.
/// Audit-emitted with old-rarity + new-rarity.
#[must_use]
pub fn transmute(g: Gear, target_rarity: Rarity, mat_cost: u8, sink: &dyn AuditSink) -> TransmuteResult {
    if g.rarity == Rarity::Mythic {
        sink.emit(
            AuditEvent::bare("gear.transmute_rejected")
                .with("reason", "already_mythic")
                .with("rarity", g.rarity.name()),
        );
        return TransmuteResult::AlreadyMaxRarity;
    }
    if g.rarity == Rarity::Legendary && target_rarity == Rarity::Mythic {
        sink.emit(
            AuditEvent::bare("gear.transmute_rejected")
                .with("reason", "forbidden_legendary_to_mythic")
                .with("from", g.rarity.name())
                .with("to", target_rarity.name()),
        );
        return TransmuteResult::ForbiddenMythicTransmute;
    }
    // Tier-step : target must be exactly +1 above current.
    let next = match g.rarity {
        Rarity::Common => Rarity::Uncommon,
        Rarity::Uncommon => Rarity::Rare,
        Rarity::Rare => Rarity::Epic,
        Rarity::Epic => Rarity::Legendary,
        _ => return TransmuteResult::AlreadyMaxRarity,
    };
    if target_rarity != next {
        sink.emit(
            AuditEvent::bare("gear.transmute_rejected")
                .with("reason", "tier_step_invalid")
                .with("from", g.rarity.name())
                .with("to", target_rarity.name()),
        );
        return TransmuteResult::ForbiddenMythicTransmute;
    }
    // Mat-cost minimum : 1 unit of next-tier mat.
    if mat_cost < 1 {
        sink.emit(
            AuditEvent::bare("gear.transmute_rejected")
                .with("reason", "insufficient_material")
                .with("mat_cost", mat_cost.to_string()),
        );
        return TransmuteResult::InsufficientMaterial;
    }
    // Build new base with upgraded mat to keep the rarity-floor invariant.
    let new_mat = next_mat_for_rarity(target_rarity);
    let new_base = BaseItem {
        slot: g.base.slot,
        item_class: g.base.item_class,
        base_mat: new_mat,
        base_stats: g.base.base_stats.clone(),
        allowed_affixes: g.base.allowed_affixes,
    };
    // New seed : XOR-shift to make affix-set genuinely fresh + replayable.
    let new_seed = g.seed ^ ((target_rarity as u128) << 64).wrapping_add(0xC0DE_CAFE);
    let new_gear = crate::stat_rolling::roll_gear(new_seed, &new_base, target_rarity);
    sink.emit(
        AuditEvent::bare("gear.transmuted")
            .with("from", g.rarity.name())
            .with("to", target_rarity.name())
            .with("mat_cost", mat_cost.to_string())
            .with("new_seed", new_seed.to_string()),
    );
    TransmuteResult::Ok(new_gear)
}

fn next_mat_for_rarity(r: Rarity) -> BaseMat {
    match r {
        Rarity::Common => BaseMat::Iron,
        Rarity::Uncommon => BaseMat::Silver,
        Rarity::Rare => BaseMat::Mithril,
        Rarity::Epic => BaseMat::Adamant,
        Rarity::Legendary => BaseMat::Voidsteel,
        Rarity::Mythic => BaseMat::Soulbound,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § bond
// ───────────────────────────────────────────────────────────────────────

/// Bond `g` to `player_id`. Only Legendary+ may bond. Audit-emitted.
/// Returns `Err(reason)` if bond-ineligible OR already-bonded.
pub fn bond(g: &mut Gear, player_id: u128, sink: &dyn AuditSink) -> Result<(), &'static str> {
    if !g.rarity.is_bond_eligible() {
        sink.emit(
            AuditEvent::bare("gear.bond_rejected")
                .with("reason", "not_bond_eligible")
                .with("rarity", g.rarity.name()),
        );
        return Err("gear rarity below Legendary cannot bond");
    }
    if g.bound_to_player {
        sink.emit(
            AuditEvent::bare("gear.bond_rejected")
                .with("reason", "already_bonded")
                .with("rarity", g.rarity.name()),
        );
        return Err("gear already bonded");
    }
    g.bound_to_player = true;
    sink.emit(
        AuditEvent::bare("gear.bonded")
            .with("rarity", g.rarity.name())
            .with("player_id", player_id.to_string()),
    );
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § reroll_affix
// ───────────────────────────────────────────────────────────────────────

/// Reroll-target — prefix-slot or suffix-slot at index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RerollTarget {
    /// Prefix-slot index.
    Prefix(usize),
    /// Suffix-slot index.
    Suffix(usize),
}

/// Reroll one affix-slot. Cost = 1×mat-tier-N + 1×Echo (caller tracks).
/// New seed-event audit-emitted ; lineage preserved (old-value in audit).
pub fn reroll_affix(
    g: &mut Gear,
    target: RerollTarget,
    seed: u128,
    sink: &dyn AuditSink,
) -> Result<(), &'static str> {
    if g.bound_to_player && g.rarity.is_bond_eligible() {
        // Post-bond glyphs immutable per GDD ; affixes still rerollable but audit-emit.
        sink.emit(
            AuditEvent::bare("gear.reroll_warn_bonded")
                .with("rarity", g.rarity.name()),
        );
    }
    let mut rng = DetRng::new(seed);
    match target {
        RerollTarget::Prefix(i) => {
            let slot = g.prefixes.get_mut(i).ok_or("prefix index oor")?;
            let pre = prefix_for_descriptor(&slot.descriptor)
                .ok_or("descriptor not a prefix")?;
            let old_value = slot.value;
            let mut desc = pre.descriptor();
            desc.tier_band = slot.tier;
            let new_seed = rng.next_u64();
            let new_value = roll_affix(
                &mut DetRng::from_state(new_seed.wrapping_add(1)),
                &desc,
                slot.tier,
            );
            slot.value = new_value;
            slot.seed = new_seed;
            sink.emit(
                AuditEvent::bare("gear.rerolled")
                    .with("kind", "prefix")
                    .with("index", i.to_string())
                    .with("old_value", format!("{old_value:.4}"))
                    .with("new_value", format!("{new_value:.4}")),
            );
            Ok(())
        }
        RerollTarget::Suffix(i) => {
            let slot = g.suffixes.get_mut(i).ok_or("suffix index oor")?;
            let suf = suffix_for_descriptor(&slot.descriptor)
                .ok_or("descriptor not a suffix")?;
            let old_value = slot.value;
            let mut desc = suf.descriptor();
            desc.tier_band = slot.tier;
            let new_seed = rng.next_u64();
            let new_value = roll_affix(
                &mut DetRng::from_state(new_seed.wrapping_add(1)),
                &desc,
                slot.tier,
            );
            slot.value = new_value;
            slot.seed = new_seed;
            sink.emit(
                AuditEvent::bare("gear.rerolled")
                    .with("kind", "suffix")
                    .with("index", i.to_string())
                    .with("old_value", format!("{old_value:.4}"))
                    .with("new_value", format!("{new_value:.4}")),
            );
            Ok(())
        }
    }
}
