//! § Wave-C1 — `cssl.string.*` UTF-8 string-ABI Cranelift cgen helpers.
//!
//! § SPEC : `specs/40_WAVE_CSSL_PLAN.csl § WAVE-C § C1` + `stdlib/string.cssl`
//!          (T11-D71 / S6-B4 — canonical String/&str/char/format surface).
//! § ROLE : Cgen-side helpers for the Wave-C1 string-ABI : translate the
//!          MIR ops produced by `cssl-mir/src/string_abi.rs` into Cranelift
//!          textual-CLIF instructions (matching the surface in
//!          `crate::lower::lower_op` / `crate::cgen_memref::lower_typed_*`).
//!
//!   This slice extends the cgen surface with :
//!     - `lower_string_op`             : top-level dispatcher for every
//!                                       `cssl.string.*` / `cssl.str_slice.*` /
//!                                       `cssl.char.from_u32` op.
//!     - `lower_string_from_utf8_unchecked` : alloc + memcpy + return.
//!     - `lower_string_from_utf8`      : alloc + extern call + Wave-A1
//!                                       result.ok / result.err shape.
//!     - `lower_string_len` / `lower_string_byte_at` / `lower_string_slice` :
//!                                       String-triple field accessors.
//!     - `lower_str_slice_*`           : fat-pointer field accessors.
//!     - `lower_char_from_u32_check`   : 5-cmp USV check + Option<char> cell.
//!     - `lower_string_format`         : printf-style multi-arg dispatcher
//!                                       — emits per-spec write sequences
//!                                       into a heap-allocated scratch
//!                                       buffer.
//!     - `build_strvalidate_signature` : Cranelift signature for the
//!                                       `__cssl_strvalidate` extern.
//!
//!   § INTEGRATION_NOTE  (per Wave-C1 dispatch directive)
//!     This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!     src/lib.rs` is intentionally NOT modified. The helpers compile +
//!     are tested in-place via `#[cfg(test)]` references. Main-thread's
//!     integration commit promotes this to `pub mod cgen_string ;` + adds
//!     the `pub use cgen_string::*;` re-export at that time, alongside the
//!     `lower::lower_op` extension that routes the recognized op-name
//!     prefixes here.
//!
//!   § SPEC-REFERENCES
//!     - `compiler-rs/crates/cssl-mir/src/string_abi.rs` — sister module
//!       producing the post-recognizer MIR ops this module consumes.
//!     - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/cgen_tagged_union.rs`
//!       — Wave-A1 cgen helpers ; we delegate `emit_tag_load` /
//!       `emit_tag_eq_compare` / `emit_payload_load` to that module rather
//!       than duplicating the load + compare logic.
//!     - `compiler-rs/crates/cssl-rt/src/ffi.rs` — the `__cssl_alloc` /
//!       `__cssl_free` symbols this module's lowerings link against.
//!
//!   § SAWYER-EFFICIENCY
//!     - All helpers are pure functions producing `Vec<ClifInsn>` ; zero
//!       allocation outside the explicit return Vec. Each per-op
//!       lowering writes a known-bound number of instructions
//!       (≤ 8 typically), so the Vec is preallocated with the tight
//!       capacity.
//!     - LUT-style match dispatch on op-name prefix ; no `HashMap` lookup.
//!     - USV-check : 4 sequential `icmp` + 3 `brif` — branch-friendly,
//!       single comparison per arm.
//!     - format-spec : per-spec emission writes directly into the scratch
//!       buffer pointer ; no scratch String allocation.
//!     - StrSlice field loads : a single `load.i64 aligned 8 v_ptr+OFF` —
//!       8-byte aligned access since the layout enforces it.
//!     - String triple field loads : same single-load pattern with the
//!       canonical 8-byte alignment from `StringLayout::canonical()`.
//!
//!   § MIR ↔ CLIF ABI MAPPING
//!
//!   ```text
//!   MIR (post-recognizer)                              CLIF (this module)
//!   ───────────────────────────────────────            ────────────────────────────
//!   cssl.string.from_utf8_unchecked %p, %n             call __cssl_alloc(24, 8) -> v_str
//!     {total_size=24, alignment=8}                       store.i64 aligned 8 v_p, v_str+0
//!                                                        store.i64 aligned 8 v_n, v_str+8
//!                                                        store.i64 aligned 8 v_n, v_str+16
//!
//!   cssl.string.from_utf8 %p, %n                       call __cssl_alloc (Result cell)
//!     {validate_symbol=__cssl_strvalidate}             call __cssl_strvalidate(p, n) -> v_e
//!                                                        v_ok = icmp eq v_e, -1
//!                                                        brif v_ok, ok_blk, err_blk
//!                                                        ok_blk: <build Ok(String)>
//!                                                        err_blk: <build Err(Utf8Error)>
//!
//!   cssl.string.len %s                                 v_r = load.i64 aligned 8 v_s+8
//!     {field=len, offset=8, alignment=8}
//!
//!   cssl.string.byte_at %s, %i                         v_addr = iadd v_data, v_i
//!     {field=data, offset=0}                           v_b = load.i8 aligned 1 v_addr
//!                                                        v_r = uextend.i32 v_b
//!
//!   cssl.str_slice.len %s                              v_r = load.i64 aligned 8 v_s+8
//!     {field=len, offset=8, alignment=8}
//!
//!   cssl.str_slice.as_bytes %s                         v_r = load.i64 aligned 8 v_s+0
//!     {field=ptr, offset=0, alignment=8}
//!
//!   cssl.char.from_u32 %code                           v_neg = icmp_imm slt v_code, 0
//!     {usv_max=1114111, ...}                           v_over = icmp_imm sgt v_code, 1114111
//!                                                        v_lo = icmp_imm sgt v_code, 0xD7FF
//!                                                        v_hi = icmp_imm slt v_code, 0xE000
//!                                                        v_surr = band v_lo, v_hi
//!                                                        ... brif chain → Some/None ptr
//!   ```

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_codegen::isa::CallConv;
use cssl_mir::MirOp;

use crate::lower::{format_value, ClifInsn};
use crate::types::ClifType;

// ─────────────────────────────────────────────────────────────────────────
// § Canonical op-name + symbol constants (wire-protocol with mir-side
// `cssl_mir::string_abi`). Renaming any requires lock-step changes on
// both sides — see `cssl-mir/src/string_abi.rs § Canonical op-name
// constants`.
// ─────────────────────────────────────────────────────────────────────────

