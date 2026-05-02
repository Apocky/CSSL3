// § cssl-host-netcode-fps : FPS network-replication kernel (pure-state-machine)
//
// ─ Quake/Source/Valve-style client-prediction + server-reconciliation
// ─ Valve-style lag-compensation (server-side world-rewind)
// ─ GGPO-style rollback-netcode (N-frames symmetric replay)
// ─ ω-field state-delta compression (sparse + RLE-friendly)
// ─ Snapshot-interpolation for remote-entities (Q16.16 lerp)
// ─ Adaptive bandwidth budget (AIMD ; 32 kbps floor, 256 kbps target)
//
// PRIME-DIRECTIVE attestation :
//   ─ Σ-mask gates EVERY replicated cell ; default-deny across-players
//   ─ Sensitive-category cells are STRUCTURALLY refused at the gate ;
//     refusal-count surfaces to attestation (loud-fail by-design)
//   ─ ¬ surveillance ; ¬ heuristic-aimbot detection ; player-pubkey-hash
//     is the only identity ; session-scoped
//   ─ sovereign-bypass would be RECORDED via attestation hook ; this crate
//     exposes no implicit-bypass surface

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]
#![allow(clippy::module_name_repetitions)]
// Wrap-aware u32↔i32 conversions are intentional throughout (tick math).
#![allow(clippy::cast_possible_wrap)]

pub mod bandwidth;
pub mod delta;
pub mod input;
pub mod lag_comp;
pub mod prediction;
pub mod reconciliation;
pub mod ring;
pub mod rollback;
pub mod sigma;
pub mod sim;
pub mod snapshot;
pub mod state;
pub mod tick;

// Re-exports : a stable public surface.
pub use bandwidth::{BandwidthBudget, MAX_KBPS, MIN_KBPS, TARGET_KBPS};
pub use delta::{full_replication_bytes, CellChange, SimDelta};
pub use input::{action, InputFrame};
pub use lag_comp::{validate_hit_at, HistoryRing, LagCompOutcome, DEFAULT_LAGCOMP_WINDOW};
pub use prediction::{ClientPrediction, PredictErr, DEFAULT_PREDICT_RING};
pub use reconciliation::{reconcile, ReconcileReport};
pub use ring::TickRing;
pub use rollback::{RollbackEngine, RollbackErr, DEFAULT_ROLLBACK_FRAMES};
pub use sigma::{
    gate_for_send, PeerSlot, SigmaCategory, SigmaCategoryRepr, SigmaMask, SigmaRefusal, MAX_PEERS,
};
pub use sim::{MovementStub, NoopSim, Simulation};
pub use snapshot::{lerp_state, SnapshotPair, DEFAULT_RENDER_LAG_TICKS};
pub use state::{Cell, CellId, CellValue, SimState};
pub use tick::{
    micros_to_tick, tick_to_micros, TickId, COMPETITIVE_TICK_RATE_HZ, DEFAULT_TICK_RATE_HZ,
};

