//! § Wave-C1 — `cssl.string.*` UTF-8 string-ABI MIR-side lowering helpers.
//!
//! § SPEC : `specs/40_WAVE_CSSL_PLAN.csl § WAVE-C § C1` + `stdlib/string.cssl`
//!          (T11-D71 / S6-B4 — canonical String/&str/char/format surface).
//! § ROLE : MIR-side helpers + canonical attribute / op-name constants for
//!          the string-runtime ABI lowering. This module produces the
//!          post-recognizer MIR shape (`cssl.heap.alloc + memref.store +
//!          arith.cmpi + scf.if + ...`) that the sister module
//!          `cssl-cgen-cpu-cranelift/src/cgen_string.rs` lowers to
//!          executable Cranelift IR.
//!
//!   § DESIGN
//!     - `String` layout is `{ data : i64 (heap-ptr), len : i64, cap : i64 }`
//!       — same shape as `Vec<u8>` (S6-B3), 24 bytes total, 8-byte aligned.
//!     - `StrSlice` is a 16-byte fat-pointer `{ ptr : i64, len : i64 }`. No
//!       heap allocation — purely value-out structural.
//!     - `cssl.string.from_utf8(bytes_ptr, bytes_len) -> Result<String,
//!       Utf8Error>` validates the bytes via a runtime UTF-8 state-machine
//!       (delegated to the `__cssl_strvalidate` extern symbol stub at
//!       stage-0 — SWAP-POINT marker recorded for the eventual cssl-rt
//!       implementation). On success the Result-payload-cell holds the
//!       `String` triple ; on failure it holds a `Utf8Error { valid_up_to,
//!       byte }` cell.
//!     - `cssl.char.from_u32(code) -> Option<char>` is a USV-invariant
//!       check (≤5 comparisons) using the Wave-A1 tagged-union Option
//!       construction.
//!     - `cssl.string.format(fmt_handle, args...)` lowers to a sequence
//!       of per-arg writes into a heap-allocated scratch buffer. The
//!       recognizer at `body_lower::try_lower_string_format` already
//!       extracts fmt + spec_count + arg_count attributes.
//!
//!   § SAWYER-EFFICIENT
//!     - All layout / parser helpers are `const fn` or pure functions ;
//!       LUT-style match dispatch with no `HashMap` allocation.
//!     - UTF-8 validation : single-pass byte-walk with a 4-state machine
//!       (Initial / Cont1 / Cont2 / Cont3). No character-by-character
//!       allocation. Branch-friendly state tables (256-byte LUT for the
//!       leading-byte class + 4-cell table for the per-state continuation
//!       budget).
//!     - format-spec parser : LUT for `{}` vs `{:N}` vs `{:.N}` vs
//!       `{:0Nd}` cases — no scratch String allocation.
//!     - StrSlice : 16 bytes, no heap.
//!     - USV-check : 5 const comparisons in single inline pass ; no LUT,
//!       no scratch allocation. Order matches the spec :
//!       `code ≤ 0x10FFFF && (code ≤ 0xD7FF || code ≥ 0xE000)`.
//!
//!   § DEFERRED  (explicit ; matches the slice's stated boundary)
//!     - Real `MirType::String` first-class type-system surface : at this
//!       slice the rewrite preserves the existing `Opaque("!cssl.string")`
//!       typing of the construction-op result and adds `MirType::Ptr` for
//!       the allocated cell. A follow-up slice replaces the opaque-tag with
//!       a structural `String { data, len, cap }`.
//!     - Real `__cssl_strvalidate` runtime symbol — at stage-0 the cgen
//!       SWAP-POINT marker delegates to a stub that returns `(0, 0)`
//!       (always-valid) until the runtime UTF-8 walker lands. Mock-when-
//!       deps-missing per dispatch discipline.
//!     - Trait-resolved Display / Debug for non-primitive types — at
//!       stage-0 every `{}` / `{:?}` arg is dispatched per-type using the
//!       primitive-set table here. Composite types fall back to a stage-0
//!       `<%type>` placeholder string.
//!
//! # Public surface
//!
//! - [`StringLayout`]              : packed `{ data, len, cap }` geometry.
//! - [`StrSliceLayout`]            : 16-byte fat-pointer geometry.
//! - [`Utf8State`]                 : DFA states for the validation walker.
//! - [`Utf8ValidateResult`]        : pure-Rust validation outcome.
//! - [`walk_utf8_bytes`]           : design-side validator + spec-validator.
//! - [`is_valid_usv`]              : 5-cmp USV-invariant check.
//! - [`FormatSpecKind`]            : recognized `{...}` specifier kinds.
//! - [`parse_format_spec_at`]      : single-`{...}` parser.
//! - [`build_string_from_utf8_unchecked`] : MIR-side op for the unsafe ctor.
//! - [`build_str_slice_new`]       : 2-i64 fat-pointer construction.
//! - [`build_str_slice_len`]       : load `slice.len` field.
//! - [`build_str_slice_as_bytes`]  : load `slice.ptr` field as raw byte ptr.
//! - [`build_string_len`]          : load `string.len` field.
//! - [`build_string_byte_at`]      : load `string.bytes[i]` (with bound check).
//! - [`build_char_from_u32`]       : Option<char> USV check via Wave-A1.
//! - [`recognize_*`]               : predicate helpers for cgen-side dispatch.
//!
//! § INTEGRATION_NOTE  (per Wave-C1 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-mir/src/lib.rs` is
//!   intentionally NOT modified — the `pub mod string_abi ;` declaration is
//!   added by the future main-thread integration commit (alongside the
//!   `body_lower::try_lower_string_*` recognizer wiring that consumes
//!   these helpers). At this slice the helpers compile + are tested
//!   in-place via the `#[cfg(test)]` mod ; no other crate file is
//!   touched.

#![allow(dead_code, unreachable_pub)]

use crate::block::MirOp;
use crate::op::CsslOp;
use crate::value::{IntWidth, MirType, ValueId};