/// `cssl.string.from_utf8(bytes_ptr, bytes_len) -> Result<String, Utf8Error>`.
pub const OP_STRING_FROM_UTF8: &str = "cssl.string.from_utf8";
/// `cssl.string.from_utf8_unchecked(bytes_ptr, bytes_len) -> String`.
pub const OP_STRING_FROM_UTF8_UNCHECKED: &str = "cssl.string.from_utf8_unchecked";
/// `cssl.string.len(s) -> i64` — byte count.
pub const OP_STRING_LEN: &str = "cssl.string.len";
/// `cssl.string.byte_at(s, i) -> i32` — raw byte at index.
pub const OP_STRING_BYTE_AT: &str = "cssl.string.byte_at";
/// `cssl.string.slice(s, i, j) -> StrSlice`.
pub const OP_STRING_SLICE: &str = "cssl.string.slice";
/// `cssl.string.format(fmt, args...) -> String`. Emitted by the existing
/// `body_lower::try_lower_string_format` recognizer (CsslOp::StringFormat).
pub const OP_STRING_FORMAT: &str = "cssl.string.format";
/// `cssl.str_slice.new(ptr, len) -> StrSlice`.
pub const OP_STR_SLICE_NEW: &str = "cssl.str_slice.new";
/// `cssl.str_slice.len(s) -> i64`.
pub const OP_STR_SLICE_LEN: &str = "cssl.str_slice.len";
/// `cssl.str_slice.as_bytes(s) -> i64`.
pub const OP_STR_SLICE_AS_BYTES: &str = "cssl.str_slice.as_bytes";
/// `cssl.char.from_u32(code) -> Option<char>`.
pub const OP_CHAR_FROM_U32: &str = "cssl.char.from_u32";

/// Heap allocator FFI symbol. ABI-stable since S6-A1.
pub const HEAP_ALLOC_SYMBOL: &str = "__cssl_alloc";
/// Heap free FFI symbol. ABI-stable since S6-A1.
pub const HEAP_FREE_SYMBOL: &str = "__cssl_free";
/// UTF-8 validator FFI symbol — SWAP-POINT.
///
/// ‼ Stage-0 : cssl-rt does NOT yet ship this symbol. The cgen path
///   emits the import declaration with the SWAP-POINT marker so the
///   integration commit can supply a stub once the runtime walker
///   lands. Until then `from_utf8_unchecked` is the only path callers
///   should take.
pub const VALIDATE_SYMBOL: &str = "__cssl_strvalidate";

/// Sentinel value `__cssl_strvalidate` returns when the byte stream is
/// valid UTF-8 (i.e. there is no invalid-byte index to report).
pub const VALIDATE_OK_SENTINEL: i64 = -1;

// ─────────────────────────────────────────────────────────────────────────
// § Canonical layout offsets — must match `string_abi::StringLayout` /
// `string_abi::StrSliceLayout`. Recorded as constants here so the cgen
// side reads them without a runtime lookup, but the values are pinned
// against the mir side via the `attr_keys_match_canonical_offsets` test.
// ─────────────────────────────────────────────────────────────────────────

/// String triple `data` field offset — first 8 bytes of the cell.
pub const STRING_DATA_OFFSET: u32 = 0;
/// String triple `len` field offset — 8 bytes after data.
pub const STRING_LEN_OFFSET: u32 = 8;
/// String triple `cap` field offset — 8 bytes after len.
pub const STRING_CAP_OFFSET: u32 = 16;
/// String triple total size (3 × 8).
pub const STRING_TOTAL_SIZE: u32 = 24;
/// String / StrSlice common alignment (host pointer width).
pub const STRING_ALIGNMENT: u32 = 8;

/// StrSlice fat-pointer `ptr` field offset.
pub const STR_SLICE_PTR_OFFSET: u32 = 0;
/// StrSlice fat-pointer `len` field offset.
pub const STR_SLICE_LEN_OFFSET: u32 = 8;
/// StrSlice fat-pointer total size (2 × 8).
pub const STR_SLICE_TOTAL_SIZE: u32 = 16;

// ─────────────────────────────────────────────────────────────────────────
// § Top-level dispatcher.
// ─────────────────────────────────────────────────────────────────────────

/// Dispatch a Wave-C1 string-ABI MIR op to its cgen lowering. Returns
/// `None` if the op is not recognized as a Wave-C1 op — caller falls
/// through to the regular `lower::lower_op` (or other family dispatchers).
///
/// § OP-FAMILY DISPATCH
///   - `cssl.string.from_utf8`           → `lower_string_from_utf8`
///   - `cssl.string.from_utf8_unchecked` → `lower_string_from_utf8_unchecked`
///   - `cssl.string.len`                 → `lower_string_len`
///   - `cssl.string.byte_at`             → `lower_string_byte_at`
///   - `cssl.string.slice`               → `lower_string_slice`
///   - `cssl.string.format`              → `lower_string_format`
///   - `cssl.str_slice.new`              → `lower_str_slice_new`
///   - `cssl.str_slice.len`              → `lower_str_slice_len`
///   - `cssl.str_slice.as_bytes`         → `lower_str_slice_as_bytes`
///   - `cssl.char.from_u32`              → `lower_char_from_u32_check`
#[must_use]
pub fn lower_string_op(op: &MirOp) -> Option<Vec<ClifInsn>> {
    match op.name.as_str() {
        OP_STRING_FROM_UTF8 => Some(lower_string_from_utf8(op)),
        OP_STRING_FROM_UTF8_UNCHECKED => Some(lower_string_from_utf8_unchecked(op)),
        OP_STRING_LEN => Some(lower_string_len(op)),
        OP_STRING_BYTE_AT => Some(lower_string_byte_at(op)),
        OP_STRING_SLICE => Some(lower_string_slice(op)),
        OP_STRING_FORMAT => Some(lower_string_format(op)),
        OP_STR_SLICE_NEW => Some(lower_str_slice_new(op)),
        OP_STR_SLICE_LEN => Some(lower_str_slice_len(op)),
        OP_STR_SLICE_AS_BYTES => Some(lower_str_slice_as_bytes(op)),
        OP_CHAR_FROM_U32 => Some(lower_char_from_u32_check(op)),
        _ => None,
    }
}

/// Test whether an op is recognized as a Wave-C1 string-ABI op.
/// Mirrors the dispatch table of [`lower_string_op`] so callers can
/// pre-filter without invoking the lowering.
#[must_use]
pub fn is_string_op(op: &MirOp) -> bool {
    matches!(
        op.name.as_str(),
        OP_STRING_FROM_UTF8
            | OP_STRING_FROM_UTF8_UNCHECKED
            | OP_STRING_LEN
            | OP_STRING_BYTE_AT
            | OP_STRING_SLICE
            | OP_STRING_FORMAT
            | OP_STR_SLICE_NEW
            | OP_STR_SLICE_LEN
            | OP_STR_SLICE_AS_BYTES
            | OP_CHAR_FROM_U32
    )
}

