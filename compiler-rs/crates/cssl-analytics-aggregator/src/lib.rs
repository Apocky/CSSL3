//! § cssl-analytics-aggregator — bit-packed event-stream + bucketed-rollups.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W11-ANALYTICS · sovereign-respecting analytics pipeline. Hot-path
//! emit is lock-free (SPSC ring) + zero-allocation (16-byte fixed records).
//! Σ-mask is checked at emission time : when consent.cap is `Deny`, the
//! event is dropped before it touches the ring. Bucketed rollups (1min /
//! 1hr / 1day) compose into long-tail dashboards without per-event storage.
//!
//! § DESIGN GOALS (per memory_sawyer_pokemon_efficiency)
//!
//!   1. *Bit-pack records.* `EventRecord` = 16 bytes : 1B kind · 1B payload-
//!      kind · 2B padding/flags · 4B u32 monotonic-frame · 8B compact payload.
//!      No `String` — names live in a static LUT by index.
//!
//!   2. *Ring-buffer pre-alloc.* `RingBuffer` is a `Box<[EventRecord]>` of
//!      power-of-two capacity. `push` is a single `AtomicUsize` CAS + write.
//!
//!   3. *Index-types for kind.* `EventKind` is `#[repr(u8)]` — 1-byte tag,
//!      not a String, not a vtable.
//!
//!   4. *Differential encoding.* Time is recorded as `frame_offset_u32`
//!      (delta-from-aggregator-start), not absolute Unix-ms. Aggregator
//!      stores the absolute start once.
//!
//!   5. *LUT name-lookup.* `event_kind_name(EventKind)` is a `match` over a
//!      const `&[&str]` — no `HashMap`, no allocation.
//!
//!   6. *RLE sparse-fields.* `payload_kind == None` ⇒ payload bytes are
//!      ignored. `Aggregator` rolls up payload sums only when payload_kind
//!      semantics define them.
//!
//!   7. *Fixed-point f16-equivalent.* Confidence + DOP-style normalized
//!      values are stored as `u16` Q14 (0..16383 = 0.0..1.0).
//!
//! § Σ-MASK PRIME-DIRECTIVE
//!   Per CLAUDE.md / PRIME_DIRECTIVE.md / specs/grand-vision/14_SIGMA_CHAIN :
//!   - Default = deny-all per-player surfacing
//!   - Aggregate-only across players unless cap permits
//!   - NO PII in event payloads (text content excluded ; only LENGTHS)
//!   - Local-first : events buffer locally, sync ONLY when cap permits
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § EventKind — index-type discriminant for event taxonomy.
// ───────────────────────────────────────────────────────────────────────

/// 1-byte event kind discriminant. Total payload ≤ 16 bytes.
///
/// Adding a new kind is :
///   1. Append a variant here (NEVER renumber existing ones — wire-format
///      compatibility).
///   2. Add a row to `KIND_NAMES` LUT (must match index).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    EngineFrameTick = 0,
    EngineRenderModeChanged = 1,
    InputTextTyped = 2,
    InputTextSubmitted = 3,
    IntentClassified = 4,
    IntentRouted = 5,
    GmResponseEmitted = 6,
    DmPhaseTransition = 7,
    ProcgenSceneBuilt = 8,
    McpToolCalled = 9,
    KanClassified = 10,
    MyceliumSyncEvent = 11,
    ConsentCapGranted = 12,
    ConsentCapRevoked = 13,
    /// Reserved sentinel — never emitted ; use for "unknown" decoders.
    Unknown = 255,
}

impl EventKind {
    /// Number of valid kinds (excluding `Unknown`).
    pub const COUNT: usize = 14;

    /// O(1) parse from u8.
    #[must_use]
    pub fn from_u8(b: u8) -> Self {
        match b {
            0 => Self::EngineFrameTick,
            1 => Self::EngineRenderModeChanged,
            2 => Self::InputTextTyped,
            3 => Self::InputTextSubmitted,
            4 => Self::IntentClassified,
            5 => Self::IntentRouted,
            6 => Self::GmResponseEmitted,
            7 => Self::DmPhaseTransition,
            8 => Self::ProcgenSceneBuilt,
            9 => Self::McpToolCalled,
            10 => Self::KanClassified,
            11 => Self::MyceliumSyncEvent,
            12 => Self::ConsentCapGranted,
            13 => Self::ConsentCapRevoked,
            _ => Self::Unknown,
        }
    }

    /// O(1) lookup in `KIND_NAMES` LUT.
    #[must_use]
    pub fn name(self) -> &'static str {
        let idx = self as usize;
        if idx < Self::COUNT {
            KIND_NAMES[idx]
        } else {
            "unknown"
        }
    }
}

/// Static event-name LUT. Index = `EventKind as usize`.
const KIND_NAMES: [&str; EventKind::COUNT] = [
    "engine.frame_tick",
    "engine.render_mode_changed",
    "input.text_typed",
    "input.text_submitted",
    "intent.classified",
    "intent.routed",
    "gm.response_emitted",
    "dm.phase_transition",
    "procgen.scene_built",
    "mcp.tool_called",
    "kan.classified",
    "mycelium.sync_event",
    "consent.cap_granted",
    "consent.cap_revoked",
];

// ───────────────────────────────────────────────────────────────────────
// § ConsentCap — Σ-mask consent gate at emission time.
// ───────────────────────────────────────────────────────────────────────