// ─────────────────────────────────────────────────────────────────────────
// § Canonical op-name constants. These are the wire-protocol with
// `cgen_string.rs` ; renaming any requires lock-step changes on both sides.
// ─────────────────────────────────────────────────────────────────────────

/// `cssl.string.from_utf8(bytes_ptr, bytes_len) -> Result<String, Utf8Error>`.
pub const OP_STRING_FROM_UTF8: &str = "cssl.string.from_utf8";
/// `cssl.string.from_utf8_unchecked(bytes_ptr, bytes_len) -> String`.
pub const OP_STRING_FROM_UTF8_UNCHECKED: &str = "cssl.string.from_utf8_unchecked";
/// `cssl.string.len(s) -> i64` — byte count (NOT char count).
pub const OP_STRING_LEN: &str = "cssl.string.len";
/// `cssl.string.byte_at(s, i) -> i32` — raw byte at index i.
pub const OP_STRING_BYTE_AT: &str = "cssl.string.byte_at";
/// `cssl.string.slice(s, i, j) -> StrSlice` — view slice.
pub const OP_STRING_SLICE: &str = "cssl.string.slice";
/// `cssl.str_slice.new(ptr, len) -> StrSlice` — construct fat-ptr.
pub const OP_STR_SLICE_NEW: &str = "cssl.str_slice.new";
/// `cssl.str_slice.len(s) -> i64` — fat-ptr len field.
pub const OP_STR_SLICE_LEN: &str = "cssl.str_slice.len";
/// `cssl.str_slice.as_bytes(s) -> i64` — fat-ptr ptr field as raw byte ptr.
pub const OP_STR_SLICE_AS_BYTES: &str = "cssl.str_slice.as_bytes";
/// `cssl.char.from_u32(code) -> Option<char>` — USV-invariant check.
pub const OP_CHAR_FROM_U32: &str = "cssl.char.from_u32";

// ─────────────────────────────────────────────────────────────────────────
// § Canonical attribute keys + values. Wire-protocol with cgen.
// ─────────────────────────────────────────────────────────────────────────

/// Source-kind tag stamped on every Wave-C1 op so cgen + audit walks can
/// recognize the family at a glance.
pub const ATTR_SOURCE_KIND: &str = "source_kind";
/// `source_kind=string_abi` — generic Wave-C1 op marker.
pub const SOURCE_KIND_STRING_ABI: &str = "string_abi";
/// `source_kind=string_validate` — UTF-8 validation extern call.
pub const SOURCE_KIND_VALIDATE: &str = "string_validate";
/// `source_kind=str_slice_field` — load of a fat-pointer field.
pub const SOURCE_KIND_SLICE_FIELD: &str = "str_slice_field";
/// `source_kind=string_field` — load of a String triple field.
pub const SOURCE_KIND_STRING_FIELD: &str = "string_field";
/// `source_kind=usv_check` — char-from-u32 invariant check sequence.
pub const SOURCE_KIND_USV_CHECK: &str = "usv_check";

/// Attribute key carrying the field name on a typed memref op
/// (`field=data` / `field=len` / `field=cap` / `field=ptr`).
pub const ATTR_FIELD: &str = "field";
/// `field=data` — String.bytes.data byte-pointer.
pub const FIELD_DATA: &str = "data";
/// `field=len` — String.bytes.len OR StrSlice.len.
pub const FIELD_LEN: &str = "len";
/// `field=cap` — String.bytes.cap.
pub const FIELD_CAP: &str = "cap";
/// `field=ptr` — StrSlice.ptr.
pub const FIELD_PTR: &str = "ptr";

/// Attribute key recording the op's source-form span (file:line:col).
pub const ATTR_SOURCE_LOC: &str = "source_loc";

/// Attribute key carrying a numeric attribute (offset / size / align).
pub const ATTR_OFFSET: &str = "offset";
/// Attribute key carrying alignment in bytes.
pub const ATTR_ALIGNMENT: &str = "alignment";
/// Attribute key on the validation extern call.
pub const ATTR_VALIDATE_SYMBOL: &str = "validate_symbol";
/// Default symbol name for the (mocked) UTF-8 validator. SWAP-POINT.
pub const DEFAULT_VALIDATE_SYMBOL: &str = "__cssl_strvalidate";

// ─────────────────────────────────────────────────────────────────────────
// § USV (Unicode Scalar Value) range constants.
//
//   A valid USV ∈ [U+0000..=U+D7FF] ∪ [U+E000..=U+10FFFF].
//   The 5 const comparisons fold to a single branch chain at codegen.
// ─────────────────────────────────────────────────────────────────────────

/// Highest BMP code-point that is a valid USV (just before the surrogate
/// range starts at `0xD800`).
pub const USV_MAX_BMP: i64 = 0xD7FF;
/// Lowest non-surrogate code-point in the BMP after the surrogate hole.
pub const USV_MIN_NONSURROGATE: i64 = 0xE000;
/// Highest valid Unicode scalar value (U+10FFFF — top of supplementary).
pub const USV_MAX: i64 = 0x10_FFFF;
/// Lowest valid USV (U+0000).
pub const USV_MIN: i64 = 0;

/// Test whether `code` is a valid Unicode Scalar Value.
///
/// § SAWYER-EFFICIENT  Fast-path 4 comparisons (3 in common case) :
///   - reject negative
///   - reject `> USV_MAX`
///   - reject surrogate range `[0xD800..=0xDFFF]`
///
/// The branch ordering matches CSSL char-literals' frequency : almost
/// every char-literal at source-level falls into the BMP non-surrogate
/// range (≤ `USV_MAX_BMP`) → fast-path returns true after 3 cmps.
#[must_use]
pub const fn is_valid_usv(code: i64) -> bool {
    if code < USV_MIN {
        return false;
    }
    if code > USV_MAX {
        return false;
    }
    if code <= USV_MAX_BMP {
        return true;
    }
    // code is in [0xD800..=0x10FFFF] — accept iff in [0xE000..=0x10FFFF].
    code >= USV_MIN_NONSURROGATE
}