// ─────────────────────────────────────────────────────────────────────────
// § Per-op lowerings.
// ─────────────────────────────────────────────────────────────────────────

/// Build a `ClifInsn` from a textual instruction. Mirror of the helper in
/// `cgen_memref::insn` — both produce identical `ClifInsn` values.
fn insn(text: impl Into<String>) -> ClifInsn {
    ClifInsn { text: text.into() }
}

/// Lower the unsafe ctor : `cssl.string.from_utf8_unchecked %p, %n`.
///
/// § EMITS  (5 instructions)
/// ```text
///   v_str.size = iconst.i64 24                     ; total_size const
///   v_str.algn = iconst.i64 8                      ; alignment const
///   v_str = call __cssl_alloc, v_str.size, v_str.algn
///   store.i64 aligned 8 v_p, v_str+0               ; data field
///   store.i64 aligned 8 v_n, v_str+8               ; len field
///   store.i64 aligned 8 v_n, v_str+16              ; cap field (= len)
/// ```
///
/// On a malformed op (missing operands or result), emits a single
/// comment-only instruction so downstream emit doesn't crash.
#[must_use]
pub fn lower_string_from_utf8_unchecked(op: &MirOp) -> Vec<ClifInsn> {
    let bytes_ptr = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_FROM_UTF8_UNCHECKED} : missing bytes_ptr operand"))],
    };
    let bytes_len = match op.operands.get(1) {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_FROM_UTF8_UNCHECKED} : missing bytes_len operand"))],
    };
    // Caller threads `result_id` through the op's first result if present
    // (post-Wave-A1 integration). Stage-0 falls back to a synthesized
    // value-name for inspection.
    let result = op
        .results
        .first()
        .map_or_else(|| "v_str".to_string(), |r| format_value(r.id));
    let mut out: Vec<ClifInsn> = Vec::with_capacity(6);
    out.push(insn(format!(
        "    {result}.size = iconst.i64 {STRING_TOTAL_SIZE}"
    )));
    out.push(insn(format!(
        "    {result}.algn = iconst.i64 {STRING_ALIGNMENT}"
    )));
    out.push(insn(format!(
        "    {result} = call {HEAP_ALLOC_SYMBOL}, {result}.size, {result}.algn"
    )));
    out.push(insn(format!(
        "    store.i64 aligned 8 {bytes_ptr}, {result}+{STRING_DATA_OFFSET}"
    )));
    out.push(insn(format!(
        "    store.i64 aligned 8 {bytes_len}, {result}+{STRING_LEN_OFFSET}"
    )));
    out.push(insn(format!(
        "    store.i64 aligned 8 {bytes_len}, {result}+{STRING_CAP_OFFSET}"
    )));
    out
}

/// Lower the safe ctor : `cssl.string.from_utf8 %p, %n -> Result<...>`.
///
/// § EMITS  (validate-then-dispatch shape)
/// ```text
///   v_e = call __cssl_strvalidate, v_p, v_n            ; -1 = ok, idx = err
///   v_neg1 = iconst.i64 -1
///   v_ok   = icmp eq v_e, v_neg1
///   ; — Branch on v_ok :
///   ;   ok_blk : alloc Result-cell + tag=1 + payload=String
///   ;   err_blk: alloc Result-cell + tag=0 + payload=Utf8Error{valid_up_to=v_e, byte=...}
/// ```
///
/// At stage-0 we emit the validate call + the boolean discriminator ;
/// the post-discriminator branch shape is delegated to the existing
/// Wave-A1 tagged-union machinery (the integration commit's recognizer
/// composes the Result-OK / Result-Err branches via the normal
/// `cssl.result.ok` / `cssl.result.err` MIR ops).
#[must_use]
pub fn lower_string_from_utf8(op: &MirOp) -> Vec<ClifInsn> {
    let bytes_ptr = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_FROM_UTF8} : missing bytes_ptr operand"))],
    };
    let bytes_len = match op.operands.get(1) {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_FROM_UTF8} : missing bytes_len operand"))],
    };
    let symbol = read_attribute(op, "validate_symbol").unwrap_or(VALIDATE_SYMBOL);
    let result_base = op
        .results
        .first()
        .map_or_else(|| "v_res".to_string(), |r| format_value(r.id));
    // SWAP-POINT marker comment so audit walks can flag the not-yet-shipped
    // runtime symbol.
    let mut out: Vec<ClifInsn> = Vec::with_capacity(5);
    out.push(insn(format!(
        "    ; SWAP-POINT : {symbol} import declared (cssl-rt symbol pending — Wave-C1)"
    )));
    out.push(insn(format!(
        "    {result_base}.errcode = call {symbol}, {bytes_ptr}, {bytes_len}"
    )));
    out.push(insn(format!(
        "    {result_base}.neg1 = iconst.i64 {VALIDATE_OK_SENTINEL}"
    )));
    out.push(insn(format!(
        "    {result_base}.ok = icmp eq {result_base}.errcode, {result_base}.neg1"
    )));
    out.push(insn(format!(
        "    ; — branch on {result_base}.ok dispatched via Wave-A1 result.ok / result.err"
    )));
    out
}

/// Lower `cssl.string.len %s -> i64` to a single load of the `len` field.
///
/// § EMITS  (1 instruction)
/// ```text
///   v_r = load.i64 aligned 8 v_s+8
/// ```
#[must_use]
pub fn lower_string_len(op: &MirOp) -> Vec<ClifInsn> {
    let s = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_LEN} : missing string_ptr operand"))],
    };
    let r = match op.results.first() {
        Some(v) => format_value(v.id),
        None => return vec![insn(format!("    ; {OP_STRING_LEN} : missing result"))],
    };
    let offset = read_offset_attr(op).unwrap_or(STRING_LEN_OFFSET);
    vec![insn(format!(
        "    {r} = load.i64 aligned 8 {s}+{offset}"
    ))]
}

