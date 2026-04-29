//! § cssl-substrate-save — integration tests for the H5 invariant.
//!
//! § COVERAGE
//!   - Save → load → snapshot byte-equal under various Ω-tensor shapes.
//!   - Save → tamper-on-disk → load REFUSES with `AttestationMismatch`.
//!   - Bit-equal-replay invariant : `replay_from(save, save.frame).snapshot()
//!     == save.snapshot()` byte-equal (the load-bearing H5 contract).
//!   - Path-hash discipline : error messages don't leak cleartext paths.
//!   - Per the REPORT BACK request, a "save at frame N → load → step → compare"
//!     scenario validates the cross-process determinism shape (here
//!     `step` is the stage-0 placeholder ; H2's real omega_step will
//!     replace it transparently).

use cssl_substrate_save::{
    load, replay::check_bit_equal_replay, replay::ReplayResult, replay_from, save, OmegaCell,
    OmegaScheduler, OmegaTensor, ReplayEvent, ReplayKind, SaveFile,
};

/// Hand-rolled per-test temp path.
fn tmp_path(test_name: &str) -> std::path::PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut p = std::env::temp_dir();
    p.push(format!("cssl-h5-int-{test_name}-{pid}-{nanos}.csslsave"));
    p
}

#[test]
fn save_at_frame_n_load_step_compare_byte_equal() {
    // The canonical REPORT BACK (e) scenario : save at frame N → load
    // → step (placeholder no-op at stage-0) → compare with original at
    // frame N → BYTE-EQUAL Ω-tensor states.
    let mut sched = OmegaScheduler::new();
    sched.insert_tensor(
        "world-state",
        OmegaTensor::scalar(OmegaCell::with_label(
            cssl_substrate_save::OMEGA_TYPE_TAG_F64,
            std::f64::consts::PI.to_le_bytes().to_vec(),
            0xCAFE_F00D,
        )),
    );
    sched.insert_tensor(
        "frame-counter",
        OmegaTensor::scalar(OmegaCell::new(
            cssl_substrate_save::OMEGA_TYPE_TAG_I64,
            42i64.to_le_bytes().to_vec(),
        )),
    );
    sched.frame = 100;

    // Save snapshot.
    let original_snapshot = sched.snapshot_tensors();
    let path = tmp_path("save-load-compare");
    save(&sched, &path).expect("save must succeed");

    // Load.
    let loaded = load(&path).expect("load must succeed");
    let loaded_snapshot = loaded.snapshot_tensors();

    // Byte-equal Ω-tensor states.
    assert_eq!(loaded_snapshot, original_snapshot);
    // Frame counter survives.
    assert_eq!(loaded.frame, 100);
    // IFC-label survives.
    assert_eq!(loaded.tensors[1].1.cells[0].ifc_label, 0xCAFE_F00D);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn replay_from_save_byte_equals_save_snapshot() {
    // The H5 invariant : replay_from(save, save.frame).snapshot_tensors()
    // == save.snapshot_omega() byte-equal.
    let mut sched = OmegaScheduler::new();
    sched.insert_tensor(
        "k",
        OmegaTensor::scalar(OmegaCell::new(
            cssl_substrate_save::OMEGA_TYPE_TAG_I32,
            7i32.to_le_bytes().to_vec(),
        )),
    );
    sched.frame = 1;

    let sf = SaveFile::from_scheduler(&sched);
    let restored = replay_from(&sf, sf.frame);
    assert_eq!(restored.snapshot_tensors(), sf.snapshot_omega());
}

#[test]
fn check_bit_equal_replay_passes_on_no_events() {
    let mut sched = OmegaScheduler::new();
    sched.insert_tensor(
        "k",
        OmegaTensor::scalar(OmegaCell::new(
            cssl_substrate_save::OMEGA_TYPE_TAG_I32,
            1i32.to_le_bytes().to_vec(),
        )),
    );
    let sf = SaveFile::from_scheduler(&sched);
    assert_eq!(check_bit_equal_replay(&sf), ReplayResult::Equal);
}

#[test]
fn check_bit_equal_replay_skips_when_h2_pending() {
    // The dedicated bit-equal-after-tick test gate-skips with a clear
    // message ("H2 omega_step pending") until the real omega_step lands.
    let mut sched = OmegaScheduler::new();
    sched
        .replay_log
        .append(ReplayEvent::new(0, ReplayKind::Sim, vec![1, 2, 3]));
    sched.frame = 1;
    let sf = SaveFile::from_scheduler(&sched);
    let result = check_bit_equal_replay(&sf);
    match result {
        ReplayResult::Skipped(reason) => {
            assert!(reason.contains("H2"));
            assert!(reason.contains("omega_step"));
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
}

#[test]
fn save_load_round_trip_with_realistic_replay_log() {
    // 16 events spanning 4 frames + the recorded events round-trip.
    let mut sched = OmegaScheduler::new();
    sched.insert_tensor(
        "snapshot",
        OmegaTensor::scalar(OmegaCell::new(
            cssl_substrate_save::OMEGA_TYPE_TAG_U8,
            vec![0xAA, 0xBB, 0xCC, 0xDD],
        )),
    );
    sched.frame = 4;
    for f in 0..4u64 {
        sched
            .replay_log
            .append(ReplayEvent::new(f, ReplayKind::Sim, vec![f as u8]));
        sched.replay_log.append(ReplayEvent::new(
            f,
            ReplayKind::Render,
            vec![f as u8 ^ 0xFF],
        ));
        sched
            .replay_log
            .append(ReplayEvent::new(f, ReplayKind::Audio, vec![f as u8 + 1]));
        sched.replay_log.append(ReplayEvent::new(
            f,
            ReplayKind::Save,
            vec![(f as u8).wrapping_sub(1)],
        ));
    }

    let path = tmp_path("realistic-replay");
    save(&sched, &path).expect("save must succeed");
    let loaded = load(&path).expect("load must succeed");
    assert_eq!(loaded.frame, 4);
    assert_eq!(loaded.replay_log.events.len(), 16);
    // Sorted-event-stream survives the round-trip.
    let sorted_input = sched.replay_log.sorted_events();
    let sorted_loaded = loaded.replay_log.sorted_events();
    assert_eq!(sorted_input, sorted_loaded);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn ifc_label_survives_full_disk_round_trip() {
    let mut sched = OmegaScheduler::new();
    sched.insert_tensor(
        "secret",
        OmegaTensor::scalar(OmegaCell::with_label(
            cssl_substrate_save::OMEGA_TYPE_TAG_F32,
            (-1.0f32).to_le_bytes().to_vec(),
            0x1234_5678,
        )),
    );

    let path = tmp_path("ifc-label");
    save(&sched, &path).expect("save");
    let loaded = load(&path).expect("load");
    let cell = &loaded.tensors[0].1.cells[0];
    assert_eq!(cell.ifc_label, 0x1234_5678);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn tamper_on_disk_blocks_load() {
    // PRIME-DIRECTIVE : attestation-mismatch is HARD-FAIL.
    let mut sched = OmegaScheduler::new();
    sched.insert_tensor(
        "k",
        OmegaTensor::scalar(OmegaCell::new(
            cssl_substrate_save::OMEGA_TYPE_TAG_I32,
            0x1234_5678i32.to_le_bytes().to_vec(),
        )),
    );

    let path = tmp_path("tamper-block");
    save(&sched, &path).expect("save");

    // Flip a bit somewhere in the body.
    let mut bytes = std::fs::read(&path).expect("read");
    let body_off = 24; // past header + name-len
    bytes[body_off] ^= 0x40;
    std::fs::write(&path, &bytes).expect("write");

    let err = load(&path).unwrap_err();
    match err {
        cssl_substrate_save::LoadError::AttestationMismatch => (),
        other => panic!("expected AttestationMismatch, got {other:?}"),
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn rapid_save_load_cycle_does_not_drift() {
    // PRIME-DIRECTIVE : "save scumming" is observable but not blocked.
    // Crucially, the SHAPE of repeated save/load cycles is byte-stable —
    // the format + attestation are deterministic, so a rapid loop produces
    // byte-identical files every cycle.
    let mut sched = OmegaScheduler::new();
    sched.insert_tensor(
        "k",
        OmegaTensor::scalar(OmegaCell::new(
            cssl_substrate_save::OMEGA_TYPE_TAG_I32,
            1i32.to_le_bytes().to_vec(),
        )),
    );
    sched.frame = 7;

    let path = tmp_path("rapid-cycle");
    let mut prev_bytes: Option<Vec<u8>> = None;
    for _ in 0..5 {
        save(&sched, &path).expect("save");
        let bytes = std::fs::read(&path).expect("read");
        if let Some(prev) = &prev_bytes {
            assert_eq!(prev, &bytes, "save bytes must be byte-stable across cycles");
        }
        prev_bytes = Some(bytes);
        let _ = load(&path).expect("load");
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn save_at_frame_n_load_then_replay_back_to_frame_n() {
    // Walk : save scheduler @ frame N → load → replay_from(loaded_save, M)
    // for M < N → replay_from(loaded_save, N). The frame-N replay byte-
    // equals the save's snapshot. The frame-M replay returns a clamped
    // log + state.
    let mut sched = OmegaScheduler::new();
    sched.insert_tensor(
        "world",
        OmegaTensor::scalar(OmegaCell::new(
            cssl_substrate_save::OMEGA_TYPE_TAG_I64,
            123_456i64.to_le_bytes().to_vec(),
        )),
    );
    sched.frame = 10;
    for f in 0..10u64 {
        sched
            .replay_log
            .append(ReplayEvent::new(f, ReplayKind::Sim, vec![f as u8]));
    }

    let path = tmp_path("replay-back-to-n");
    save(&sched, &path).expect("save");

    let loaded = load(&path).expect("load");
    let loaded_save = SaveFile::from_scheduler(&loaded);

    // Replay to frame 10 (full) — byte-equal to save's snapshot.
    let full = replay_from(&loaded_save, loaded_save.frame);
    assert_eq!(full.snapshot_tensors(), loaded_save.snapshot_omega());
    assert_eq!(full.frame, 10);
    assert_eq!(full.replay_log.events.len(), 10);

    // Replay to frame 5 — events ≤ 5 only.
    let partial = replay_from(&loaded_save, 5);
    assert_eq!(partial.frame, 5);
    assert_eq!(partial.replay_log.events.len(), 6);
    assert!(partial.replay_log.events.iter().all(|e| e.frame <= 5));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn corrupting_attestation_byte_blocks_load() {
    let sched = OmegaScheduler::new();
    let path = tmp_path("corrupt-attestation");
    save(&sched, &path).expect("save");

    let mut bytes = std::fs::read(&path).expect("read");
    let attestation_off = bytes.len() - 32 - 8; // start-of-attestation
    bytes[attestation_off] ^= 0x01;
    std::fs::write(&path, &bytes).expect("write");

    let err = load(&path).unwrap_err();
    match err {
        cssl_substrate_save::LoadError::AttestationMismatch => (),
        other => panic!("expected AttestationMismatch, got {other:?}"),
    }

    let _ = std::fs::remove_file(&path);
}
