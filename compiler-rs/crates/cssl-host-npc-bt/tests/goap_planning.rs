// § tests/goap_planning.rs — A* planner correctness + plan-cap
// ════════════════════════════════════════════════════════════════════
// § I> 4 tests : multi-step plan · plan-cap respected · no-path-None · cost-monotone
// ════════════════════════════════════════════════════════════════════

use cssl_host_npc_bt::goap::{FactValue, GoapAction, GoapState, plan, plan_with_budget};
use std::collections::BTreeMap;
use std::time::Duration;

fn b(v: bool) -> FactValue {
    FactValue::Bool(v)
}

fn act(name: &str, pre: &[(u32, FactValue)], eff: &[(u32, FactValue)], cost: u32) -> GoapAction {
    let mut p = BTreeMap::new();
    for (k, v) in pre {
        p.insert(*k, v.clone());
    }
    let mut e = BTreeMap::new();
    for (k, v) in eff {
        e.insert(*k, v.clone());
    }
    GoapAction {
        name: name.into(),
        preconditions: p,
        effects: e,
        cost_centi: cost,
    }
}

#[test]
fn finds_three_step_plan() {
    // facts : 0=AtHome 1=AtMarket 2=HasFood 3=Sated
    let mut s = GoapState::new();
    s.set(0, b(true));
    let mut g = GoapState::new();
    g.set(3, b(true));
    let actions = vec![
        act("GoMarket", &[(0, b(true))], &[(1, b(true)), (0, b(false))], 200),
        act("BuyFood", &[(1, b(true))], &[(2, b(true))], 100),
        act("Eat", &[(2, b(true))], &[(3, b(true)), (2, b(false))], 20),
    ];
    let p = plan(s, g, &actions).expect("expected plan");
    assert_eq!(p.len(), 3);
    let names: Vec<&str> = p.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["GoMarket", "BuyFood", "Eat"]);
}

#[test]
fn no_path_returns_none() {
    let mut s = GoapState::new();
    s.set(0, b(false));
    let mut g = GoapState::new();
    g.set(0, b(true));
    // No actions whose effects produce the goal-fact.
    let actions = vec![act("Nop", &[], &[(99, b(true))], 100)];
    assert_eq!(plan(s, g, &actions), None);
}

#[test]
fn tight_node_budget_respected() {
    // Force the planner to abort on node-budget exhaustion before reaching the goal.
    let mut s = GoapState::new();
    s.set(0, b(true));
    let mut g = GoapState::new();
    g.set(99, b(true));
    // Many irrelevant actions to consume node-budget.
    let mut actions = vec![];
    for i in 0..32 {
        actions.push(act(&format!("a{}", i), &[(0, b(true))], &[(50 + i, b(true))], 100));
    }
    let res = plan_with_budget(s, g, &actions, Duration::from_millis(50), 1);
    assert_eq!(res, None);
}

#[test]
fn picks_lower_cost_path() {
    let mut s = GoapState::new();
    s.set(0, b(true));
    let mut g = GoapState::new();
    g.set(1, b(true));
    let actions = vec![
        act("Cheap", &[(0, b(true))], &[(1, b(true))], 50),
        act("Expensive", &[(0, b(true))], &[(1, b(true))], 500),
    ];
    let p = plan(s, g, &actions).expect("expected plan");
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].name, "Cheap");
}
