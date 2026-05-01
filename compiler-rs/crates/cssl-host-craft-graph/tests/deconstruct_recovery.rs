// § tests : deconstruct recovery clamp · glyph-shard 50%-drop · tool-quality
// ══════════════════════════════════════════════════════════════════
use cssl_host_craft_graph::deconstruct::{deconstruct, CraftedItem, DeconstructTool};
use cssl_host_craft_graph::glyph::{AffixDescriptor, GlyphInstance, GlyphSlot};
use cssl_host_craft_graph::material::Material;
use cssl_host_craft_graph::recipe::ItemClass;

fn sample_item() -> CraftedItem {
    CraftedItem {
        recipe_id: 7,
        class: ItemClass::Weapon,
        tier: 3,
        lineage: vec![(Material::Mithril, 4), (Material::Ironwood, 2)],
        glyph_slots: vec![GlyphSlot::with(GlyphInstance {
            shard_id: 1,
            affix: AffixDescriptor::new("A-fire", 10),
        })],
    }
}

#[test]
fn t_recovery_fraction_clamped_30_to_70() {
    // ‼ GDD invariant : recovery ∈ [0.30, 0.70]. NEVER 0%.
    let item = sample_item();
    // skill=0, tool_q=0 → raw = base ≤ 0.55 ; clamp lower-bound triggers @ Voidsmelter (base 0.45).
    let r = deconstruct(&item, DeconstructTool::Voidsmelter, 0, 0, 0.99);
    assert!(r.recovery_fraction >= 0.30 && r.recovery_fraction <= 0.70);

    // skill=100, tool_q=100 → raw = 0.55 + 0.4 + 0.2 = 1.15 → clamp upper.
    let r2 = deconstruct(&item, DeconstructTool::Phaseloom, 100, 100, 0.99);
    assert!((r2.recovery_fraction - 0.70).abs() < 1e-6);
}

#[test]
fn t_returned_mats_never_zero_when_orig_nonzero() {
    // ‼ GDD : NEVER 0%. Even at min-recovery, every nonzero-input material
    // returns ≥ 1 of itself.
    let item = sample_item();
    let r = deconstruct(&item, DeconstructTool::Voidsmelter, 0, 0, 0.99);
    assert_eq!(r.returned_mats.len(), 2);
    for &(_, count) in &r.returned_mats {
        assert!(count >= 1, "Each material must return ≥ 1");
    }
}

#[test]
fn t_glyph_shard_drops_at_50_percent() {
    let item = sample_item();
    // roll < 0.50 ⇒ shard drops ; roll ≥ 0.50 ⇒ no shard.
    let r_drop = deconstruct(&item, DeconstructTool::Soulhammer, 50, 50, 0.10);
    assert!(r_drop.glyph_shard_drop, "roll 0.10 < 0.50 ⇒ shard drops");

    let r_no = deconstruct(&item, DeconstructTool::Soulhammer, 50, 50, 0.75);
    assert!(!r_no.glyph_shard_drop, "roll 0.75 ≥ 0.50 ⇒ no shard");
}

#[test]
fn t_tool_quality_affects_recovery() {
    let item = sample_item();
    let r_low = deconstruct(&item, DeconstructTool::Soulhammer, 50, 0, 0.99);
    let r_high = deconstruct(&item, DeconstructTool::Soulhammer, 50, 100, 0.99);

    // higher tool-quality ⇒ ≥ recovery (monotonic on tool_q within clamp).
    assert!(r_high.recovery_fraction >= r_low.recovery_fraction);
}