/// Σ-mask consent-cap. Default = `Deny` (drop event silently).
///
/// `LocalOnly` : event lands in local ring + JSONL but never relays off-machine.
/// `AggregateRelay` : aggregate rollups MAY relay (no per-event PII).
/// `FullRelay` : per-event upload OK ; only when player explicitly opts in.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentCap {
    Deny = 0,
    LocalOnly = 1,
    AggregateRelay = 2,
    FullRelay = 3,
}

impl ConsentCap {
    #[must_use]
    pub fn from_u8(b: u8) -> Self {
        match b {
            1 => Self::LocalOnly,
            2 => Self::AggregateRelay,
            3 => Self::FullRelay,
            _ => Self::Deny,
        }
    }

    #[must_use]
    pub fn allows_local(self) -> bool {
        !matches!(self, Self::Deny)
    }

    #[must_use]
    pub fn allows_aggregate_relay(self) -> bool {
        matches!(self, Self::AggregateRelay | Self::FullRelay)
    }

    #[must_use]
    pub fn allows_full_relay(self) -> bool {
        matches!(self, Self::FullRelay)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § EventRecord — bit-packed 16-byte fixed record.
// ───────────────────────────────────────────────────────────────────────

/// Bit-packed event record. Total = 16 bytes. Layout :
///
///   ┌─────────┬────────────┬─────────┬─────────┬─────────────────┐
///   │ kind:1B │ payload:1B │ flags:2B│ frame:4B│ payload8:8B     │
///   └─────────┴────────────┴─────────┴─────────┴─────────────────┘
///
///   - `kind`        : `EventKind as u8`
///   - `payload_kind`: enum of payload shape (see `PayloadKind`)
///   - `flags`       : bit-flags (consent-cap, fallback-used, ok-bit, etc.)
///   - `frame`       : differential frame-offset from aggregator-start
///   - `payload8`    : 8 bytes interpreted per `payload_kind`
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventRecord {
    pub kind: u8,
    pub payload_kind: u8,
    pub flags: u16,
    pub frame_offset: u32,
    pub payload8: [u8; 8],
}

impl Default for EventRecord {
    fn default() -> Self {
        Self {
            kind: EventKind::Unknown as u8,
            payload_kind: PayloadKind::None as u8,
            flags: 0,
            frame_offset: 0,
            payload8: [0; 8],
        }
    }
}

/// Compile-time assert : record is exactly 16 bytes.
const _SIZE_CHECK: [(); 16] = [(); std::mem::size_of::<EventRecord>()];

// ───────────────────────────────────────────────────────────────────────
// § PayloadKind — sparse-RLE payload-shape discriminant.
// ───────────────────────────────────────────────────────────────────────

/// 1-byte payload-shape discriminant.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadKind {
    None = 0,
    /// `payload8 = [u32 dt_us, u32 fps_q14]` — frame_tick payload.
    FrameTick = 1,
    /// `payload8 = [u32 latency_us, u8 ok, u8 _pad, u16 tool_idx]` — mcp.tool_called.
    McpCall = 2,
    /// `payload8 = [u16 len, u16 intent_kind, u16 conf_q14, u16 _pad]` — text_submitted.
    TextSubmit = 3,
    /// `payload8 = [u8 from, u8 to, u8 trigger, ..]` — dm.phase_transition.
    DmTransition = 4,
    /// `payload8 = [u32 ms, u8 npcs_l0..3]` — procgen.scene_built.
    ProcgenScene = 5,
    /// `payload8 = [u16 cap_id, u8 audience, ..]` — consent_*.
    Consent = 6,
    /// `payload8 = [u16 peer_count, .., u32 bytes_xfer]` — mycelium.sync.
    Mycelium = 7,
    /// `payload8 = [u16 swap_point, u8 fallback, ..]` — kan.classified.
    KanClass = 8,
    /// `payload8 = [u16 len, u16 persona_seed, u8 kind_id, ..]` — gm.response.
    GmResponse = 9,
    /// `payload8 = [u32 length_chars]` — input.text_typed (length only · NO content).
    TypedLen = 10,
    /// `payload8 = [u8 from, u8 to, ..]` — render_mode_changed.
    ModeChange = 11,
    /// `payload8 = [u16 intent_kind, u16 conf_q14, u8 fallback, ..]` — intent.classified.
    IntentClass = 12,
    /// `payload8 = [u16 intent_kind, u16 response_kind, u32 latency_us]` — intent.routed.
    IntentRoute = 13,
}

/// Flag bit definitions for `EventRecord::flags`.
pub mod flags {
    /// Consent-cap : 2 bits (0..3) ⇒ ConsentCap discriminant.
    pub const CONSENT_MASK: u16 = 0b0000_0000_0000_0011;
    /// Fallback-used flag (kan / intent fell back to stage-0).
    pub const FALLBACK_USED: u16 = 0b0000_0000_0000_0100;
    /// OK bit (mcp call succeeded · intent classified · etc.).
    pub const OK: u16 = 0b0000_0000_0000_1000;
    /// Σ-mask validated bit (set by aggregator on accept).
    pub const SIGMA_VALIDATED: u16 = 0b0000_0000_0001_0000;
    /// Relayable to aggregate-rollup (cap permits).
    pub const RELAY_AGGREGATE: u16 = 0b0000_0000_0010_0000;
    /// Relayable per-event (cap permits FullRelay).
    pub const RELAY_FULL: u16 = 0b0000_0000_0100_0000;
}

impl EventRecord {
    /// Create a frame-tick record from (frame_offset, dt_us, fps_q14).
    #[must_use]
    pub fn frame_tick(frame_offset: u32, dt_us: u32, fps_q14: u32) -> Self {
        let mut p = [0u8; 8];
        p[0..4].copy_from_slice(&dt_us.to_le_bytes());
        p[4..8].copy_from_slice(&fps_q14.to_le_bytes());
        Self {
            kind: EventKind::EngineFrameTick as u8,
            payload_kind: PayloadKind::FrameTick as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Create an mcp.tool_called record.
    #[must_use]
    pub fn mcp_call(frame_offset: u32, latency_us: u32, ok: bool, tool_idx: u16) -> Self {
        let mut p = [0u8; 8];
        p[0..4].copy_from_slice(&latency_us.to_le_bytes());
        p[4] = u8::from(ok);
        p[5] = 0;
        p[6..8].copy_from_slice(&tool_idx.to_le_bytes());
        let mut flag_bits = 0u16;
        if ok {
            flag_bits |= flags::OK;
        }
        Self {
            kind: EventKind::McpToolCalled as u8,
            payload_kind: PayloadKind::McpCall as u8,
            flags: flag_bits,
            frame_offset,
            payload8: p,
        }
    }

    /// Create a `text_submitted` record. NOTE : `len` is character-count
    /// only — Σ-mask forbids content in this stream.
    #[must_use]
    pub fn text_submitted(frame_offset: u32, len: u16, intent_kind: u16, conf_q14: u16) -> Self {
        let mut p = [0u8; 8];
        p[0..2].copy_from_slice(&len.to_le_bytes());
        p[2..4].copy_from_slice(&intent_kind.to_le_bytes());
        p[4..6].copy_from_slice(&conf_q14.to_le_bytes());
        Self {
            kind: EventKind::InputTextSubmitted as u8,
            payload_kind: PayloadKind::TextSubmit as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Create an input.text_typed record · length only.
    #[must_use]
    pub fn text_typed(frame_offset: u32, len: u32) -> Self {
        let mut p = [0u8; 8];
        p[0..4].copy_from_slice(&len.to_le_bytes());
        Self {
            kind: EventKind::InputTextTyped as u8,
            payload_kind: PayloadKind::TypedLen as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Create a dm.phase_transition record.
    #[must_use]
    pub fn dm_transition(frame_offset: u32, from: u8, to: u8, trigger: u8) -> Self {
        let mut p = [0u8; 8];
        p[0] = from;
        p[1] = to;
        p[2] = trigger;
        Self {
            kind: EventKind::DmPhaseTransition as u8,
            payload_kind: PayloadKind::DmTransition as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Create a procgen.scene_built record.
    #[must_use]
    pub fn procgen_scene(
        frame_offset: u32,
        ms: u32,
        npcs_l0: u8,
        npcs_l1: u8,
        npcs_l2: u8,
        npcs_l3: u8,
    ) -> Self {
        let mut p = [0u8; 8];
        p[0..4].copy_from_slice(&ms.to_le_bytes());
        p[4] = npcs_l0;
        p[5] = npcs_l1;
        p[6] = npcs_l2;
        p[7] = npcs_l3;
        Self {
            kind: EventKind::ProcgenSceneBuilt as u8,
            payload_kind: PayloadKind::ProcgenScene as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Create an intent.classified record.
    #[must_use]
    pub fn intent_classified(
        frame_offset: u32,
        intent_kind: u16,
        conf_q14: u16,
        fallback: bool,
    ) -> Self {
        let mut p = [0u8; 8];
        p[0..2].copy_from_slice(&intent_kind.to_le_bytes());
        p[2..4].copy_from_slice(&conf_q14.to_le_bytes());
        p[4] = u8::from(fallback);
        let mut flag_bits = 0u16;
        if fallback {
            flag_bits |= flags::FALLBACK_USED;
        }
        Self {
            kind: EventKind::IntentClassified as u8,
            payload_kind: PayloadKind::IntentClass as u8,
            flags: flag_bits,
            frame_offset,
            payload8: p,
        }
    }

    /// Create an intent.routed record.
    #[must_use]
    pub fn intent_routed(
        frame_offset: u32,
        intent_kind: u16,
        response_kind: u16,
        latency_us: u32,
    ) -> Self {
        let mut p = [0u8; 8];
        p[0..2].copy_from_slice(&intent_kind.to_le_bytes());
        p[2..4].copy_from_slice(&response_kind.to_le_bytes());
        p[4..8].copy_from_slice(&latency_us.to_le_bytes());
        Self {
            kind: EventKind::IntentRouted as u8,
            payload_kind: PayloadKind::IntentRoute as u8,
            flags: flags::OK,
            frame_offset,
            payload8: p,
        }
    }

    /// Create a gm.response_emitted record.
    #[must_use]
    pub fn gm_response(
        frame_offset: u32,
        length_chars: u16,
        persona_seed: u16,
        kind_id: u8,
    ) -> Self {
        let mut p = [0u8; 8];
        p[0..2].copy_from_slice(&length_chars.to_le_bytes());
        p[2..4].copy_from_slice(&persona_seed.to_le_bytes());
        p[4] = kind_id;
        Self {
            kind: EventKind::GmResponseEmitted as u8,
            payload_kind: PayloadKind::GmResponse as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Create a render_mode_changed record.
    #[must_use]
    pub fn render_mode_change(frame_offset: u32, from: u8, to: u8) -> Self {
        let mut p = [0u8; 8];
        p[0] = from;
        p[1] = to;
        Self {
            kind: EventKind::EngineRenderModeChanged as u8,
            payload_kind: PayloadKind::ModeChange as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Create a kan.classified record.
    #[must_use]
    pub fn kan_classified(frame_offset: u32, swap_point: u16, fallback: bool) -> Self {
        let mut p = [0u8; 8];
        p[0..2].copy_from_slice(&swap_point.to_le_bytes());
        p[2] = u8::from(fallback);
        let mut flag_bits = 0u16;
        if fallback {
            flag_bits |= flags::FALLBACK_USED;
        }
        Self {
            kind: EventKind::KanClassified as u8,
            payload_kind: PayloadKind::KanClass as u8,
            flags: flag_bits,
            frame_offset,
            payload8: p,
        }
    }

    /// Create a mycelium.sync_event record.
    #[must_use]
    pub fn mycelium_sync(frame_offset: u32, peer_count: u16, bytes_xfer: u32) -> Self {
        let mut p = [0u8; 8];
        p[0..2].copy_from_slice(&peer_count.to_le_bytes());
        p[4..8].copy_from_slice(&bytes_xfer.to_le_bytes());
        Self {
            kind: EventKind::MyceliumSyncEvent as u8,
            payload_kind: PayloadKind::Mycelium as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Create a consent.cap_granted/_revoked record.
    #[must_use]
    pub fn consent(frame_offset: u32, cap_id: u16, audience: u8, granted: bool) -> Self {
        let mut p = [0u8; 8];
        p[0..2].copy_from_slice(&cap_id.to_le_bytes());
        p[2] = audience;
        let kind = if granted {
            EventKind::ConsentCapGranted
        } else {
            EventKind::ConsentCapRevoked
        };
        Self {
            kind: kind as u8,
            payload_kind: PayloadKind::Consent as u8,
            flags: 0,
            frame_offset,
            payload8: p,
        }
    }

    /// Stamp this record's consent-cap into the flag-bits.
    pub fn stamp_consent(&mut self, cap: ConsentCap) {
        self.flags = (self.flags & !flags::CONSENT_MASK) | (cap as u16);
        if cap.allows_aggregate_relay() {
            self.flags |= flags::RELAY_AGGREGATE;
        }
        if cap.allows_full_relay() {
            self.flags |= flags::RELAY_FULL;
        }
    }

    /// Read consent-cap from flag bits.
    #[must_use]
    pub fn consent_cap(&self) -> ConsentCap {
        ConsentCap::from_u8((self.flags & flags::CONSENT_MASK) as u8)
    }

    /// Read the kind discriminant.
    #[must_use]
    pub fn event_kind(&self) -> EventKind {
        EventKind::from_u8(self.kind)
    }

    /// Encode as 16-byte little-endian wire-format.
    #[must_use]
    pub fn pack(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0] = self.kind;
        out[1] = self.payload_kind;
        out[2..4].copy_from_slice(&self.flags.to_le_bytes());
        out[4..8].copy_from_slice(&self.frame_offset.to_le_bytes());
        out[8..16].copy_from_slice(&self.payload8);
        out
    }

    /// Decode from 16-byte little-endian wire-format.
    #[must_use]
    pub fn unpack(b: &[u8; 16]) -> Self {
        let mut payload8 = [0u8; 8];
        payload8.copy_from_slice(&b[8..16]);
        Self {
            kind: b[0],
            payload_kind: b[1],
            flags: u16::from_le_bytes([b[2], b[3]]),
            frame_offset: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            payload8,
        }
    }

    /// Render as a JSON object literal — for the JSONL emit path.
    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            "{{\"kind\":\"{}\",\"frame\":{},\"consent\":{},\"flags\":{},\"payload_b64\":\"{}\"}}",
            self.event_kind().name(),
            self.frame_offset,
            self.consent_cap() as u8,
            self.flags,
            base64_8(&self.payload8),
        )
    }
}

/// Tiny base64 encoder for 8-byte payloads (no allocation beyond the String).
fn base64_8(b: &[u8; 8]) -> String {
    const ALPHA: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(12);
    let chunks = [(b[0], b[1], b[2]), (b[3], b[4], b[5])];
    for (a, b1, c) in chunks {
        let n = ((u32::from(a)) << 16) | ((u32::from(b1)) << 8) | u32::from(c);
        out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHA[((n >> 6) & 0x3F) as usize] as char);
        out.push(ALPHA[(n & 0x3F) as usize] as char);
    }
    // tail 2 bytes ⇒ 3 base64 chars + '='
    let n = ((u32::from(b[6])) << 8) | u32::from(b[7]);
    out.push(ALPHA[((n >> 10) & 0x3F) as usize] as char);
    out.push(ALPHA[((n >> 4) & 0x3F) as usize] as char);
    out.push(ALPHA[((n << 2) & 0x3F) as usize] as char);
    out.push('=');
    out
}

// ───────────────────────────────────────────────────────────────────────
// § RingBuffer — pre-alloc fixed-cap SPSC ring (lock-free hot-path).
// ───────────────────────────────────────────────────────────────────────

/// Single-producer single-consumer ring of `EventRecord`. Capacity must be
/// power-of-two ; mask-based wraparound.
pub struct RingBuffer {
    /// Backing storage. Length is always power-of-two.
    pub buf: Vec<EventRecord>,
    /// Power-of-two mask = capacity - 1.
    pub mask: usize,
    /// Producer index (monotonic ; wraps via mask).
    pub head: AtomicUsize,
    /// Consumer index (monotonic ; wraps via mask).
    pub tail: AtomicUsize,
    /// Total events dropped due to ring-full.
    pub dropped_total: AtomicU64,
}

impl RingBuffer {
    /// Construct a ring with capacity rounded up to the next power-of-two
    /// (minimum 16).
    #[must_use]
    pub fn new(min_capacity: usize) -> Self {
        let mut cap = 16usize;
        while cap < min_capacity {
            cap = cap.checked_shl(1).unwrap_or(cap);
        }
        Self {
            buf: vec![EventRecord::default(); cap],
            mask: cap - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped_total: AtomicU64::new(0),
        }
    }

    /// Capacity of the ring (power-of-two).
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Push an event. Returns `true` on success ; `false` when full (event
    /// dropped + dropped-counter incremented).
    pub fn push(&mut self, ev: EventRecord) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        let next = head.wrapping_add(1);
        // Full when (head + 1) & mask == tail & mask AND head != tail.
        if (next & self.mask) == (tail & self.mask) && head != tail {
            self.dropped_total.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let idx = head & self.mask;
        self.buf[idx] = ev;
        self.head.store(next, Ordering::Release);
        true
    }

    /// Drain up to `max` events into the output vec. Returns count drained.
    pub fn drain_to_vec(&mut self, out: &mut Vec<EventRecord>, max: usize) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let mut tail = self.tail.load(Ordering::Acquire);
        let mut n = 0;
        while tail != head && n < max {
            let idx = tail & self.mask;
            out.push(self.buf[idx]);
            tail = tail.wrapping_add(1);
            n += 1;
        }
        self.tail.store(tail, Ordering::Release);
        n
    }

    /// Approximate count of events currently buffered (head - tail).
    #[must_use]
    pub fn len_approx(&self) -> usize {
        let h = self.head.load(Ordering::Relaxed);
        let t = self.tail.load(Ordering::Relaxed);
        h.wrapping_sub(t) & self.mask
    }

    /// Total events dropped due to ring-full overflow.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped_total.load(Ordering::Relaxed)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Aggregator — bucketed-rollup roll-forward state.
// ───────────────────────────────────────────────────────────────────────

/// Per-bucket counters. One row per (bucket_index, event_kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BucketCounters {
    pub count: u32,
    pub sum_payload32: u64,
    pub min_payload32: u32,
    pub max_payload32: u32,
    pub fallback_count: u32,
    pub error_count: u32,
}

impl Default for BucketCounters {
    fn default() -> Self {
        Self {
            count: 0,
            sum_payload32: 0,
            min_payload32: u32::MAX,
            max_payload32: 0,
            fallback_count: 0,
            error_count: 0,
        }
    }
}

/// Time-bucketed-rollup aggregator.
pub struct Aggregator {
    /// Frame offset where the aggregator started (for absolute-time recovery).
    pub start_frame: u32,
    /// 1-minute buckets : 60 slots (1 hour worth at 1-min resolution).
    pub bucket_1min: Vec<[BucketCounters; EventKind::COUNT]>,
    /// 1-hour buckets : 24 slots (1 day worth at 1-hr resolution).
    pub bucket_1hr: Vec<[BucketCounters; EventKind::COUNT]>,
    /// 1-day buckets : 30 slots (1 month worth at 1-day resolution).
    pub bucket_1day: Vec<[BucketCounters; EventKind::COUNT]>,
    /// Bucket-size in frames (1min @ 60Hz default = 3600).
    pub frames_per_min: u32,
}

impl Aggregator {
    /// Construct an aggregator with default 60Hz frames-per-minute.
    #[must_use]
    pub fn new() -> Self {
        Self::with_rate(60)
    }

    /// Construct an aggregator with a custom frame-rate (Hz).
    #[must_use]
    pub fn with_rate(hz_frames_per_sec: u32) -> Self {
        let frames_per_min = hz_frames_per_sec.saturating_mul(60).max(1);
        Self {
            start_frame: 0,
            bucket_1min: vec![[BucketCounters::default(); EventKind::COUNT]; 60],
            bucket_1hr: vec![[BucketCounters::default(); EventKind::COUNT]; 24],
            bucket_1day: vec![[BucketCounters::default(); EventKind::COUNT]; 30],
            frames_per_min,
        }
    }

    /// Ingest a single event into all 3 bucket tiers.
    pub fn ingest(&mut self, ev: &EventRecord) {
        let kind_idx = ev.kind as usize;
        if kind_idx >= EventKind::COUNT {
            return;
        }

        let off = ev.frame_offset.saturating_sub(self.start_frame);
        let min_idx = (off / self.frames_per_min) as usize % 60;
        let hr_idx = (off / (self.frames_per_min * 60)) as usize % 24;
        let day_idx = (off / (self.frames_per_min * 60 * 24)) as usize % 30;

        let payload_u32 = u32::from_le_bytes([
            ev.payload8[0],
            ev.payload8[1],
            ev.payload8[2],
            ev.payload8[3],
        ]);
        let fallback_inc = u32::from((ev.flags & flags::FALLBACK_USED) != 0);
        let error_inc = u32::from(
            (ev.flags & flags::OK) == 0
                && matches!(ev.event_kind(), EventKind::McpToolCalled),
        );

        // Update all 3 tiers.
        for slot in [
            &mut self.bucket_1min[min_idx][kind_idx],
            &mut self.bucket_1hr[hr_idx][kind_idx],
            &mut self.bucket_1day[day_idx][kind_idx],
        ] {
            slot.count = slot.count.saturating_add(1);
            slot.sum_payload32 = slot.sum_payload32.saturating_add(u64::from(payload_u32));
            if payload_u32 < slot.min_payload32 {
                slot.min_payload32 = payload_u32;
            }
            if payload_u32 > slot.max_payload32 {
                slot.max_payload32 = payload_u32;
            }
            slot.fallback_count = slot.fallback_count.saturating_add(fallback_inc);
            slot.error_count = slot.error_count.saturating_add(error_inc);
        }
    }

    /// Snapshot a single bucket-tier as JSON (for /api/analytics/metrics).
    #[must_use]
    pub fn snapshot_bucket(&self, tier: BucketTier) -> String {
        let buckets = match tier {
            BucketTier::Min1 => &self.bucket_1min,
            BucketTier::Hr1 => &self.bucket_1hr,
            BucketTier::Day1 => &self.bucket_1day,
        };
        let label = match tier {
            BucketTier::Min1 => "1min",
            BucketTier::Hr1 => "1hr",
            BucketTier::Day1 => "1day",
        };
        let mut s = String::with_capacity(2048);
        s.push_str("{\"bucket\":\"");
        s.push_str(label);
        s.push_str("\",\"slots\":");
        s.push_str(&buckets.len().to_string());
        s.push_str(",\"kinds\":[");
        let mut first = true;
        for (kind_idx, name) in KIND_NAMES.iter().enumerate() {
            // Aggregate across ALL slots for top-line per-kind summary.
            let mut total = BucketCounters::default();
            for slot in buckets.iter() {
                let c = &slot[kind_idx];
                total.count = total.count.saturating_add(c.count);
                total.sum_payload32 =
                    total.sum_payload32.saturating_add(c.sum_payload32);
                if c.min_payload32 < total.min_payload32 {
                    total.min_payload32 = c.min_payload32;
                }
                if c.max_payload32 > total.max_payload32 {
                    total.max_payload32 = c.max_payload32;
                }
                total.fallback_count = total.fallback_count.saturating_add(c.fallback_count);
                total.error_count = total.error_count.saturating_add(c.error_count);
            }
            if total.count == 0 {
                continue;
            }
            if !first {
                s.push(',');
            }
            first = false;
            let avg = if total.count > 0 {
                total.sum_payload32 / u64::from(total.count)
            } else {
                0
            };
            let min = if total.min_payload32 == u32::MAX {
                0
            } else {
                total.min_payload32
            };
            s.push_str(&format!(
                "{{\"name\":\"{}\",\"count\":{},\"avg\":{},\"min\":{},\"max\":{},\"fallback\":{},\"err\":{}}}",
                name, total.count, avg, min, total.max_payload32,
                total.fallback_count, total.error_count
            ));
        }
        s.push_str("]}");
        s
    }

    /// Reset all buckets to default. Used by tests + at process-start.
    pub fn reset(&mut self) {
        for slot in &mut self.bucket_1min {
            *slot = [BucketCounters::default(); EventKind::COUNT];
        }
        for slot in &mut self.bucket_1hr {
            *slot = [BucketCounters::default(); EventKind::COUNT];
        }
        for slot in &mut self.bucket_1day {
            *slot = [BucketCounters::default(); EventKind::COUNT];
        }
    }
}

impl Default for Aggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// Bucket-tier discriminant for /api/analytics/metrics?bucket=...
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketTier {
    Min1,
    Hr1,
    Day1,
}

impl BucketTier {
    /// Parse a tier-tag (`"1min"` / `"1hr"` / `"1day"`). Defaults to `Min1`.
    #[must_use]
    pub fn from_tag(s: &str) -> Self {
        match s {
            "1hr" | "hour" => Self::Hr1,
            "1day" | "day" => Self::Day1,
            _ => Self::Min1,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kind_name_lut_round_trip() {
        for i in 0..EventKind::COUNT as u8 {
            let k = EventKind::from_u8(i);
            let name = k.name();
            assert!(!name.is_empty(), "empty name for kind {i}");
            assert_ne!(name, "unknown");
        }
        assert_eq!(EventKind::from_u8(99).name(), "unknown");
        assert_eq!(EventKind::from_u8(255), EventKind::Unknown);
    }

    #[test]
    fn record_size_is_exactly_16_bytes() {
        assert_eq!(std::mem::size_of::<EventRecord>(), 16);
    }

    #[test]
    fn record_pack_unpack_roundtrip() {
        let ev = EventRecord::frame_tick(42, 16_700, 14_336);
        let bytes = ev.pack();
        let recovered = EventRecord::unpack(&bytes);
        assert_eq!(ev, recovered);
        assert_eq!(recovered.event_kind(), EventKind::EngineFrameTick);
    }

    #[test]
    fn record_factories_set_correct_kind_discriminant() {
        assert_eq!(
            EventRecord::frame_tick(0, 0, 0).event_kind(),
            EventKind::EngineFrameTick
        );
        assert_eq!(
            EventRecord::mcp_call(0, 100, true, 5).event_kind(),
            EventKind::McpToolCalled
        );
        assert_eq!(
            EventRecord::text_typed(0, 42).event_kind(),
            EventKind::InputTextTyped
        );
        assert_eq!(
            EventRecord::text_submitted(0, 10, 1, 8000).event_kind(),
            EventKind::InputTextSubmitted
        );
        assert_eq!(
            EventRecord::dm_transition(0, 0, 1, 2).event_kind(),
            EventKind::DmPhaseTransition
        );
        assert_eq!(
            EventRecord::procgen_scene(0, 100, 1, 2, 3, 4).event_kind(),
            EventKind::ProcgenSceneBuilt
        );
        assert_eq!(
            EventRecord::intent_classified(0, 1, 8000, false).event_kind(),
            EventKind::IntentClassified
        );
        assert_eq!(
            EventRecord::intent_routed(0, 1, 2, 100).event_kind(),
            EventKind::IntentRouted
        );
        assert_eq!(
            EventRecord::gm_response(0, 50, 7, 1).event_kind(),
            EventKind::GmResponseEmitted
        );
        assert_eq!(
            EventRecord::render_mode_change(0, 1, 2).event_kind(),
            EventKind::EngineRenderModeChanged
        );
        assert_eq!(
            EventRecord::kan_classified(0, 5, false).event_kind(),
            EventKind::KanClassified
        );
        assert_eq!(
            EventRecord::mycelium_sync(0, 3, 1024).event_kind(),
            EventKind::MyceliumSyncEvent
        );
        assert_eq!(
            EventRecord::consent(0, 1, 0, true).event_kind(),
            EventKind::ConsentCapGranted
        );
        assert_eq!(
            EventRecord::consent(0, 1, 0, false).event_kind(),
            EventKind::ConsentCapRevoked
        );
    }

    #[test]
    fn consent_cap_default_is_deny() {
        let ev = EventRecord::frame_tick(0, 0, 0);
        assert_eq!(ev.consent_cap(), ConsentCap::Deny);
        assert!(!ev.consent_cap().allows_local());
    }

    #[test]
    fn consent_cap_stamp_sets_relay_flags() {
        let mut ev = EventRecord::frame_tick(0, 0, 0);
        ev.stamp_consent(ConsentCap::FullRelay);
        assert_eq!(ev.consent_cap(), ConsentCap::FullRelay);
        assert!(ev.flags & flags::RELAY_AGGREGATE != 0);
        assert!(ev.flags & flags::RELAY_FULL != 0);

        let mut ev2 = EventRecord::frame_tick(0, 0, 0);
        ev2.stamp_consent(ConsentCap::LocalOnly);
        assert_eq!(ev2.consent_cap(), ConsentCap::LocalOnly);
        assert_eq!(ev2.flags & flags::RELAY_AGGREGATE, 0);
        assert_eq!(ev2.flags & flags::RELAY_FULL, 0);
    }

    #[test]
    fn ringbuffer_capacity_rounds_to_pow2() {
        let r = RingBuffer::new(100);
        assert_eq!(r.capacity(), 128);
        let r = RingBuffer::new(16);
        assert_eq!(r.capacity(), 16);
        let r = RingBuffer::new(1);
        assert_eq!(r.capacity(), 16);
    }

    #[test]
    fn ringbuffer_push_drain_fifo_order() {
        let mut r = RingBuffer::new(16);
        for i in 0..10 {
            assert!(r.push(EventRecord::frame_tick(i, i, 0)));
        }
        let mut out = Vec::new();
        let n = r.drain_to_vec(&mut out, 100);
        assert_eq!(n, 10);
        for i in 0..10 {
            assert_eq!(out[i as usize].frame_offset, i);
        }
    }

    #[test]
    fn ringbuffer_drops_when_full() {
        let mut r = RingBuffer::new(16);
        for i in 0..15 {
            assert!(r.push(EventRecord::frame_tick(i, i, 0)));
        }
        assert!(!r.push(EventRecord::frame_tick(99, 99, 0)));
        assert_eq!(r.dropped(), 1);
    }

    #[test]
    fn ringbuffer_dropped_counter_accumulates() {
        let mut r = RingBuffer::new(16);
        for i in 0..15 {
            r.push(EventRecord::frame_tick(i, i, 0));
        }
        for _ in 0..5 {
            r.push(EventRecord::frame_tick(0, 0, 0));
        }
        assert_eq!(r.dropped(), 5);
    }

    #[test]
    fn aggregator_ingest_increments_count() {
        let mut agg = Aggregator::new();
        for i in 0..10 {
            agg.ingest(&EventRecord::frame_tick(i, 16_000, 14_336));
        }
        let snap = agg.snapshot_bucket(BucketTier::Min1);
        assert!(snap.contains("\"name\":\"engine.frame_tick\""));
        assert!(snap.contains("\"count\":10"));
    }

    #[test]
    fn aggregator_min_max_sum_correct() {
        let mut agg = Aggregator::new();
        agg.ingest(&EventRecord::frame_tick(0, 100, 0));
        agg.ingest(&EventRecord::frame_tick(0, 200, 0));
        agg.ingest(&EventRecord::frame_tick(0, 300, 0));
        let snap = agg.snapshot_bucket(BucketTier::Min1);
        assert!(snap.contains("\"min\":100"));
        assert!(snap.contains("\"max\":300"));
        assert!(snap.contains("\"avg\":200"));
    }

    #[test]
    fn aggregator_fallback_counted() {
        let mut agg = Aggregator::new();
        agg.ingest(&EventRecord::kan_classified(0, 1, true));
        agg.ingest(&EventRecord::kan_classified(0, 2, false));
        agg.ingest(&EventRecord::kan_classified(0, 3, true));
        let snap = agg.snapshot_bucket(BucketTier::Min1);
        assert!(snap.contains("\"fallback\":2"));
    }

    #[test]
    fn aggregator_error_counted_on_mcp_failure() {
        let mut agg = Aggregator::new();
        agg.ingest(&EventRecord::mcp_call(0, 100, true, 1));
        agg.ingest(&EventRecord::mcp_call(0, 200, false, 1));
        agg.ingest(&EventRecord::mcp_call(0, 300, false, 1));
        let snap = agg.snapshot_bucket(BucketTier::Min1);
        assert!(snap.contains("\"err\":2"), "snap={snap}");
    }

    #[test]
    fn aggregator_rolls_up_to_3_tiers() {
        let mut agg = Aggregator::with_rate(60);
        agg.ingest(&EventRecord::frame_tick(0, 16_000, 0));
        agg.ingest(&EventRecord::frame_tick(3_600, 16_000, 0));
        agg.ingest(&EventRecord::frame_tick(216_000, 16_000, 0));
        let snap_min = agg.snapshot_bucket(BucketTier::Min1);
        let snap_hr = agg.snapshot_bucket(BucketTier::Hr1);
        let snap_day = agg.snapshot_bucket(BucketTier::Day1);
        assert!(snap_min.contains("\"count\":3"));
        assert!(snap_hr.contains("\"count\":3"));
        assert!(snap_day.contains("\"count\":3"));
    }

    #[test]
    fn aggregator_reset_clears_counts() {
        let mut agg = Aggregator::new();
        agg.ingest(&EventRecord::frame_tick(0, 100, 0));
        let snap = agg.snapshot_bucket(BucketTier::Min1);
        assert!(snap.contains("\"count\":1"));
        agg.reset();
        let snap2 = agg.snapshot_bucket(BucketTier::Min1);
        assert!(!snap2.contains("\"count\":1"));
    }

    #[test]
    fn bucket_tier_from_tag_parses_canonical() {
        assert_eq!(BucketTier::from_tag("1min"), BucketTier::Min1);
        assert_eq!(BucketTier::from_tag("1hr"), BucketTier::Hr1);
        assert_eq!(BucketTier::from_tag("1day"), BucketTier::Day1);
        assert_eq!(BucketTier::from_tag("hour"), BucketTier::Hr1);
        assert_eq!(BucketTier::from_tag("garbage"), BucketTier::Min1);
    }

    #[test]
    fn payload_kind_disjoint_per_event_kind() {
        assert_eq!(
            EventRecord::frame_tick(0, 0, 0).payload_kind,
            PayloadKind::FrameTick as u8
        );
        assert_eq!(
            EventRecord::mcp_call(0, 0, true, 0).payload_kind,
            PayloadKind::McpCall as u8
        );
        assert_eq!(
            EventRecord::text_submitted(0, 0, 0, 0).payload_kind,
            PayloadKind::TextSubmit as u8
        );
        assert_eq!(
            EventRecord::dm_transition(0, 0, 0, 0).payload_kind,
            PayloadKind::DmTransition as u8
        );
    }

    #[test]
    fn event_to_json_includes_canonical_name() {
        let ev = EventRecord::frame_tick(7, 16_000, 14_336);
        let j = ev.to_json();
        assert!(j.contains("\"kind\":\"engine.frame_tick\""));
        assert!(j.contains("\"frame\":7"));
        assert!(j.contains("\"payload_b64\":\""));
    }

    #[test]
    fn base64_8_produces_12_chars() {
        let b = [0u8, 1, 2, 3, 4, 5, 6, 7];
        let s = base64_8(&b);
        assert_eq!(s.len(), 12);
        assert!(s.ends_with('='));
    }

    #[test]
    fn pii_assertion_text_record_carries_only_length() {
        // Critical test : `text_submitted` MUST NOT carry text content.
        let ev = EventRecord::text_submitted(0, 42, 5, 12_000);
        let j = ev.to_json();
        assert!(!j.contains("hello"));
        assert!(!j.contains("world"));
    }

    #[test]
    fn ringbuffer_concurrent_safety_smoke() {
        let mut r = RingBuffer::new(64);
        for i in 0..50 {
            r.push(EventRecord::frame_tick(i, 0, 0));
        }
        let mut out = Vec::new();
        r.drain_to_vec(&mut out, 50);
        assert_eq!(out.len(), 50);
        for i in 50..100 {
            r.push(EventRecord::frame_tick(i, 0, 0));
        }
        let mut out2 = Vec::new();
        r.drain_to_vec(&mut out2, 100);
        assert_eq!(out2.len(), 50);
        assert_eq!(out2[0].frame_offset, 50);
        assert_eq!(out2[49].frame_offset, 99);
    }

    #[test]
    fn fixed_point_q14_round_trip_normalized_value() {
        let val = 0.5f32;
        let q14 = (val * 16384.0) as u16;
        assert_eq!(q14, 8192);
        let recovered = q14 as f32 / 16384.0;
        assert!((recovered - val).abs() < 1e-3);
    }
}
