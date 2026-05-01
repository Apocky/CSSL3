// § biome_dag_traversal ← DAG-walk integration tests
// ════════════════════════════════════════════════════════════════════
// § I> meta-progress gates edge-availability
// § I> Sanctum + Citadel both → Forge (DAG-converge)
// ════════════════════════════════════════════════════════════════════

use cssl_host_roguelike_run::biome_dag::BiomeDag;
use cssl_host_roguelike_run::meta_progress::MetaProgress;
use cssl_host_roguelike_run::Biome;

#[test]
fn meta_gates_unlock_progressive_biomes() {
    let dag = BiomeDag::default();
    let mut meta = MetaProgress::new();

    // No perks → from Crypt-clear → no next-options yet.
    let next = dag.available_next(Some(Biome::Crypt), &meta);
    assert!(next.is_empty(), "expected gated, got {next:?}");

    // Unlock deep-1 → Citadel becomes available.
    meta.unlock_perk("deep-1").unwrap();
    let next = dag.available_next(Some(Biome::Crypt), &meta);
    assert_eq!(next, vec![Biome::Citadel]);
}

#[test]
fn dag_converges_at_forge() {
    let dag = BiomeDag::default();
    let mut meta = MetaProgress::new();
    meta.unlock_perk("iron").unwrap();

    // Both Citadel-clear and Sanctum-clear lead to Forge.
    let from_citadel = dag.available_next(Some(Biome::Citadel), &meta);
    let from_sanctum = dag.available_next(Some(Biome::Sanctum), &meta);
    assert_eq!(from_citadel, vec![Biome::Forge]);
    assert_eq!(from_sanctum, vec![Biome::Forge]);
}

#[test]
fn endless_spire_is_terminal() {
    let dag = BiomeDag::default();
    let mut meta = MetaProgress::new();
    // Even with all perks, Endless has no outgoing edges.
    for k in ["deep-1", "verdant", "iron", "descent", "storm", "ascent"] {
        meta.unlock_perk(k).unwrap();
    }
    let next = dag.available_next(Some(Biome::EndlessSpire), &meta);
    assert!(next.is_empty());
}