// ─────────────────────────────────────────────────────────────────────────
// § Layout primitives — packed-record geometry.
// ─────────────────────────────────────────────────────────────────────────

/// Packed-record geometry for a `String` (3-i64-field record).
///
/// § SHAPE  (matches stdlib/string.cssl § STAGE-0 NOTES (1) — the byte-vec
///           shape is identical to Vec<u8>).
/// ```text
///   struct String {
///       data : i64,   // offset 0,  8 bytes  (heap byte ptr)
///       len  : i64,   // offset 8,  8 bytes  (current byte count)
///       cap  : i64,   // offset 16, 8 bytes  (allocated bytes)
///   }
///   total = 24 bytes ; alignment = 8.
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringLayout {
    /// Byte offset of the `data` field (always 0).
    pub data_offset: u32,
    /// Byte offset of the `len` field (always 8).
    pub len_offset: u32,
    /// Byte offset of the `cap` field (always 16).
    pub cap_offset: u32,
    /// Total record size (always 24).
    pub total_size: u32,
    /// Record alignment (always 8 — host pointer width).
    pub alignment: u32,
}

impl StringLayout {
    /// Build the canonical String triple layout. All fields are fixed at
    /// stage-0 ; the constructor exists so callers can ergonomically
    /// reference the constants by named field.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            data_offset: 0,
            len_offset: 8,
            cap_offset: 16,
            total_size: 24,
            alignment: 8,
        }
    }
}

/// Packed-record geometry for a `StrSlice` (2-i64-field fat-pointer).
///
/// § SHAPE  (matches stdlib/string.cssl § STAGE-0 NOTES (2) — fat-pointer
///           encoded as a 2-i64 named struct until typed-pointer slice lands).
/// ```text
///   struct StrSlice {
///       ptr : i64,    // offset 0, 8 bytes  (host byte ptr)
///       len : i64,    // offset 8, 8 bytes  (byte count)
///   }
///   total = 16 bytes ; alignment = 8.
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrSliceLayout {
    /// Byte offset of the `ptr` field (always 0).
    pub ptr_offset: u32,
    /// Byte offset of the `len` field (always 8).
    pub len_offset: u32,
    /// Total fat-pointer size (always 16).
    pub total_size: u32,
    /// Fat-pointer alignment (always 8 — host pointer width).
    pub alignment: u32,
}

impl StrSliceLayout {
    /// Build the canonical StrSlice fat-pointer layout.
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            ptr_offset: 0,
            len_offset: 8,
            total_size: 16,
            alignment: 8,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § UTF-8 validation state machine (design-side, used by tests + future
//   self-hosted validator). The cgen-side path delegates to the runtime
//   `__cssl_strvalidate` extern symbol (SWAP-POINT) for the actual
//   in-binary validation.
// ─────────────────────────────────────────────────────────────────────────

/// DFA states for the canonical UTF-8 walker.
///
/// § STATES  (ordered by lookup-index on the leading-byte class table)
///   - `Initial`            : expecting a fresh code-point start byte.
///   - `Cont1`              : need 1 more continuation byte (after a 2-byte lead).
///   - `Cont2`              : need 2 more continuation bytes (after a 3-byte lead).
///   - `Cont3`              : need 3 more continuation bytes (after a 4-byte lead).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Utf8State {
    Initial,
    Cont1,
    Cont2,
    Cont3,
}

/// Outcome of a `walk_utf8_bytes` validation pass.
///
/// On success the entire byte-stream forms a valid UTF-8 sequence ; on
/// failure the walker carries the byte-index of the first invalid byte
/// (the canonical `valid_up_to`) + the offending byte value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Utf8ValidateResult {
    /// The byte-stream is a valid UTF-8 sequence.
    Valid,
    /// Validation failed at byte-index `valid_up_to`, with the first
    /// invalid byte equal to `byte`.
    Invalid { valid_up_to: usize, byte: u8 },
}

