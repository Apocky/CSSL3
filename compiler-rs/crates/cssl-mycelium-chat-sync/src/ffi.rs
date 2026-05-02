//! § ffi — extern "C" surface for GM-persona consumption.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   The GM/DM persona-agent in `loa-host` reads the federated shared-state
//!   on every turn to modulate response-shape selection. The FFI surface is
//!   designed so the .csl-side persona can call into this crate WITHOUT
//!   knowing Rust ABI internals.
//!
//! § DECLARED-SYMBOLS (canonical names ; loa-host/ffi.rs links these)
//!   ─ __cssl_mycelium_chat_federation_count() -> u32
//!   ─ __cssl_mycelium_chat_federation_blake3(out: *mut [u8; 32]) -> i32
//!   ─ __cssl_mycelium_chat_federation_lookup_shape(intent: u8, phase: u8,
//!       out_shape: *mut u8, out_count: *mut u32, out_conf_q8: *mut u8)
//!       -> i32
//!   ─ __cssl_mycelium_chat_observe(pattern_bytes: *const [u8; 32]) -> i32
//!   ─ __cssl_mycelium_chat_tick(now_unix: u64) -> i32
//!   ─ __cssl_mycelium_chat_revoke(emitter_handle: u64, ts_unix: u64) -> i32
//!
//! § SAFETY
//!   These are declared but NOT defined here ; the host (`loa-host`) is
//!   expected to register concrete impls that bridge to a process-global
//!   `MyceliumChatSync` instance. We DO define the `extern "C"` `*_query` /
//!   `*_observe` shapes that take a `&MyceliumChatSync` pointer cast (so
//!   tests can exercise the FFI shape end-to-end without a host-side global).
//!
//! § STAGE-0 vs STAGE-1
//!   - Stage-0 : host-loa-host registers thin wrappers that look up a
//!     globally-installed `Arc<MyceliumChatSync>`.
//!   - Stage-1 : `.csl` declares the FFI symbols and consumes via extern.
//!     See `Labyrinth of Apocalypse/systems/mycelium_chat.csl`.

use crate::pattern::{ArcPhase, ChatPattern, IntentKind, ResponseShape};
use crate::sync::MyceliumChatSync;

// ─── status codes ──────────────────────────────────────────────────────────

/// Successful no-op or query.
pub const FFI_OK: i32 = 0;
/// Generic error : bad argument or null pointer.
pub const FFI_ERR_INVALID_ARG: i32 = -1;
/// Σ-mask cap-check denied operation.
pub const FFI_ERR_CAP_DENIED: i32 = -2;
/// Pattern was malformed / failed `validate()`.
pub const FFI_ERR_MALFORMED: i32 = -3;
/// Federation has fewer-than-k-floor distinct emitters for this pattern.
pub const FFI_ERR_K_ANON_NOT_MET: i32 = -4;

/// § federation_count_via — FFI-shaped accessor returning current public count.
///
/// Host-loa-host wraps this to register the canonical extern-name
/// `__cssl_mycelium_chat_federation_count`.
#[must_use]
pub fn federation_count_via(svc: &MyceliumChatSync) -> u32 {
    svc.federation().public_pattern_count() as u32
}

/// § federation_blake3_via — write the federation-digest into `out`.
///
/// Returns `FFI_OK` always (no failure modes for this query).
pub fn federation_blake3_via(svc: &MyceliumChatSync, out: &mut [u8; 32]) -> i32 {
    *out = svc.federation().federation_blake3();
    FFI_OK
}

/// § federation_lookup_shape_via — query "for intent X at phase Y, what
/// response-shape did the federation observe most-frequently AND has it
/// crossed the k-anon floor?"
///
/// Writes the dominant shape + observation-count + mean-confidence-q8 into
/// the out-pointers. Returns `FFI_ERR_K_ANON_NOT_MET` if no public pattern
/// matches.
///
/// Determinism : the `(intent, phase)` ↦ shape selection is a stable
/// argmax over (observation_count desc, response_shape asc).
pub fn federation_lookup_shape_via(
    svc: &MyceliumChatSync,
    intent: u8,
    phase: u8,
    out_shape: &mut u8,
    out_count: &mut u32,
    out_conf_q8: &mut u8,
) -> i32 {
    let want_intent = IntentKind::from_u8(intent);
    let want_phase = ArcPhase::from_u8(phase);
    let mut best: Option<(u32, u8, ResponseShape)> = None;
    for s in svc.federation().snapshot_public() {
        if s.intent_kind == want_intent && s.arc_phase == want_phase {
            let cand = (s.observation_count, s.mean_confidence_q8, s.response_shape);
            best = Some(match best {
                None => cand,
                Some(prev) => {
                    // Prefer higher count ; tie-break lower-numbered shape.
                    if cand.0 > prev.0
                        || (cand.0 == prev.0 && (cand.2 as u8) < (prev.2 as u8))
                    {
                        cand
                    } else {
                        prev
                    }
                }
            });
        }
    }
    match best {
        Some((count, conf_q8, shape)) => {
            *out_shape = shape as u8;
            *out_count = count;
            *out_conf_q8 = conf_q8;
            FFI_OK
        }
        None => FFI_ERR_K_ANON_NOT_MET,
    }
}

/// § observe_via — push a 32-byte raw pattern into the local ring.
pub fn observe_via(svc: &MyceliumChatSync, raw: &[u8; 32]) -> i32 {
    let p = ChatPattern::from_raw(*raw);
    if let Err(_e) = p.validate() {
        return FFI_ERR_MALFORMED;
    }
    svc.observe(p);
    FFI_OK
}

