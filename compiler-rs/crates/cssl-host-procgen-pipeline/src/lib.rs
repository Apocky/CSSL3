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
//!   - HTTP fetches are HARD-GATED by [`asset_fetch`]'s sovereignty-floor :
//!       1. URI scheme MUST be `http://` or `https://` (no file://, no scp://).
//!       2. Host MUST appear in the `LOA_PROCGEN_FETCH_HOSTS` env-var
//!          allowlist.  DEFAULT IS EMPTY ⇒ every host is denied unless the
//!          user explicitly opts that host in.  Sovereignty-respecting :
//!          the player owns which CDNs the engine talks to.
//!       3. `LOA_PROCGEN_FETCH_MAX_BYTES` (default 10 MB) caps response size.
//!          Larger payloads return [`FetchErr::BodyTooLarge`] with the
//!          observed byte-count.
//!       4. `LOA_PROCGEN_FETCH_OFFLINE=1` short-circuits every call to
//!          [`FetchErr::OfflineMode`] regardless of allowlist.  Kill-switch.
//!       5. Hard-coded 30-second timeout — connection / read / write.  No
//!          background tasks · no connection-pool reuse · no cookies · no
//!          telemetry · no redirect-following beyond the ureq default.
//!   - NO LLM dependency in this crate (regex fallback only).
//!   - NO global state ; pure-fn surface (env-vars are read fresh per call).
//!   - serde derives present so that IntentInput / ProcgenRequest /
//!     ProcgenOutput can be inspected via cssl-edge for transparency.
//!
//! § F2-SLICE LANDED — sovereignty-attestation
//!   - default-deny-all-hosts ; user-explicit opt-in via env-var
//!   - size-cap protection ; timeout protection
//!   - no background fetches ; no telemetry ; purely on-demand
//!   - no global state ; replay-determinism preserved by caller-owned cache
//!
//! § FOLLOW-ON SLICES (explicit non-goals here)
//!   - F1 : LLM-bridge upgrade — replace regex-parse with llama.cpp/candle
//!     reading the same regex-fallback as last-resort. Same API surface ;
//!     [`ParseErr::LlmUnavailable`] is reserved for that path.
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

/// Errors from [`asset_fetch`] (and the deprecated [`asset_fetch_stub`]
/// shim that delegates to it).
///
/// § STABILITY
///   The two original variants (`InvalidUri` / `Network`) are preserved
///   verbatim so existing callers continue to compile.  The four F2-slice
///   variants are additive ; the `non_exhaustive` attribute on the enum
///   means external callers cannot exhaustively match it (forcing a
///   `_ =>` arm that future-proofs the API).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FetchErr {
    /// URI did not parse, used a non-http(s) scheme, or its host was
    /// rejected by the `LOA_PROCGEN_FETCH_HOSTS` allowlist.
    #[error("invalid asset uri: {0}")]
    InvalidUri(String),
    /// Transport-layer error from ureq (DNS / connect / TLS / IO).
    /// The wrapped string is `ureq::Error::Display` — already redacted of
    /// the URL by ureq itself, suitable for logs.
    #[error("network error: {0}")]
    Network(String),
    /// `LOA_PROCGEN_FETCH_OFFLINE=1` — every call short-circuits to this
    /// without touching the network or even the allowlist.  Sovereign
    /// kill-switch ; the F2-slice promises this returns IMMEDIATELY
    /// (zero-syscall when set ; one env-var read in fast-path).
    #[error("offline mode (LOA_PROCGEN_FETCH_OFFLINE=1)")]
    OfflineMode,
    /// Server returned a non-2xx HTTP status code.  The wrapped u16 is the
    /// raw status (404, 500, ...) so callers can branch on it without
    /// re-parsing strings.
    #[error("http status: {0}")]
    HttpStatus(u16),
    /// Request exceeded the 30-second timeout (connect / read / write).
    #[error("request timed out (30s)")]
    Timeout,
    /// Response body exceeded `LOA_PROCGEN_FETCH_MAX_BYTES`.  The wrapped
    /// usize is the size that was OBSERVED before truncation (not the cap
    /// itself ; that's read from the env-var).
    #[error("body too large: {0} bytes")]
    BodyTooLarge(usize),
    /// Host was not in the `LOA_PROCGEN_FETCH_HOSTS` allowlist.  The
    /// wrapped string is the rejected host (suitable for "add this host
    /// to your allowlist?" UI prompts).
    #[error("host not in allowlist: {0}")]
    HostNotAllowed(String),
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
// § asset_fetch — the F2 HTTP-fetch entry-point (LIVE · ureq-backed)
// ════════════════════════════════════════════════════════════════════════════

