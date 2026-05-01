// § biome-DAG ← GDDs/ROGUELIKE_LOOP.csl §BIOME-DAG
// ════════════════════════════════════════════════════════════════════
// § I> 8 biomes · DAG (¬ tree) · meta-progress-gated edges
// § I> entry-points : Hub → Crypt | Forest (echoes ≥ 0)
// § I> branching : at-each-junction player picks 1-of-N biomes
// ════════════════════════════════════════════════════════════════════

use crate::meta_progress::MetaProgress;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// § Biome enum · 8 zones per ROGUELIKE_LOOP §BIOME-DAG.
///
/// Ordering follows GDD-listed traversal flow ; serde uses unit-variant repr
/// for compact wire-format on run-share receipts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Biome {
    /// Entry-tier : poison-res-needed · light-low.
    Crypt,
    /// Entry-tier : stamina-drain · stealth-favored.
    ForestPath,
    /// Mid-tier (gated by deep-1) : armor-pen · vertical-traversal.
    Citadel,
    /// Late-tier (gated by descent) : dark-vision · pressure-DOT.
    Abyss,
    /// Mid-tier (gated by iron) : fire-res · forge-buffs-available.
    Forge,
    /// Mid-tier (gated by verdant) : holy-affinity · social-encounters +30%.
    Sanctum,
    /// Late-tier (gated by storm) : weather-volatility · timed-shrines.
    Maelstrom,
    /// Endgame (gated by ascent) : compounding +0.05/floor · all-affixes-active.
    EndlessSpire,
}

impl Biome {
    /// All 8 biomes in deterministic order (for iteration / catalog UIs).
    pub const ALL: [Biome; 8] = [
        Biome::Crypt,
        Biome::ForestPath,
        Biome::Citadel,
        Biome::Abyss,
        Biome::Forge,
        Biome::Sanctum,
        Biome::Maelstrom,
        Biome::EndlessSpire,
    ];

    /// Stable display key for run-share receipts and DM/GM handoff payloads.
    pub fn key(&self) -> &'static str {
        match self {
            Biome::Crypt => "crypt",
            Biome::ForestPath => "forest_path",
            Biome::Citadel => "citadel",
            Biome::Abyss => "abyss",
            Biome::Forge => "forge",
            Biome::Sanctum => "sanctum",
            Biome::Maelstrom => "maelstrom",
            Biome::EndlessSpire => "endless_spire",
        }
    }
}

/// § Edge-condition : meta-node string-key required for traversal.
///
/// Empty-string ≡ no gate (entry-edges from Hub).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeCondition {
    /// Required meta-node key (e.g. "deep-1") ; empty = unconditional.
    pub required_meta_node: String,
}

impl EdgeCondition {
    /// Unconditional edge (Hub entry).
    pub fn unconditional() -> Self {
        Self {
            required_meta_node: String::new(),
        }
    }

    /// Edge gated by a meta-node key.
    pub fn gated(meta_node: impl Into<String>) -> Self {
        Self {
            required_meta_node: meta_node.into(),
        }
    }

    /// Is this edge unlocked given the player's meta-progress ?
    pub fn satisfied(&self, meta: &MetaProgress) -> bool {
        if self.required_meta_node.is_empty() {
            return true;
        }
        meta.has_perk(&self.required_meta_node)
    }
}

/// § BiomeDag : adjacency-list with edge-conditions.
///
/// `Option<Biome>::None` represents the Hub (root). All other entries
/// keyed by the source biome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeDag {
    /// Hub-entries (no source-biome).
    pub hub_entries: Vec<(Biome, EdgeCondition)>,
    /// Per-biome adjacency : after clearing biome K, available next options.
    pub adjacency: BTreeMap<Biome, Vec<(Biome, EdgeCondition)>>,
}

/// § DAG-construction error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BiomeDagErr {
    /// Source biome unknown to the DAG.
    UnknownSource(Biome),
    /// Cyclic reference detected (DAG must remain acyclic).
    CycleDetected,
}

