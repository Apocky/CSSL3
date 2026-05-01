//! § gm.rs — `GameMaster` aggregator + cap-gated emit surface.
//!
//! Wires the GM's four building-blocks together :
//!   - [`crate::cap_ladder::GmCapTable`] cap-bits.
//!   - [`crate::template_table::TemplateTable`] prose-fill.
//!   - [`crate::pacing::PacingPolicy`] trait-object.
//!   - [`crate::audit_sink::AuditSink`] trait-object.
//!
//! All emit-methods are cap-gated. Cap-deny is recorded as
//! `gm.cap_denied` in the audit-sink ; the call returns a degrade-style
//! result (silent-pass for text-emit, error for tone-tune). Voice-emit
//! is intentionally `Err(VoiceNotImplemented)` per the W6-deferred
//! mandate ; the cap exists so a future TTS-wire-up can be a one-line
//! method-body change without expanding the cap-namespace.

use std::sync::atomic::{AtomicU64, Ordering};

use cssl_host_cocreative::bias::BiasVector;
use serde::{Deserialize, Serialize};

use crate::audit_sink::{AuditEvent, AuditSink};
use crate::cap_ladder::{
    GmCapTable, GM_CAP_TEXT_EMIT, GM_CAP_TONE_TUNE, GM_CAP_VOICE_EMIT,
};
use crate::pacing::{PacingHint, PacingPolicy};
use crate::scene_input::GmSceneInput;
use crate::template_table::{EventClass, TemplateTable};
use crate::types::{
    NarrativeTextFrame, PacingKind, PacingMarkEvent, PromptSuggestion, ToneAxis,
};

/// Errors returned by `GameMaster` methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GmErr {
    /// A cap-bit is missing for the requested operation.
    CapDenied { cap_bit: u32, op: String },
    /// Voice-emit is not implemented in stage-0.
    VoiceNotImplemented,
}

impl std::fmt::Display for GmErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CapDenied { cap_bit, op } => {
                write!(f, "GM cap denied : op={op} cap_bit={cap_bit}")
            }
            Self::VoiceNotImplemented => write!(
                f,
                "GM voice-emit deferred (W6 ; TTS-backend not wired in stage-0)"
            ),
        }
    }
}

impl std::error::Error for GmErr {}

/// Stable hash of a [`BiasVector`] — FNV-1a-32 over the f32-bits of
/// each θ-component. Used as one factor of the deterministic seed.
#[must_use]
pub fn bias_vec_hash_fnv1a(bias: &BiasVector) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &v in bias.theta() {
        for b in v.to_bits().to_le_bytes() {
            h ^= u32::from(b);
            h = h.wrapping_mul(0x0100_0193);
        }
    }
    h
}

/// The Game Master.
///
/// Holds the cap-table, template-table, pacing-policy, audit-sink, and
/// a small set of monotonic counters used for replay-stable timestamps
/// and silent-pass instrumentation.
pub struct GameMaster {
    pub caps: GmCapTable,
    pub templates: TemplateTable,
    pub pacing: Box<dyn PacingPolicy>,
    pub audit: Box<dyn AuditSink>,
    /// Monotonic counter feeding replay-stable `ts_micros` values.
    /// Each emit increments the cursor by 1 and the recorded timestamp
    /// is `cursor * micros_per_tick`.
    cursor: AtomicU64,
    micros_per_tick: u64,
    /// Count of silent-pass events caused by `GM_CAP_TEXT_EMIT` denial.
    text_silent_count: AtomicU64,
}

impl GameMaster {
    /// Construct a `GameMaster` with all components wired.
    ///
    /// `micros_per_tick` is the synthetic-clock step ; tests typically
    /// pass `1` for unit-step timestamps.
    #[must_use]
    pub fn new(
        caps: GmCapTable,
        templates: TemplateTable,
        pacing: Box<dyn PacingPolicy>,
        audit: Box<dyn AuditSink>,
        micros_per_tick: u64,
    ) -> Self {
        Self {
            caps,
            templates,
            pacing,
            audit,
            cursor: AtomicU64::new(0),
            micros_per_tick,
            text_silent_count: AtomicU64::new(0),
        }
    }

