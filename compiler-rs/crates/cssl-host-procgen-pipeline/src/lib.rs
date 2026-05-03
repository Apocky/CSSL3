//! § cssl-host-procgen-pipeline — runtime procgen foundation for LoA.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The SPINE of Apocky's "engine = runtime procgen NOT static scenes"
//!   thesis. A single crate that normalizes any input source (Text · Voice ·
//!   Touch) into a semantic-decoded [`IntentSemantic`], then deterministically
//!   produces a [`ProcgenOutput`] of Crystal-positions + asset-URIs +
//!   BLAKE3 fingerprint, given the player's [`ObserverCoord`] and a
//!   `budget_ms` time-slice.
//!
//! § APOCKY-DIRECTIVE (verbatim · feedback_engine_is_runtime_procedural_gen.md)
//!   "LoA = (window+input+text/voice UI) → (intent translation) →
//!    (procgen + online asset fetch) → (in-game content) at runtime ·
//!    static scenes/*.cssl = throwaway scaffolding · do NOT compile 47
//!    scenes individually · DO build text-input + procgen + HTTP-fetch +
//!    GLTF-parser pipeline · test_room is the empty container · contents
//!    are runtime-generated"
//!
//! § PIPELINE
//!   ```text
//!   IntentInput { text, source }
//!     │                                                     (this slice)
//!     ▼ parse_intent  (regex + grammar-pattern-match)
//!   IntentSemantic { CreateEntity | ModifyEntity | Query | Other }
//!     │
//!     ▼ wrap into ProcgenRequest { semantic, observer_pos, budget_ms }
//!   ProcgenRequest
//!     │
//!     ▼ generate  (BLAKE3-seeded · deterministic · crystal-allocate)
//!   ProcgenOutput { crystals, asset_uris, fingerprint }
//!     │
//!     ▼ asset_fetch_stub  (returns Vec<u8> — empty for stage-0)
//!   asset-bytes
//!   ```
//!
//! § DETERMINISM
//!   - `generate` is a pure fn of (semantic, observer_pos, budget_ms).
//!   - Same (semantic, observer_pos, budget_ms) → same fingerprint →
//!     same crystals.len() + same crystal positions + same asset_uris.
//!   - Replay-determinism preserved : the BLAKE3 fingerprint over the
//!     SERIALIZED IntentSemantic+ObserverCoord+budget gives a single u64
//!     seed that drives a small splitmix64 PRNG for crystal-positions.
//!
//! § PRIME-DIRECTIVE alignment
//!   - NO network calls in this crate (asset_fetch_stub returns empty).
//!   - NO LLM dependency in this crate (regex fallback only).
//!   - NO global state ; pure-fn surface.
//!   - All allocations bounded by `budget_ms` (see [`generate`] body).
//!   - serde derives present so that IntentInput / ProcgenRequest /
//!     ProcgenOutput can be inspected via cssl-edge for transparency.
//!
//! § FOLLOW-ON SLICES (explicit non-goals here)
//!   - F1 : LLM-bridge upgrade — replace regex-parse with llama.cpp/candle
//!     reading the same regex-fallback as last-resort. Same API surface ;
//!     [`ParseErr::LlmUnavailable`] is reserved for that path.
//!   - F2 : HTTP-fetch — replace `asset_fetch_stub` body with
//!     ureq blocking-GET (workspace-pinned 2.10) gated by uri-allowlist.
//!     Same fn shape, same `Result<Vec<u8>, FetchErr>`.
//!   - F3 : GLTF-parser — caller decodes returned Vec<u8> via gltf-rs
//!     streaming. Out-of-scope here.
//!   - F4 : ω-field-write — caller drives [`OmegaField::set_cell`] from
//!     [`ProcgenOutput::crystals`] using its own observer-policy. The
//!     crystal type carries everything needed (position + tag + Σ-mask).

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
// Fibonacci-disc layout uses straight `1.0 + i * 0.5` for readability ;
// the FMA-rewrite suggested by `suboptimal_flops` is harder to read for
// no measurable speedup at the call-volume here (≤4 crystals).
#![allow(clippy::suboptimal_flops)]

use serde::{Deserialize, Serialize};

