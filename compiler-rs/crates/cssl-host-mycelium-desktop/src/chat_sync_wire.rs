//! § chat_sync_wire — wire `cssl-mycelium-chat-sync` into the Mycelium app.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W11-MYCELIUM-CHAT-SYNC desktop integration
//!
//! § THESIS
//!   The chat-sync service runs alongside the agent-loop. Each turn that
//!   completes, the desktop app classifies the (user-input, reply) shape +
//!   pushes a `ChatPattern` into the local ring. A periodic digest-tick
//!   (driven by the host) drains the ring + broadcasts.
//!
//! § DEFAULT POSTURE — SOVEREIGN-RESPECTING
//!   The chat-sync is constructed in DEFAULT-DENY mode : no emitter is
//!   pre-granted. Until the user explicitly opts-in (via the consent-arch
//!   ceremony), the ring fills locally but ticks emit zero patterns.
//!
//! § INTEGRATION SURFACE
//!   - `ChatSyncWire::new()` — builds the service, leaves it inert
//!   - `ChatSyncWire::observe_turn()` — classify+push from a turn
//!   - `ChatSyncWire::opt_in_emitter()` — user-grants Σ-mask cap
//!   - `ChatSyncWire::sovereign_revoke()` — break-glass purge
//!   - `ChatSyncWire::tick_now()` — caller-driven digest-tick
//!   - `ChatSyncWire::federation_count()` — observability
//!
//! § PRIME-DIRECTIVE
//!   - default-deny ; opt-in via explicit cap-grant
//!   - Σ-mask gates on emit AND ingest (defense-in-depth via the crate)
//!   - sovereign-revoke wipes local state + broadcasts purge

use cssl_mycelium_chat_sync::{
    ArcPhase, ChatPatternBuilder, ChatPatternFederation, ChatSyncStats, IntentKind,
    MyceliumChatSync, NullBroadcastSink, ResponseShape, BroadcastSink, CAP_FLAGS_ALL,
};
use std::sync::Arc;

/// § ChatSyncWire — the desktop-app's integration handle for the federation.
pub struct ChatSyncWire {
    service: Arc<MyceliumChatSync>,
    /// Pubkey-stub for THIS desktop instance. In stage-0, a fixed-per-app
    /// 32-byte BLAKE3-derived value ; stage-1 wires a real Ed25519 keypair
    /// via host-attestation.
    local_pubkey: [u8; 32],
}

impl ChatSyncWire {
    /// § new — build a default chat-sync service with NullBroadcastSink.
    /// The caller can later wire a real broadcast-sink via the
    /// `with_broadcast_sink` builder.
    #[must_use]
    pub fn new(local_pubkey: [u8; 32]) -> Self {
        Self {
            service: Arc::new(MyceliumChatSync::new()),
            local_pubkey,
        }
    }

    /// § with_broadcast_sink — install a real (non-null) broadcast sink.
    ///
    /// Used by the desktop app to bridge into `cssl-host-mycelium`'s
    /// `TransportAdapter`. Reconstructs the underlying `MyceliumChatSync`
    /// because the sink is set at-builder time.
    #[must_use]
    pub fn with_broadcast_sink(local_pubkey: [u8; 32], sink: Arc<dyn BroadcastSink>) -> Self {
        Self {
            service: Arc::new(MyceliumChatSync::builder().sink(sink).build()),
            local_pubkey,
        }
    }

    /// § service — handle to the inner service.
    #[must_use]
    pub fn service(&self) -> Arc<MyceliumChatSync> {
        Arc::clone(&self.service)
    }

    /// § local_pubkey — borrow this instance's pubkey-stub.
    #[must_use]
    pub fn local_pubkey(&self) -> &[u8; 32] {
        &self.local_pubkey
    }

    /// § observe_turn — classify a (user-input, reply) into a `ChatPattern`
    /// and push it to the local ring. Σ-mask gating is checked at digest-
    /// time (the local ring is always sovereign-local).
    ///
    /// The classifier is intentionally cheap : token-count buckets +
    /// keyword-prefix matches. Stage-1 may swap a learned classifier ; the
    /// pattern-shape API is forward-stable.
    pub fn observe_turn(
        &self,
        user_input: &str,
        reply: &str,
        ts_unix: u64,
        region_tag: u16,
        opt_in_tier: u8,
    ) {
        let intent_kind = classify_intent(user_input);
        let response_shape = classify_response_shape(reply);
        let arc_phase = classify_arc_phase(reply);
        let confidence = response_confidence(user_input, reply);

        let Ok(pattern) = (ChatPatternBuilder {
            intent_kind,
            response_shape,
            arc_phase,
            confidence,
            ts_unix,
            region_tag,
            opt_in_tier,
            // Stage-0 : default cap-flags = ALL. The ring is sovereign-
            // local ; the cap-policy on the service still gates emit.
            // Patterns whose author is NOT yet opted-in via opt_in_emitter
            // fail the cap_check at tick-time + are dropped silently.
            cap_flags: CAP_FLAGS_ALL,
            emitter_pubkey: self.local_pubkey,
            co_signers: vec![],
        })
        .build() else {
            // Validation failure : skip ; observability covered by
            // ChatSyncStats.patterns_dropped_by_cap (well, by ring counters).
            return;
        };
        self.service.observe(pattern);
    }