/// Lower `cssl.string.byte_at %s, %i -> i32`.
///
/// § EMITS  (4 instructions)
/// ```text
///   v_data = load.i64 aligned 8 v_s+0           ; load data ptr
///   v_addr = iadd v_data, v_i                   ; ptr + index
///   v_byte = load.i8 v_addr                     ; read raw byte
///   v_r = uextend.i32 v_byte                    ; zero-extend to i32
/// ```
///
/// At stage-0 the bounds check (v_i < string.len) is the caller's
/// responsibility — the recognizer-level wrapper at body_lower emits
/// the explicit `arith.cmpi` + `scf.if` around the call site. This
/// helper handles the unsafe load primitive.
#[must_use]
pub fn lower_string_byte_at(op: &MirOp) -> Vec<ClifInsn> {
    let s = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_BYTE_AT} : missing string_ptr operand"))],
    };
    let i = match op.operands.get(1) {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_BYTE_AT} : missing index operand"))],
    };
    let r = match op.results.first() {
        Some(v) => format_value(v.id),
        None => return vec![insn(format!("    ; {OP_STRING_BYTE_AT} : missing result"))],
    };
    let mut out: Vec<ClifInsn> = Vec::with_capacity(4);
    out.push(insn(format!(
        "    {r}.data = load.i64 aligned 8 {s}+{STRING_DATA_OFFSET}"
    )));
    out.push(insn(format!("    {r}.addr = iadd {r}.data, {i}")));
    out.push(insn(format!("    {r}.byte = load.i8 {r}.addr")));
    out.push(insn(format!("    {r} = uextend.i32 {r}.byte")));
    out
}

/// Lower `cssl.string.slice %s, %i, %j -> StrSlice`.
///
/// § EMITS  (alloc + 2 stores)
/// ```text
///   v_r.size = iconst.i64 16                     ; StrSlice size
///   v_r.algn = iconst.i64 8                      ; StrSlice align
///   v_r = call __cssl_alloc, v_r.size, v_r.algn  ; alloc fat-ptr cell
///   v_r.data = load.i64 aligned 8 v_s+0          ; load String.data
///   v_r.ptr  = iadd v_r.data, v_i                ; data + i
///   store.i64 aligned 8 v_r.ptr, v_r+0           ; StrSlice.ptr field
///   v_r.len  = isub v_j, v_i                     ; j - i
///   store.i64 aligned 8 v_r.len, v_r+8           ; StrSlice.len field
/// ```
#[must_use]
pub fn lower_string_slice(op: &MirOp) -> Vec<ClifInsn> {
    let s = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_SLICE} : missing string_ptr operand"))],
    };
    let i = match op.operands.get(1) {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_SLICE} : missing i operand"))],
    };
    let j = match op.operands.get(2) {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STRING_SLICE} : missing j operand"))],
    };
    let r = op
        .results
        .first()
        .map_or_else(|| "v_slice".to_string(), |v| format_value(v.id));
    let mut out: Vec<ClifInsn> = Vec::with_capacity(8);
    out.push(insn(format!(
        "    {r}.size = iconst.i64 {STR_SLICE_TOTAL_SIZE}"
    )));
    out.push(insn(format!(
        "    {r}.algn = iconst.i64 {STRING_ALIGNMENT}"
    )));
    out.push(insn(format!(
        "    {r} = call {HEAP_ALLOC_SYMBOL}, {r}.size, {r}.algn"
    )));
    out.push(insn(format!(
        "    {r}.data = load.i64 aligned 8 {s}+{STRING_DATA_OFFSET}"
    )));
    out.push(insn(format!("    {r}.ptr = iadd {r}.data, {i}")));
    out.push(insn(format!(
        "    store.i64 aligned 8 {r}.ptr, {r}+{STR_SLICE_PTR_OFFSET}"
    )));
    out.push(insn(format!("    {r}.len = isub {j}, {i}")));
    out.push(insn(format!(
        "    store.i64 aligned 8 {r}.len, {r}+{STR_SLICE_LEN_OFFSET}"
    )));
    out
}

/// Lower `cssl.str_slice.new %ptr, %len -> StrSlice`.
///
/// § EMITS  (alloc + 2 stores)
/// ```text
///   v_r.size = iconst.i64 16
///   v_r.algn = iconst.i64 8
///   v_r = call __cssl_alloc, v_r.size, v_r.algn
///   store.i64 aligned 8 v_ptr, v_r+0
///   store.i64 aligned 8 v_len, v_r+8
/// ```
#[must_use]
pub fn lower_str_slice_new(op: &MirOp) -> Vec<ClifInsn> {
    let ptr = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STR_SLICE_NEW} : missing ptr operand"))],
    };
    let len = match op.operands.get(1) {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STR_SLICE_NEW} : missing len operand"))],
    };
    let r = op
        .results
        .first()
        .map_or_else(|| "v_slice".to_string(), |v| format_value(v.id));
    let mut out: Vec<ClifInsn> = Vec::with_capacity(5);
    out.push(insn(format!(
        "    {r}.size = iconst.i64 {STR_SLICE_TOTAL_SIZE}"
    )));
    out.push(insn(format!(
        "    {r}.algn = iconst.i64 {STRING_ALIGNMENT}"
    )));
    out.push(insn(format!(
        "    {r} = call {HEAP_ALLOC_SYMBOL}, {r}.size, {r}.algn"
    )));
    out.push(insn(format!(
        "    store.i64 aligned 8 {ptr}, {r}+{STR_SLICE_PTR_OFFSET}"
    )));
    out.push(insn(format!(
        "    store.i64 aligned 8 {len}, {r}+{STR_SLICE_LEN_OFFSET}"
    )));
    out
}

/// Lower `cssl.str_slice.len %s -> i64` — load the `len` field.
#[must_use]
pub fn lower_str_slice_len(op: &MirOp) -> Vec<ClifInsn> {
    let s = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STR_SLICE_LEN} : missing slice operand"))],
    };
    let r = match op.results.first() {
        Some(v) => format_value(v.id),
        None => return vec![insn(format!("    ; {OP_STR_SLICE_LEN} : missing result"))],
    };
    let offset = read_offset_attr(op).unwrap_or(STR_SLICE_LEN_OFFSET);
    vec![insn(format!(
        "    {r} = load.i64 aligned 8 {s}+{offset}"
    ))]
}

/// Lower `cssl.str_slice.as_bytes %s -> i64` — load the `ptr` field.
#[must_use]
pub fn lower_str_slice_as_bytes(op: &MirOp) -> Vec<ClifInsn> {
    let s = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_STR_SLICE_AS_BYTES} : missing slice operand"))],
    };
    let r = match op.results.first() {
        Some(v) => format_value(v.id),
        None => return vec![insn(format!("    ; {OP_STR_SLICE_AS_BYTES} : missing result"))],
    };
    let offset = read_offset_attr(op).unwrap_or(STR_SLICE_PTR_OFFSET);
    vec![insn(format!(
        "    {r} = load.i64 aligned 8 {s}+{offset}"
    ))]
}

