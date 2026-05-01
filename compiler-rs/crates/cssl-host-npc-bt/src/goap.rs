// § goap.rs — L2 layer ; A*-over-fact-state goal-oriented action planner
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § GOAP-PLANNER ; A*-lookahead N=8 ; plan-cap ≤ 50ms
// § I> determinism : BTreeMap-sorted-iter ; total-order on FactValue
// § I> failure : > 50ms wall-clock OR no-path → return None ; audit per-call
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Value a fact-key can take. Total-order via derived Ord.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FactValue {
    /// Boolean fact (most common).
    Bool(bool),
    /// Counter / quantity fact (≥0).
    Count(u32),
    /// Categorical / enum fact (small-integer tag).
    Tag(u32),
}

/// World-state snapshot for GOAP ; deterministic-iter via BTreeMap.
///
/// § I> Bitset<128> in the GDD is collapsed here to a sparse BTreeMap<u32, FactValue>
/// for flexibility ; A* hashing uses BTreeMap's sorted serialize.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GoapState {
    /// Sparse fact-map (key = fact-id, val = FactValue).
    pub facts: BTreeMap<u32, FactValue>,
}

impl GoapState {
    /// Empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set fact `k = v`.
    pub fn set(&mut self, k: u32, v: FactValue) {
        self.facts.insert(k, v);
    }

    /// Get fact `k` or None.
    #[must_use]
    pub fn get(&self, k: u32) -> Option<&FactValue> {
        self.facts.get(&k)
    }

    /// True iff `self` ⊇ `other` (every key in `other` matches in `self`).
    #[must_use]
    pub fn satisfies(&self, other: &GoapState) -> bool {
        other.facts.iter().all(|(k, v)| self.facts.get(k) == Some(v))
    }

    /// Hamming-style distance to a goal-state — count of facts in `goal` not satisfied.
    /// Used as the A* heuristic ; admissible (≤ true-cost when each step satisfies ≤1 fact).
    #[must_use]
    pub fn distance_to(&self, goal: &GoapState) -> u32 {
        goal.facts
            .iter()
            .filter(|(k, v)| self.facts.get(k) != Some(v))
            .count() as u32
    }

    /// Apply an action's effects in-place.
    pub fn apply(&mut self, eff: &BTreeMap<u32, FactValue>) {
        for (k, v) in eff {
            self.facts.insert(*k, v.clone());
        }
    }

    /// Total ordering key — used for A* node-id ; string-based for portability.
    #[must_use]
    pub fn key(&self) -> String {
        // BTreeMap serde is sorted-key by construction → bit-equal across hosts.
        serde_json::to_string(&self.facts).unwrap_or_default()
    }
}

/// One GOAP action — { preconditions, effects, cost }.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoapAction {
    /// Stable action-name for audit-attribs.
    pub name: String,
    /// Required fact-state (subset-match).
    pub preconditions: BTreeMap<u32, FactValue>,
    /// Effects applied on success.
    pub effects: BTreeMap<u32, FactValue>,
    /// Action-cost ; A* sums these along the plan-path. Stored as u32 (×100)
    /// to keep the priority-queue total-ordered without f32-NaN hazards.
    pub cost_centi: u32,
}

impl GoapAction {
    /// True iff `state` satisfies every precondition.
    #[must_use]
    pub fn applicable(&self, state: &GoapState) -> bool {
        self.preconditions
            .iter()
            .all(|(k, v)| state.facts.get(k) == Some(v))
    }
}

/// Plan-search result : ordered list of actions OR None on no-path / timeout.
///
/// § I> Bounds : 50ms wall ; 256 expanded-nodes max ; A* with admissible Hamming heuristic.
/// § I> ¬-panics ; ¬-allocates outside Vec/BTreeMap.
#[allow(clippy::missing_panics_doc)]
pub fn plan(
    start: GoapState,
    goal: GoapState,
    actions: &[GoapAction],
) -> Option<Vec<GoapAction>> {
    plan_with_budget(start, goal, actions, Duration::from_millis(50), 256)
}