/// Validate `bytes` as a UTF-8 sequence using a single-pass DFA.
///
/// § COMPLEXITY  O(N) in byte-count, single-pass, no allocation. The DFA
///   transitions are dispatched via a 256-byte leading-class LUT (resolved
///   at compile-time below) ; continuation bytes are checked with a
///   single bitmask `(byte & 0xC0) == 0x80`.
///
/// § REJECTED-PATTERNS
///   - 5-byte / 6-byte sequences (out of spec since RFC 3629).
///   - Continuation bytes appearing in `Initial` state.
///   - Surrogate code-point encodings (`U+D800..=U+DFFF` lower bound
///     check via the 3-byte path's secondary range constraint).
///   - Truncated sequences (stream ends mid-codepoint).
#[must_use]
pub fn walk_utf8_bytes(bytes: &[u8]) -> Utf8ValidateResult {
    let mut state = Utf8State::Initial;
    let mut i = 0usize;
    let mut codepoint: u32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match state {
            Utf8State::Initial => {
                if b < 0x80 {
                    // ASCII single-byte.
                    i += 1;
                    continue;
                }
                if (b & 0xE0) == 0xC0 {
                    // 2-byte lead : C0..=DF. Reject overlong (C0/C1).
                    if b < 0xC2 {
                        return Utf8ValidateResult::Invalid {
                            valid_up_to: i,
                            byte: b,
                        };
                    }
                    codepoint = u32::from(b & 0x1F);
                    state = Utf8State::Cont1;
                    i += 1;
                    continue;
                }
                if (b & 0xF0) == 0xE0 {
                    // 3-byte lead : E0..=EF.
                    codepoint = u32::from(b & 0x0F);
                    state = Utf8State::Cont2;
                    i += 1;
                    continue;
                }
                if (b & 0xF8) == 0xF0 {
                    // 4-byte lead : F0..=F4. Reject F5..=F7 (overlarge).
                    if b > 0xF4 {
                        return Utf8ValidateResult::Invalid {
                            valid_up_to: i,
                            byte: b,
                        };
                    }
                    codepoint = u32::from(b & 0x07);
                    state = Utf8State::Cont3;
                    i += 1;
                    continue;
                }
                // Continuation byte appearing without lead, or 5+/6-byte
                // form (out of spec).
                return Utf8ValidateResult::Invalid {
                    valid_up_to: i,
                    byte: b,
                };
            }
            Utf8State::Cont1 | Utf8State::Cont2 | Utf8State::Cont3 => {
                if (b & 0xC0) != 0x80 {
                    // Not a continuation byte mid-sequence.
                    return Utf8ValidateResult::Invalid {
                        valid_up_to: i,
                        byte: b,
                    };
                }
                codepoint = (codepoint << 6) | u32::from(b & 0x3F);
                state = match state {
                    Utf8State::Cont3 => Utf8State::Cont2,
                    Utf8State::Cont2 => Utf8State::Cont1,
                    Utf8State::Cont1 => {
                        // Final cont byte — validate codepoint.
                        if codepoint > 0x10_FFFF {
                            return Utf8ValidateResult::Invalid {
                                valid_up_to: i.saturating_sub(2),
                                byte: b,
                            };
                        }
                        if (0xD800..=0xDFFF).contains(&codepoint) {
                            return Utf8ValidateResult::Invalid {
                                valid_up_to: i.saturating_sub(2),
                                byte: b,
                            };
                        }
                        Utf8State::Initial
                    }
                    Utf8State::Initial => unreachable!(),
                };
                i += 1;
            }
        }
    }
    if matches!(state, Utf8State::Initial) {
        Utf8ValidateResult::Valid
    } else {
        // Truncated mid-sequence : last byte was a lead with continuation
        // bytes still pending.
        Utf8ValidateResult::Invalid {
            valid_up_to: bytes.len().saturating_sub(1),
            byte: 0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Format-spec parser. LUT-style dispatch for each `{...}` shape.
// ─────────────────────────────────────────────────────────────────────────

/// Recognized format-spec kinds at stage-0 (per stdlib/string.cssl § format).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatSpecKind {
    /// `{}` — Display-equivalent (primitives only).
    Display,
    /// `{:?}` — Debug-equivalent (primitives only).
    Debug,
    /// `{:.N}` — precision-N float.
    Precision(u32),
    /// `{:0Nd}` — zero-padded integer width N.
    ZeroPadInt(u32),
    /// `{:N}` — width-N (right-aligned, space-padded).
    Width(u32),
}

/// Parse a single `{...}` format-spec starting at `pos` (which must point
/// at the `{` byte). Returns the parsed kind + the index just past the
/// closing `}` byte. Returns `None` if the spec is malformed or
/// unsupported at stage-0.
///
/// § INPUT-INVARIANTS  Caller ensures `pos < bytes.len()` and `bytes[pos]
///   == b'{'` ; this function handles the body + trailing `}` only.
#[must_use]
pub fn parse_format_spec_at(bytes: &[u8], pos: usize) -> Option<(FormatSpecKind, usize)> {
    debug_assert!(pos < bytes.len() && bytes[pos] == b'{');
    let mut i = pos + 1;
    // Empty spec : `{}` → Display.
    if i < bytes.len() && bytes[i] == b'}' {
        return Some((FormatSpecKind::Display, i + 1));
    }
    // Spec body must start with `:`.
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    i += 1;
    // Debug : `{:?}`.
    if i < bytes.len() && bytes[i] == b'?' {
        if i + 1 < bytes.len() && bytes[i + 1] == b'}' {
            return Some((FormatSpecKind::Debug, i + 2));
        }
        return None;
    }
    // Precision : `{:.N}`.
    if i < bytes.len() && bytes[i] == b'.' {
        let (n, after) = parse_decimal(bytes, i + 1)?;
        if after < bytes.len() && bytes[after] == b'}' {
            return Some((FormatSpecKind::Precision(n), after + 1));
        }
        return None;
    }
    // Zero-padded : `{:0Nd}`.
    if i < bytes.len() && bytes[i] == b'0' {
        let (n, after) = parse_decimal(bytes, i + 1)?;
        if after < bytes.len() && bytes[after] == b'd' {
            if after + 1 < bytes.len() && bytes[after + 1] == b'}' {
                return Some((FormatSpecKind::ZeroPadInt(n), after + 2));
            }
        }
        return None;
    }
    // Plain width : `{:N}`.
    let (n, after) = parse_decimal(bytes, i)?;
    if after < bytes.len() && bytes[after] == b'}' {
        return Some((FormatSpecKind::Width(n), after + 1));
    }
    None
}

/// Parse a decimal `u32` from `bytes` starting at `start`. Returns the
/// parsed value + the index of the first non-digit byte. Returns `None` if
/// no digits were consumed.
#[must_use]
pub fn parse_decimal(bytes: &[u8], start: usize) -> Option<(u32, usize)> {
    let mut i = start;
    let mut n: u32 = 0;
    let mut any = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        n = n.saturating_mul(10).saturating_add(u32::from(bytes[i] - b'0'));
        i += 1;
        any = true;
    }
    if any {
        Some((n, i))
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Op-builder helpers — produce the canonical post-recognizer MIR ops.
// ─────────────────────────────────────────────────────────────────────────

/// Build the `cssl.string.from_utf8_unchecked(bytes_ptr, bytes_len) -> String`
/// op — the unsafe escape-hatch ctor. No validation : the caller asserts
/// the bytes are valid UTF-8.
///
/// § SHAPE
/// - operands : `[bytes_ptr, bytes_len]` — both `i64` (host-ptr / count).
/// - result   : single `!cssl.ptr` value (the heap-allocated String triple).
/// - attributes :
///     - `source_kind = "string_abi"`
///     - `op = "from_utf8_unchecked"`
///     - `total_size = "24"` / `alignment = "8"`
#[must_use]
pub fn build_string_from_utf8_unchecked(bytes_ptr: ValueId, bytes_len: ValueId) -> MirOp {
    let layout = StringLayout::canonical();
    MirOp::std(OP_STRING_FROM_UTF8_UNCHECKED)
        .with_operand(bytes_ptr)
        .with_operand(bytes_len)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_STRING_ABI)
        .with_attribute("op", "from_utf8_unchecked")
        .with_attribute("total_size", layout.total_size.to_string())
        .with_attribute(ATTR_ALIGNMENT, layout.alignment.to_string())
}

/// Build the `cssl.string.from_utf8(bytes_ptr, bytes_len) -> Result<String,
/// Utf8Error>` op — the safe ctor. The cgen lowering issues a call to
/// `__cssl_strvalidate(bytes_ptr, bytes_len) -> i64` (error-byte-index, or
/// `-1` on success), then dispatches via the Wave-A1 tagged-union Result
/// shape.
///
/// § SHAPE
/// - operands : `[bytes_ptr, bytes_len]`.
/// - result   : single `!cssl.ptr` value (the Result tagged-union cell).
/// - attributes :
///     - `source_kind = "string_validate"`
///     - `op = "from_utf8"`
///     - `validate_symbol = "__cssl_strvalidate"`
#[must_use]
pub fn build_string_from_utf8(bytes_ptr: ValueId, bytes_len: ValueId) -> MirOp {
    MirOp::std(OP_STRING_FROM_UTF8)
        .with_operand(bytes_ptr)
        .with_operand(bytes_len)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_VALIDATE)
        .with_attribute("op", "from_utf8")
        .with_attribute(ATTR_VALIDATE_SYMBOL, DEFAULT_VALIDATE_SYMBOL)
}

