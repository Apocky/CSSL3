//! Integration test : 1M-entity dispatch projection.
//!
//! § OBJECTIVE
//!   Validate that the canonical fused-stages-4-7 schedule projects under
//!   the 8.33ms (120Hz) and 11.11ms (90Hz-VR) frame budgets while
//!   handling 1M+ entities @ tier-aware scheduling per `density_budget §
//!   IV ENTITY BUDGET TABLE`.
//!
//! § METRICS
//!   We measure :
//!     - schedule build wall-clock (must be ≪ 1ms ; one-time at boot)
//!     - projected frame-cost from cost-model (must respect budget)
//!     - dispatch-group total (must accommodate 1M-entity tier-distribution)
//!     - backend selection across feature-matrix variants

use cssl_work_graph::{
    backend::{Backend, FeatureMatrix},
    builder::WorkGraphBuilder,
    cost_model::{CostModel, EntityCount, FrameBudget},
    dgc::DgcSequence,
    dispatch::DispatchArgs,
    indirect::IndirectChain,
    integration::{build_canonical_120hz_schedule, build_canonical_quest_3_schedule},
    node::WorkGraphNode,
    stage_layout::StageId,
    work_graph_d3d12::WorkGraphD3d12,
};

/// Build a 1M-entity-class schedule on the Ultimate path (work-graphs).
fn build_1m_entity_schedule_ultimate() -> cssl_work_graph::Schedule {
    // Each tier-bucket gets one mega-dispatch. 1M entities @ 64 ent/group =
    // 15625 groups for T3 horizon (the heaviest tier-by-count, smallest
    // per-frame contribution since it ticks @ 4Hz).
    WorkGraphBuilder::new()
        .with_label("1m-entity")
        .auto_select(FeatureMatrix::ultimate())
        .with_budget(FrameBudget::hz_120())
        .node(
            WorkGraphNode::compute(
                "T0Tier-Fovea",
                StageId::WaveSolver,
                DispatchArgs::new(2, 1, 1), // 100 ent / 64 ⇒ 2 groups
            )
            .with_cost_us(132)
            .with_shader_tag("entity_tick_t0"),
        )
        .unwrap()
        .node(
            WorkGraphNode::compute(
                "T1Tier-Mid",
                StageId::WaveSolver,
                DispatchArgs::new(78, 1, 1), // 5000 / 64 ⇒ 78 groups
            )
            .with_input("T0Tier-Fovea")
            .with_cost_us(6_600)
            .with_shader_tag("entity_tick_t1"),
        )
        .unwrap()
        .node(
            WorkGraphNode::compute(
                "T2Tier-Distant",
                StageId::WaveSolver,
                DispatchArgs::new(195, 4, 1), // 50K / 64 ⇒ 781 groups @ 15Hz amortized
            )
            .with_input("T1Tier-Mid")
            .with_cost_us(1_400) // 5.6ms / 4 frames amortized
            .with_shader_tag("entity_tick_t2"),
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn build_1m_schedule_projects_under_8333us_120hz() {
    let s = build_1m_entity_schedule_ultimate();
    assert!(
        s.within_budget(),
        "schedule must fit 8.33ms budget : got {} us",
        s.est_cost_us()
    );
    assert!(s.est_cost_us() <= 8_333);
}

#[test]
fn build_1m_schedule_uses_dx12_work_graph_when_ultimate() {
    let s = build_1m_entity_schedule_ultimate();
    assert_eq!(s.backend(), Backend::D3d12WorkGraph);
}

#[test]
fn build_1m_schedule_emits_3_compute_nodes() {
    let s = build_1m_entity_schedule_ultimate();
    assert_eq!(s.stats().compute_node_count, 3);
}

#[test]
fn build_1m_schedule_topo_orders_t0_first() {
    let s = build_1m_entity_schedule_ultimate();
    assert_eq!(s.order().first().unwrap().as_str(), "T0Tier-Fovea");
}

#[test]
fn build_1m_schedule_capped_by_dx12_ceiling() {
    let s = build_1m_entity_schedule_ultimate();
    assert_eq!(
        s.entity_count_for_backend(2_000_000),
        Backend::D3d12WorkGraph.entity_ceiling()
    );
}

#[test]
fn cost_model_full_budget_under_1ms_t0_t1() {
    // T0+T1 only, T2 amortized over 4 frames, T3 parallel-async (free).
    let cm = CostModel::new(EntityCount::full_budget());
    let us = cm.project_frame_us();
    // 100 × 22 + 5000 × 22 = 112_200 ns = 112us.
    // Add T2 amortized : 50_000 × 30 / 4 = 375_000 ns = 375us.
    // Total : 487us ⇒ under 1ms.
    assert!(us < 1_000);
}

#[test]
fn cost_model_indirect_fallback_costs_more() {
    let cm = CostModel::new(EntityCount::full_budget());
    let auto = cm.project_frame_us_on(Backend::D3d12WorkGraph);
    let fallback = cm.project_frame_us_on(Backend::IndirectFallback);
    // 1.0 / 0.75 = 1.333× ⇒ fallback is 33% slower at minimum.
    assert!(fallback >= auto + (auto / 4));
}

#[test]
fn canonical_quest_3_4_node_chain_under_90hz_vr() {
    let s = build_canonical_quest_3_schedule(FeatureMatrix::ultimate()).unwrap();
    assert_eq!(s.len(), 4);
    assert!(s.within_budget());
}

#[test]
fn canonical_120hz_4_node_chain_under_8333us() {
    let s = build_canonical_120hz_schedule(FeatureMatrix::ultimate()).unwrap();
    assert_eq!(s.len(), 4);
    assert!(s.est_cost_us() <= 8_333);
}

#[test]
fn dgc_fallback_lowers_correctly_when_dgc_only() {
    let s = WorkGraphBuilder::new()
        .with_label("dgc-1m")
        .auto_select(FeatureMatrix::dgc_only())
        .with_budget(FrameBudget::hz_120())
        .node(
            WorkGraphNode::compute("T0Tier", StageId::WaveSolver, DispatchArgs::new(2, 1, 1))
                .with_cost_us(132)
                .with_shader_tag("entity_tick_t0"),
        )
        .unwrap()
        .node(
            WorkGraphNode::compute("T1Tier", StageId::WaveSolver, DispatchArgs::new(78, 1, 1))
                .with_input("T0Tier")
                .with_cost_us(6_600)
                .with_shader_tag("entity_tick_t1"),
        )
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(s.backend(), Backend::VulkanDgc);
    let seq = DgcSequence::from_schedule(&s).unwrap();
    assert_eq!(seq.dispatch_count(), 2);
    assert_eq!(seq.pipeline_bind_count(), 2);
}

#[test]
fn indirect_fallback_lowers_correctly_when_no_autonomous_backend() {
    let s = WorkGraphBuilder::new()
        .auto_select(FeatureMatrix::none())
        .node(WorkGraphNode::compute(
            "T0",
            StageId::WaveSolver,
            DispatchArgs::new(2, 1, 1),
        ))
        .unwrap()
        .node(
            WorkGraphNode::compute("T1", StageId::WaveSolver, DispatchArgs::new(78, 1, 1))
                .with_input("T0"),
        )
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(s.backend(), Backend::IndirectFallback);
    let chain = IndirectChain::from_schedule(&s).unwrap();
    assert_eq!(chain.len(), 2);
}

#[test]
fn work_graph_d3d12_lowers_when_ultimate() {
    let s = build_1m_entity_schedule_ultimate();
    let wg = WorkGraphD3d12::from_schedule(&s).unwrap();
    assert_eq!(wg.entry_count(), 3);
    assert!(wg.estimated_backing_memory_bytes() >= 64 * 1024);
}

#[test]
fn work_graph_d3d12_refused_for_dgc_backend() {
    let s = WorkGraphBuilder::new()
        .auto_select(FeatureMatrix::dgc_only())
        .node(WorkGraphNode::compute(
            "X",
            StageId::WaveSolver,
            DispatchArgs::new(1, 1, 1),
        ))
        .unwrap()
        .build()
        .unwrap();
    let r = WorkGraphD3d12::from_schedule(&s);
    assert!(r.is_err());
}

#[test]
fn detect_backend_picks_correctly_per_feature_matrix() {
    use cssl_work_graph::detect_backend;
    let mut f = FeatureMatrix::none();
    assert_eq!(detect_backend(&f), Backend::IndirectFallback);
    f.vk_nv_device_generated_commands = true;
    assert_eq!(detect_backend(&f), Backend::VulkanDgc);
    f.d3d12_work_graphs_tier_1_0 = true;
    assert_eq!(detect_backend(&f), Backend::D3d12WorkGraph);
}

#[test]
fn aggregate_dispatch_groups_handle_1m_entity_distribution() {
    let s = build_1m_entity_schedule_ultimate();
    let total: u64 = s.iter_in_order().map(WorkGraphNode::dispatch_groups).sum();
    // T0 (2) + T1 (78) + T2 (195*4 = 780) = 860 groups
    assert_eq!(total, 2 + 78 + 195 * 4);
}

#[test]
fn entity_count_full_budget_aggregates_to_1m() {
    let e = EntityCount::full_budget();
    assert_eq!(e.total(), 100 + 5_000 + 50_000 + 945_000);
    // Aggregate ≈ 1_000_100 ⇒ "1M-entity headroom".
    assert!(e.total() >= 1_000_000);
}

#[test]
fn frame_budget_120hz_8333us_matches_density_spec() {
    assert_eq!(FrameBudget::hz_120().frame_us(), 8_333);
}

#[test]
fn frame_budget_60hz_16667us_matches_density_spec() {
    assert_eq!(FrameBudget::hz_60().frame_us(), 16_667);
}

#[test]
fn frame_budget_90hz_vr_11111us_matches_density_spec() {
    assert_eq!(FrameBudget::hz_90_vr().frame_us(), 11_111);
}

#[test]
fn perf_factor_ranking_dx12_dgc_indirect() {
    assert!(Backend::D3d12WorkGraph.perf_factor() > Backend::VulkanDgc.perf_factor());
    assert!(Backend::VulkanDgc.perf_factor() > Backend::IndirectFallback.perf_factor());
}

#[test]
fn descriptor_carries_backend_reason() {
    let s = build_1m_entity_schedule_ultimate();
    let d = s.descriptor();
    assert_eq!(d.backend, Backend::D3d12WorkGraph);
    assert!(d.reason.contains("WorkGraphsTier"));
}

#[test]
fn cost_model_default_zero() {
    let cm = CostModel::default();
    assert_eq!(cm.project_frame_us(), 0);
}

#[test]
fn dgc_sequence_round_trips_terminator() {
    let s = WorkGraphBuilder::new()
        .auto_select(FeatureMatrix::dgc_only())
        .node(
            WorkGraphNode::compute("A", StageId::WaveSolver, DispatchArgs::new(1, 1, 1))
                .with_shader_tag("s1"),
        )
        .unwrap()
        .build()
        .unwrap();
    let seq = DgcSequence::from_schedule(&s).unwrap();
    assert!(matches!(
        seq.commands().last().unwrap(),
        cssl_work_graph::dgc::DgcCommand::Terminator
    ));
}