    /// § opt_in_emitter — extend the Σ-mask cap-policy for THIS desktop's
    /// pubkey. Default-deny posture : without this call, ticks emit zero.
    pub fn opt_in_emitter(&self) {
        // Compute the emitter_handle (blake3-trunc) the same way the
        // ChatPatternBuilder does, then grant.
        let h = derive_local_handle(&self.local_pubkey);
        self.service.grant_emitter(h, CAP_FLAGS_ALL);
    }

    /// § sovereign_revoke — break-glass : zero cap-policy + purge ring +
    /// purge federation + broadcast purge-request.
    pub fn sovereign_revoke(&self, ts_unix: u64) {
        let h = derive_local_handle(&self.local_pubkey);
        self.service.revoke_emitter(h, ts_unix);
    }

    /// § tick_now — caller-driven digest-tick.
    pub fn tick_now(&self, now_unix: u64) {
        let _ = self.service.tick(now_unix);
    }

    /// § federation_count — current public-pattern count (k-anon-met).
    #[must_use]
    pub fn federation_count(&self) -> usize {
        self.service.federation().public_pattern_count()
    }

    /// § stats — observability snapshot.
    #[must_use]
    pub fn stats(&self) -> ChatSyncStats {
        self.service.stats()
    }

    /// § federation_blake3 — replay-stable digest of public-set.
    #[must_use]
    pub fn federation_blake3(&self) -> [u8; 32] {
        self.service.federation().federation_blake3()
    }

    /// § federation — borrow the read-handle for GM/DM modulator. This is
    /// the canonical way the persona-agent reads the federation.
    #[must_use]
    pub fn federation(&self) -> &ChatPatternFederation {
        self.service.federation()
    }
}

impl std::fmt::Debug for ChatSyncWire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatSyncWire")
            .field("federation_count", &self.federation_count())
            .field("stats", &self.stats())
            .finish()
    }
}

impl Default for ChatSyncWire {
    /// Default uses an all-zero pubkey + null sink ; suitable for unit-tests
    /// + pre-bootstrap construction. Production wiring MUST replace the
    /// pubkey with a real Ed25519-derived value via `new()`.
    fn default() -> Self {
        Self {
            service: Arc::new(MyceliumChatSync::builder().sink(Arc::new(NullBroadcastSink)).build()),
            local_pubkey: [0_u8; 32],
        }
    }
}

// ─── classifiers (stage-0 ; replaceable) ────────────────────────────────────

fn classify_intent(input: &str) -> IntentKind {
    let s = input.to_ascii_lowercase();
    let s = s.trim();
    if s.is_empty() {
        return IntentKind::Unknown;
    }
    if s.ends_with('?') {
        return IntentKind::Question;
    }
    // Imperative-ish prefixes ⟶ Command.
    for p in [
        "do ", "make ", "set ", "use ", "run ", "go ", "open ", "close ", "give ",
    ] {
        if s.starts_with(p) {
            return IntentKind::Command;
        }
    }
    if s.contains("attack") || s.contains("fight") || s.contains("strike") {
        return IntentKind::Combat;
    }
    if s.contains("explore") || s.contains("look") || s.contains("search") {
        return IntentKind::Exploration;
    }
    if s.contains("craft") || s.contains("forge") || s.contains("recipe") {
        return IntentKind::Crafting;
    }
    if s.contains("speak") || s.contains("greet") || s.contains("trade") {
        return IntentKind::Social;
    }
    if s.contains("feel") || s.contains("think") || s.contains("remember") {
        return IntentKind::Reflection;
    }
    if s.contains("lore") || s.contains("history") || s.contains("legend") {
        return IntentKind::Worldbuilding;
    }
    if s.starts_with('/') {
        return IntentKind::Meta;
    }
    IntentKind::Unknown
}

fn classify_response_shape(reply: &str) -> ResponseShape {
    let s = reply.trim();
    if s.is_empty() {
        return ResponseShape::Unknown;
    }
    let token_estimate = s.split_whitespace().count();
    let has_dialogue = s.contains('"') || s.contains('\u{201C}'); // smart-quote also
    let has_bullet = s.contains("\n- ") || s.contains("\n* ") || s.starts_with("- ");
    let ends_question = s.ends_with('?');
    let has_dice_or_stat = s.contains("d20")
        || s.contains("HP ")
        || s.contains("HP:")
        || s.contains("DC ")
        || s.contains('%');

    if ends_question {
        return ResponseShape::QuestionBack;
    }
    if has_dice_or_stat {
        return ResponseShape::MechanicalReadout;
    }
    if has_bullet {
        return ResponseShape::BulletedOptions;
    }
    if has_dialogue {
        return ResponseShape::DialogueDriven;
    }
    if token_estimate <= 50 && !s.contains('.') {
        return ResponseShape::ShortDirect;
    }
    if token_estimate <= 30 && s.contains('!') {
        return ResponseShape::StorybeatPunch;
    }
    if token_estimate >= 100 {
        return ResponseShape::ScenicNarrative;
    }
    ResponseShape::AmbientHint
}