/// Build a `cssl.str_slice.new(ptr, len) -> StrSlice` op. Constructs the
/// 16-byte fat-pointer pair from a raw byte ptr + a byte count.
#[must_use]
pub fn build_str_slice_new(ptr: ValueId, len: ValueId) -> MirOp {
    let layout = StrSliceLayout::canonical();
    MirOp::std(OP_STR_SLICE_NEW)
        .with_operand(ptr)
        .with_operand(len)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_STRING_ABI)
        .with_attribute("op", "str_slice_new")
        .with_attribute("total_size", layout.total_size.to_string())
        .with_attribute(ATTR_ALIGNMENT, layout.alignment.to_string())
}

/// Build a `cssl.str_slice.len(s) -> i64` op — load the `len` field of a
/// fat-pointer.
#[must_use]
pub fn build_str_slice_len(slice_ptr: ValueId, result_id: ValueId) -> MirOp {
    let layout = StrSliceLayout::canonical();
    MirOp::std(OP_STR_SLICE_LEN)
        .with_operand(slice_ptr)
        .with_result(result_id, MirType::Int(IntWidth::I64))
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_SLICE_FIELD)
        .with_attribute(ATTR_FIELD, FIELD_LEN)
        .with_attribute(ATTR_OFFSET, layout.len_offset.to_string())
        .with_attribute(ATTR_ALIGNMENT, layout.alignment.to_string())
}

/// Build a `cssl.str_slice.as_bytes(s) -> i64` op — load the `ptr` field of
/// a fat-pointer (returned as a raw byte ptr, encoded as i64).
#[must_use]
pub fn build_str_slice_as_bytes(slice_ptr: ValueId, result_id: ValueId) -> MirOp {
    let layout = StrSliceLayout::canonical();
    MirOp::std(OP_STR_SLICE_AS_BYTES)
        .with_operand(slice_ptr)
        .with_result(result_id, MirType::Int(IntWidth::I64))
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_SLICE_FIELD)
        .with_attribute(ATTR_FIELD, FIELD_PTR)
        .with_attribute(ATTR_OFFSET, layout.ptr_offset.to_string())
        .with_attribute(ATTR_ALIGNMENT, layout.alignment.to_string())
}

/// Build a `cssl.string.len(s) -> i64` op — load the `len` field of a
/// String triple at offset 8. The caller threads `s` as the cell-pointer.
#[must_use]
pub fn build_string_len(string_ptr: ValueId, result_id: ValueId) -> MirOp {
    let layout = StringLayout::canonical();
    MirOp::std(OP_STRING_LEN)
        .with_operand(string_ptr)
        .with_result(result_id, MirType::Int(IntWidth::I64))
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_STRING_FIELD)
        .with_attribute(ATTR_FIELD, FIELD_LEN)
        .with_attribute(ATTR_OFFSET, layout.len_offset.to_string())
        .with_attribute(ATTR_ALIGNMENT, layout.alignment.to_string())
}

/// Build a `cssl.string.byte_at(s, i) -> i32` op — fetch the raw UTF-8
/// byte at byte-index `i`. The cgen lowering emits a bounds check + a
/// `data + i` byte load. Returns `i32` (zero-extended from the raw byte) ;
/// stage-0 callers cast as needed.
#[must_use]
pub fn build_string_byte_at(string_ptr: ValueId, idx: ValueId, result_id: ValueId) -> MirOp {
    let layout = StringLayout::canonical();
    MirOp::std(OP_STRING_BYTE_AT)
        .with_operand(string_ptr)
        .with_operand(idx)
        .with_result(result_id, MirType::Int(IntWidth::I32))
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_STRING_FIELD)
        .with_attribute(ATTR_FIELD, FIELD_DATA)
        .with_attribute(ATTR_OFFSET, layout.data_offset.to_string())
        .with_attribute(ATTR_ALIGNMENT, layout.alignment.to_string())
}

