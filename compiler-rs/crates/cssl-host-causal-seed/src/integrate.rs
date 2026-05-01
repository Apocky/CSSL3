// § T11-W4-CAUSAL : integrate ← forward integration over causal-DAG
// ════════════════════════════════════════════════════════════════════
// § I> WorldVector ← caller-defined dimensionality ; flat Vec<f32>
// § I> CausalEffect = trait { apply(dt, &mut WorldVector) ; label }
// § I> CausalIntegrator = ⟨dag, effects: id→Effect, world, t_micros⟩
// § I> step_micros(dt) : topo-walk → ∀ node-with-effect, apply(dt, world)
// § I> determinism : topo-order = stable ; effects boxed-but-pure
// ════════════════════════════════════════════════════════════════════

use crate::dag::{CausalDag, DagErr};
use std::collections::HashMap;

/// Caller-defined-dimensionality state vector — flat f32 array.
///
/// § I> the integrator does not interpret semantics ; effects do
/// § I> caller pre-allocates with `WorldVector::zeros(dim)` then assigns slots
#[derive(Debug, Clone, PartialEq)]
pub struct WorldVector {
    pub v: Vec<f32>,
}

impl WorldVector {
    /// Construct vector of length `n` filled with zero.
    #[must_use]
    pub fn zeros(n: usize) -> Self {
        Self { v: vec![0.0; n] }
    }

    /// Construct from explicit values.
    #[must_use]
    pub fn from_vec(v: Vec<f32>) -> Self {
        Self { v }
    }

    /// Length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.v.len()
    }

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
}

/// Behaviour invoked when a node's effect fires during a step.
///
/// § I> apply(dt, world) : caller mutates `world.v` over delta-time `dt` (seconds)
/// § I> label : human-readable for trace/debug ; not load-bearing
pub trait CausalEffect {
    fn apply(&self, dt: f32, world: &mut WorldVector);
    fn label(&self) -> &str;
}

/// Integrator ← couples DAG-topology with effect-set + state-vector + clock.
///
/// `effects` maps node-id → boxed effect ; nodes WITHOUT an effect are
/// pure-structural (e.g. group-roots) and contribute zero to the world-step.
///
/// § I> if dag.has_cycle() then step_micros returns DagErr::Cyclic ; world unchanged.
pub struct CausalIntegrator {
    dag: CausalDag,
    effects: HashMap<u64, Box<dyn CausalEffect>>,
    world: WorldVector,
    t_micros: u64,
}

impl CausalIntegrator {
    /// New integrator with `world_dim`-zero state at t=0.
    #[must_use]
    pub fn new(dag: CausalDag, world_dim: usize) -> Self {
        Self {
            dag,
            effects: HashMap::new(),
            world: WorldVector::zeros(world_dim),
            t_micros: 0,
        }
    }

    /// New integrator with caller-provided initial state.
    #[must_use]
    pub fn with_world(dag: CausalDag, world: WorldVector) -> Self {
        Self {
            dag,
            effects: HashMap::new(),
            world,
            t_micros: 0,
        }
    }

    /// Borrow the embedded DAG.
    #[must_use]
    pub fn dag(&self) -> &CausalDag {
        &self.dag
    }

    /// Borrow current world-vector.
    #[must_use]
    pub fn world(&self) -> &WorldVector {
        &self.world
    }

    /// Current simulation time (microseconds since t=0).
    #[must_use]
    pub fn time_micros(&self) -> u64 {
        self.t_micros
    }

    /// Bind an effect to a node-id ; replaces any prior binding.
    pub fn bind_effect(&mut self, node_id: u64, effect: Box<dyn CausalEffect>) {
        self.effects.insert(node_id, effect);
    }

    /// Number of bound effects.
    #[must_use]
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    /// Advance simulation by `dt_micros` ; for each node in topo-order
    /// invoke its bound effect (if any) with dt=seconds. Increments `t_micros`.
    ///
    /// Returns Err(DagErr::Cyclic) if DAG has a cycle ; in that case
    /// state is unchanged and t_micros does NOT advance.
    pub fn step_micros(&mut self, dt_micros: u64) -> Result<(), DagErr> {
        let order = self.dag.topological_order()?;
        // dt_micros : u64 → f32 seconds. Precision-loss is acceptable in this
        // domain : story-as-physics steps are typically ≤ 1 hour-of-game-time
        // (3.6e9 µs) which fits f32 mantissa cleanly. The intermediate f64 is
        // for division-rounding fidelity, not for representing the full u64 range.
        #[allow(clippy::cast_precision_loss)]
        let dt_micros_f64 = dt_micros as f64;
        let dt_secs = (dt_micros_f64 / 1_000_000.0) as f32;
        for n in order {
            if let Some(eff) = self.effects.get(&n) {
                eff.apply(dt_secs, &mut self.world);
            }
        }
        self.t_micros = self.t_micros.saturating_add(dt_micros);
        Ok(())
    }
}

/// Built-in test/util effect : `world.v[target_idx] += delta_per_sec * dt`.
pub struct LinearEffect {
    pub target_idx: usize,
    pub delta_per_sec: f32,
    pub label: String,
}

impl LinearEffect {
    #[must_use]
    pub fn new(target_idx: usize, delta_per_sec: f32, label: impl Into<String>) -> Self {
        Self {
            target_idx,
            delta_per_sec,
            label: label.into(),
        }
    }
}

impl CausalEffect for LinearEffect {
    fn apply(&self, dt: f32, world: &mut WorldVector) {
        if let Some(slot) = world.v.get_mut(self.target_idx) {
            *slot += self.delta_per_sec * dt;
        }
        // Out-of-bounds idx ← silent no-op ; caller pre-sized world incorrectly.
    }

    fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
#[allow(clippy::many_single_char_names, clippy::similar_names)]
mod tests {
    use super::*;
    use crate::dag::CausalDag;
    use crate::edge::EdgeKind;
    use crate::node::NodeKind;

    #[test]
    fn empty_step_no_effect_advances_clock_only() {
        let mut g = CausalDag::new();
        let _ = g.add_node(NodeKind::WorldState, "s");
        let mut sim = CausalIntegrator::new(g, 4);
        assert_eq!(sim.time_micros(), 0);
        assert_eq!(sim.world().v, vec![0.0; 4]);
        sim.step_micros(1_000_000).expect("step");
        assert_eq!(sim.time_micros(), 1_000_000);
        assert_eq!(sim.world().v, vec![0.0; 4]);
    }

    #[test]
    fn linear_effect_applies_proportional_to_dt() {
        let mut g = CausalDag::new();
        let n = g.add_node(NodeKind::Event, "tick");
        let mut sim = CausalIntegrator::new(g, 1);
        sim.bind_effect(n, Box::new(LinearEffect::new(0, 10.0, "rate-10/s")));
        // Step 0.5s ← +5.0 expected.
        sim.step_micros(500_000).expect("step");
        let v = sim.world().v[0];
        assert!((v - 5.0).abs() < 1e-3, "expected ~5.0, got {v}");
        // Another 0.5s ← +5.0 more.
        sim.step_micros(500_000).expect("step");
        let v2 = sim.world().v[0];
        assert!((v2 - 10.0).abs() < 1e-3, "expected ~10.0, got {v2}");
    }

    #[test]
    fn two_effects_additive_in_topo_order() {
        let mut g = CausalDag::new();
        let a = g.add_node(NodeKind::StoryBeat, "a");
        let b = g.add_node(NodeKind::Event, "b");
        g.add_edge(a, b, EdgeKind::Causes, 1.0).expect("a→b");
        let mut sim = CausalIntegrator::new(g, 2);
        // a writes slot[0] : +2.0/s ; b writes slot[1] : +3.0/s
        sim.bind_effect(a, Box::new(LinearEffect::new(0, 2.0, "a-effect")));
        sim.bind_effect(b, Box::new(LinearEffect::new(1, 3.0, "b-effect")));
        assert_eq!(sim.effect_count(), 2);
        // 2 seconds.
        sim.step_micros(2_000_000).expect("step");
        let v = &sim.world().v;
        assert!((v[0] - 4.0).abs() < 1e-3, "v[0]={}", v[0]);
        assert!((v[1] - 6.0).abs() < 1e-3, "v[1]={}", v[1]);
    }

    #[test]
    fn cycle_skip_respects_topo_error_state_preserved() {
        let mut g = CausalDag::new();
        let a = g.add_node(NodeKind::Event, "a");
        let b = g.add_node(NodeKind::Event, "b");
        let c = g.add_node(NodeKind::Event, "c");
        g.add_edge(a, b, EdgeKind::Follows, 0.1).expect("a→b");
        g.add_edge(b, c, EdgeKind::Follows, 0.1).expect("b→c");
        g.add_edge(c, a, EdgeKind::Follows, 0.1).expect("c→a"); // cycle
        let mut sim = CausalIntegrator::new(g, 1);
        sim.bind_effect(a, Box::new(LinearEffect::new(0, 999.0, "nope")));
        // Pre-step state.
        let pre_t = sim.time_micros();
        let pre_v = sim.world().v.clone();
        // Attempted step → Cyclic err.
        let r = sim.step_micros(1_000_000);
        assert_eq!(r, Err(DagErr::Cyclic));
        // State unchanged.
        assert_eq!(sim.time_micros(), pre_t);
        assert_eq!(sim.world().v, pre_v);
    }

    #[test]
    fn out_of_bounds_effect_idx_is_silent_noop() {
        let mut g = CausalDag::new();
        let n = g.add_node(NodeKind::Event, "oob");
        let mut sim = CausalIntegrator::new(g, 2);
        // target_idx=99 > world.len()=2 → silent skip, world unchanged.
        sim.bind_effect(n, Box::new(LinearEffect::new(99, 1000.0, "oob-effect")));
        sim.step_micros(1_000_000).expect("step");
        assert_eq!(sim.world().v, vec![0.0, 0.0]);
    }

    #[test]
    fn world_vector_constructors_and_metadata() {
        let z = WorldVector::zeros(5);
        assert_eq!(z.len(), 5);
        assert!(!z.is_empty());
        assert!(z.v.iter().all(|&x| x == 0.0));

        let v = WorldVector::from_vec(vec![1.0, -2.0, 3.5]);
        assert_eq!(v.len(), 3);
        assert_eq!(v.v, vec![1.0, -2.0, 3.5]);

        let empty = WorldVector::zeros(0);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        // LinearEffect.label exposes the configured string for trace/debug.
        let eff = LinearEffect::new(0, 1.0, "lbl");
        assert_eq!(eff.label(), "lbl");
    }

    #[test]
    fn time_advances_monotonically_across_many_steps() {
        let mut g = CausalDag::new();
        let _ = g.add_node(NodeKind::WorldState, "w");
        let mut sim = CausalIntegrator::new(g, 1);
        let mut prev = sim.time_micros();
        for i in 1..=50_u64 {
            sim.step_micros(100).expect("step");
            let now = sim.time_micros();
            assert!(now >= prev, "step {i}: {now} < {prev}");
            assert_eq!(now, i * 100);
            prev = now;
        }
        assert_eq!(sim.time_micros(), 5_000);
    }
}
