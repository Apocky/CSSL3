//! § cap_gating — exercises the GM cap-bit ladder.
//!
//! Verifies `GM_CAP_TEXT_EMIT`, `GM_CAP_VOICE_EMIT`, `GM_CAP_TONE_TUNE`
//! gate their respective methods + emit `gm.cap_denied` audit events
//! when missing.

use cssl_host_cocreative::bias::BiasVector;
use cssl_host_gm::{
    AuditEvent, AuditSink, GameMaster, GmCapTable, GmErr, GmSceneInput, RecordingAuditSink,
    Stage0PacingPolicy, TemplateTable, ToneAxis, GM_CAP_TEXT_EMIT, GM_CAP_TONE_TUNE,
    GM_CAP_VOICE_EMIT,
};

/// Thin Box-adapter that lets the test alias the sink via `Arc` so the
/// assertion-side can read events after the GM owns its boxed reference.
struct ArcSink(std::sync::Arc<RecordingAuditSink>);

impl AuditSink for ArcSink {
    fn record(&self, event: AuditEvent) {
        self.0.record(event);
    }
}

fn build_gm(caps: GmCapTable) -> (GameMaster, std::sync::Arc<RecordingAuditSink>) {
    // Two Arcs alias the same recording — one moves into the GM, one
    // returns to the test for read-side assertions.
    let sink_a = std::sync::Arc::new(RecordingAuditSink::new());
    let sink_b = sink_a.clone();
    let gm = GameMaster::new(
        caps,
        TemplateTable::default_stage0(),
        Box::new(Stage0PacingPolicy),
        Box::new(ArcSink(sink_a)),
        1,
    );
    (gm, sink_b)
}

#[test]
fn text_emit_denied_when_cap_missing() {
    let (gm, sink) = build_gm(GmCapTable::empty());
    let r = gm.emit_text(&GmSceneInput::default_empty(), ToneAxis::neutral());
    assert!(matches!(
        r,
        Err(GmErr::CapDenied {
            cap_bit: GM_CAP_TEXT_EMIT,
            ..
        })
    ));
    assert_eq!(gm.text_silent_count(), 1);
    assert_eq!(sink.count_kind("gm.cap_denied"), 1);
}

#[test]
fn voice_emit_denied_when_cap_missing() {
    let (gm, sink) = build_gm(GmCapTable::empty());
    let r = gm.emit_voice(&GmSceneInput::default_empty(), ToneAxis::neutral());
    assert!(matches!(
        r,
        Err(GmErr::CapDenied {
            cap_bit: GM_CAP_VOICE_EMIT,
            ..
        })
    ));
    assert_eq!(sink.count_kind("gm.cap_denied"), 1);
}

#[test]
fn tone_tune_denied_when_cap_missing_returns_neutral() {
    let (gm, sink) = build_gm(GmCapTable::empty());
    let bias = BiasVector::from_slice(&[0.4, -0.3, 0.2]);
    let tone = gm.tune_tone(&bias);
    assert_eq!(tone, ToneAxis::neutral());
    assert_eq!(sink.count_kind("gm.cap_denied"), 1);
}

#[test]
fn all_caps_granted_passes_through() {
    let (gm, _sink) = build_gm(GmCapTable::all());
    let mut s = GmSceneInput::default_empty();
    s.phi_tags = vec![101];
    assert!(gm.emit_text(&s, ToneAxis::neutral()).is_ok());
    let r_voice = gm.emit_voice(&s, ToneAxis::neutral());
    // voice cap granted but stage-0 returns VoiceNotImplemented.
    assert_eq!(r_voice, Err(GmErr::VoiceNotImplemented));
    let bias = BiasVector::from_slice(&[0.1, 0.0, 0.0]);
    let tone = gm.tune_tone(&bias);
    assert!(tone.warm > 0.5);
}

#[test]
fn cap_table_compose_with_without() {
    let t = GmCapTable::empty()
        .with(GM_CAP_TEXT_EMIT)
        .with(GM_CAP_TONE_TUNE);
    assert!(t.has(GM_CAP_TEXT_EMIT));
    assert!(t.has(GM_CAP_TONE_TUNE));
    assert!(!t.has(GM_CAP_VOICE_EMIT));
    let t2 = t.without(GM_CAP_TEXT_EMIT);
    assert!(!t2.has(GM_CAP_TEXT_EMIT));
    assert!(t2.has(GM_CAP_TONE_TUNE));
}