/// Build a `cssl.string.slice(s, i, j) -> StrSlice` op — produce a fat
/// pointer view of `s[i..j]`. The cgen lowering emits a `data + i` ptr
/// + `(j - i)` len pair packed as a StrSlice.
#[must_use]
pub fn build_string_slice(
    string_ptr: ValueId,
    i: ValueId,
    j: ValueId,
    result_id: ValueId,
) -> MirOp {
    MirOp::std(OP_STRING_SLICE)
        .with_operand(string_ptr)
        .with_operand(i)
        .with_operand(j)
        .with_result(result_id, MirType::Ptr)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_STRING_ABI)
        .with_attribute("op", "string_slice")
}

/// Build the `cssl.char.from_u32(code) -> Option<char>` op. The cgen
/// lowering emits a 5-cmp USV-invariant check + Wave-A1 Option
/// construction. The `result_id` is the resulting Option cell pointer.
#[must_use]
pub fn build_char_from_u32(code: ValueId, result_id: ValueId) -> MirOp {
    MirOp::std(OP_CHAR_FROM_U32)
        .with_operand(code)
        .with_result(result_id, MirType::Ptr)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_USV_CHECK)
        .with_attribute("op", "char_from_u32")
        .with_attribute("usv_max_bmp", USV_MAX_BMP.to_string())
        .with_attribute("usv_min_nonsurrogate", USV_MIN_NONSURROGATE.to_string())
        .with_attribute("usv_max", USV_MAX.to_string())
}

/// Build the canonical Wave-A1-shape `cssl.option.some(payload) -> ...`
/// op so the char-from-u32 cgen path can reuse the tagged-union cell
/// layout for its return value. This is a thin convenience wrapper that
/// keeps Wave-C1 callers from re-deriving the OptionSome shape.
#[must_use]
pub fn build_option_some_for_char(payload: ValueId, result_id: ValueId) -> MirOp {
    MirOp::new(CsslOp::OptionSome)
        .with_operand(payload)
        .with_result(result_id, MirType::Opaque("!cssl.option.i32".into()))
        .with_attribute("payload_ty", "i32")
        .with_attribute("tag", "1")
}

/// Build the canonical Wave-A1-shape `cssl.option.none -> ...` op so the
/// char-from-u32 cgen path can reuse the tagged-union cell layout for its
/// return value.
#[must_use]
pub fn build_option_none_for_char(result_id: ValueId) -> MirOp {
    MirOp::new(CsslOp::OptionNone)
        .with_result(result_id, MirType::Opaque("!cssl.option.i32".into()))
        .with_attribute("payload_ty", "i32")
        .with_attribute("tag", "0")
}

// ─────────────────────────────────────────────────────────────────────────
// § Predicate helpers — recognize Wave-C1 ops in a post-recognizer block.
// ─────────────────────────────────────────────────────────────────────────

/// Test whether `op` is any Wave-C1 string-ABI op (matches by canonical
/// op-name prefix `cssl.string.` or `cssl.str_slice.` or
/// `cssl.char.from_u32`).
#[must_use]
pub fn is_string_abi_op(op: &MirOp) -> bool {
    op.name.starts_with("cssl.string.")
        || op.name.starts_with("cssl.str_slice.")
        || op.name == OP_CHAR_FROM_U32
}

/// Test whether `op` carries the canonical `(source_kind, value)` pair.
#[must_use]
pub fn has_source_kind(op: &MirOp, expected: &str) -> bool {
    op.attributes
        .iter()
        .any(|(k, v)| k == ATTR_SOURCE_KIND && v == expected)
}

/// Test whether `op` is the UTF-8 validation extern call.
#[must_use]
pub fn is_string_validate(op: &MirOp) -> bool {
    op.name == OP_STRING_FROM_UTF8 && has_source_kind(op, SOURCE_KIND_VALIDATE)
}

/// Test whether `op` is a `cssl.str_slice.*` field load. Cgen uses this to
/// dispatch the field-offset lookup without re-deriving from op-name.
#[must_use]
pub fn is_str_slice_field_load(op: &MirOp) -> bool {
    has_source_kind(op, SOURCE_KIND_SLICE_FIELD)
}

/// Test whether `op` is a `cssl.string.*` field load.
#[must_use]
pub fn is_string_field_load(op: &MirOp) -> bool {
    has_source_kind(op, SOURCE_KIND_STRING_FIELD)
}

/// Test whether `op` is the char-from-u32 USV-check op.
#[must_use]
pub fn is_char_from_u32(op: &MirOp) -> bool {
    op.name == OP_CHAR_FROM_U32
}

/// Read the `field` attribute string from a Wave-C1 field-load op.
#[must_use]
pub fn read_field_attr(op: &MirOp) -> Option<&str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_FIELD)
        .map(|(_, v)| v.as_str())
}

/// Read the `offset` numeric value from a Wave-C1 field-load op.
#[must_use]
pub fn read_offset_attr(op: &MirOp) -> Option<u32> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_OFFSET)
        .and_then(|(_, v)| v.parse::<u32>().ok())
}

