//! § Integration tests for cssl-inspect (T11-D162).
//!
//! Spec-acceptance gate : 50+ tests pass. We deliver 60+.

use cssl_inspect::{
    capture_frame as _, Cap, CaptureFormat, ConsentBit, DevMode, EntityId, EntitySnapshot,
    FieldCellSnapshot, InspectError, Inspector, MaterialView, MortonKey, SceneGraphSnapshot,
    SigmaConsentBits, SigmaOverlay, TelemetryEgress, TimeMode, ATTESTATION, SLICE_ID,
};

fn build_scene_with_three_cells() -> SceneGraphSnapshot {
    let mut s = SceneGraphSnapshot::empty();
    s.push_cell(FieldCellSnapshot::new(
        MortonKey::new(0x1),
        "open/grass",
        MaterialView::new("grass", 1),
        1.0,
        [0.0, 0.0, 0.0],
        100,
    ));
    s.push_cell(FieldCellSnapshot::new(
        MortonKey::new(0x2),
        "biometric:face_geometry",
        MaterialView::new("skin", 2),
        1.05,
        [0.0, 0.0, 0.0],
        100,
    ));
    s.push_cell(FieldCellSnapshot::new(
        MortonKey::new(0x3),
        "private:reproductive_state",
        MaterialView::new("organ", 3),
        1.10,
        [0.0, 0.0, 0.0],
        100,
    ));
    s.push_entity(EntitySnapshot::new(EntityId::new(10), "open/human"));
    s.push_entity(EntitySnapshot::new(
        EntityId::new(11),
        "biometric:gait_signature",
    ));
    s
}

fn make_inspector(scene: SceneGraphSnapshot) -> Inspector {
    Inspector::attach(Cap::<DevMode>::dev_for_tests(), scene).expect("attach with dev cap")
}

// ─── A : ATTESTATION + identity ─────────────────────────────

#[test]
fn a01_attestation_const_present() {
    assert!(ATTESTATION.contains("no hurt nor harm"));
}

#[test]
fn a02_attestation_const_full_phrase() {
    assert_eq!(
        ATTESTATION,
        "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
    );
}

#[test]
fn a03_slice_id_t11() {
    assert!(SLICE_ID.starts_with("T11-D162"));
}

#[test]
fn a04_slice_id_mentions_cssl_inspect() {
    assert!(SLICE_ID.contains("cssl-inspect"));
}

// ─── B : capability gating ──────────────────────────────────

#[test]
fn b01_attach_with_dev_cap_succeeds() {
    let cap = Cap::<DevMode>::dev_for_tests();
    let scene = SceneGraphSnapshot::empty();
    assert!(Inspector::attach(cap, scene).is_ok());
}

#[test]
fn b02_attach_without_dev_cap_refused() {
    let cap = Cap::<DevMode>::synthetic_nondev_for_tests();
    let scene = SceneGraphSnapshot::empty();
    let r = Inspector::attach(cap, scene);
    assert!(
        matches!(r, Err(InspectError::CapabilityMissing { needed }) if needed.contains("DevMode"))
    );
}

#[test]
fn b03_capture_with_egress_cap_succeeds() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let r = insp.capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 });
    assert!(r.is_ok());
}

#[test]
fn b04_capture_without_egress_cap_refused() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let bad = Cap::<TelemetryEgress>::synthetic_nonegress_for_tests();
    let r = insp.capture_frame(&bad, CaptureFormat::PngSrgb { bit_depth: 8 });
    assert!(matches!(r, Err(InspectError::CapabilityMissing { .. })));
}

#[test]
fn b05_egress_cap_permits_egress() {
    let cap = Cap::<TelemetryEgress>::egress_for_tests();
    assert!(cap.permits_egress());
}

#[test]
fn b06_inspector_exposes_cap_dev() {
    let insp = make_inspector(SceneGraphSnapshot::empty());
    assert!(insp.cap_dev().permits_dev_mode());
}

// ─── C : Σ-mask read-gate ───────────────────────────────────

#[test]
fn c01_inspect_open_cell_succeeds() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let r = insp.inspect_cell(MortonKey::new(0x1));
    assert!(r.is_ok());
}