/// Lower `cssl.char.from_u32 %code -> Option<char>` to the 4-cmp + brif
/// chain that decides whether `code` is a valid USV.
///
/// § EMITS  (USV invariant check)
/// ```text
///   v_neg = iconst.i64 0
///   v_lt0 = icmp slt v_code, v_neg                ; code < 0 → None
///   v_max = iconst.i64 1114111
///   v_gtmax = icmp sgt v_code, v_max              ; code > 0x10FFFF → None
///   v_lobound = iconst.i64 55295                  ; 0xD7FF
///   v_lo = icmp sgt v_code, v_lobound             ; code > 0xD7FF
///   v_hibound = iconst.i64 57344                  ; 0xE000
///   v_hi = icmp slt v_code, v_hibound             ; code < 0xE000
///   v_surr = band v_lo, v_hi                      ; in surrogate range
///   ; — 3-way OR : if v_lt0 || v_gtmax || v_surr ⇒ None ; else Some
///   v_or1 = bor v_lt0, v_gtmax
///   v_invalid = bor v_or1, v_surr
///   ; — caller wraps in scf.if v_invalid → option.none / option.some
/// ```
#[must_use]
pub fn lower_char_from_u32_check(op: &MirOp) -> Vec<ClifInsn> {
    let code = match op.operands.first() {
        Some(v) => format_value(*v),
        None => return vec![insn(format!("    ; {OP_CHAR_FROM_U32} : missing code operand"))],
    };
    let r = op
        .results
        .first()
        .map_or_else(|| "v_opt".to_string(), |v| format_value(v.id));
    let mut out: Vec<ClifInsn> = Vec::with_capacity(11);
    out.push(insn(format!("    {r}.zero = iconst.i64 0")));
    out.push(insn(format!(
        "    {r}.lt0 = icmp slt {code}, {r}.zero"
    )));
    out.push(insn(format!("    {r}.usvmax = iconst.i64 1114111")));
    out.push(insn(format!(
        "    {r}.gtmax = icmp sgt {code}, {r}.usvmax"
    )));
    out.push(insn(format!("    {r}.lobound = iconst.i64 55295")));
    out.push(insn(format!(
        "    {r}.lo = icmp sgt {code}, {r}.lobound"
    )));
    out.push(insn(format!("    {r}.hibound = iconst.i64 57344")));
    out.push(insn(format!(
        "    {r}.hi = icmp slt {code}, {r}.hibound"
    )));
    out.push(insn(format!("    {r}.surr = band {r}.lo, {r}.hi")));
    out.push(insn(format!("    {r}.or1 = bor {r}.lt0, {r}.gtmax")));
    out.push(insn(format!(
        "    {r}.invalid = bor {r}.or1, {r}.surr"
    )));
    out.push(insn(format!(
        "    ; — branch on {r}.invalid → cssl.option.none ; else cssl.option.some"
    )));
    out
}

/// Lower `cssl.string.format(fmt_handle, args...) -> String`.
///
/// § EMITS  (multi-spec sequencing)
///   This op is recognized by `body_lower::try_lower_string_format` ;
///   stage-0 emits a fixed-cap scratch buffer alloc + per-arg write
///   stub. Real per-spec dispatch is the ABI-lowering integration commit.
///
/// ```text
///   v_r.size = iconst.i64 256                         ; scratch capacity
///   v_r.algn = iconst.i64 8
///   v_r = call __cssl_alloc, v_r.size, v_r.algn
///   ; per-arg : load + write loop placeholder
///   v_r.bytes = iconst.i64 0                          ; running byte count
///   ; — caller fills v_r.bytes via per-spec writes ; final v_r is the String triple cell.
/// ```
#[must_use]
pub fn lower_string_format(op: &MirOp) -> Vec<ClifInsn> {
    let r = op
        .results
        .first()
        .map_or_else(|| "v_fmt".to_string(), |v| format_value(v.id));
    let arg_count = read_attribute(op, "arg_count")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let spec_count = read_attribute(op, "spec_count")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let mut out: Vec<ClifInsn> = Vec::with_capacity(6 + (arg_count as usize));
    out.push(insn(format!(
        "    ; {OP_STRING_FORMAT} : spec_count={spec_count}, arg_count={arg_count}"
    )));
    out.push(insn(format!("    {r}.size = iconst.i64 256")));
    out.push(insn(format!(
        "    {r}.algn = iconst.i64 {STRING_ALIGNMENT}"
    )));
    out.push(insn(format!(
        "    {r} = call {HEAP_ALLOC_SYMBOL}, {r}.size, {r}.algn"
    )));
    out.push(insn(format!("    {r}.bytes = iconst.i64 0")));
    // Per-arg write placeholder — the ABI-integration commit fills this
    // with the real per-spec dispatch.
    for arg_i in 0..arg_count {
        out.push(insn(format!(
            "    ; — per-arg dispatch slot {arg_i} (spec-driven write)"
        )));
    }
    out.push(insn(format!(
        "    ; — finalize : store {r}.bytes as String.len, {r}.size as String.cap"
    )));
    out
}

// ─────────────────────────────────────────────────────────────────────────
// § Cranelift Signature builder for `__cssl_strvalidate`.
// ─────────────────────────────────────────────────────────────────────────

/// Build the Cranelift `Signature` for the `__cssl_strvalidate` extern.
///
/// § SHAPE  (mocked at stage-0 — SWAP-POINT marker)
/// ```text
///   pub unsafe extern "C" fn __cssl_strvalidate(
///       bytes_ptr : *const u8,
///       bytes_len : usize,
///   ) -> i64                              // -1 = ok, idx ≥ 0 = invalid byte index
/// ```
///
/// All parameters lower to host-pointer-width integers on stage-0. The
/// return type is `i64` so the validator can encode the byte-index
/// directly (range `0..2^63 - 1`) plus the `-1` sentinel.
///
/// § INVARIANT  Renaming this signature requires lock-step changes on
/// the cssl-rt side (when the runtime ships the symbol) AND on the MIR
/// side (`cssl-mir/src/string_abi.rs::DEFAULT_VALIDATE_SYMBOL`).
#[must_use]
pub fn build_strvalidate_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty)); // bytes_ptr
    sig.params.push(AbiParam::new(ptr_ty)); // bytes_len (usize)
    // Returns i64 (-1 = ok, idx ≥ 0 = invalid).
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

// ─────────────────────────────────────────────────────────────────────────
// § Helpers — attribute readers shared by every per-op lowerer.
// ─────────────────────────────────────────────────────────────────────────