    /// How many `emit_text` calls were silent-passed due to cap-denial.
    #[must_use]
    pub fn text_silent_count(&self) -> u64 {
        self.text_silent_count.load(Ordering::Relaxed)
    }

    fn next_ts(&self) -> u64 {
        let n = self.cursor.fetch_add(1, Ordering::Relaxed) + 1;
        n.saturating_mul(self.micros_per_tick)
    }

    /// Emit one prose frame for the given scene + tone.
    ///
    /// § Cap : `GM_CAP_TEXT_EMIT` required. Missing → silent-pass +
    /// counter increment + audit-record `gm.cap_denied`.
    /// § Failure : Φ-tag-miss → degrade-to-generic prose + audit
    /// `gm.degrade.no_phi_tag`. Φ-tag pool-miss → degrade-to-generic
    /// prose + audit `gm.degrade.no_template`.
    ///
    /// Determinism : the pool-pick seed is
    /// `(zone_id, class_tag, phi_tags_hash, scene_cursor)` packed into
    /// a u64 ; replay-bit-equal across runs given identical inputs.
    pub fn emit_text(
        &self,
        scene: &GmSceneInput,
        tone: ToneAxis,
    ) -> Result<NarrativeTextFrame, GmErr> {
        if !self.caps.has(GM_CAP_TEXT_EMIT) {
            self.text_silent_count.fetch_add(1, Ordering::Relaxed);
            self.audit.record(AuditEvent {
                kind: String::from("gm.cap_denied"),
                status: String::from("deny"),
                ts_micros: self.next_ts(),
                note: format!("cap_bit={GM_CAP_TEXT_EMIT} op=emit_text"),
            });
            return Err(GmErr::CapDenied {
                cap_bit: GM_CAP_TEXT_EMIT,
                op: String::from("emit_text"),
            });
        }
        // Pick event-class : if companion present + recent companion-cue,
        // use Companion ; else Examine if utterance present ; else
        // Arrive. (Stage-0 heuristic ; DM passes class explicitly in a
        // future iteration.)
        let class = if scene.companion_present {
            EventClass::Companion
        } else if scene.player_utterance.is_some() {
            EventClass::Examine
        } else {
            EventClass::Arrive
        };
        let bucket = TemplateTable::tone_bucket(tone);
        let phi_hash = scene.phi_tags_hash_fnv1a();
        let cursor = self.cursor.load(Ordering::Relaxed);
        let seed = u64::from(phi_hash)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(cursor)
            .wrapping_add(u64::from(scene.zone_id) << 16)
            .wrapping_add(u64::from(class.wire_tag()));
        let primary_tag = scene.phi_tags.first().copied();
        let ts = self.next_ts();
        let prose = match self
            .templates
            .pick(scene.zone_id, class, bucket, primary_tag, seed)
        {
            Some((_id, p)) => p,
            None => {
                self.audit.record(AuditEvent {
                    kind: String::from("gm.degrade.no_template"),
                    status: String::from("degrade"),
                    ts_micros: ts,
                    note: format!("zone={} class={:?}", scene.zone_id, class),
                });
                String::from("you see something here")
            }
        };
        // Φ-tag-miss degrade : prose still contains "{tag}" if the
        // template required one but no tag was available. We replaced
        // {tag} with "something" upstream ; surface that as an audit
        // signal too.
        if primary_tag.is_none() {
            self.audit.record(AuditEvent {
                kind: String::from("gm.degrade.no_phi_tag"),
                status: String::from("degrade"),
                ts_micros: ts,
                note: format!("zone={}", scene.zone_id),
            });
        }
        let frame = NarrativeTextFrame {
            utf8_text: prose,
            tone,
            ts_micros: ts,
        };
        self.audit.record(AuditEvent {
            kind: String::from("gm.text_emit"),
            status: String::from("ok"),
            ts_micros: ts,
            note: format!("zone={} class={:?}", scene.zone_id, class),
        });
        Ok(frame)
    }