#[test]
fn c02_inspect_biometric_cell_refused() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let r = insp.inspect_cell(MortonKey::new(0x2));
    assert!(matches!(r, Err(InspectError::ConsentDenied { .. })));
}

#[test]
fn c03_inspect_private_cell_refused() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let r = insp.inspect_cell(MortonKey::new(0x3));
    assert!(matches!(r, Err(InspectError::ConsentDenied { .. })));
}

#[test]
fn c04_inspect_unknown_cell_not_found() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let r = insp.inspect_cell(MortonKey::new(0xdeadbeef));
    assert!(matches!(r, Err(InspectError::NotFound { .. })));
}

#[test]
fn c05_inspect_open_entity_succeeds() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let r = insp.inspect_entity(EntityId::new(10));
    assert!(r.is_ok());
}

#[test]
fn c06_inspect_biometric_entity_refused() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let r = insp.inspect_entity(EntityId::new(11));
    assert!(matches!(r, Err(InspectError::ConsentDenied { .. })));
}

#[test]
fn c07_inspect_unknown_entity_not_found() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let r = insp.inspect_entity(EntityId::new(999));
    assert!(matches!(r, Err(InspectError::NotFound { .. })));
}

#[test]
fn c08_consent_denied_carries_reason() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let err = insp.inspect_cell(MortonKey::new(0x2)).unwrap_err();
    if let InspectError::ConsentDenied { reason } = err {
        assert!(!reason.is_empty());
    } else {
        panic!("expected ConsentDenied");
    }
}

#[test]
fn c09_overlay_at_biometric_directly() {
    let sigma = SigmaOverlay::at("biometric:retina");
    assert!(!sigma.permits(ConsentBit::Observe));
}

#[test]
fn c10_overlay_at_open_directly() {
    let sigma = SigmaOverlay::at("ground/dirt");
    assert!(sigma.permits(ConsentBit::Observe));
}

#[test]
fn c11_sigma_low_only_in_snapshot() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let snap = insp.inspect_cell(MortonKey::new(0x1)).unwrap();
    assert!(snap.facet_sigma_low_only.permits(ConsentBit::Observe));
}

#[test]
fn c12_sigma_consent_bits_default_closed() {
    let bits = SigmaConsentBits::default();
    assert!(!bits.permits(ConsentBit::Observe));
}

// ─── D : audit-sequence monotonicity ────────────────────────

#[test]
fn d01_audit_seq_starts_at_zero() {
    let insp = make_inspector(SceneGraphSnapshot::empty());
    assert_eq!(insp.audit_seq(), 0);
}

#[test]
fn d02_audit_seq_increments_on_successful_read() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let _ = insp.inspect_cell(MortonKey::new(0x1)).unwrap();
    assert_eq!(insp.audit_seq(), 1);
}

#[test]
fn d03_audit_seq_increments_on_refused_read() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let _ = insp.inspect_cell(MortonKey::new(0x2));
    assert_eq!(insp.audit_seq(), 1);
}

#[test]
fn d04_audit_seq_monotone_across_mixed_reads() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let _ = insp.inspect_cell(MortonKey::new(0x1));
    let _ = insp.inspect_cell(MortonKey::new(0x2));
    let _ = insp.inspect_entity(EntityId::new(10));
    assert_eq!(insp.audit_seq(), 3);
}

#[test]
fn d05_audit_seq_propagates_into_snapshot() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let snap = insp.inspect_cell(MortonKey::new(0x1)).unwrap();
    assert_eq!(snap.audit_seq, 1);
}

#[test]
fn d06_audit_seq_bumps_on_capture() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let _ = insp.capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 });
    assert_eq!(insp.audit_seq(), 1);
}

// ─── E : time-control state machine ─────────────────────────

#[test]
fn e01_initial_mode_is_running() {
    let insp = make_inspector(SceneGraphSnapshot::empty());
    assert_eq!(insp.time_mode(), TimeMode::Running);
}

#[test]
fn e02_pause_transitions_to_paused() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let m = insp.pause().unwrap();
    assert_eq!(m, TimeMode::Paused);
}