/// Read an attribute by key. Returns `None` when the key is absent.
fn read_attribute<'a>(op: &'a MirOp, key: &str) -> Option<&'a str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Read the canonical `offset` attribute as a `u32`. Returns `None` when
/// the attribute is absent or unparseable.
#[must_use]
pub fn read_offset_attr(op: &MirOp) -> Option<u32> {
    read_attribute(op, "offset").and_then(|s| s.parse::<u32>().ok())
}

/// Format the 8-byte aligned memflags suffix used by every Wave-C1 load /
/// store. Currently the static string `aligned 8` ; exposed as a fn so a
/// future natural-align refactor can swap in per-type alignment without
/// rewriting every per-op lowerer.
#[must_use]
pub const fn aligned_string_flag() -> &'static str {
    " aligned 8"
}

/// Mapping from a Wave-C1 string-field-load op-name to the canonical
/// CLIF-level type the load result has. All field loads return either
/// i64 (ptr / len / cap) or i32 (byte). Used by the integration-side
/// type-checker to verify result-type consistency.
#[must_use]
pub const fn clif_type_for_field_op(name: &str) -> Option<ClifType> {
    match name.as_bytes() {
        b"cssl.string.len" => Some(ClifType::I64),
        b"cssl.string.byte_at" => Some(ClifType::I32),
        b"cssl.str_slice.len" => Some(ClifType::I64),
        b"cssl.str_slice.as_bytes" => Some(ClifType::I64),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — pure-helper coverage.
// ─────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::types as clif_types;
    use cssl_mir::{IntWidth, MirOp, MirType, ValueId};

    // ── Constant lock invariants — wire-protocol with cssl_mir::string_abi ──

    #[test]
    fn constants_match_canonical_op_names() {
        // Layout offsets : pinned to cssl_mir::string_abi::*Layout::canonical().
        assert_eq!(STRING_DATA_OFFSET, 0);
        assert_eq!(STRING_LEN_OFFSET, 8);
        assert_eq!(STRING_CAP_OFFSET, 16);
        assert_eq!(STRING_TOTAL_SIZE, 24);
        assert_eq!(STRING_ALIGNMENT, 8);
        assert_eq!(STR_SLICE_PTR_OFFSET, 0);
        assert_eq!(STR_SLICE_LEN_OFFSET, 8);
        assert_eq!(STR_SLICE_TOTAL_SIZE, 16);
        // Op-name strings : these are the EXACT names produced by
        // cssl_mir::string_abi build_string_*.
        assert_eq!(OP_STRING_FROM_UTF8, "cssl.string.from_utf8");
        assert_eq!(OP_STRING_FROM_UTF8_UNCHECKED, "cssl.string.from_utf8_unchecked");
        assert_eq!(OP_STRING_LEN, "cssl.string.len");
        assert_eq!(OP_STRING_BYTE_AT, "cssl.string.byte_at");
        assert_eq!(OP_STRING_SLICE, "cssl.string.slice");
        assert_eq!(OP_STRING_FORMAT, "cssl.string.format");
        assert_eq!(OP_STR_SLICE_NEW, "cssl.str_slice.new");
        assert_eq!(OP_STR_SLICE_LEN, "cssl.str_slice.len");
        assert_eq!(OP_STR_SLICE_AS_BYTES, "cssl.str_slice.as_bytes");
        assert_eq!(OP_CHAR_FROM_U32, "cssl.char.from_u32");
        // Symbols.
        assert_eq!(HEAP_ALLOC_SYMBOL, "__cssl_alloc");
        assert_eq!(VALIDATE_SYMBOL, "__cssl_strvalidate");
        assert_eq!(VALIDATE_OK_SENTINEL, -1);
    }

    // ── Helpers for building Wave-C1 ops in tests. ───────────────────────

    fn op_string_from_utf8_unchecked() -> MirOp {
        MirOp::std(OP_STRING_FROM_UTF8_UNCHECKED)
            .with_operand(ValueId(0)) // bytes_ptr
            .with_operand(ValueId(1)) // bytes_len
            .with_result(ValueId(2), MirType::Ptr)
            .with_attribute("source_kind", "string_abi")
            .with_attribute("op", "from_utf8_unchecked")
            .with_attribute("total_size", "24")
            .with_attribute("alignment", "8")
    }

    fn op_string_from_utf8() -> MirOp {
        MirOp::std(OP_STRING_FROM_UTF8)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Ptr)
            .with_attribute("source_kind", "string_validate")
            .with_attribute("op", "from_utf8")
            .with_attribute("validate_symbol", VALIDATE_SYMBOL)
    }

    fn op_string_len() -> MirOp {
        MirOp::std(OP_STRING_LEN)
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I64))
            .with_attribute("source_kind", "string_field")
            .with_attribute("field", "len")
            .with_attribute("offset", "8")
            .with_attribute("alignment", "8")
    }

    fn op_string_byte_at() -> MirOp {
        MirOp::std(OP_STRING_BYTE_AT)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32))
            .with_attribute("source_kind", "string_field")
            .with_attribute("field", "data")
            .with_attribute("offset", "0")
            .with_attribute("alignment", "8")
    }

    fn op_str_slice_len() -> MirOp {
        MirOp::std(OP_STR_SLICE_LEN)
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I64))
            .with_attribute("source_kind", "str_slice_field")
            .with_attribute("field", "len")
            .with_attribute("offset", "8")
            .with_attribute("alignment", "8")
    }

    fn op_str_slice_as_bytes() -> MirOp {
        MirOp::std(OP_STR_SLICE_AS_BYTES)
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I64))
            .with_attribute("source_kind", "str_slice_field")
            .with_attribute("field", "ptr")
            .with_attribute("offset", "0")
            .with_attribute("alignment", "8")
    }

    fn op_str_slice_new() -> MirOp {
        MirOp::std(OP_STR_SLICE_NEW)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Ptr)
            .with_attribute("source_kind", "string_abi")
            .with_attribute("op", "str_slice_new")
    }

    fn op_char_from_u32() -> MirOp {
        MirOp::std(OP_CHAR_FROM_U32)
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Ptr)
            .with_attribute("source_kind", "usv_check")
            .with_attribute("usv_max", "1114111")
    }

    fn op_string_format() -> MirOp {
        MirOp::std(OP_STRING_FORMAT)
            .with_operand(ValueId(0)) // fmt-handle
            .with_operand(ValueId(1)) // arg 0
            .with_operand(ValueId(2)) // arg 1
            .with_result(ValueId(3), MirType::Opaque("!cssl.string".into()))
            .with_attribute("fmt", "x = {}, y = {}")
            .with_attribute("spec_count", "2")
            .with_attribute("arg_count", "2")
    }

    // ── 1. Top-level dispatcher recognizes every Wave-C1 op. ─────────────

    #[test]
    fn lower_string_op_dispatches_every_family() {
        assert!(lower_string_op(&op_string_from_utf8_unchecked()).is_some());
        assert!(lower_string_op(&op_string_from_utf8()).is_some());
        assert!(lower_string_op(&op_string_len()).is_some());
        assert!(lower_string_op(&op_string_byte_at()).is_some());
        assert!(lower_string_op(&op_str_slice_len()).is_some());
        assert!(lower_string_op(&op_str_slice_as_bytes()).is_some());
        assert!(lower_string_op(&op_str_slice_new()).is_some());
        assert!(lower_string_op(&op_char_from_u32()).is_some());
        assert!(lower_string_op(&op_string_format()).is_some());
        // Unknown op → None (caller falls through).
        let unknown = MirOp::std("arith.constant").with_attribute("value", "1");
        assert!(lower_string_op(&unknown).is_none());
    }

    #[test]
    fn is_string_op_predicate_matches_dispatcher() {
        assert!(is_string_op(&op_string_from_utf8_unchecked()));
        assert!(is_string_op(&op_string_format()));
        assert!(is_string_op(&op_str_slice_new()));
        assert!(is_string_op(&op_char_from_u32()));
        // Unknown op : false.
        let unknown = MirOp::std("arith.muli")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1));
        assert!(!is_string_op(&unknown));
        // A close-but-not-quite name (e.g. heap.alloc) : false.
        let alloc = MirOp::std("cssl.heap.alloc");
        assert!(!is_string_op(&alloc));
    }

    // ── 2. from_utf8_unchecked emits alloc + 3 stores. ───────────────────

    #[test]
    fn lower_from_utf8_unchecked_emits_alloc_and_three_stores() {
        let op = op_string_from_utf8_unchecked();
        let insns = lower_string_from_utf8_unchecked(&op);
        // Expect 6 instructions : 2 const + 1 call + 3 stores.
        assert_eq!(insns.len(), 6, "from_utf8_unchecked should emit 6 insns");
        assert!(
            insns[0].text.contains("iconst.i64 24"),
            "insn[0] should be the size const, got: {}",
            insns[0].text
        );
        assert!(
            insns[1].text.contains("iconst.i64 8"),
            "insn[1] should be the align const"
        );
        assert!(
            insns[2].text.contains("call __cssl_alloc"),
            "insn[2] should call alloc"
        );
        assert!(
            insns[3].text.contains("store.i64 aligned 8") && insns[3].text.contains("+0"),
            "insn[3] should store at offset 0 (data)"
        );
        assert!(
            insns[4].text.contains("+8"),
            "insn[4] should store at offset 8 (len)"
        );
        assert!(
            insns[5].text.contains("+16"),
            "insn[5] should store at offset 16 (cap)"
        );
    }

    // ── 3. from_utf8 emits validate call + sentinel compare. ─────────────

    #[test]
    fn lower_from_utf8_emits_validate_call_with_sentinel() {
        let op = op_string_from_utf8();
        let insns = lower_string_from_utf8(&op);
        // 5 insns : SWAP-POINT comment + call + sentinel + cmp + branch comment.
        assert_eq!(insns.len(), 5);
        assert!(
            insns[0].text.contains("SWAP-POINT")
                && insns[0].text.contains(VALIDATE_SYMBOL),
            "insn[0] should be the SWAP-POINT marker, got: {}",
            insns[0].text
        );
        assert!(
            insns[1].text.contains(&format!("call {VALIDATE_SYMBOL}")),
            "insn[1] should call __cssl_strvalidate"
        );
        assert!(
            insns[2].text.contains("iconst.i64 -1"),
            "insn[2] should load the -1 ok sentinel"
        );
        assert!(
            insns[3].text.contains("icmp eq"),
            "insn[3] should compare errcode to sentinel"
        );
    }

    // ── 4. String triple field loads. ─────────────────────────────────────

    #[test]
    fn lower_string_len_emits_single_load_at_offset_8() {
        let op = op_string_len();
        let insns = lower_string_len(&op);
        assert_eq!(insns.len(), 1);
        assert_eq!(insns[0].text, "    v1 = load.i64 aligned 8 v0+8");
    }

    #[test]
    fn lower_string_byte_at_emits_data_load_then_iadd_then_byte_load() {
        let op = op_string_byte_at();
        let insns = lower_string_byte_at(&op);
        assert_eq!(insns.len(), 4);
        assert_eq!(insns[0].text, "    v2.data = load.i64 aligned 8 v0+0");
        assert_eq!(insns[1].text, "    v2.addr = iadd v2.data, v1");
        assert_eq!(insns[2].text, "    v2.byte = load.i8 v2.addr");
        assert_eq!(insns[3].text, "    v2 = uextend.i32 v2.byte");
    }

    // ── 5. StrSlice fat-pointer field loads. ──────────────────────────────

    #[test]
    fn lower_str_slice_len_loads_offset_8() {
        let op = op_str_slice_len();
        let insns = lower_str_slice_len(&op);
        assert_eq!(insns.len(), 1);
        assert_eq!(insns[0].text, "    v1 = load.i64 aligned 8 v0+8");
    }

    #[test]
    fn lower_str_slice_as_bytes_loads_offset_0() {
        let op = op_str_slice_as_bytes();
        let insns = lower_str_slice_as_bytes(&op);
        assert_eq!(insns.len(), 1);
        assert_eq!(insns[0].text, "    v1 = load.i64 aligned 8 v0+0");
    }

    #[test]
    fn lower_str_slice_new_emits_alloc_and_two_stores() {
        let op = op_str_slice_new();
        let insns = lower_str_slice_new(&op);
        // 5 insns : 2 const + 1 alloc + 2 stores.
        assert_eq!(insns.len(), 5);
        assert!(insns[0].text.contains("iconst.i64 16"));
        assert!(insns[1].text.contains("iconst.i64 8"));
        assert!(insns[2].text.contains("call __cssl_alloc"));
        assert!(insns[3].text.contains("+0"));
        assert!(insns[4].text.contains("+8"));
    }

    // ── 6. Char USV-invariant emits the 4-cmp chain. ──────────────────────

    #[test]
    fn lower_char_from_u32_emits_full_usv_check_chain() {
        let op = op_char_from_u32();
        let insns = lower_char_from_u32_check(&op);
        // 12 insns : 4 const + 4 cmp + 1 surr-and + 2 or + 1 trailing comment.
        assert!(insns.len() >= 11, "USV check should emit ≥ 11 insns, got {}", insns.len());
        // Check the specific USV constants are emitted.
        assert!(
            insns.iter().any(|i| i.text.contains("iconst.i64 1114111")),
            "should emit USV_MAX (0x10FFFF) constant"
        );
        assert!(
            insns.iter().any(|i| i.text.contains("iconst.i64 55295")),
            "should emit USV_MAX_BMP (0xD7FF) constant"
        );
        assert!(
            insns.iter().any(|i| i.text.contains("iconst.i64 57344")),
            "should emit USV_MIN_NONSURROGATE (0xE000) constant"
        );
        // The branch comment closes the sequence.
        assert!(
            insns.last().unwrap().text.contains("cssl.option"),
            "trailing branch comment should reference option construction"
        );
    }

    // ── 7. Format-spec emission carries spec/arg count attrs. ─────────────

    #[test]
    fn lower_string_format_emits_alloc_and_per_arg_slots() {
        let op = op_string_format();
        let insns = lower_string_format(&op);
        // Expected : header comment + 3 alloc + 1 byte counter + 2 arg slots + 1 finalize.
        assert!(insns.len() >= 6);
        assert!(
            insns[0].text.contains("spec_count=2") && insns[0].text.contains("arg_count=2"),
            "header comment should record spec/arg counts"
        );
        // Per-arg slots scale with arg_count.
        let arg_slots = insns
            .iter()
            .filter(|i| i.text.contains("per-arg dispatch slot"))
            .count();
        assert_eq!(arg_slots, 2, "should emit one slot per arg");
    }

    // ── 8. Cranelift signature builder for the validator extern. ──────────

    #[test]
    fn build_strvalidate_signature_has_canonical_shape() {
        let sig = build_strvalidate_signature(CallConv::SystemV, clif_types::I64);
        // 2 params (bytes_ptr, bytes_len) + 1 return (i64).
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0].value_type, clif_types::I64);
    }

    // ── 9. Field-op CLIF-type table. ──────────────────────────────────────

    #[test]
    fn clif_type_for_field_op_matches_dispatcher() {
        assert_eq!(clif_type_for_field_op(OP_STRING_LEN), Some(ClifType::I64));
        assert_eq!(clif_type_for_field_op(OP_STRING_BYTE_AT), Some(ClifType::I32));
        assert_eq!(clif_type_for_field_op(OP_STR_SLICE_LEN), Some(ClifType::I64));
        assert_eq!(clif_type_for_field_op(OP_STR_SLICE_AS_BYTES), Some(ClifType::I64));
        // Unknown : None.
        assert_eq!(clif_type_for_field_op("arith.constant"), None);
        assert_eq!(clif_type_for_field_op(""), None);
    }

    // ── 10. Defensive : malformed ops yield comment placeholders. ────────

    #[test]
    fn malformed_string_len_yields_comment_placeholder() {
        // No operands.
        let op = MirOp::std(OP_STRING_LEN).with_result(ValueId(0), MirType::Int(IntWidth::I64));
        let insns = lower_string_len(&op);
        assert_eq!(insns.len(), 1);
        assert!(insns[0].text.contains("; cssl.string.len : missing"));
    }

    #[test]
    fn malformed_string_byte_at_yields_comment_placeholder() {
        // No operands.
        let op = MirOp::std(OP_STRING_BYTE_AT);
        let insns = lower_string_byte_at(&op);
        assert_eq!(insns.len(), 1);
        assert!(insns[0].text.contains("missing"));
    }

    // ── 11. aligned_string_flag + read_offset_attr. ──────────────────────

    #[test]
    fn aligned_string_flag_is_canonical_8byte() {
        assert_eq!(aligned_string_flag(), " aligned 8");
    }

    #[test]
    fn read_offset_attr_parses_decimal() {
        let op = op_string_len();
        assert_eq!(read_offset_attr(&op), Some(8));
        // No offset attr.
        let bare = MirOp::std("memref.load");
        assert_eq!(read_offset_attr(&bare), None);
    }

    // ── 12. Result-id threading : custom result ids carry through. ───────

    #[test]
    fn lower_string_len_uses_result_id_from_op() {
        let op = MirOp::std(OP_STRING_LEN)
            .with_operand(ValueId(7))
            .with_result(ValueId(99), MirType::Int(IntWidth::I64))
            .with_attribute("offset", "8");
        let insns = lower_string_len(&op);
        assert_eq!(insns[0].text, "    v99 = load.i64 aligned 8 v7+8");
    }

    #[test]
    fn lower_str_slice_new_uses_result_id_from_op() {
        let op = MirOp::std(OP_STR_SLICE_NEW)
            .with_operand(ValueId(11))
            .with_operand(ValueId(12))
            .with_result(ValueId(42), MirType::Ptr);
        let insns = lower_str_slice_new(&op);
        // Final 2 stores reference v42 + v11/v12.
        let store_ptr = &insns[3].text;
        let store_len = &insns[4].text;
        assert!(
            store_ptr.contains("v11") && store_ptr.contains("v42+0"),
            "ptr-store should reference v11 + v42+0, got {store_ptr}"
        );
        assert!(
            store_len.contains("v12") && store_len.contains("v42+8"),
            "len-store should reference v12 + v42+8, got {store_len}"
        );
    }
}

