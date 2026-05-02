// § integration.rs · cssl-host-nd-substrate · cross-module scenarios
// ══════════════════════════════════════════════════════════════════
// § Cross-module integration : crystals with N-D extents · player navigates
// non-spatial dimensions · lens-rotation reveals hidden cells · consent-
// gate refuses unauthorized observers. These exercise the full surface
// the way a real LoA-host would.
// ══════════════════════════════════════════════════════════════════

use cssl_host_nd_substrate::{
    axis,
    lens::{
        causal_arc_for_stage0, mood_temporal_for_stage0, spatial_xyz_for_stage0,
        DimensionalLens, LensRotation,
    },
    NdCoord, NdField, Stage0Coord, Stage0Field, STAGE0_N,
};

#[test]
fn crystal_with_nd_extent_visible_through_lens() {
    // § A crystal "exists" in a 2×2×2 spatial cube but ALSO across 3 mood-bands.
    // Spatial-renderer should see 8 cells × 1 (mood-fixed) = 8 cells visible.
    let mut field: Stage0Field<&'static str> = NdField::new();
    let lo: Stage0Coord = NdCoord::from_axes([0, 0, 0, 0, 0, 0, 0, 0]);
    let hi: Stage0Coord = NdCoord::from_axes([1, 1, 1, 0, 2, 0, 0, 0]);
    let written = field.insert_extent(&lo, &hi, &"obsidian-crystal").unwrap();
    // 2 × 2 × 2 × 1 × 3 × 1 × 1 × 1 = 24 cells.
    assert_eq!(written, 24);

    let lens = spatial_xyz_for_stage0();
    let visible = field.query_visible_through_lens(&lens).unwrap();
    // All 24 cells project — spatial-axes vary across 8 unique [x,y,z]
    // and the same xyz appears 3 times for each mood-band.
    assert_eq!(visible.len(), 24);

    // Box-query restricts to the 8 spatial cells regardless of mood.
    let visible_box = field
        .query_box_through_lens(&lens, [0, 0, 0], [1, 1, 1])
        .unwrap();
    assert_eq!(visible_box.len(), 24);
}

#[test]
fn player_navigates_mood_dimension() {
    // § Player starts at spatial origin with neutral mood (4=0).
    // They walk SEVEN steps "deeper into melancholy" : axis-4 from 0 to -7.
    // Spatial position never changes ; mood-temporal lens reveals progress.
    let mut player: Stage0Coord = NdCoord::origin();
    for _ in 0..7 {
        player.set(axis::MOOD, player.get(axis::MOOD).unwrap() - 1).unwrap();
    }
    assert_eq!(player.get(axis::MOOD).unwrap(), -7);

    // Spatial-lens shows nothing changed.
    let spatial = spatial_xyz_for_stage0();
    assert_eq!(spatial.project_to_3d(&player).unwrap(), [0, 0, 0]);

    // Mood-temporal lens reveals the journey : mood maps to X.
    let mood_lens = mood_temporal_for_stage0();
    assert_eq!(mood_lens.project_to_3d(&player).unwrap(), [-7, 0, 0]);
}

#[test]
fn lens_rotation_reveals_hidden_axes() {
    // § A cell hidden in the temporal axis is invisible to spatial-lens
    // but appears once observer rotates the lens to expose temporal.
    let mut field: Stage0Field<u32> = NdField::new();
    let hidden_cell: Stage0Coord = NdCoord::from_axes([0, 0, 0, 42, 0, 0, 0, 0]);
    field.insert(hidden_cell, 7);

    let mut lens = spatial_xyz_for_stage0();
    let visible = field.query_visible_through_lens(&lens).unwrap();
    // Spatial projection collapses temporal-axis difference : cell shows at [0,0,0].
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].0, [0, 0, 0]);

    // Rotate : promote temporal (axis 3) into the X-slot.
    lens.apply(LensRotation::PromoteSource {
        slot: 0,
        new_axis: axis::TEMPORAL,
    })
    .unwrap();
    let visible_rotated = field.query_visible_through_lens(&lens).unwrap();
    assert_eq!(visible_rotated[0].0, [42, 0, 0]);
}

#[test]
fn consent_revoke_disables_perception() {
    // § Substrate-discipline : revoke is permanent.
    let mut lens = spatial_xyz_for_stage0();
    let coord: Stage0Coord = NdCoord::from_axes([1, 2, 3, 0, 0, 0, 0, 0]);
    assert!(lens.project_to_3d(&coord).is_ok());
    lens.revoke();
    assert!(lens.project_to_3d(&coord).is_err());
    // Re-consent after revoke is structurally-rejected.
    assert!(lens.consent(99).is_err());
}

#[test]
fn causal_arc_lens_for_narrative_debugger() {
    // § Causal-arc lens projects (causality, arc, temporal) for a
    // narrative-debugger view : "show me what caused what at which point in
    // the story-arc, ordered in time".
    let lens = causal_arc_for_stage0();
    let event: Stage0Coord =
        NdCoord::from_axes([100, 100, 100, 5, 0, 7, 11, 0]); // temporal=5 arc=7 causality=11
    let xyz = lens.project_to_3d(&event).unwrap();
    assert_eq!(xyz, [11, 7, 5]);
}

#[test]
fn lens_with_explicit_consent_records_tick() {
    use cssl_host_nd_substrate::lens::ConsentState;
    // § Consent at tick 12345 ; auditable consent-state.
    let lens = DimensionalLens::with_consent(
        vec![0, 1, 2, 3, 4, 5, 6, 7],
        [0, 1, 2],
        STAGE0_N,
        12345,
    )
    .unwrap();
    assert_eq!(
        lens.consent_state(),
        ConsentState::Consented { at_tick: 12345 }
    );
}

#[test]
fn distance_metric_distinguishes_spatial_vs_mood() {
    // § Two cells that are spatially-identical but mood-distant should
    // appear close to spatial-lens AND far to mood-only distance metric.
    let a: Stage0Coord = NdCoord::from_axes([5, 5, 5, 0, 0, 0, 0, 0]);
    let b: Stage0Coord = NdCoord::from_axes([5, 5, 5, 0, 100, 0, 0, 0]);
    let spatial = a.squared_distance_along(&b, &[0, 1, 2]).unwrap();
    let mood_only = a.squared_distance_along(&b, &[axis::MOOD]).unwrap();
    assert_eq!(spatial, 0);
    assert_eq!(mood_only, 100 * 100);
}

#[test]
fn const_generic_widens_to_n_32() {
    // § Const-generic surface scales — N can grow without code changes.
    // Verify N=32 compiles + round-trips.
    let mut f: NdField<u8, 32> = NdField::new();
    let mut axes = [0i32; 32];
    for (i, slot) in axes.iter_mut().enumerate() {
        *slot = i32::try_from(i).expect("32 < i32::MAX");
    }
    let coord: NdCoord<32> = NdCoord::from_axes(axes);
    f.insert(coord, 99);
    assert_eq!(f.get(&coord), Some(&99));
    assert_eq!(f.len(), 1);
}