#[test]
fn e03_resume_transitions_to_running() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    insp.pause().unwrap();
    let m = insp.resume().unwrap();
    assert_eq!(m, TimeMode::Running);
}

#[test]
fn e04_pause_idempotent() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    insp.pause().unwrap();
    insp.pause().unwrap();
    assert_eq!(insp.time_control().pause_count(), 1);
}

#[test]
fn e05_resume_idempotent_when_already_running() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    insp.resume().unwrap();
    assert_eq!(insp.time_control().resume_count(), 0);
}

#[test]
fn e06_step_zero_refused() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    insp.pause().unwrap();
    let r = insp.step(0);
    assert!(matches!(r, Err(InspectError::TimeControlRefused { .. })));
}

#[test]
fn e07_step_from_running_refused() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let r = insp.step(5);
    assert!(matches!(r, Err(InspectError::TimeControlRefused { .. })));
}

#[test]
fn e08_step_from_paused_succeeds() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    insp.pause().unwrap();
    let m = insp.step(3).unwrap();
    assert_eq!(m, TimeMode::Paused);
}

#[test]
fn e09_step_accumulates_frames() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    insp.pause().unwrap();
    insp.step(2).unwrap();
    insp.step(5).unwrap();
    assert_eq!(insp.time_control().frames_stepped(), 7);
}

#[test]
fn e10_pause_resume_cycle() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    for _ in 0..3 {
        insp.pause().unwrap();
        insp.resume().unwrap();
    }
    assert_eq!(insp.time_control().pause_count(), 3);
    assert_eq!(insp.time_control().resume_count(), 3);
}

// ─── F : capture_frame across formats ───────────────────────

#[test]
fn f01_capture_png_succeeds() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let h = insp
        .capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 })
        .unwrap();
    assert_eq!(h.format_tag, "png_srgb");
}

#[test]
fn f02_capture_exr_succeeds() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let h = insp
        .capture_frame(
            &egress,
            CaptureFormat::ExrHdr {
                half_precision: true,
            },
        )
        .unwrap();
    assert_eq!(h.format_tag, "exr_hdr");
}

#[test]
fn f03_capture_spectral_bin_succeeds() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let h = insp
        .capture_frame(&egress, CaptureFormat::SpectralBin { n_bands: 16 })
        .unwrap();
    assert_eq!(h.format_tag, "spectral_bin");
}

#[test]
fn f04_capture_png_invalid_bit_depth_refused() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let r = insp.capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 12 });
    assert!(matches!(
        r,
        Err(InspectError::CaptureFormatUnsupported { .. })
    ));
}

#[test]
fn f05_capture_spectral_zero_bands_refused() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let r = insp.capture_frame(&egress, CaptureFormat::SpectralBin { n_bands: 0 });
    assert!(matches!(
        r,
        Err(InspectError::CaptureFormatUnsupported { .. })
    ));
}

#[test]
fn f06_capture_three_formats_distinct_hashes() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let h1 = insp
        .capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 })
        .unwrap();
    let h2 = insp
        .capture_frame(
            &egress,
            CaptureFormat::ExrHdr {
                half_precision: true,
            },
        )
        .unwrap();
    let h3 = insp
        .capture_frame(&egress, CaptureFormat::SpectralBin { n_bands: 16 })
        .unwrap();
    assert_ne!(h1.output_path_hash, h2.output_path_hash);
    assert_ne!(h2.output_path_hash, h3.output_path_hash);
    assert_ne!(h1.output_path_hash, h3.output_path_hash);
}

#[test]
fn f07_capture_handle_size_nonzero() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let h = insp
        .capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 16 })
        .unwrap();
    assert!(h.size_bytes > 0);
}

#[test]
fn f08_capture_handle_path_hash_32_bytes() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let h = insp
        .capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 })
        .unwrap();
    assert_eq!(h.output_path_hash.raw().len(), 32);
}

#[test]
fn f09_capture_increments_audit_seq_each_call() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    insp.capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 })
        .unwrap();
    insp.capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 })
        .unwrap();
    assert_eq!(insp.audit_seq(), 2);
}