/// Crate version string for handshake / debug logs.
pub const NETCODE_FPS_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod integration {
    use super::*;

    /// End-to-end : prediction → server-reconciliation round-trip survives
    /// a 5-tick misprediction-correction cycle without losing input ordering.
    #[test]
    fn prediction_reconciliation_round_trip() {
        let mut s0 = SimState::new(TickId::ZERO);
        s0.set(Cell {
            id: CellId(1),
            value: CellValue::V3(0, 0, 0),
            mask: SigmaMask::public(),
        });
        let mut cp = ClientPrediction::new(s0, 64);
        // Client predicts 10 ticks of move_x = 10.
        for t in 0..10u32 {
            let mut i = InputFrame::empty(TickId(t), t + 1);
            i.move_x = 10;
            cp.predict(&MovementStub, i).unwrap();
        }
        // Server confirms : tick 4 = (40, 0, 0). Predicted matches.
        let mut server_t4 = SimState::new(TickId(4));
        server_t4.set(Cell {
            id: CellId(1),
            value: CellValue::V3(40, 0, 0),
            mask: SigmaMask::public(),
        });
        let r = reconcile(&mut cp, &MovementStub, server_t4);
        assert!(r.matched);
        // Now server corrects : tick 6 = (30, 0, 0) due to wall.
        let mut server_t6 = SimState::new(TickId(6));
        server_t6.set(Cell {
            id: CellId(1),
            value: CellValue::V3(30, 0, 0),
            mask: SigmaMask::public(),
        });
        let r = reconcile(&mut cp, &MovementStub, server_t6);
        assert!(!r.matched);
        // Replays ticks 7, 8, 9, 10 from corrected baseline (latest = tick 10).
        assert_eq!(r.replayed, 4);
        let s10 = cp.predicted_at(TickId(10)).unwrap();
        match s10.get(CellId(1)).unwrap().value {
            CellValue::V3(x, _, _) => assert_eq!(x, 70), // 30 + 4*10
            _ => panic!(),
        }
    }

    /// Σ-mask attestation : sensitive cells are LOUDLY refused at the gate.
    #[test]
    fn sigma_mask_sensitive_refused_loudly() {
        let a = SimState::new(TickId(0));
        let mut b = SimState::new(TickId(1));
        b.set(Cell {
            id: CellId(99),
            value: CellValue::V3(1, 2, 3),
            mask: SigmaMask {
                category: SigmaCategoryRepr::Sensitive,
                bits: u64::MAX,
            },
        });
        let (delta, refusals) = SimDelta::for_recipient(&a, &b, PeerSlot(0));
        assert_eq!(refusals, 1, "sensitive must surface refusal-count");
        assert!(delta.changes.is_empty(), "sensitive cell never ships");
    }

    /// Bandwidth attestation : delta-only replication achieves > 90 % savings
    /// when only one cell changes out of 100.
    #[test]
    fn delta_bandwidth_savings_over_full_rep() {
        let mut prev = SimState::new(TickId(0));
        let mut cur = SimState::new(TickId(1));
        for i in 0..100u32 {
            let c = Cell {
                id: CellId(i),
                value: CellValue::V3(i as i32, 0, 0),
                mask: SigmaMask::public(),
            };
            prev.set(c.clone());
            cur.set(c);
        }
        cur.set(Cell {
            id: CellId(50),
            value: CellValue::V3(9999, 0, 0),
            mask: SigmaMask::public(),
        });
        let d = SimDelta::between(&prev, &cur);
        let full = full_replication_bytes(&cur);
        let ratio = BandwidthBudget::compression_ratio(d.bandwidth_bytes(), full);
        assert!(
            ratio < 6553,
            "delta should compress to < 10 % of full ({ratio} Q16.16)"
        );
    }

    /// Snapshot-interp + delta-compress + bandwidth-budget integrate cleanly.
    #[test]
    fn snapshot_interp_with_bandwidth_budget() {
        let mut budget = BandwidthBudget::new(TARGET_KBPS);
        let mut pair = SnapshotPair::default();
        let mut s_old = SimState::new(TickId(0));
        s_old.set(Cell {
            id: CellId(1),
            value: CellValue::V3(0, 0, 0),
            mask: SigmaMask::public(),
        });
        let mut s_new = SimState::new(TickId(6));
        s_new.set(Cell {
            id: CellId(1),
            value: CellValue::V3(60, 0, 0),
            mask: SigmaMask::public(),
        });
        pair.push(s_old);
        pair.push(s_new);
        let d = SimDelta::between(pair.older.as_ref().unwrap(), pair.newer.as_ref().unwrap());
        let bytes = d.bandwidth_bytes() as u32;
        assert!(budget.may_send(bytes, 16_667));
        let mid = lerp_state(&pair, TickId(3)).unwrap();
        match mid.get(CellId(1)).unwrap().value {
            CellValue::V3(x, _, _) => assert_eq!(x, 30),
            _ => panic!(),
        }
    }

    /// Lag-comp + reconciliation compose : the server can validate a hit
    /// against a rewound state while the client predicts forward.
    #[test]
    fn lag_comp_rewind_validates_against_history() {
        let mut hist = HistoryRing::new(64);
        for t in 0..10u32 {
            let mut s = SimState::new(TickId(t));
            s.set(Cell {
                id: CellId(7),
                value: CellValue::V3((t * 5) as i32, 0, 0),
                mask: SigmaMask::public(),
            });
            hist.record(s);
        }
        let outcome = validate_hit_at(&hist, TickId(4), TickId(9), |s| {
            let p = s.get(CellId(7)).unwrap();
            match p.value {
                CellValue::V3(x, _, _) => x == 20, // 4 * 5
                _ => false,
            }
        });
        assert_eq!(outcome, LagCompOutcome::Accept);
    }

    /// Rollback-netcode : N-frame budget enforced.
    #[test]
    fn rollback_n_frames_enforced() {
        let mut s0 = SimState::new(TickId::ZERO);
        s0.set(Cell {
            id: CellId(1),
            value: CellValue::V3(0, 0, 0),
            mask: SigmaMask::public(),
        });
        let mut e = RollbackEngine::new(s0, 4);
        e.add_peer(PeerSlot(0));
        for t in 0..10u32 {
            let mut i = InputFrame::empty(TickId(t), t + 1);
            i.move_x = 5;
            e.record_input(PeerSlot(0), i);
            e.advance(&MovementStub);
        }
        let r = e.rollback_and_replay(&MovementStub, TickId(2));
        assert!(matches!(r, Err(RollbackErr::BudgetExceeded { .. })));
        let r2 = e.rollback_and_replay(&MovementStub, TickId(7));
        assert!(r2.is_ok());
    }

    #[test]
    fn version_string_present() {
        assert!(!NETCODE_FPS_VERSION.is_empty());
    }

    /// Exhaustive Σ-refusal taxonomy : Public OK ; Sensitive Err ; Local Err ;
    /// Scoped-miss Err ; Scoped-hit OK.
    #[test]
    fn sigma_refusal_taxonomy_pinned() {
        let mut scoped = SigmaMask::scoped(0);
        scoped.allow(PeerSlot(3));

        assert!(gate_for_send(&SigmaMask::public(), PeerSlot(0)).is_ok());
        assert_eq!(
            gate_for_send(&SigmaMask::deny_all(), PeerSlot(0)),
            Err(SigmaRefusal::LocalOnly)
        );
        assert_eq!(
            gate_for_send(
                &SigmaMask {
                    category: SigmaCategoryRepr::Sensitive,
                    bits: u64::MAX,
                },
                PeerSlot(0)
            ),
            Err(SigmaRefusal::SensitiveBanned)
        );
        assert_eq!(
            gate_for_send(&scoped, PeerSlot(0)),
            Err(SigmaRefusal::ScopedNotInSet)
        );
        assert!(gate_for_send(&scoped, PeerSlot(3)).is_ok());
    }
}