    /// Emit a pacing-mark event from a previously-computed hint.
    ///
    /// No cap-gate (pacing marks are pure-derivative of DM-supplied
    /// tension state ; they carry no user-facing prose). Audit-records
    /// `gm.pacing_mark` with magnitude.
    pub fn emit_pacing_mark(&self, hint: PacingHint) -> Result<PacingMarkEvent, GmErr> {
        let magnitude = match hint.kind {
            PacingKind::Rise | PacingKind::Fall => hint.tension_target,
            PacingKind::Hold => 0.0,
        };
        let ts = self.next_ts();
        let event = PacingMarkEvent {
            kind: hint.kind,
            magnitude,
            ts_micros: ts,
        };
        self.audit.record(AuditEvent {
            kind: String::from("gm.pacing_mark"),
            status: String::from("ok"),
            ts_micros: ts,
            note: format!("kind={:?} magnitude={magnitude}", hint.kind),
        });
        Ok(event)
    }

    /// Voice-emit — DEFERRED.
    ///
    /// The cap exists (`GM_CAP_VOICE_EMIT`) but the TTS path is not
    /// wired in stage-0. Always returns
    /// [`GmErr::VoiceNotImplemented`] ; the audit-sink records
    /// `gm.voice_deferred` so callers can detect the gap in telemetry.
    pub fn emit_voice(
        &self,
        _scene: &GmSceneInput,
        _tone: ToneAxis,
    ) -> Result<NarrativeTextFrame, GmErr> {
        if !self.caps.has(GM_CAP_VOICE_EMIT) {
            self.audit.record(AuditEvent {
                kind: String::from("gm.cap_denied"),
                status: String::from("deny"),
                ts_micros: self.next_ts(),
                note: format!("cap_bit={GM_CAP_VOICE_EMIT} op=emit_voice"),
            });
            return Err(GmErr::CapDenied {
                cap_bit: GM_CAP_VOICE_EMIT,
                op: String::from("emit_voice"),
            });
        }
        self.audit.record(AuditEvent {
            kind: String::from("gm.voice_deferred"),
            status: String::from("degrade"),
            ts_micros: self.next_ts(),
            note: String::from("TTS-backend not wired in stage-0"),
        });
        Err(GmErr::VoiceNotImplemented)
    }

    /// Tune the warm/terse/poetic axes from a cocreative bias-vector.
    ///
    /// § Cap : `GM_CAP_TONE_TUNE` required. Missing → returns
    /// neutral-tone + `gm.cap_denied` audit event ; never panics.
    ///
    /// § ALGORITHM
    ///   The first three theta-components map directly to (warm, terse,
    ///   poetic) deltas ; the bias is applied around 0.5 and clamped.
    ///   Bias vectors of dim < 3 leave the missing axes at 0.5.
    ///
    /// § DETERMINISM
    ///   Pure function of `bias.theta()` — bias-vec-hash-keyed for
    ///   replay verification.
    pub fn tune_tone(&self, bias: &BiasVector) -> ToneAxis {
        if !self.caps.has(GM_CAP_TONE_TUNE) {
            self.audit.record(AuditEvent {
                kind: String::from("gm.cap_denied"),
                status: String::from("deny"),
                ts_micros: self.next_ts(),
                note: format!("cap_bit={GM_CAP_TONE_TUNE} op=tune_tone"),
            });
            return ToneAxis::neutral();
        }
        let theta = bias.theta();
        let warm = 0.5 + theta.first().copied().unwrap_or(0.0);
        let terse = 0.5 + theta.get(1).copied().unwrap_or(0.0);
        let poetic = 0.5 + theta.get(2).copied().unwrap_or(0.0);
        let tone = ToneAxis::clamped(warm, terse, poetic);
        self.audit.record(AuditEvent {
            kind: String::from("gm.tone_tune"),
            status: String::from("ok"),
            ts_micros: self.next_ts(),
            note: format!(
                "bias_hash={:08x} warm={} terse={} poetic={}",
                bias_vec_hash_fnv1a(bias),
                tone.warm,
                tone.terse,
                tone.poetic
            ),
        });
        tone
    }