#[test]
fn f10_capture_handle_audit_seq_matches_inspector() {
    let mut insp = make_inspector(SceneGraphSnapshot::empty());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let h = insp
        .capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 })
        .unwrap();
    assert_eq!(h.audit_seq, insp.audit_seq());
}

// ─── G : read-only API ───────────────────────────────────────

#[test]
fn g01_scene_accessor_is_borrow() {
    let insp = make_inspector(build_scene_with_three_cells());
    let scene_ref: &SceneGraphSnapshot = insp.scene();
    assert_eq!(scene_ref.cell_count(), 3);
}

#[test]
fn g02_scene_iter_cells() {
    let insp = make_inspector(build_scene_with_three_cells());
    let count = insp.scene().cells().count();
    assert_eq!(count, 3);
}

#[test]
fn g03_scene_iter_entities() {
    let insp = make_inspector(build_scene_with_three_cells());
    let count = insp.scene().entities().count();
    assert_eq!(count, 2);
}

#[test]
fn g04_scene_lookup_cell_by_key_present() {
    let insp = make_inspector(build_scene_with_three_cells());
    assert!(insp.scene().cell_by_key(MortonKey::new(0x1)).is_some());
}

#[test]
fn g05_scene_lookup_cell_by_key_absent() {
    let insp = make_inspector(build_scene_with_three_cells());
    assert!(insp.scene().cell_by_key(MortonKey::new(0xdead)).is_none());
}

#[test]
fn g06_inspector_methods_dont_mutate_scene() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let before = insp.scene().cell_count();
    let _ = insp.inspect_cell(MortonKey::new(0x1));
    let _ = insp.inspect_cell(MortonKey::new(0x2));
    let after = insp.scene().cell_count();
    assert_eq!(before, after);
}

#[test]
fn g07_inspector_audit_seq_independent_of_scene() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let _ = insp.inspect_cell(MortonKey::new(0x1));
    assert_eq!(insp.scene().cell_count(), 3);
    assert_eq!(insp.audit_seq(), 1);
}

// ─── H : error discrimination ────────────────────────────────

#[test]
fn h01_error_consent_denied_distinct_from_not_found() {
    let consent = InspectError::ConsentDenied { reason: "x".into() };
    let not_found = InspectError::NotFound { what: "y".into() };
    assert_ne!(consent, not_found);
}

#[test]
fn h02_error_clone_round_trip() {
    let e = InspectError::ConsentDenied {
        reason: "test".into(),
    };
    let cloned: InspectError = Clone::clone(&e);
    assert_eq!(e, cloned);
}

#[test]
fn h03_error_display_renders() {
    let e = InspectError::ConsentDenied {
        reason: "test reason".into(),
    };
    let s = format!("{e}");
    assert!(s.contains("test reason"));
}

#[test]
fn h04_error_capability_missing_carries_needed() {
    let e = InspectError::CapabilityMissing {
        needed: "Cap<DevMode>".into(),
    };
    let s = format!("{e}");
    assert!(s.contains("DevMode"));
}

#[test]
fn h05_time_control_refused_distinct() {
    let a = InspectError::TimeControlRefused { reason: "x".into() };
    let b = InspectError::CaptureFormatUnsupported { tag: "x".into() };
    assert_ne!(a, b);
}

// ─── J : misc surface coverage ───────────────────────────────

#[test]
fn j01_morton_key_round_trip() {
    let k = MortonKey::new(0xcafe_babe);
    assert_eq!(k.raw(), 0xcafe_babe);
}

#[test]
fn j02_entity_id_round_trip() {
    let e = EntityId::new(42);
    assert_eq!(e.raw(), 42);
}

#[test]
fn j03_material_view_construct() {
    let m = MaterialView::new("metal", 7);
    assert_eq!(m.class, "metal");
}

#[test]
fn j04_field_cell_snapshot_clone() {
    let c = FieldCellSnapshot::new(
        MortonKey::new(1),
        "ok",
        MaterialView::new("a", 0),
        1.0,
        [0.0; 3],
        0,
    );
    let c2: FieldCellSnapshot = Clone::clone(&c);
    assert_eq!(c, c2);
}