impl Default for BiomeDag {
    /// Canonical DAG per ROGUELIKE_LOOP §BIOME-DAG.
    fn default() -> Self {
        let mut adjacency: BTreeMap<Biome, Vec<(Biome, EdgeCondition)>> = BTreeMap::new();

        // Crypt-clear → Citadel (deep-1)
        adjacency.insert(
            Biome::Crypt,
            vec![(Biome::Citadel, EdgeCondition::gated("deep-1"))],
        );
        // Forest-clear → Sanctum (verdant)
        adjacency.insert(
            Biome::ForestPath,
            vec![(Biome::Sanctum, EdgeCondition::gated("verdant"))],
        );
        // Citadel-clear → Forge (iron)
        adjacency.insert(
            Biome::Citadel,
            vec![(Biome::Forge, EdgeCondition::gated("iron"))],
        );
        // Sanctum-clear → Forge (iron) — DAG-converge
        adjacency.insert(
            Biome::Sanctum,
            vec![(Biome::Forge, EdgeCondition::gated("iron"))],
        );
        // Forge-clear → Abyss (descent)
        adjacency.insert(
            Biome::Forge,
            vec![(Biome::Abyss, EdgeCondition::gated("descent"))],
        );
        // Abyss-clear → Maelstrom (storm)
        adjacency.insert(
            Biome::Abyss,
            vec![(Biome::Maelstrom, EdgeCondition::gated("storm"))],
        );
        // Maelstrom-clear → Endless (ascent)
        adjacency.insert(
            Biome::Maelstrom,
            vec![(Biome::EndlessSpire, EdgeCondition::gated("ascent"))],
        );
        // Endless terminal (no outgoing)
        adjacency.insert(Biome::EndlessSpire, vec![]);

        Self {
            hub_entries: vec![
                (Biome::Crypt, EdgeCondition::unconditional()),
                (Biome::ForestPath, EdgeCondition::unconditional()),
            ],
            adjacency,
        }
    }
}

impl BiomeDag {
    /// Available next biomes from `current` (or Hub if `None`) given meta.
    ///
    /// Returns deterministic-ordered Vec — order matches GDD edge-list order
    /// (insertion-order via the Default constructor).
    pub fn available_next(
        &self,
        current: Option<Biome>,
        meta: &MetaProgress,
    ) -> Vec<Biome> {
        let edges: &[(Biome, EdgeCondition)] = current.map_or_else(
            || self.hub_entries.as_slice(),
            |b| self.adjacency.get(&b).map_or(&[][..], Vec::as_slice),
        );
        edges
            .iter()
            .filter(|(_, cond)| cond.satisfied(meta))
            .map(|(b, _)| *b)
            .collect()
    }

    /// Verify the DAG has no cycles (defensive — Default is acyclic by
    /// construction, but BiomeDag is a public type and may be hand-built).
    pub fn validate_acyclic(&self) -> Result<(), BiomeDagErr> {
        // White / Gray / Black colored DFS.
        let mut visited: BTreeSet<Biome> = BTreeSet::new();
        let mut stack: BTreeSet<Biome> = BTreeSet::new();
        for &start in self.adjacency.keys() {
            if !visited.contains(&start) && self.dfs_cycle(start, &mut visited, &mut stack)? {
                return Err(BiomeDagErr::CycleDetected);
            }
        }
        Ok(())
    }

    fn dfs_cycle(
        &self,
        node: Biome,
        visited: &mut BTreeSet<Biome>,
        stack: &mut BTreeSet<Biome>,
    ) -> Result<bool, BiomeDagErr> {
        visited.insert(node);
        stack.insert(node);
        if let Some(edges) = self.adjacency.get(&node) {
            for (next, _) in edges {
                if stack.contains(next) {
                    return Ok(true);
                }
                if !visited.contains(next) && self.dfs_cycle(*next, visited, stack)? {
                    return Ok(true);
                }
            }
        }
        stack.remove(&node);
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_eight_biomes_present() {
        assert_eq!(Biome::ALL.len(), 8);
    }

    #[test]
    fn default_dag_is_acyclic() {
        let dag = BiomeDag::default();
        assert!(dag.validate_acyclic().is_ok());
    }

    #[test]
    fn hub_entries_are_unconditional() {
        let dag = BiomeDag::default();
        let meta = MetaProgress::new();
        let next = dag.available_next(None, &meta);
        assert_eq!(next, vec![Biome::Crypt, Biome::ForestPath]);
    }
}