/// Plan with explicit budget — testable variant of `plan`.
pub fn plan_with_budget(
    start: GoapState,
    goal: GoapState,
    actions: &[GoapAction],
    wall_budget: Duration,
    node_budget: u32,
) -> Option<Vec<GoapAction>> {
    use std::collections::BinaryHeap;

    if start.satisfies(&goal) {
        return Some(Vec::new());
    }

    // A* with f = g + h ; min-heap via Reverse.
    #[derive(PartialEq, Eq)]
    struct Frontier {
        f_centi: u32,
        g_centi: u32,
        state_key: String,
        path: Vec<usize>, // indices into `actions`
        state: GoapState,
    }
    impl Ord for Frontier {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // Min-heap — invert so the smallest f comes first.
            other
                .f_centi
                .cmp(&self.f_centi)
                .then_with(|| other.state_key.cmp(&self.state_key))
        }
    }
    impl PartialOrd for Frontier {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    let started = Instant::now();
    let mut visited: BTreeMap<String, u32> = BTreeMap::new();
    let mut heap: BinaryHeap<Frontier> = BinaryHeap::new();

    let h0 = start.distance_to(&goal) * 100;
    let start_key = start.key();
    visited.insert(start_key.clone(), 0);
    heap.push(Frontier {
        f_centi: h0,
        g_centi: 0,
        state_key: start_key,
        path: Vec::new(),
        state: start,
    });

    let mut expanded: u32 = 0;

    while let Some(cur) = heap.pop() {
        if started.elapsed() > wall_budget {
            return None; // plan-cap : fallback to caller's Idle
        }
        if expanded >= node_budget {
            return None;
        }
        expanded += 1;

        if cur.state.satisfies(&goal) {
            let plan: Vec<GoapAction> = cur.path.iter().map(|i| actions[*i].clone()).collect();
            return Some(plan);
        }

        for (idx, a) in actions.iter().enumerate() {
            if !a.applicable(&cur.state) {
                continue;
            }
            let mut next = cur.state.clone();
            next.apply(&a.effects);
            let next_key = next.key();
            let g_next = cur.g_centi.saturating_add(a.cost_centi);
            if let Some(prev_g) = visited.get(&next_key) {
                if *prev_g <= g_next {
                    continue;
                }
            }
            visited.insert(next_key.clone(), g_next);
            let h = next.distance_to(&goal) * 100;
            let f = g_next.saturating_add(h);
            let mut path = cur.path.clone();
            path.push(idx);
            heap.push(Frontier {
                f_centi: f,
                g_centi: g_next,
                state_key: next_key,
                path,
                state: next,
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(v: bool) -> FactValue {
        FactValue::Bool(v)
    }

    #[test]
    fn empty_goal_returns_empty_plan() {
        let s = GoapState::new();
        let g = GoapState::new();
        let p = plan(s, g, &[]);
        assert_eq!(p, Some(Vec::new()));
    }

    #[test]
    fn no_path_returns_none() {
        let mut s = GoapState::new();
        s.set(0, b(false));
        let mut g = GoapState::new();
        g.set(0, b(true));
        let p = plan(s, g, &[]);
        assert_eq!(p, None);
    }

    #[test]
    fn single_step_plan() {
        let mut s = GoapState::new();
        s.set(0, b(false));
        let mut g = GoapState::new();
        g.set(0, b(true));
        let mut eff = BTreeMap::new();
        eff.insert(0, b(true));
        let act = GoapAction {
            name: "Toggle".into(),
            preconditions: BTreeMap::new(),
            effects: eff,
            cost_centi: 100,
        };
        let plan_res = plan(s, g, &[act]).expect("expected a plan");
        assert_eq!(plan_res.len(), 1);
        assert_eq!(plan_res[0].name, "Toggle");
    }
}