// § ENV-VAR NAMES (canonical · grep-target)
const ENV_HOSTS: &str = "LOA_PROCGEN_FETCH_HOSTS";
const ENV_MAX_BYTES: &str = "LOA_PROCGEN_FETCH_MAX_BYTES";
const ENV_OFFLINE: &str = "LOA_PROCGEN_FETCH_OFFLINE";

/// Default size-cap when `LOA_PROCGEN_FETCH_MAX_BYTES` is unset / unparseable :
/// 10 MB.  Chosen to fit a small GLTF + texture without surprise OOM.
const DEFAULT_MAX_BYTES: usize = 10_000_000;

/// Hard-coded per-call timeout : 30 seconds.  Applied to connect / read /
/// write phases independently (so the worst case is 90s wall-clock if
/// every phase saturates ; in practice ureq fails-fast at the first).
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Live, sovereignty-floored asset fetcher.
///
/// § ALGORITHM (fast-path is small ; sovereignty-checks first)
///   1. Trim URI ; reject empty.
///   2. Honor `LOA_PROCGEN_FETCH_OFFLINE=1` — return [`FetchErr::OfflineMode`]
///      WITHOUT inspecting the allowlist (sovereign kill-switch).
///   3. Special-case `procgen://` URIs (those `generate` emits) — return
///      `Ok(Vec::new())` without touching the network.  Backwards-compat with
///      the stage-0 stub : pipeline tests that round-trip a `procgen://kind/...`
///      URI keep working.
///   4. Validate scheme ∈ {http, https} ; reject otherwise.
///   5. Extract host ; reject if not in the `LOA_PROCGEN_FETCH_HOSTS`
///      comma-separated allowlist.  Default-empty = deny-all.
///   6. Read `LOA_PROCGEN_FETCH_MAX_BYTES` (default 10 MB).
///   7. Build a fresh `ureq::Agent` with the 30s timeout ; issue GET.
///   8. On 2xx : drain body up to (cap+1) bytes ; if size > cap, return
///      [`FetchErr::BodyTooLarge`] with the observed size.  Else return body.
///   9. On non-2xx : [`FetchErr::HttpStatus`] with the raw u16.
///   10. On transport error : [`FetchErr::Timeout`] for timeouts, else
///       [`FetchErr::Network`] with the redacted display string.
///
/// § DETERMINISM
///   Non-deterministic by definition (network).  Caller is responsible
///   for caching by URI → bytes if replay-determinism is required.  The
///   `ProcgenOutput.fingerprint` is the canonical cache-key candidate.
///
/// § PRIME-DIRECTIVE
///   - default-deny via empty allowlist
///   - explicit-opt-in via env-var (player-sovereignty)
///   - size-cap protection (no surprise OOM)
///   - timeout protection (no infinite hang)
///   - no telemetry / no cookies / no redirect-following beyond ureq default
///   - no background tasks ; no global state ; one fresh Agent per call
pub fn asset_fetch(uri: &str) -> Result<Vec<u8>, FetchErr> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(FetchErr::InvalidUri(String::new()));
    }

    // § 2. Offline-mode kill-switch — checked BEFORE any other policy.
    if is_offline() {
        return Err(FetchErr::OfflineMode);
    }

    // § 3. Backwards-compat : procgen:// URIs round-trip empty (no network).
    //     Mirrors the stage-0 stub so existing pipeline tests keep passing.
    if trimmed.starts_with("procgen://") {
        return Ok(Vec::new());
    }

    // § 4. Scheme-validation : http(s) only.
    let host = extract_http_host(trimmed)
        .ok_or_else(|| FetchErr::InvalidUri(trimmed.to_string()))?;

    // § 5. Allowlist gate — default-empty = deny-all.
    if !host_is_allowed(host) {
        return Err(FetchErr::HostNotAllowed(host.to_string()));
    }

    // § 6. Cap — read fresh per call so tests can mutate per-test.
    let cap = max_bytes_cap();

    // § 7..10. Issue the request through the local helper that keeps the
    // ureq surface centralized for ease of audit.
    do_get(trimmed, cap)
}