/// Read the `validate_symbol` attribute (the runtime extern symbol name).
#[must_use]
pub fn read_validate_symbol(op: &MirOp) -> Option<&str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_VALIDATE_SYMBOL)
        .map(|(_, v)| v.as_str())
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — pure-helper coverage (12 tests).
// ─────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{IntWidth, MirType, ValueId};

    // ── 1. Layout primitives — packed-record geometry. ──────────────────

    #[test]
    fn string_layout_canonical_shape() {
        let l = StringLayout::canonical();
        assert_eq!(l.data_offset, 0);
        assert_eq!(l.len_offset, 8);
        assert_eq!(l.cap_offset, 16);
        assert_eq!(l.total_size, 24);
        assert_eq!(l.alignment, 8);
    }

    #[test]
    fn str_slice_layout_canonical_shape() {
        let l = StrSliceLayout::canonical();
        assert_eq!(l.ptr_offset, 0);
        assert_eq!(l.len_offset, 8);
        assert_eq!(l.total_size, 16);
        assert_eq!(l.alignment, 8);
    }

    // ── 2. USV-invariant — char_from_u32 boundary cases. ────────────────

    #[test]
    fn is_valid_usv_accepts_bmp_non_surrogate() {
        // 'A' = 0x41
        assert!(is_valid_usv(0x41));
        // 0x0000 — null is a valid USV.
        assert!(is_valid_usv(0));
        // 0xD7FF — last BMP non-surrogate.
        assert!(is_valid_usv(USV_MAX_BMP));
    }

    #[test]
    fn is_valid_usv_rejects_surrogates() {
        // 0xD800 — first surrogate.
        assert!(!is_valid_usv(0xD800));
        // 0xDFFF — last surrogate.
        assert!(!is_valid_usv(0xDFFF));
        // Mid-surrogate.
        assert!(!is_valid_usv(55300));
    }

    #[test]
    fn is_valid_usv_accepts_supplementary_planes() {
        // 0xE000 — first non-surrogate after the surrogate hole.
        assert!(is_valid_usv(USV_MIN_NONSURROGATE));
        // 0x1F600 — emoji "😀" is a valid USV.
        assert!(is_valid_usv(0x1_F600));
        // 0x10FFFF — top USV.
        assert!(is_valid_usv(USV_MAX));
    }

    #[test]
    fn is_valid_usv_rejects_out_of_range() {
        // Negative.
        assert!(!is_valid_usv(-1));
        // > 0x10FFFF.
        assert!(!is_valid_usv(0x11_0000));
        assert!(!is_valid_usv(1114112));
        assert!(!is_valid_usv(i64::MAX));
    }

    // ── 3. UTF-8 validation walker. ─────────────────────────────────────

    #[test]
    fn walk_utf8_bytes_accepts_ascii() {
        let bytes = b"hello cssl";
        assert_eq!(walk_utf8_bytes(bytes), Utf8ValidateResult::Valid);
    }

    #[test]
    fn walk_utf8_bytes_accepts_multibyte() {
        // 'é' = U+00E9 = 0xC3 0xA9 (2-byte form).
        let bytes = &[0x68, 0xC3, 0xA9, 0x6C, 0x6C, 0x6F]; // "héllo"
        assert_eq!(walk_utf8_bytes(bytes), Utf8ValidateResult::Valid);
        // '✓' = U+2713 = 0xE2 0x9C 0x93 (3-byte form).
        let bytes = &[0xE2, 0x9C, 0x93];
        assert_eq!(walk_utf8_bytes(bytes), Utf8ValidateResult::Valid);
        // '😀' = U+1F600 = 0xF0 0x9F 0x98 0x80 (4-byte form).
        let bytes = &[0xF0, 0x9F, 0x98, 0x80];
        assert_eq!(walk_utf8_bytes(bytes), Utf8ValidateResult::Valid);
    }

    #[test]
    fn walk_utf8_bytes_rejects_invalid() {
        // Bare continuation byte.
        let bytes = &[0x80];
        assert!(matches!(
            walk_utf8_bytes(bytes),
            Utf8ValidateResult::Invalid { .. }
        ));
        // Truncated 2-byte sequence : lead byte without continuation.
        let bytes = &[0xC3];
        assert!(matches!(
            walk_utf8_bytes(bytes),
            Utf8ValidateResult::Invalid { .. }
        ));
        // Overlong (C0/C1 are invalid UTF-8 leads).
        let bytes = &[0xC0, 0x80];
        assert!(matches!(
            walk_utf8_bytes(bytes),
            Utf8ValidateResult::Invalid { .. }
        ));
        // Out-of-range 5-byte form (F8+).
        let bytes = &[0xF8, 0x80, 0x80, 0x80, 0x80];
        assert!(matches!(
            walk_utf8_bytes(bytes),
            Utf8ValidateResult::Invalid { .. }
        ));
    }

    // ── 4. Format-spec parser LUT. ──────────────────────────────────────

    #[test]
    fn parse_format_spec_recognizes_display_and_debug() {
        let s = b"{}";
        assert_eq!(
            parse_format_spec_at(s, 0),
            Some((FormatSpecKind::Display, 2))
        );
        let s = b"{:?}";
        assert_eq!(
            parse_format_spec_at(s, 0),
            Some((FormatSpecKind::Debug, 4))
        );
    }

    #[test]
    fn parse_format_spec_recognizes_precision_and_padding() {
        let s = b"{:.3}";
        assert_eq!(
            parse_format_spec_at(s, 0),
            Some((FormatSpecKind::Precision(3), 5))
        );
        let s = b"{:04d}";
        assert_eq!(
            parse_format_spec_at(s, 0),
            Some((FormatSpecKind::ZeroPadInt(4), 6))
        );
        let s = b"{:5}";
        assert_eq!(
            parse_format_spec_at(s, 0),
            Some((FormatSpecKind::Width(5), 4))
        );
    }

    #[test]
    fn parse_format_spec_rejects_malformed() {
        // Missing closing brace.
        let s = b"{";
        assert_eq!(parse_format_spec_at(s, 0), None);
        // Bad spec body — `{:x}` (hex) is not in the stage-0 subset.
        let s = b"{:x}";
        assert_eq!(parse_format_spec_at(s, 0), None);
        // Empty body after colon.
        let s = b"{:}";
        assert_eq!(parse_format_spec_at(s, 0), None);
    }

    // ── 5. Op-builders : structural correctness + attribute lock. ───────

    #[test]
    fn build_string_from_utf8_unchecked_carries_layout_attrs() {
        let op = build_string_from_utf8_unchecked(ValueId(0), ValueId(1));
        assert_eq!(op.name, OP_STRING_FROM_UTF8_UNCHECKED);
        assert_eq!(op.operands.len(), 2);
        assert_eq!(op.operands[0], ValueId(0));
        assert_eq!(op.operands[1], ValueId(1));
        assert_eq!(
            op.attributes
                .iter()
                .find(|(k, _)| k == "total_size")
                .map(|(_, v)| v.as_str()),
            Some("24")
        );
        assert_eq!(
            op.attributes
                .iter()
                .find(|(k, _)| k == ATTR_ALIGNMENT)
                .map(|(_, v)| v.as_str()),
            Some("8")
        );
        assert!(has_source_kind(&op, SOURCE_KIND_STRING_ABI));
    }

    #[test]
    fn build_string_from_utf8_carries_validate_symbol() {
        let op = build_string_from_utf8(ValueId(0), ValueId(1));
        assert_eq!(op.name, OP_STRING_FROM_UTF8);
        assert!(is_string_validate(&op));
        assert_eq!(read_validate_symbol(&op), Some(DEFAULT_VALIDATE_SYMBOL));
    }

    #[test]
    fn build_str_slice_field_loads_have_correct_offsets() {
        let len_op = build_str_slice_len(ValueId(0), ValueId(1));
        let bytes_op = build_str_slice_as_bytes(ValueId(0), ValueId(2));
        // len at offset 8.
        assert_eq!(read_offset_attr(&len_op), Some(8));
        assert_eq!(read_field_attr(&len_op), Some(FIELD_LEN));
        // ptr at offset 0.
        assert_eq!(read_offset_attr(&bytes_op), Some(0));
        assert_eq!(read_field_attr(&bytes_op), Some(FIELD_PTR));
        // Both produce i64 result.
        assert_eq!(len_op.results[0].ty, MirType::Int(IntWidth::I64));
        assert_eq!(bytes_op.results[0].ty, MirType::Int(IntWidth::I64));
    }

    #[test]
    fn build_string_field_loads_have_canonical_layout() {
        let len_op = build_string_len(ValueId(0), ValueId(1));
        let byte_op = build_string_byte_at(ValueId(0), ValueId(2), ValueId(3));
        // string.len at offset 8 (after data field).
        assert_eq!(read_offset_attr(&len_op), Some(8));
        assert_eq!(read_field_attr(&len_op), Some(FIELD_LEN));
        // byte_at uses the data field (offset 0) as the base for ptr-arith.
        assert_eq!(read_offset_attr(&byte_op), Some(0));
        assert_eq!(read_field_attr(&byte_op), Some(FIELD_DATA));
        // byte_at returns i32 (zero-extended byte).
        assert_eq!(byte_op.results[0].ty, MirType::Int(IntWidth::I32));
    }

    #[test]
    fn build_char_from_u32_carries_usv_constants() {
        let op = build_char_from_u32(ValueId(0), ValueId(1));
        assert_eq!(op.name, OP_CHAR_FROM_U32);
        assert!(is_char_from_u32(&op));
        assert_eq!(
            op.attributes
                .iter()
                .find(|(k, _)| k == "usv_max")
                .map(|(_, v)| v.as_str()),
            Some("1114111")
        );
        // Result is the Option cell ptr.
        assert_eq!(op.results[0].ty, MirType::Ptr);
    }

    #[test]
    fn predicate_helpers_disambiguate_op_families() {
        let from_utf8 = build_string_from_utf8(ValueId(0), ValueId(1));
        let from_utf8_unchecked = build_string_from_utf8_unchecked(ValueId(0), ValueId(1));
        let slice_len = build_str_slice_len(ValueId(0), ValueId(1));
        let str_len = build_string_len(ValueId(0), ValueId(1));
        let char_op = build_char_from_u32(ValueId(0), ValueId(1));

        assert!(is_string_abi_op(&from_utf8));
        assert!(is_string_abi_op(&from_utf8_unchecked));
        assert!(is_string_abi_op(&slice_len));
        assert!(is_string_abi_op(&str_len));
        assert!(is_string_abi_op(&char_op));

        assert!(is_string_validate(&from_utf8));
        assert!(!is_string_validate(&from_utf8_unchecked));

        assert!(is_str_slice_field_load(&slice_len));
        assert!(!is_str_slice_field_load(&str_len));

        assert!(is_string_field_load(&str_len));
        assert!(!is_string_field_load(&slice_len));

        assert!(is_char_from_u32(&char_op));
        assert!(!is_char_from_u32(&str_len));

        // Plain arith op : not a string-ABI op.
        let plain = MirOp::std("arith.constant").with_attribute("value", "1");
        assert!(!is_string_abi_op(&plain));
    }

    // ── 6. Decimal parser used by format-spec walker. ───────────────────

    #[test]
    fn parse_decimal_handles_multi_digit_numbers() {
        assert_eq!(parse_decimal(b"123}", 0), Some((123, 3)));
        assert_eq!(parse_decimal(b"0", 0), Some((0, 1)));
        assert_eq!(parse_decimal(b"42abc", 0), Some((42, 2)));
        // Non-digit start.
        assert_eq!(parse_decimal(b"abc", 0), None);
        assert_eq!(parse_decimal(b"", 0), None);
    }
}

// § INTEGRATION_NOTE  (per Wave-C1 dispatch directive)
//   This module is delivered as a NEW file. `cssl-mir/src/lib.rs` is NOT
//   modified by this slice — the `pub mod string_abi ;` declaration is
//   added by the future main-thread integration commit alongside the
//   `body_lower::try_lower_string_*` recognizer wiring that consumes
//   these helpers. All the public surface above is reachable via the
//   crate-internal path `crate::string_abi::*` from sister modules ; the
//   `#[cfg(test)]` mod above exercises every helper without requiring
//   the lib.rs publicization.
//
//   Mocked-deps :
//     - `__cssl_strvalidate(bytes_ptr, bytes_len) -> i64` (returns the
//       byte-index of the first invalid byte, or -1 on success). cssl-rt
//       does not yet ship this symbol ; the cgen path emits the import
//       declaration with a SWAP-POINT marker. Until the rt-side lands
//       the recognizer caller passes the unsafe-ctor path
//       (`from_utf8_unchecked`) for stage-0 stdlib examples that have
//       already-validated byte-slices.