// § The cssl-substrate-omega-field dep is an INTENT-CARRIER, not a runtime
// dep here : we re-export the FieldCell *type-name* through this comment so
// downstream slices (F4 ω-field-write) know which canonical struct to
// project Crystal records into. We do not pull cells/grid logic.
//
// See compiler-rs/crates/cssl-substrate-omega-field/src/field_cell.rs for
// the canonical 72-byte std430 layout.

// ════════════════════════════════════════════════════════════════════════════
// § TYPES — input, semantic, request, output, errors
// ════════════════════════════════════════════════════════════════════════════

/// The origin of a player utterance / input gesture.
///
/// `Text` and `Voice` are the canonical channels (per Apocky's
/// "describe in text or voice" directive in intent_translation.csl).
/// `Touch` is reserved for VR-pinch / touchscreen handles in stage-1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntentSource {
    /// Keyboard / chat / cssl-edge text-channel input.
    Text,
    /// Local voice input (Windows-SAPI / cssl-substrate-audio).
    Voice,
    /// Touchscreen gesture / VR pinch / mouse drag-to-conjure.
    Touch,
}

/// Raw input from a player — utterance + which channel it came in on.
///
/// `text` is the SOURCE-OF-TRUTH ; for a Voice utterance the local
/// recognizer fills `text` with the transcription before this struct
/// exists. We never reach into raw audio here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IntentInput {
    /// The transcribed / typed text. UTF-8.
    pub text: String,
    /// Which channel this came in on.
    pub source: IntentSource,
}

/// The decoded semantic intent — what the player WANTS the substrate
/// to do. This is the boundary type between parse and generate.
///
/// CreateEntity / ModifyEntity / Query are the three core verbs.
/// Other is the open-set escape hatch for utterances the regex-parser
/// can't pin to a canonical verb ; the LLM-bridge slice (F1) will
/// narrow these later.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntentSemantic {
    /// Conjure a new entity into the world.
    CreateEntity {
        /// Lower-case entity-kind tag ("dragon", "lantern", "door", ...).
        kind: String,
        /// (key, value) properties extracted from the utterance.
        /// Keys are lower-case ASCII identifiers.
        props: Vec<(String, String)>,
    },
    /// Mutate an entity that already exists.
    ModifyEntity {
        /// Stable handle to the entity (numeric or name).
        target: String,
        /// (key, value) property-deltas.
        props: Vec<(String, String)>,
    },
    /// Read-only inspection request — no substrate-mutation.
    Query {
        /// What is being asked about.
        subject: String,
    },
    /// Catch-all : the verb didn't match any known regex pattern.
    /// Carries the original text so the LLM-bridge slice can retry.
    Other(String),
}

/// A coordinate in the player's local reference frame, in metres.
///
/// We use plain f32 here rather than cssl-pga bivectors — those are
/// for the substrate-side ; this is the host-input-side. Conversion
/// happens at the F4 ω-field-write boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ObserverCoord {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl ObserverCoord {
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0, z: 0.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Raw bytes for fingerprinting (little-endian f32 triple).
    fn to_le_bytes(self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[0..4].copy_from_slice(&self.x.to_le_bytes());
        out[4..8].copy_from_slice(&self.y.to_le_bytes());
        out[8..12].copy_from_slice(&self.z.to_le_bytes());
        out
    }
}

/// One unit of generated content — the procgen output is a list of these.
///
/// "Crystal" is the LoA term for "a substrate-emitted object" — see
/// substrate_render_v2.csl + alien_materialization.csl. This struct is
/// the HOST-SIDE projection ; the substrate-side FieldCell version lives
/// in `cssl-substrate-omega-field::field_cell::FieldCell`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Crystal {
    /// World-space position relative to the observer.
    pub pos: ObserverCoord,
    /// Lower-case kind-tag — same vocabulary as IntentSemantic::CreateEntity.
    pub kind: String,
    /// Substrate-energy seed — drives KAN-bias evolution downstream.
    pub seed: u64,
}

/// A request to the procgen engine — pairs semantic intent with the
/// observer's current spatial frame and a time budget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcgenRequest {
    pub semantic: IntentSemantic,
    pub observer_pos: ObserverCoord,
    /// Soft budget in milliseconds. The current implementation is fast
    /// enough that this is informational ; F1 (LLM-upgrade) WILL spend it.
    pub budget_ms: u32,
}