fn classify_arc_phase(_reply: &str) -> ArcPhase {
    // Stage-0 : we don't have arc-tracking yet ; default to Interlude
    // which is the most-neutral shape. Stage-1 : wire host-side dm_arc.
    ArcPhase::Interlude
}

fn response_confidence(input: &str, reply: &str) -> f32 {
    let in_len = input.split_whitespace().count();
    let out_len = reply.split_whitespace().count();
    if in_len == 0 || out_len == 0 {
        return 0.0;
    }
    // Naive : bounded ratio of (out / in) clamped to 0.1..=1.0.
    let r = (out_len as f32) / ((in_len as f32) + 5.0);
    (r * 0.5).clamp(0.1, 1.0)
}

fn derive_local_handle(pubkey: &[u8; 32]) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(b"cssl-mycelium-chat-sync\0emitter_handle\0v1");
    h.update(pubkey);
    let bytes = h.finalize();
    let mut buf = [0_u8; 8];
    buf.copy_from_slice(&bytes.as_bytes()[..8]);
    u64::from_le_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_wire() -> ChatSyncWire {
        ChatSyncWire::new([7_u8; 32])
    }

    #[test]
    fn classify_intent_question_via_terminal_questionmark() {
        assert_eq!(classify_intent("where am I?"), IntentKind::Question);
    }

    #[test]
    fn classify_intent_command_via_imperative() {
        assert_eq!(classify_intent("do the dance"), IntentKind::Command);
        assert_eq!(classify_intent("open the door"), IntentKind::Command);
    }

    #[test]
    fn classify_intent_combat() {
        assert_eq!(classify_intent("I attack the goblin"), IntentKind::Combat);
    }

    #[test]
    fn classify_intent_unknown_default() {
        assert_eq!(classify_intent("hmm okay"), IntentKind::Unknown);
    }

    #[test]
    fn classify_response_shape_short_direct() {
        assert_eq!(classify_response_shape("yes done"), ResponseShape::ShortDirect);
    }

    #[test]
    fn classify_response_shape_question_back() {
        assert_eq!(
            classify_response_shape("are you sure?"),
            ResponseShape::QuestionBack
        );
    }

    #[test]
    fn classify_response_shape_mechanical() {
        assert_eq!(
            classify_response_shape("Roll d20 + 5 vs DC 15"),
            ResponseShape::MechanicalReadout
        );
    }

    #[test]
    fn observe_turn_pushes_to_ring() {
        let wire = mk_wire();
        wire.observe_turn(
            "Where am I?",
            "You stand in a moss-lit chamber.",
            60 * 100,
            1,
            1,
        );
        assert_eq!(wire.service.ring().len(), 1);
    }

    #[test]
    fn opt_in_emitter_then_tick_emits_pattern() {
        let wire = mk_wire();
        wire.opt_in_emitter();
        wire.observe_turn(
            "Where am I?",
            "You stand in a moss-lit chamber.",
            60 * 100,
            1,
            1,
        );
        wire.tick_now(60 * 100);
        let s = wire.stats();
        assert_eq!(s.patterns_emitted, 1);
        assert_eq!(s.patterns_dropped_by_cap, 0);
    }

    #[test]
    fn no_opt_in_drops_at_emit() {
        let wire = mk_wire();
        // skip opt_in_emitter ⟶ default-deny posture.
        wire.observe_turn(
            "Where am I?",
            "You stand in a moss-lit chamber.",
            60 * 100,
            1,
            1,
        );
        wire.tick_now(60 * 100);
        let s = wire.stats();
        assert_eq!(s.patterns_emitted, 0);
        assert_eq!(s.patterns_dropped_by_cap, 1);
    }

    #[test]
    fn sovereign_revoke_clears_local_state() {
        let wire = mk_wire();
        wire.opt_in_emitter();
        wire.observe_turn("test", "ok", 60 * 100, 1, 1);
        wire.sovereign_revoke(1234);
        // ring purged + cap-policy zeroed
        assert!(wire.service.ring().is_empty());
        wire.tick_now(60 * 100);
        let s = wire.stats();
        assert_eq!(s.revokes_processed, 1);
        // No patterns survived to be emitted.
        assert_eq!(s.patterns_emitted, 0);
    }

    #[test]
    fn federation_count_starts_zero() {
        let wire = mk_wire();
        assert_eq!(wire.federation_count(), 0);
    }

    #[test]
    fn debug_does_not_panic() {
        let wire = mk_wire();
        let s = format!("{wire:?}");
        assert!(s.contains("ChatSyncWire"));
    }
}