#[test]
fn j05_entity_snapshot_builder() {
    let e = EntitySnapshot::new(EntityId::new(1), "tag")
        .with_aura(0.7)
        .with_bones(206);
    assert!((e.aura_amp - 0.7).abs() < f32::EPSILON);
    assert_eq!(e.bone_joint_count, 206);
}

#[test]
fn j06_scene_from_parts_round_trip() {
    let cells = vec![FieldCellSnapshot::new(
        MortonKey::new(1),
        "ok",
        MaterialView::new("a", 0),
        1.0,
        [0.0; 3],
        0,
    )];
    let entities = vec![EntitySnapshot::new(EntityId::new(2), "ok")];
    let s = SceneGraphSnapshot::from_parts(cells, entities);
    assert_eq!(s.cell_count(), 1);
    assert_eq!(s.entity_count(), 1);
}

#[test]
fn j07_overlay_substring_match_biometric_inside_path() {
    let sigma = SigmaOverlay::at("data.biometric.flag");
    assert!(!sigma.permits(ConsentBit::Observe));
}

#[test]
fn j08_overlay_neutral_path_open() {
    let sigma = SigmaOverlay::at("substrate/cell/0/material");
    assert!(sigma.permits(ConsentBit::Observe));
}

#[test]
fn j09_consent_bit_enum_repr() {
    assert_eq!(ConsentBit::Observe as u32, 0);
    assert_eq!(ConsentBit::Sample as u32, 1);
}

#[test]
fn j10_time_mode_stepping_carries_remaining() {
    let m = TimeMode::Stepping { remaining: 7 };
    if let TimeMode::Stepping { remaining } = m {
        assert_eq!(remaining, 7);
    } else {
        panic!("expected Stepping");
    }
}

// ─── K : composite scenarios ─────────────────────────────────

#[test]
fn k01_full_iteration_loop_sketch() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let snap = insp.inspect_cell(MortonKey::new(0x1)).unwrap();
    assert_eq!(snap.morton_key, MortonKey::new(0x1));
    insp.pause().unwrap();
    insp.step(2).unwrap();
    insp.resume().unwrap();
    let h = insp
        .capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 16 })
        .unwrap();
    assert_eq!(h.format_tag, "png_srgb");
    assert!(insp.audit_seq() >= 2);
}

#[test]
fn k02_repeated_biometric_attempts_all_refused_audited() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    for _ in 0..5 {
        let r = insp.inspect_cell(MortonKey::new(0x2));
        assert!(matches!(r, Err(InspectError::ConsentDenied { .. })));
    }
    assert_eq!(insp.audit_seq(), 5);
}

#[test]
fn k03_mixed_open_and_refused_reads_all_counted() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let _ = insp.inspect_cell(MortonKey::new(0x1));
    let _ = insp.inspect_cell(MortonKey::new(0x2));
    let _ = insp.inspect_cell(MortonKey::new(0x3));
    let _ = insp.inspect_entity(EntityId::new(10));
    let _ = insp.inspect_entity(EntityId::new(11));
    assert_eq!(insp.audit_seq(), 5);
}

#[test]
fn k04_capture_after_refused_inspect_continues() {
    let mut insp = make_inspector(build_scene_with_three_cells());
    let egress = Cap::<TelemetryEgress>::egress_for_tests();
    let _ = insp.inspect_cell(MortonKey::new(0x2));
    let r = insp.capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 });
    assert!(r.is_ok());
}

#[test]
fn k05_attach_fresh_inspector_resets_audit_seq() {
    let cap = Cap::<DevMode>::dev_for_tests();
    let scene = SceneGraphSnapshot::empty();
    let insp = Inspector::attach(cap, scene).unwrap();
    assert_eq!(insp.audit_seq(), 0);
    assert_eq!(insp.time_mode(), TimeMode::Running);
}

#[test]
fn k06_distinct_inspectors_independent_audit_seq() {
    let mut insp_a = make_inspector(build_scene_with_three_cells());
    let insp_b = make_inspector(build_scene_with_three_cells());
    let _ = insp_a.inspect_cell(MortonKey::new(0x1));
    assert_eq!(insp_a.audit_seq(), 1);
    assert_eq!(insp_b.audit_seq(), 0);
}