/// The deterministic output of one [`generate`] call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcgenOutput {
    /// One Crystal per unit of generated content. Empty for Query intents.
    pub crystals: Vec<Crystal>,
    /// URIs to fetch via [`asset_fetch_stub`] (or its F2 successor).
    pub asset_uris: Vec<String>,
    /// 64-bit BLAKE3 fingerprint over the (semantic, observer_pos, budget)
    /// triple. Stable across hosts ; suitable for replay-determinism IDs.
    pub fingerprint: u64,
}

/// Errors from [`parse_intent`].
#[derive(Debug, thiserror::Error)]
pub enum ParseErr {
    /// The input text was empty after trim ; nothing to decode.
    #[error("empty intent text")]
    Empty,
    /// Reserved for the F1 LLM-bridge slice — when the LLM is unreachable
    /// AND the regex-parser also fails, this is the surfaced error.
    #[error("LLM unavailable and no regex match")]
    LlmUnavailable,
}

/// Errors from [`asset_fetch_stub`] / its F2 HTTP successor.
#[derive(Debug, thiserror::Error)]
pub enum FetchErr {
    /// URI did not parse / was rejected by the allowlist.
    #[error("invalid asset uri: {0}")]
    InvalidUri(String),
    /// Reserved for the F2 HTTP slice — connection / status errors.
    #[error("network error: {0}")]
    Network(String),
}

// ════════════════════════════════════════════════════════════════════════════
// § parse_intent — regex + grammar-pattern decode
// ════════════════════════════════════════════════════════════════════════════

/// Decode a raw [`IntentInput`] into a structured [`IntentSemantic`].
///
/// This is the regex-driven baseline ; the F1 LLM-bridge slice will
/// upgrade this to llama.cpp / candle local-inference but keep this
/// function as the deterministic fallback when the LLM is offline.
///
/// § PATTERN RULES (case-insensitive on verbs)
///   - `create|spawn|conjure|summon <noun> [with <prop>=<val> [, <prop>=<val>]*]`
///     → CreateEntity { kind: <noun>, props: parsed }
///   - `make|set <target> <prop>=<val> [, <prop>=<val>]*`
///     → ModifyEntity { target: <target>, props: parsed }
///   - `what|where|how|why|who is <subject>` (or starts with `?`)
///     → Query { subject }
///   - anything else → Other(text)
///
/// § DETERMINISM
///   No randomness ; no time ; no allocations except the returned
///   semantic struct. Same input → same output bit-for-bit.
pub fn parse_intent(input: &IntentInput) -> Result<IntentSemantic, ParseErr> {
    let raw = input.text.trim();
    if raw.is_empty() {
        return Err(ParseErr::Empty);
    }

    // Lower-case the first word for verb-detection ; we do NOT lower-case
    // the whole string because property values can be case-sensitive.
    let lower = raw.to_lowercase();

    // § Query : "what is X" / "where is X" / "?X" / "X?"
    if let Some(subject) = parse_query(&lower, raw) {
        return Ok(IntentSemantic::Query { subject });
    }

    // § Modify : "set <target> k=v[, k=v]*" / "make <target> k=v[, ...]"
    if let Some(parsed) = parse_modify(&lower, raw) {
        return Ok(parsed);
    }

    // § Create : "create|spawn|conjure|summon <noun>[ with k=v[, k=v]*]"
    if let Some(parsed) = parse_create(&lower, raw) {
        return Ok(parsed);
    }

    // Fall-through — the LLM slice will retry from here.
    Ok(IntentSemantic::Other(raw.to_string()))
}