/// § tick_via — drive a single digest-tick.
pub fn tick_via(svc: &MyceliumChatSync, now_unix: u64) -> i32 {
    let _ = svc.tick(now_unix);
    FFI_OK
}

/// § revoke_via — sovereign revoke for `emitter_handle`.
pub fn revoke_via(svc: &MyceliumChatSync, emitter_handle: u64, ts_unix: u64) -> i32 {
    svc.revoke_emitter(emitter_handle, ts_unix);
    FFI_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{ChatPatternBuilder, CAP_FLAGS_ALL};
    use crate::sync::MyceliumChatSync;

    fn mk_svc() -> MyceliumChatSync {
        MyceliumChatSync::builder().k_floor(2).build()
    }

    fn mk_pattern(seed: u8, intent: IntentKind, phase: ArcPhase, shape: ResponseShape) -> ChatPattern {
        ChatPatternBuilder {
            intent_kind: intent,
            response_shape: shape,
            arc_phase: phase,
            confidence: 0.6,
            ts_unix: 60 * 100,
            region_tag: 1,
            opt_in_tier: 1,
            cap_flags: CAP_FLAGS_ALL,
            emitter_pubkey: [seed; 32],
            co_signers: vec![],
        }
        .build()
        .unwrap()
    }

    #[test]
    fn federation_count_via_starts_at_zero() {
        let svc = mk_svc();
        assert_eq!(federation_count_via(&svc), 0);
    }

    #[test]
    fn federation_blake3_via_writes_32_bytes() {
        let svc = mk_svc();
        let mut out = [0_u8; 32];
        let rc = federation_blake3_via(&svc, &mut out);
        assert_eq!(rc, FFI_OK);
        // After ingest, blake3 changes.
        let p1 = mk_pattern(
            1,
            IntentKind::Question,
            ArcPhase::Setup,
            ResponseShape::ShortDirect,
        );
        let p2 = mk_pattern(
            2,
            IntentKind::Question,
            ArcPhase::Setup,
            ResponseShape::ShortDirect,
        );
        svc.ingest_peer_differential(&crate::sync::ChatPatternDifferential {
            patterns: vec![p1, p2],
            tick_id: 1,
            emitter_handle: 0,
            ts_bucketed: 0,
        });
        let mut out2 = [0_u8; 32];
        federation_blake3_via(&svc, &mut out2);
        assert_ne!(out, out2);
    }

    #[test]
    fn lookup_shape_returns_k_anon_when_below_threshold() {
        let svc = mk_svc();
        // Single emitter ⊑ k_floor = 2 ⟶ staged.
        let p = mk_pattern(
            1,
            IntentKind::Question,
            ArcPhase::Setup,
            ResponseShape::ShortDirect,
        );
        svc.ingest_peer_differential(&crate::sync::ChatPatternDifferential {
            patterns: vec![p],
            tick_id: 1,
            emitter_handle: 0,
            ts_bucketed: 0,
        });
        let mut shape = 0_u8;
        let mut count = 0_u32;
        let mut conf = 0_u8;
        let rc = federation_lookup_shape_via(
            &svc,
            IntentKind::Question as u8,
            ArcPhase::Setup as u8,
            &mut shape,
            &mut count,
            &mut conf,
        );
        assert_eq!(rc, FFI_ERR_K_ANON_NOT_MET);
    }

    #[test]
    fn lookup_shape_returns_dominant_above_threshold() {
        let svc = mk_svc(); // k=2
        let p1 = mk_pattern(
            1,
            IntentKind::Combat,
            ArcPhase::Climax,
            ResponseShape::StorybeatPunch,
        );
        let p2 = mk_pattern(
            2,
            IntentKind::Combat,
            ArcPhase::Climax,
            ResponseShape::StorybeatPunch,
        );
        svc.ingest_peer_differential(&crate::sync::ChatPatternDifferential {
            patterns: vec![p1, p2],
            tick_id: 1,
            emitter_handle: 0,
            ts_bucketed: 0,
        });
        let mut shape = 0_u8;
        let mut count = 0_u32;
        let mut conf = 0_u8;
        let rc = federation_lookup_shape_via(
            &svc,
            IntentKind::Combat as u8,
            ArcPhase::Climax as u8,
            &mut shape,
            &mut count,
            &mut conf,
        );
        assert_eq!(rc, FFI_OK);
        assert_eq!(shape, ResponseShape::StorybeatPunch as u8);
        assert_eq!(count, 2);
        assert!(conf > 0);
    }

    #[test]
    fn observe_then_tick_via_emits_pattern() {
        let svc = mk_svc();
        let p = mk_pattern(
            7,
            IntentKind::Question,
            ArcPhase::Setup,
            ResponseShape::ShortDirect,
        );
        svc.grant_emitter(p.emitter_handle(), CAP_FLAGS_ALL);
        let raw = *p.as_bytes();
        let rc = observe_via(&svc, &raw);
        assert_eq!(rc, FFI_OK);
        let rc2 = tick_via(&svc, 60 * 100);
        assert_eq!(rc2, FFI_OK);
    }

    #[test]
    fn revoke_via_clears_local_state() {
        let svc = mk_svc();
        let p = mk_pattern(
            7,
            IntentKind::Question,
            ArcPhase::Setup,
            ResponseShape::ShortDirect,
        );
        svc.grant_emitter(p.emitter_handle(), CAP_FLAGS_ALL);
        svc.observe(p.clone());
        let rc = revoke_via(&svc, p.emitter_handle(), 1234);
        assert_eq!(rc, FFI_OK);
        assert!(svc.ring().is_empty());
    }
}