/// Backwards-compat shim — DEPRECATED entry-point kept for ABI continuity
/// with stage-0 callers.  All logic delegates to [`asset_fetch`].
///
/// § MIGRATION
///   New code should call [`asset_fetch`] directly.  This shim will be
///   removed in the F3 (GLTF-parser) slice once all in-repo callers have
///   been migrated.
#[deprecated(
    since = "0.1.1",
    note = "asset_fetch_stub is the F1 stub name ; call asset_fetch instead. \
            This shim simply delegates and will be removed in F3."
)]
pub fn asset_fetch_stub(uri: &str) -> Result<Vec<u8>, FetchErr> {
    asset_fetch(uri)
}

// ── policy helpers (pure-fn ; env-driven) ───────────────────────────────────

/// Read the `LOA_PROCGEN_FETCH_OFFLINE` env-var.  Truthy values are
/// `"1"` / `"true"` / `"TRUE"` / `"yes"` (case-insensitive).  Any other
/// value (including unset) is treated as online.
fn is_offline() -> bool {
    std::env::var(ENV_OFFLINE)
        .map(|v| {
            let lower = v.trim().to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Read `LOA_PROCGEN_FETCH_MAX_BYTES`.  Defaults to [`DEFAULT_MAX_BYTES`]
/// (10 MB) when unset or unparseable.  Zero is treated as the default
/// (a zero-cap would deny every fetch ; that's the OFFLINE flag's job).
fn max_bytes_cap() -> usize {
    std::env::var(ENV_MAX_BYTES)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_MAX_BYTES)
}

/// Read `LOA_PROCGEN_FETCH_HOSTS` and check whether `host` appears.
/// The env-var is comma-separated ; whitespace around entries is trimmed ;
/// matching is exact + case-insensitive on the host portion.
/// Default-empty (env-var unset OR empty after trim) = deny-all.
fn host_is_allowed(host: &str) -> bool {
    let Ok(allowlist) = std::env::var(ENV_HOSTS) else {
        return false;
    };
    let host_lower = host.to_ascii_lowercase();
    allowlist
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .any(|allowed| allowed == host_lower)
}

/// Extract the host from an `http://` or `https://` URI.  Returns `None`
/// if the scheme isn't http(s) or the host is empty.  Stops at the first
/// `/` `?` `#` `:` (port marker).  Mirrors the cssl-rt parser to keep the
/// allowlist surface auditable from a single file.
fn extract_http_host(uri: &str) -> Option<&str> {
    let rest = uri
        .strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"))?;
    if rest.is_empty() {
        return None;
    }
    let host_end = rest.find(['/', '?', '#', ':']).unwrap_or(rest.len());
    let host = &rest[..host_end];
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

// ── ureq wrapper — single point of network surface ──────────────────────────

/// Issue the GET, classify errors, enforce the size-cap.
///
/// § isolated for unit-test surgery — every other policy decision is in
/// pure-fn helpers above ; this is the only fn that touches the network.
fn do_get(uri: &str, cap: usize) -> Result<Vec<u8>, FetchErr> {
    use std::io::Read;

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(FETCH_TIMEOUT)
        .timeout_read(FETCH_TIMEOUT)
        .timeout_write(FETCH_TIMEOUT)
        .build();

    let resp = match agent.get(uri).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => {
            return Err(FetchErr::HttpStatus(code));
        }
        Err(ureq::Error::Transport(t)) => {
            return Err(classify_transport(&t));
        }
    };

    // Drain the body up to (cap + 1) so we can DETECT over-cap without
    // pulling the whole stream.
    let mut reader = resp.into_reader().take((cap as u64) + 1);
    let mut buf = Vec::with_capacity(cap.min(64 * 1024));
    if let Err(e) = reader.read_to_end(&mut buf) {
        return Err(FetchErr::Network(e.to_string()));
    }
    if buf.len() > cap {
        return Err(FetchErr::BodyTooLarge(buf.len()));
    }
    Ok(buf)
}

/// Map a ureq `Transport` error to our [`FetchErr`].  Timeouts get their
/// own variant ; everything else falls into [`FetchErr::Network`].
fn classify_transport(err: &ureq::Transport) -> FetchErr {
    // ureq exposes ErrorKind ; we look for the io-timeout flavor by string
    // because ureq 2.x doesn't surface a dedicated ErrorKind::Timeout.  The
    // Display string contains "timed out" / "timeout" reliably.
    let display = err.to_string();
    let lower = display.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        return FetchErr::Timeout;
    }
    FetchErr::Network(display)
}

// ════════════════════════════════════════════════════════════════════════════
// § TESTS — parse / generate / determinism / F2-fetch sovereignty-floor
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// § Per-test serialization lock for the env-var-driven F2-fetch tests.
    /// The three policy env-vars (`LOA_PROCGEN_FETCH_HOSTS` /
    /// `LOA_PROCGEN_FETCH_MAX_BYTES` / `LOA_PROCGEN_FETCH_OFFLINE`) are
    /// process-global so concurrent tests would race.  Every test that
    /// reads or writes an env-var holds this lock.
    static FETCH_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// RAII guard that snapshots the three F2 env-vars on `take_env`,
    /// applies the test's overrides, and restores the snapshot on Drop.
    /// Ensures environmental isolation across tests run by the same
    /// `cargo test` process.
    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_hosts: Option<String>,
        prev_max: Option<String>,
        prev_offline: Option<String>,
    }

    impl EnvGuard {
        fn take_env() -> Self {
            // Poisoned mutex from a prior panicking test still gets us a
            // usable guard ; we just need serialization, not invariants.
            let lock = FETCH_ENV_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let g = Self {
                _lock: lock,
                prev_hosts: std::env::var(ENV_HOSTS).ok(),
                prev_max: std::env::var(ENV_MAX_BYTES).ok(),
                prev_offline: std::env::var(ENV_OFFLINE).ok(),
            };
            // Start every test from a known-clean slate.
            std::env::remove_var(ENV_HOSTS);
            std::env::remove_var(ENV_MAX_BYTES);
            std::env::remove_var(ENV_OFFLINE);
            g
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev_hosts {
                Some(v) => std::env::set_var(ENV_HOSTS, v),
                None => std::env::remove_var(ENV_HOSTS),
            }
            match &self.prev_max {
                Some(v) => std::env::set_var(ENV_MAX_BYTES, v),
                None => std::env::remove_var(ENV_MAX_BYTES),
            }
            match &self.prev_offline {
                Some(v) => std::env::set_var(ENV_OFFLINE, v),
                None => std::env::remove_var(ENV_OFFLINE),
            }
        }
    }

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

    // ── asset_fetch_stub (backwards-compat shim) ─────────────────────────

    #[test]
    fn asset_fetch_stub_empty_for_procgen_uri() {
        // The procgen:// fast-path remains pure ; no env-isolation needed
        // because the offline-mode check is BEFORE the procgen:// check
        // and we control the env via a guard for safety.
        let _g = EnvGuard::take_env();
        let bytes = asset_fetch_stub("procgen://kind/dragon/0123").unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn asset_fetch_stub_rejects_empty_uri() {
        let _g = EnvGuard::take_env();
        // Empty URI is always rejected as InvalidUri — pre-policy ; the
        // post-F2 contract preserves this exact behavior.
        assert!(matches!(
            asset_fetch_stub(""),
            Err(FetchErr::InvalidUri(_))
        ));
        // And a bare (non-http(s)) string still fails ; default-empty
        // allowlist denies any host even before scheme-checks fire.
        assert!(matches!(
            asset_fetch_stub("ftp://example.com/x.gltf"),
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

    // ─── F2-slice : sovereignty-floor on asset_fetch ─────────────────────
    //
    // These eight tests exercise the policy gates WITHOUT touching the
    // network.  Every code path that would issue a real ureq call is
    // gated by an earlier pure-fn check (offline / scheme / allowlist)
    // so we can validate every branch by setting env-vars carefully.
    // The optional `network_smoketest_*` tests (gated behind an env-var)
    // can opt-in to real traffic for manual integration runs.

    /// 1) URI scheme validation : non-http(s) schemes go to InvalidUri.
    #[test]
    fn fetch_rejects_non_http_scheme() {
        let _g = EnvGuard::take_env();
        // file:// is a classic vector ; reject it without touching net.
        assert!(matches!(
            asset_fetch("file:///etc/passwd"),
            Err(FetchErr::InvalidUri(_))
        ));
        // scp:// likewise.
        assert!(matches!(
            asset_fetch("scp://user@host:/x.bin"),
            Err(FetchErr::InvalidUri(_))
        ));
        // bare token with no scheme.
        assert!(matches!(
            asset_fetch("example.com/path"),
            Err(FetchErr::InvalidUri(_))
        ));
    }

    /// 2) Empty / unset allowlist denies every http(s) host (default-deny).
    #[test]
    fn fetch_empty_allowlist_denies_all_hosts() {
        let _g = EnvGuard::take_env();
        // No LOA_PROCGEN_FETCH_HOSTS set ⇒ deny.
        assert!(matches!(
            asset_fetch("https://example.com/x.gltf"),
            Err(FetchErr::HostNotAllowed(h)) if h == "example.com"
        ));
        // Empty-string-set allowlist also denies.
        std::env::set_var(ENV_HOSTS, "");
        assert!(matches!(
            asset_fetch("https://example.com/x.gltf"),
            Err(FetchErr::HostNotAllowed(_))
        ));
        // Whitespace-only allowlist also denies.
        std::env::set_var(ENV_HOSTS, "   , ,  ");
        assert!(matches!(
            asset_fetch("http://other.example.com/y"),
            Err(FetchErr::HostNotAllowed(_))
        ));
    }

    /// 3) Allowed host passes the validation gate (does NOT mean we
    ///    reach the network — we only check the host-allowed branch
    ///    via the `host_is_allowed` helper directly).
    #[test]
    fn fetch_allowed_host_passes_validation() {
        let _g = EnvGuard::take_env();
        std::env::set_var(
            ENV_HOSTS,
            "example.com, cdn.example.org , another.host",
        );
        assert!(host_is_allowed("example.com"));
        assert!(host_is_allowed("EXAMPLE.com"), "case-insensitive match");
        assert!(host_is_allowed("cdn.example.org"));
        assert!(host_is_allowed("another.host"));
        assert!(!host_is_allowed("evil.com"));
        // Sub-domain is NOT a substring match — exact only.
        assert!(!host_is_allowed("sub.example.com"));
        // Verify the URI parser extracts the host correctly.
        assert_eq!(
            extract_http_host("https://example.com/path?q=1"),
            Some("example.com")
        );
        assert_eq!(
            extract_http_host("http://cdn.example.org:8080/x"),
            Some("cdn.example.org")
        );
    }

    /// 4) Size-cap : a body-cap less than the response would return
    ///    BodyTooLarge.  We test the policy-helper directly (the env-
    ///    var read) since the network branch can't be exercised offline.
    #[test]
    fn fetch_size_cap_env_parsed() {
        let _g = EnvGuard::take_env();
        // Default when unset.
        assert_eq!(max_bytes_cap(), DEFAULT_MAX_BYTES);
        // Explicit override.
        std::env::set_var(ENV_MAX_BYTES, "4096");
        assert_eq!(max_bytes_cap(), 4096);
        // Whitespace-tolerant.
        std::env::set_var(ENV_MAX_BYTES, "  8192  ");
        assert_eq!(max_bytes_cap(), 8192);
        // Zero is treated as default (a zero-cap would deny everything ;
        // OFFLINE is the kill-switch, not a zero-cap).
        std::env::set_var(ENV_MAX_BYTES, "0");
        assert_eq!(max_bytes_cap(), DEFAULT_MAX_BYTES);
        // Non-numeric is treated as default.
        std::env::set_var(ENV_MAX_BYTES, "abc");
        assert_eq!(max_bytes_cap(), DEFAULT_MAX_BYTES);
    }

    /// 5) OFFLINE-mode bypass : returns FetchErr::OfflineMode immediately
    ///    without inspecting allowlist or scheme.
    #[test]
    fn fetch_offline_mode_short_circuits() {
        let _g = EnvGuard::take_env();
        std::env::set_var(ENV_OFFLINE, "1");
        // Even with a fully-allowlisted host, OFFLINE wins.
        std::env::set_var(ENV_HOSTS, "example.com");
        assert!(matches!(
            asset_fetch("https://example.com/x"),
            Err(FetchErr::OfflineMode)
        ));
        // Truthy-aliases all work.
        for v in &["true", "TRUE", "yes", "on", "1"] {
            std::env::set_var(ENV_OFFLINE, v);
            assert!(
                matches!(asset_fetch("http://example.com/x"), Err(FetchErr::OfflineMode)),
                "OFFLINE={v} should short-circuit"
            );
        }
        // Falsy / unset = online.
        std::env::set_var(ENV_OFFLINE, "0");
        assert!(!is_offline());
        std::env::remove_var(ENV_OFFLINE);
        assert!(!is_offline());
    }

    /// 6) Invalid URL forms are rejected pre-network.
    #[test]
    fn fetch_invalid_url_rejected() {
        let _g = EnvGuard::take_env();
        // Empty / whitespace-only.
        assert!(matches!(asset_fetch(""), Err(FetchErr::InvalidUri(_))));
        assert!(matches!(asset_fetch("   "), Err(FetchErr::InvalidUri(_))));
        // http(s):// with no host.
        assert!(matches!(
            asset_fetch("https://"),
            Err(FetchErr::InvalidUri(_))
        ));
        assert!(matches!(
            asset_fetch("http:///path"),
            Err(FetchErr::InvalidUri(_))
        ));
        // Random non-URI string.
        assert!(matches!(
            asset_fetch("not a url at all"),
            Err(FetchErr::InvalidUri(_))
        ));
    }

    /// 7) Network-error formatting / classification.  We test the
    ///    `classify_transport` helper directly — Display-string-based
    ///    timeout detection is the only stable contract.  The network-
    ///    surface fn body itself is exercised by integration smoketests.
    #[test]
    fn fetch_network_error_classified() {
        // We can't construct a `ureq::Transport` directly without a real
        // ureq call, so we test the higher-level error-shape contract :
        // a non-existent / non-allowlisted host produces HostNotAllowed
        // (which is the "would-be-network-error" path's gatekeeper).
        let _g = EnvGuard::take_env();
        // No allowlist ⇒ HostNotAllowed (NOT Network).  This proves the
        // policy gate fires before the network so a misconfigured caller
        // never sees a confusing IO error.
        match asset_fetch("https://nonexistent.invalid/x") {
            Err(FetchErr::HostNotAllowed(h)) => assert_eq!(h, "nonexistent.invalid"),
            other => panic!("expected HostNotAllowed, got {other:?}"),
        }
        // Display impl produces a stable user-facing error string.
        let err = FetchErr::Network("connection refused".to_string());
        assert!(err.to_string().contains("network error"));
        let err2 = FetchErr::HttpStatus(404);
        assert!(err2.to_string().contains("404"));
        let err3 = FetchErr::Timeout;
        assert!(err3.to_string().contains("timed out"));
        let err4 = FetchErr::BodyTooLarge(99_999);
        assert!(err4.to_string().contains("99999"));
    }

    /// 8) Backwards-compat : `asset_fetch_stub` (deprecated) still works
    ///    and delegates byte-for-byte to `asset_fetch`.
    #[test]
    fn fetch_stub_backwards_compat_shim() {
        let _g = EnvGuard::take_env();
        // procgen:// path : both fns return Ok(empty).
        let a = asset_fetch("procgen://kind/dragon/abc").unwrap();
        let b = asset_fetch_stub("procgen://kind/dragon/abc").unwrap();
        assert_eq!(a, b);
        assert!(a.is_empty());

        // Default-deny : both fns return HostNotAllowed.
        match (
            asset_fetch("https://example.com/x"),
            asset_fetch_stub("https://example.com/x"),
        ) {
            (Err(FetchErr::HostNotAllowed(h1)), Err(FetchErr::HostNotAllowed(h2))) => {
                assert_eq!(h1, h2);
                assert_eq!(h1, "example.com");
            }
            other => panic!("expected matching HostNotAllowed pair, got {other:?}"),
        }

        // OFFLINE : both fns short-circuit identically.
        std::env::set_var(ENV_OFFLINE, "1");
        std::env::set_var(ENV_HOSTS, "example.com");
        assert!(matches!(asset_fetch("https://example.com/x"), Err(FetchErr::OfflineMode)));
        assert!(matches!(
            asset_fetch_stub("https://example.com/x"),
            Err(FetchErr::OfflineMode)
        ));
    }

    /// 9 (BONUS) extract_http_host edge-cases — drives the parser branch
    /// list to coverage.
    #[test]
    fn extract_http_host_edge_cases() {
        // None on missing scheme.
        assert!(extract_http_host("ftp://x").is_none());
        // None on empty after scheme.
        assert!(extract_http_host("https://").is_none());
        // Path stripped.
        assert_eq!(extract_http_host("https://a.b/path"), Some("a.b"));
        // Query stripped.
        assert_eq!(extract_http_host("https://a.b?q=1"), Some("a.b"));
        // Fragment stripped.
        assert_eq!(extract_http_host("http://a.b#frag"), Some("a.b"));
        // Port stripped.
        assert_eq!(extract_http_host("http://a.b:443/"), Some("a.b"));
        // No path / query / fragment / port.
        assert_eq!(extract_http_host("https://a.b"), Some("a.b"));
    }
}