    /// Compute pacing for a scene + tension trajectory + fatigue.
    ///
    /// Convenience wrapper that delegates to the wrapped pacing-policy
    /// and records `gm.pacing_compute` for telemetry.
    pub fn compute_pacing(
        &self,
        tension_vec: &[f32],
        scene_cursor: u32,
        player_fatigue: f32,
    ) -> PacingHint {
        let hint = self
            .pacing
            .compute_pacing(tension_vec, scene_cursor, player_fatigue);
        self.audit.record(AuditEvent {
            kind: String::from("gm.pacing_compute"),
            status: String::from("ok"),
            ts_micros: self.next_ts(),
            note: format!(
                "kind={:?} beat_spacing_ms={} idle_allow={}",
                hint.kind, hint.beat_spacing_ms, hint.idle_allow
            ),
        });
        hint
    }

    /// Emit a small list of action-prompts for the player.
    ///
    /// Stage-0 picks 3 generic prompts based on companion-presence +
    /// Φ-tags ; the Collaborator role refines this in a later wave.
    /// `max_select` is fixed at 3 for stage-0.
    pub fn prompt_suggestions(&self, scene: &GmSceneInput) -> PromptSuggestion {
        let mut items = Vec::with_capacity(3);
        if let Some(&first) = scene.phi_tags.first() {
            let name = self.templates.resolve_tag(first);
            items.push(format!("examine the {name}"));
        } else {
            items.push(String::from("look around"));
        }
        if scene.companion_present {
            items.push(String::from("speak with your companion"));
        } else {
            items.push(String::from("call out"));
        }
        items.push(String::from("wait"));
        PromptSuggestion {
            items,
            max_select: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_sink::NullAuditSink;
    use crate::pacing::Stage0PacingPolicy;

    fn build_gm(caps: GmCapTable) -> GameMaster {
        GameMaster::new(
            caps,
            TemplateTable::default_stage0(),
            Box::new(Stage0PacingPolicy),
            Box::new(NullAuditSink),
            1,
        )
    }

    #[test]
    fn emit_text_without_cap_silent_passes() {
        let gm = build_gm(GmCapTable::empty());
        let s = GmSceneInput::default_empty();
        let r = gm.emit_text(&s, ToneAxis::neutral());
        assert!(matches!(r, Err(GmErr::CapDenied { .. })));
        assert_eq!(gm.text_silent_count(), 1);
    }

    #[test]
    fn emit_text_with_cap_succeeds() {
        let gm = build_gm(GmCapTable::all());
        let mut s = GmSceneInput::default_empty();
        s.phi_tags = vec![101];
        let r = gm.emit_text(&s, ToneAxis::neutral()).unwrap();
        assert!(!r.utf8_text.is_empty());
    }

    #[test]
    fn voice_emit_deferred() {
        let gm = build_gm(GmCapTable::all());
        let s = GmSceneInput::default_empty();
        let r = gm.emit_voice(&s, ToneAxis::neutral());
        assert_eq!(r, Err(GmErr::VoiceNotImplemented));
    }

    #[test]
    fn bias_vec_hash_stable() {
        let b = BiasVector::from_slice(&[0.1_f32, 0.2, 0.3]);
        let h1 = bias_vec_hash_fnv1a(&b);
        let h2 = bias_vec_hash_fnv1a(&b);
        assert_eq!(h1, h2);
    }
}