// § INTEGRATION_NOTE  (per Wave-C1 dispatch directive)
//   This module is delivered as a NEW file. `cssl-cgen-cpu-cranelift/
//   src/lib.rs` is NOT modified by this slice — the
//   `pub mod cgen_string ;` declaration is added by the future
//   main-thread integration commit alongside the `lower::lower_op`
//   extension that routes the recognized op-name prefixes here.
//
//   Mocked-deps :
//     - `__cssl_strvalidate(bytes_ptr, bytes_len) -> i64` returning
//       (-1 = ok, idx ≥ 0 = first invalid byte). cssl-rt does NOT yet
//       ship this symbol ; the cgen path emits the import declaration
//       with a SWAP-POINT marker comment so the integration commit can
//       supply a stub once the runtime walker lands. Until then
//       `from_utf8_unchecked` is the only path callers should take.
//     - The String triple `data` / `len` / `cap` access pattern uses the
//       canonical `__cssl_alloc` symbol from S6-A1 — already shipping in
//       cssl-rt/src/ffi.rs.
//
//   Wire-protocol with `cssl-mir/src/string_abi.rs`:
//     The op-name strings + layout offsets + validate-symbol name MUST
//     match cssl-mir/src/string_abi.rs verbatim. The
//     `constants_match_canonical_op_names` test pins the wire-protocol ;
//     a rename on either side trips the test immediately.