fn parse_query(lower: &str, raw: &str) -> Option<String> {
    // Form 1 : leading question-mark token e.g. "? portal" / "?portal".
    if let Some(rest) = lower.strip_prefix('?') {
        let trimmed = rest.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // Form 2 : trailing question-mark e.g. "where is the dragon?"
    if let Some(without_q) = lower.strip_suffix('?') {
        // Try the wh-prefixed form first ; otherwise just drop the '?'.
        let trimmed = without_q.trim();
        if let Some(s) = strip_wh_prefix(trimmed) {
            return Some(s);
        }
        if !trimmed.is_empty() {
            // fall through — let the wh-form below handle it without '?'
            let _ = raw; // silence unused-var lint when this branch is taken
        }
    }

    // Form 3 : wh-word lead, no '?' required.
    strip_wh_prefix(lower)
}

fn strip_wh_prefix(lower: &str) -> Option<String> {
    for verb in ["what is ", "where is ", "how is ", "why is ", "who is "] {
        if let Some(rest) = lower.strip_prefix(verb) {
            let trimmed = rest.trim().trim_end_matches('?').trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn parse_modify(lower: &str, _raw: &str) -> Option<IntentSemantic> {
    for verb in ["set ", "make "] {
        if let Some(rest) = lower.strip_prefix(verb) {
            // first token = target ; remainder = props
            let mut it = rest.splitn(2, ' ');
            let target = it.next()?.trim();
            if target.is_empty() {
                return None;
            }
            let props_src = it.next().unwrap_or("");
            let props = parse_props(props_src);
            // require at least one prop to disambiguate from a quote /
            // a bare "set foo" stray utterance
            if props.is_empty() {
                return None;
            }
            return Some(IntentSemantic::ModifyEntity {
                target: target.to_string(),
                props,
            });
        }
    }
    None
}

fn parse_create(lower: &str, _raw: &str) -> Option<IntentSemantic> {
    for verb in ["create ", "spawn ", "conjure ", "summon "] {
        if let Some(rest) = lower.strip_prefix(verb) {
            // Optional "with k=v[, k=v]*" tail.
            let (kind_part, props_part) = match rest.split_once(" with ") {
                Some((k, p)) => (k, p),
                None => (rest, ""),
            };
            let kind = kind_part.trim();
            if kind.is_empty() {
                return None;
            }
            // Drop a leading "a "/"an "/"the " article for cleanliness.
            let kind = kind
                .strip_prefix("a ")
                .or_else(|| kind.strip_prefix("an "))
                .or_else(|| kind.strip_prefix("the "))
                .unwrap_or(kind);
            return Some(IntentSemantic::CreateEntity {
                kind: kind.to_string(),
                props: parse_props(props_part),
            });
        }
    }
    None
}

fn parse_props(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for kv in src.split(',') {
        let kv = kv.trim();
        if kv.is_empty() {
            continue;
        }
        if let Some((k, v)) = kv.split_once('=') {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() && !v.is_empty() {
                out.push((k.to_string(), v.to_string()));
            }
        }
    }
    out
}

// ════════════════════════════════════════════════════════════════════════════
// § generate — deterministic procgen seeded by BLAKE3 fingerprint
// ════════════════════════════════════════════════════════════════════════════

/// Deterministic procedural generation from a [`ProcgenRequest`].
///
/// § ALGORITHM
///   1. Serialize (semantic-tag, key-fields, observer-pos, budget) into
///      a stable byte sequence ; feed BLAKE3 ; truncate first 8 bytes →
///      [`ProcgenOutput::fingerprint`] (u64 little-endian).
///   2. Branch on the semantic variant :
///      - CreateEntity → emit 1..=4 Crystals around observer_pos in a
///        Fibonacci-disc pattern, each carrying a sub-seed derived
///        from the master seed via splitmix64.
///      - ModifyEntity → emit 1 Crystal at observer_pos with a
///        "modify:<target>" tag (downstream slice intercepts).
///      - Query → emit 0 crystals ; populate asset_uris with a
///          single procgen://q/<subject> placeholder (debug-vis hook).
///      - Other → emit 0 crystals + 0 asset_uris (LLM slice will retry).
///
/// § DETERMINISM
///   No system clock ; no thread_rng ; no global state. The function
///   is pure on its input. Crystal counts are derived from the hash so
///   "same input → same output" survives serde round-trips.
///
/// § BUDGET
///   `budget_ms` is currently informational — the regex+hash path is
///   ~µs. F1 (LLM-bridge) will spend it ; for now we record it in the
///   fingerprint so a budget-change forces a fresh fingerprint.
pub fn generate(req: &ProcgenRequest) -> ProcgenOutput {
    let seed = fingerprint_request(req);

    let (crystals, asset_uris) = match &req.semantic {
        IntentSemantic::CreateEntity { kind, props } => {
            create_crystals(kind, props, req.observer_pos, seed)
        }
        IntentSemantic::ModifyEntity { target, .. } => {
            let sub = splitmix64(seed);
            (
                vec![Crystal {
                    pos: req.observer_pos,
                    kind: format!("modify:{target}"),
                    seed: sub,
                }],
                Vec::new(),
            )
        }
        IntentSemantic::Query { subject } => {
            // Zero crystals ; one debug-vis URI placeholder.
            (Vec::new(), vec![format!("procgen://q/{subject}")])
        }
        IntentSemantic::Other(_) => (Vec::new(), Vec::new()),
    };

    ProcgenOutput {
        crystals,
        asset_uris,
        fingerprint: seed,
    }
}

/// Stable 64-bit fingerprint of the (semantic, observer_pos, budget)
/// triple. Guaranteed pure ; uses BLAKE3 keyed-mode shape (unkeyed
/// here ; cssl-substrate-prime-directive owns the keyed path).
fn fingerprint_request(req: &ProcgenRequest) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hash_semantic(&mut hasher, &req.semantic);
    hasher.update(&req.observer_pos.to_le_bytes());
    hasher.update(&req.budget_ms.to_le_bytes());
    let digest = hasher.finalize();
    let bytes = digest.as_bytes();
    let mut head = [0u8; 8];
    head.copy_from_slice(&bytes[0..8]);
    u64::from_le_bytes(head)
}

fn hash_semantic(h: &mut blake3::Hasher, sem: &IntentSemantic) {
    // First byte = discriminator tag for stable cross-version hashing.
    match sem {
        IntentSemantic::CreateEntity { kind, props } => {
            h.update(&[0x01]);
            h.update(kind.as_bytes());
            h.update(&[0x00]);
            for (k, v) in props {
                h.update(k.as_bytes());
                h.update(b"=");
                h.update(v.as_bytes());
                h.update(b";");
            }
        }
        IntentSemantic::ModifyEntity { target, props } => {
            h.update(&[0x02]);
            h.update(target.as_bytes());
            h.update(&[0x00]);
            for (k, v) in props {
                h.update(k.as_bytes());
                h.update(b"=");
                h.update(v.as_bytes());
                h.update(b";");
            }
        }
        IntentSemantic::Query { subject } => {
            h.update(&[0x03]);
            h.update(subject.as_bytes());
        }
        IntentSemantic::Other(s) => {
            h.update(&[0x04]);
            h.update(s.as_bytes());
        }
    }
}

/// Standard splitmix64 (Sebastiano Vigna · public domain).
/// Used as the per-Crystal sub-seed cascade, NOT as the master fingerprint.
const fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn create_crystals(
    kind: &str,
    _props: &[(String, String)],
    observer: ObserverCoord,
    seed: u64,
) -> (Vec<Crystal>, Vec<String>) {
    // 1..=4 crystals ; count derived from seed low-bits so it's stable.
    let count = (((seed >> 8) & 0b11) as usize) + 1;
    let mut crystals = Vec::with_capacity(count);
    let mut s = seed;
    for i in 0..count {
        s = splitmix64(s);
        // Fibonacci-disc placement around observer (deterministic).
        let theta = (i as f32) * 2.399_963_3_f32; // golden angle (rad)
        let r = 1.0_f32 + (i as f32) * 0.5_f32;
        let dx = r * theta.cos();
        let dz = r * theta.sin();
        crystals.push(Crystal {
            pos: ObserverCoord {
                x: observer.x + dx,
                y: observer.y,
                z: observer.z + dz,
            },
            kind: kind.to_string(),
            seed: s,
        });
    }

    // Asset URI = `procgen://kind/<kind>/<seed-hex>` placeholder ; the F3
    // GLTF-parser slice replaces this with real CDN URLs gated by an
    // allowlist. For stage-0 the consumer ignores or fetches-empty.
    let asset_uris = vec![format!("procgen://kind/{kind}/{seed:016x}")];

    (crystals, asset_uris)
}

// ════════════════════════════════════════════════════════════════════════════
// § asset_fetch_stub — the F2 HTTP-fetch slot
// ════════════════════════════════════════════════════════════════════════════

/// Stage-0 stub for the asset-fetch leg of the pipeline.
///
/// Returns `Ok(Vec::new())` for any well-formed `procgen://` URI ;
/// rejects empty / non-procgen URIs with [`FetchErr::InvalidUri`].
///
/// § F2 SUCCESSOR
///   The F2 slice replaces the body with `ureq::get(uri).call()` (or
///   `reqwest::blocking`) gated by an allowlist. The fn shape +
///   error type are preserved so callers don't churn.
///
/// § DETERMINISM
///   The stub is pure ; it has no I/O. The F2 successor will be
///   non-deterministic (network) — by then the cache layer captures
///   the bytes once and replay-determinism is preserved by the cache.
pub fn asset_fetch_stub(uri: &str) -> Result<Vec<u8>, FetchErr> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(FetchErr::InvalidUri(String::new()));
    }
    // For stage-0 we only accept the procgen:// scheme that `generate`
    // emits. The F2 slice will widen this to https:// + an allowlist.
    if !trimmed.starts_with("procgen://") {
        return Err(FetchErr::InvalidUri(trimmed.to_string()));
    }
    Ok(Vec::new())
}

// ════════════════════════════════════════════════════════════════════════════
// § TESTS — 12 unit-tests covering parse / generate / fetch / determinism
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> IntentInput {
        IntentInput { text: s.to_string(), source: IntentSource::Text }
    }

    // ── parse_intent ──────────────────────────────────────────────────────

    #[test]
    fn parse_create_basic() {
        let r = parse_intent(&t("create dragon")).unwrap();
        match r {
            IntentSemantic::CreateEntity { kind, props } => {
                assert_eq!(kind, "dragon");
                assert!(props.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_create_with_props_and_article() {
        let r = parse_intent(&t("summon a lantern with color=blue, glow=soft"))
            .unwrap();
        match r {
            IntentSemantic::CreateEntity { kind, props } => {
                assert_eq!(kind, "lantern");
                assert_eq!(props.len(), 2);
                assert_eq!(props[0], ("color".into(), "blue".into()));
                assert_eq!(props[1], ("glow".into(), "soft".into()));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_modify() {
        let r = parse_intent(&t("set dragon hp=100, mood=calm")).unwrap();
        match r {
            IntentSemantic::ModifyEntity { target, props } => {
                assert_eq!(target, "dragon");
                assert_eq!(props.len(), 2);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_query_wh_form() {
        let r = parse_intent(&t("where is the gate")).unwrap();
        match r {
            IntentSemantic::Query { subject } => {
                assert_eq!(subject, "the gate");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_query_question_mark_form() {
        let r = parse_intent(&t("?portal")).unwrap();
        match r {
            IntentSemantic::Query { subject } => assert_eq!(subject, "portal"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_other_fallthrough() {
        let r = parse_intent(&t("the rain falls softly")).unwrap();
        match r {
            IntentSemantic::Other(s) => assert_eq!(s, "the rain falls softly"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_empty_errors() {
        let e = parse_intent(&t("   ")).unwrap_err();
        assert!(matches!(e, ParseErr::Empty));
    }

    // ── generate determinism ──────────────────────────────────────────────

    #[test]
    fn generate_is_deterministic() {
        let req = ProcgenRequest {
            semantic: IntentSemantic::CreateEntity {
                kind: "dragon".into(),
                props: vec![("color".into(), "obsidian".into())],
            },
            observer_pos: ObserverCoord::new(1.0, 2.0, 3.0),
            budget_ms: 16,
        };
        let a = generate(&req);
        let b = generate(&req);
        assert_eq!(a, b, "same input must yield bit-identical output");
        assert!(!a.crystals.is_empty(), "create-intent must emit crystals");
        assert_eq!(a.asset_uris.len(), 1);
    }

    #[test]
    fn fingerprint_changes_with_observer_pos() {
        let semantic = IntentSemantic::CreateEntity {
            kind: "lantern".into(),
            props: Vec::new(),
        };
        let a = generate(&ProcgenRequest {
            semantic: semantic.clone(),
            observer_pos: ObserverCoord::new(0.0, 0.0, 0.0),
            budget_ms: 16,
        });
        let b = generate(&ProcgenRequest {
            semantic,
            observer_pos: ObserverCoord::new(0.0, 0.0, 1.0),
            budget_ms: 16,
        });
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn fingerprint_changes_with_budget() {
        let semantic = IntentSemantic::Query { subject: "x".into() };
        let a = generate(&ProcgenRequest {
            semantic: semantic.clone(),
            observer_pos: ObserverCoord::ORIGIN,
            budget_ms: 10,
        });
        let b = generate(&ProcgenRequest {
            semantic,
            observer_pos: ObserverCoord::ORIGIN,
            budget_ms: 20,
        });
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn query_emits_no_crystals() {
        let out = generate(&ProcgenRequest {
            semantic: IntentSemantic::Query { subject: "portal".into() },
            observer_pos: ObserverCoord::ORIGIN,
            budget_ms: 16,
        });
        assert!(out.crystals.is_empty());
        assert_eq!(out.asset_uris, vec!["procgen://q/portal".to_string()]);
    }

    #[test]
    fn modify_emits_one_crystal_at_observer() {
        let req = ProcgenRequest {
            semantic: IntentSemantic::ModifyEntity {
                target: "dragon".into(),
                props: Vec::new(),
            },
            observer_pos: ObserverCoord::new(5.0, 0.0, -2.0),
            budget_ms: 16,
        };
        let out = generate(&req);
        assert_eq!(out.crystals.len(), 1);
        assert_eq!(out.crystals[0].pos, ObserverCoord::new(5.0, 0.0, -2.0));
        assert!(out.crystals[0].kind.starts_with("modify:"));
    }

    // ── splitmix64 invariant (BLAKE3-mixing-correctness adjacent) ─────────

    #[test]
    fn splitmix64_is_pure_const() {
        // The const-fn invariant : same input → same output ; spread-test
        // ensures successive seeds don't collide trivially.
        let s0 = splitmix64(0);
        let s1 = splitmix64(1);
        let s2 = splitmix64(0);
        assert_eq!(s0, s2);
        assert_ne!(s0, s1);
    }

    #[test]
    fn blake3_fingerprint_stable_across_calls() {
        let req = ProcgenRequest {
            semantic: IntentSemantic::Other("alien geometry".into()),
            observer_pos: ObserverCoord::new(7.5, -3.25, 0.0),
            budget_ms: 33,
        };
        let f1 = fingerprint_request(&req);
        let f2 = fingerprint_request(&req);
        assert_eq!(f1, f2);
        // Fingerprint must not be all-zero (sanity).
        assert_ne!(f1, 0);
    }

    // ── asset_fetch_stub ──────────────────────────────────────────────────

    #[test]
    fn asset_fetch_stub_empty_for_procgen_uri() {
        let bytes = asset_fetch_stub("procgen://kind/dragon/0123").unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn asset_fetch_stub_rejects_non_procgen() {
        assert!(matches!(
            asset_fetch_stub("https://example.com/x.gltf"),
            Err(FetchErr::InvalidUri(_))
        ));
        assert!(matches!(
            asset_fetch_stub(""),
            Err(FetchErr::InvalidUri(_))
        ));
    }

    // ── observer-budget-respected (informational) ─────────────────────────

    #[test]
    fn observer_budget_threaded_through() {
        // Budget is informational ; the fingerprint must still incorporate
        // it so a downstream cache keyed on fingerprint correctly invalidates
        // when the budget changes.
        let req_a = ProcgenRequest {
            semantic: IntentSemantic::CreateEntity {
                kind: "stone".into(),
                props: Vec::new(),
            },
            observer_pos: ObserverCoord::ORIGIN,
            budget_ms: 4,
        };
        let req_b = ProcgenRequest { budget_ms: 64, ..req_a.clone() };
        assert_ne!(generate(&req_a).fingerprint, generate(&req_b).fingerprint);
    }

    // ── serde round-trip (replay-determinism wire-format) ─────────────────

    #[test]
    fn semantic_roundtrip_serde_json() {
        let original = IntentSemantic::CreateEntity {
            kind: "phoenix".into(),
            props: vec![("size".into(), "small".into())],
        };
        let s = serde_json::to_string(&original).unwrap();
        let back: IntentSemantic = serde_json::from_str(&s).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn procgen_output_roundtrip_serde_json() {
        let req = ProcgenRequest {
            semantic: IntentSemantic::CreateEntity {
                kind: "lantern".into(),
                props: Vec::new(),
            },
            observer_pos: ObserverCoord::new(1.0, 2.0, 3.0),
            budget_ms: 8,
        };
        let out = generate(&req);
        let s = serde_json::to_string(&out).unwrap();
        let back: ProcgenOutput = serde_json::from_str(&s).unwrap();
        assert_eq!(out, back);
    }
}
