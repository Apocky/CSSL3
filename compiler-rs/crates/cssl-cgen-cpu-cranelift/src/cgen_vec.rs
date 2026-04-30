//! § Wave-A2-γ-redo — `cssl.vec.*` Cranelift cgen helpers.
//!
//! § ROLE
//!   Cgen-side helpers for the Vec full struct-ABI : translate the post-
//!   `vec_abi::expand_vec_module` MIR shape — `cssl.heap.alloc` + 3 ×
//!   `memref.store` (data/len/cap) + `cssl.heap.realloc` (in the grow-arm
//!   of `vec_push`) + `cssl.heap.dealloc` (in `vec_drop`) — into the
//!   Cranelift CLIF surface that the JIT / object backend executes.
//!
//!   The heavy lifting is already in place : the cgen layer handles
//!   `arith.constant` / `memref.load` / `memref.store` / `cssl.heap.alloc`
//!   / `cssl.heap.realloc` / `cssl.heap.dealloc` / `scf.if` / `arith.cmpi`
//!   natively (see object.rs op-dispatch table). What this slice adds :
//!
//!   - canonical attribute-readers that recognize the
//!     `field=data/len/cap` markers stamped by `vec_abi` plus the
//!     `source_kind=vec_cell / vec_data / vec_alias` markers emitted by
//!     the expansion pass.
//!   - predicate helpers that let the JIT / Object backends decide
//!     whether a given op is part of a Vec triple-cell sequence (useful
//!     for skipping no-op aliases + surfacing vec-cell allocs in pre-emit
//!     diagnostic dumps).
//!   - cranelift Signature helpers for the `__cssl_realloc` import (the
//!     dealloc + alloc imports are already shared via cgen_string +
//!     cgen_heap_dealloc ; vec_push uses realloc which has its own
//!     ABI shape).
//!   - per-fn pre-scan helpers : "does this fn need a __cssl_realloc
//!     import declared" — keeps the import surface lean.
//!
//! § INTEGRATION_NOTE  (per W-A2-γ-redo dispatch directive)
//!   This module is delivered as a NEW file alongside the existing
//!   `cgen_tagged_union` / `cgen_heap_dealloc` / `cgen_string` companions.
//!   The helpers compile + are tested in-place via `#[cfg(test)]` ; the
//!   `pub mod cgen_vec ;` line in `lib.rs` is added by this same commit.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-mir/src/vec_abi.rs` — sister module that
//!     produces the post-rewrite MIR shape this module consumes.
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/object.rs` —
//!     existing per-fn import-declare path + `emit_heap_call` shared
//!     helper.
//!   - `compiler-rs/crates/cssl-rt/src/ffi.rs` — `__cssl_realloc` ABI
//!     symbol the realloc op lowers to (3-param : `(ptr, old_bytes,
//!     new_bytes) -> ptr`, with element-alignment carried in the op's
//!     attribute slot rather than as a 4th param).
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required Vec storage.
//!   - The per-block pre-scan walks ops ONCE ; O(N) in op count + early-
//!     exits on first match.
//!   - Predicate helpers do single-pass linear scans of the op's
//!     attribute slice (typical N ≤ 6) — strictly faster than a
//!     hash-table at this size.
//!
//! § MIR ↔ CLIF ABI MAPPING
//!
//!   ```text
//!   MIR (post-expand)                                CLIF
//!   ───────────────────────────────────────────      ────────────────────────────
//!   cssl.heap.alloc {bytes=24, alignment=8,          call __cssl_alloc(24, 8) -> ptr
//!     source_kind=vec_cell}                            (already in object.rs path ;
//!                                                      vec-cell annotation is informational)
//!   memref.store %null_ptr, %t  {offset=0,           store.i64 v_null, v_t (off 0)
//!     field=data, alignment=8}
//!   memref.store %0,        %t  {offset=8,           store.i64 v_zero, v_t (off 8)
//!     field=len, alignment=8}
//!   memref.store %0,        %t  {offset=16,          store.i64 v_zero, v_t (off 16)
//!     field=cap, alignment=8}
//!
//!   cssl.heap.realloc %old_data, %old_bytes,         call __cssl_realloc(v_old_data,
//!     %new_bytes {alignment=4,                          v_old_bytes, v_new_bytes) -> ptr
//!     source_kind=vec_data}                            (object.rs emit_heap_call path)
//!
//!   memref.load %t {offset=8, field=len} -> i64      v_len = load.i64 v_t (off 8)
//!   memref.load %t {offset=16, field=cap} -> i64     v_cap = load.i64 v_t (off 16)
//!   memref.load %t {offset=0, field=data} -> ptr     v_data = load.i64 v_t (off 0)
//!
//!   cssl.ptr.offset %data, %byte_off                 v_addr = iadd v_data, v_byte_off
//!     -> ptr
//!   memref.load %addr {offset=0, field=elem} -> T    v_elem = load.* v_addr (off 0)
//!   ```

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_codegen::isa::CallConv;
use cssl_mir::{MirBlock, MirOp};

// ─────────────────────────────────────────────────────────────────────────
// § Canonical attribute keys + values stamped by `vec_abi`.
//
//   These const strings are the wire-protocol between the MIR rewriter
//   and this cgen layer. Renaming any of them requires lock-step changes
//   on both sides ; the constants make the lock-step explicit + grep-
//   friendly.
// ─────────────────────────────────────────────────────────────────────────

/// Attribute key carrying the canonical `field=data` / `field=len` /
/// `field=cap` / `field=elem` marker on `memref.load` / `memref.store`
/// ops emitted by `vec_abi::expand_vec_*`.
pub const ATTR_FIELD: &str = "field";
/// `field=data` value — the 8-byte data-pointer slot at offset 0.
pub const ATTR_FIELD_DATA: &str = "data";
/// `field=len` value — the 8-byte length slot at offset 8.
pub const ATTR_FIELD_LEN: &str = "len";
/// `field=cap` value — the 8-byte capacity slot at offset 16.
pub const ATTR_FIELD_CAP: &str = "cap";
/// `field=elem` value — an element slot in the data buffer (offset is
/// `i * sizeof_T` from the data pointer).
pub const ATTR_FIELD_ELEM: &str = "elem";

/// Attribute key carrying the source-kind marker (`vec_cell` /
/// `vec_data` / `vec_alias`).
pub const ATTR_SOURCE_KIND: &str = "source_kind";
/// `source_kind=vec_cell` — the heap-alloc that owns a Vec triple-cell.
pub const SOURCE_KIND_VEC_CELL: &str = "vec_cell";
/// `source_kind=vec_data` — the heap-(re)alloc / dealloc that owns the
/// Vec element buffer.
pub const SOURCE_KIND_VEC_DATA: &str = "vec_data";
/// `source_kind=vec_alias` — the bitcast that re-routes the original
/// vec-op result-id to the new triple-ptr / loaded element.
pub const SOURCE_KIND_VEC_ALIAS: &str = "vec_alias";

/// Attribute key carrying the byte offset on a typed memref op.
pub const ATTR_OFFSET: &str = "offset";

/// Attribute key carrying the element alignment on a vec data-buffer
/// realloc / dealloc op.
pub const ATTR_ALIGNMENT: &str = "alignment";

/// Attribute key carrying the `payload_ty` textual element-type form
/// (e.g. `"i32"` / `"f64"` / `"!cssl.ptr"`).
pub const ATTR_PAYLOAD_TY: &str = "payload_ty";

/// Attribute key carrying the `origin` source-fn textual form
/// (`vec_new` / `vec_push.grow` / `vec_index.slot_addr` / ...).
pub const ATTR_ORIGIN: &str = "origin";

// ─────────────────────────────────────────────────────────────────────────
// § Constants — triple-cell layout on stage-0's 64-bit host.
// ─────────────────────────────────────────────────────────────────────────

/// `data` field byte offset within a Vec triple cell. Always 0.
pub const VEC_DATA_OFFSET: u32 = 0;
/// `len` field byte offset within a Vec triple cell. Always 8.
pub const VEC_LEN_OFFSET: u32 = 8;
/// `cap` field byte offset within a Vec triple cell. Always 16.
pub const VEC_CAP_OFFSET: u32 = 16;
/// Total Vec triple-cell byte size : `3 × 8 = 24`.
pub const VEC_CELL_SIZE: u32 = 24;
/// Vec triple-cell alignment.
pub const VEC_CELL_ALIGN: u32 = 8;
/// Per-field byte width on 64-bit host (8 = `sizeof(usize)` =
/// `sizeof(*mut u8)` = `sizeof(i64)`).
pub const VEC_FIELD_SIZE: u32 = 8;

// ─────────────────────────────────────────────────────────────────────────
// § FFI symbol contracts — alloc / realloc / dealloc.
//
//   These match the existing object.rs constants byte-for-byte. They're
//   re-declared here so the vec-cgen path doesn't depend on object.rs's
//   private const surface.
// ─────────────────────────────────────────────────────────────────────────

/// FFI symbol for the realloc bridge. ABI-stable from S6-A1 forward.
pub const HEAP_REALLOC_SYMBOL: &str = "__cssl_realloc";
/// FFI symbol for the alloc bridge.
pub const HEAP_ALLOC_SYMBOL: &str = "__cssl_alloc";
/// FFI symbol for the dealloc bridge.
pub const HEAP_FREE_SYMBOL: &str = "__cssl_free";

/// MIR op-name string the cgen-import path matches against for the
/// realloc bridge.
pub const MIR_REALLOC_OP_NAME: &str = "cssl.heap.realloc";
/// MIR op-name string for the canonical alloc op.
pub const MIR_ALLOC_OP_NAME: &str = "cssl.heap.alloc";

// ─────────────────────────────────────────────────────────────────────────
// § Predicate helpers — recognize Vec ops in the post-rewrite MIR.
// ─────────────────────────────────────────────────────────────────────────

/// Test whether `op` carries the canonical `(source_kind, value)` pair.
#[must_use]
pub fn has_source_kind(op: &MirOp, expected: &str) -> bool {
    op.attributes
        .iter()
        .any(|(k, v)| k == ATTR_SOURCE_KIND && v == expected)
}

/// Test whether `op` carries the canonical `(field, value)` pair.
#[must_use]
pub fn has_field(op: &MirOp, expected: &str) -> bool {
    op.attributes
        .iter()
        .any(|(k, v)| k == ATTR_FIELD && v == expected)
}

/// Test whether `op` is the `cssl.heap.alloc` that owns a Vec triple
/// cell (24 bytes, 8-aligned, source_kind=vec_cell).
#[must_use]
pub fn is_vec_cell_alloc(op: &MirOp) -> bool {
    op.name == MIR_ALLOC_OP_NAME && has_source_kind(op, SOURCE_KIND_VEC_CELL)
}

/// Test whether `op` is the `cssl.heap.realloc` inside a `vec_push` grow
/// arm (source_kind=vec_data).
#[must_use]
pub fn is_vec_data_realloc(op: &MirOp) -> bool {
    op.name == MIR_REALLOC_OP_NAME && has_source_kind(op, SOURCE_KIND_VEC_DATA)
}

/// Test whether `op` is the `cssl.heap.dealloc` of a Vec data-buffer
/// (source_kind=vec_data) — emitted by `vec_drop` on the data slot
/// before the triple-cell free.
#[must_use]
pub fn is_vec_data_dealloc(op: &MirOp) -> bool {
    op.name == "cssl.heap.dealloc" && has_source_kind(op, SOURCE_KIND_VEC_DATA)
}

/// Test whether `op` is the `cssl.heap.dealloc` of the Vec triple-cell
/// (source_kind=vec_cell) — emitted by `vec_drop` after the data-slot
/// free.
#[must_use]
pub fn is_vec_cell_dealloc(op: &MirOp) -> bool {
    op.name == "cssl.heap.dealloc" && has_source_kind(op, SOURCE_KIND_VEC_CELL)
}

/// Test whether `op` is the `arith.bitcast` alias that re-routes the
/// original `cssl.vec.*` op result-id to the rewritten value (triple-ptr
/// / element-result / loaded i64). Cgen skips emitting a CLIF
/// instruction for these (pure value-map plumbing).
#[must_use]
pub fn is_vec_alias(op: &MirOp) -> bool {
    op.name == "arith.bitcast" && has_source_kind(op, SOURCE_KIND_VEC_ALIAS)
}

/// Test whether `op` is a `memref.store` of the `data` field.
#[must_use]
pub fn is_data_store(op: &MirOp) -> bool {
    op.name == "memref.store" && has_field(op, ATTR_FIELD_DATA)
}

/// Test whether `op` is a `memref.store` of the `len` field.
#[must_use]
pub fn is_len_store(op: &MirOp) -> bool {
    op.name == "memref.store" && has_field(op, ATTR_FIELD_LEN)
}

/// Test whether `op` is a `memref.store` of the `cap` field.
#[must_use]
pub fn is_cap_store(op: &MirOp) -> bool {
    op.name == "memref.store" && has_field(op, ATTR_FIELD_CAP)
}

/// Test whether `op` is a `memref.load` of the `data` field — the
/// entry point of the index / push tail.
#[must_use]
pub fn is_data_load(op: &MirOp) -> bool {
    op.name == "memref.load" && has_field(op, ATTR_FIELD_DATA)
}

/// Test whether `op` is a `memref.load` of the `len` field.
#[must_use]
pub fn is_len_load(op: &MirOp) -> bool {
    op.name == "memref.load" && has_field(op, ATTR_FIELD_LEN)
}

/// Test whether `op` is a `memref.load` of the `cap` field.
#[must_use]
pub fn is_cap_load(op: &MirOp) -> bool {
    op.name == "memref.load" && has_field(op, ATTR_FIELD_CAP)
}

/// Test whether `op` is a `cssl.ptr.offset` op emitted by `vec_push` /
/// `vec_index` for slot-address calculation (data + i*sizeof_T).
#[must_use]
pub fn is_vec_slot_addr(op: &MirOp) -> bool {
    op.name == "cssl.ptr.offset"
        && op
            .attributes
            .iter()
            .any(|(k, v)| k == ATTR_ORIGIN && (v.contains("slot_addr")))
}

// ─────────────────────────────────────────────────────────────────────────
// § Attribute readers — pull canonical values off a Vec op.
// ─────────────────────────────────────────────────────────────────────────

/// Read the `payload_ty` attribute (the textual element-type form).
/// Returns `None` when absent (op is not part of a Vec sequence or the
/// attribute was stripped during rewrite).
#[must_use]
pub fn read_payload_ty(op: &MirOp) -> Option<&str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_PAYLOAD_TY)
        .map(|(_, v)| v.as_str())
}

/// Read the `origin` attribute — useful for diagnostic dumps that want
/// to surface "this realloc is from the vec_push grow-arm" without
/// re-walking up the parent op-chain.
#[must_use]
pub fn read_origin(op: &MirOp) -> Option<&str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_ORIGIN)
        .map(|(_, v)| v.as_str())
}

/// Read the `offset` attribute — used by memref.load / memref.store ops
/// stamped by vec_abi expansion.
#[must_use]
pub fn read_offset(op: &MirOp) -> Option<u32> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_OFFSET)
        .and_then(|(_, v)| v.parse::<u32>().ok())
}

/// Read the `alignment` attribute — used by realloc / dealloc ops in
/// the vec-data-buffer path.
#[must_use]
pub fn read_alignment(op: &MirOp) -> Option<u32> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_ALIGNMENT)
        .and_then(|(_, v)| v.parse::<u32>().ok())
}

// ─────────────────────────────────────────────────────────────────────────
// § cranelift signature builders — `__cssl_alloc` + `__cssl_realloc` +
//   `__cssl_free` shapes.
// ─────────────────────────────────────────────────────────────────────────

/// Build the cranelift `Signature` for the `__cssl_alloc` import.
///
/// § SHAPE  (matches `cssl-rt/src/ffi.rs § __cssl_alloc`)
/// ```text
///   pub unsafe extern "C" fn __cssl_alloc(
///       size  : usize,
///       align : usize,
///   ) -> *mut u8
/// ```
#[must_use]
pub fn build_alloc_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    let abi_ptr = AbiParam::new(ptr_ty);
    sig.params.push(abi_ptr); // size
    sig.params.push(abi_ptr); // align
    sig.returns.push(abi_ptr); // ptr
    sig
}

/// Build the cranelift `Signature` for the `__cssl_realloc` import.
///
/// § SHAPE  (matches `cssl-rt/src/ffi.rs § __cssl_realloc`)
/// ```text
///   pub unsafe extern "C" fn __cssl_realloc(
///       ptr       : *mut u8,
///       old_bytes : usize,
///       new_bytes : usize,
///   ) -> *mut u8
/// ```
/// The element-alignment is carried in the op's `alignment` attribute
/// rather than as a 4th param ; the cssl-rt impl pads `old_bytes` and
/// `new_bytes` to the host's max alignment for safety.
#[must_use]
pub fn build_realloc_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    let abi_ptr = AbiParam::new(ptr_ty);
    sig.params.push(abi_ptr); // ptr
    sig.params.push(abi_ptr); // old_bytes
    sig.params.push(abi_ptr); // new_bytes
    sig.returns.push(abi_ptr); // new_ptr
    sig
}

/// Build the cranelift `Signature` for the `__cssl_free` import. Wraps
/// `cgen_heap_dealloc::build_dealloc_signature` so callers wiring the
/// vec path don't need to import both modules.
#[must_use]
pub fn build_free_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    let abi_ptr = AbiParam::new(ptr_ty);
    sig.params.push(abi_ptr); // ptr
    sig.params.push(abi_ptr); // size
    sig.params.push(abi_ptr); // align
    sig
}

// ─────────────────────────────────────────────────────────────────────────
// § per-fn pre-scan — "does this fn need any of {alloc,realloc,free}"
// ─────────────────────────────────────────────────────────────────────────

/// Bitfield reporting which heap-FFI imports a fn needs.
///
/// Sawyer-style packed flags — packs the 3 booleans into a single u8
/// instead of separate bool fields so the per-fn scan result fits in a
/// register + can be merged across blocks via bitwise-OR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VecHeapImportSet {
    bits: u8,
}

const NEEDS_ALLOC: u8 = 0b001;
const NEEDS_REALLOC: u8 = 0b010;
const NEEDS_FREE: u8 = 0b100;

impl VecHeapImportSet {
    /// Empty set — no imports needed.
    #[must_use]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// `true` iff `__cssl_alloc` is needed.
    #[must_use]
    pub const fn needs_alloc(self) -> bool {
        (self.bits & NEEDS_ALLOC) != 0
    }

    /// `true` iff `__cssl_realloc` is needed.
    #[must_use]
    pub const fn needs_realloc(self) -> bool {
        (self.bits & NEEDS_REALLOC) != 0
    }

    /// `true` iff `__cssl_free` is needed.
    #[must_use]
    pub const fn needs_free(self) -> bool {
        (self.bits & NEEDS_FREE) != 0
    }

    /// `true` iff at least one heap import is needed.
    #[must_use]
    pub const fn any(self) -> bool {
        self.bits != 0
    }

    /// Bitwise-merge two sets.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    /// Add the alloc flag.
    #[must_use]
    pub const fn with_alloc(self) -> Self {
        Self {
            bits: self.bits | NEEDS_ALLOC,
        }
    }

    /// Add the realloc flag.
    #[must_use]
    pub const fn with_realloc(self) -> Self {
        Self {
            bits: self.bits | NEEDS_REALLOC,
        }
    }

    /// Add the free flag.
    #[must_use]
    pub const fn with_free(self) -> Self {
        Self {
            bits: self.bits | NEEDS_FREE,
        }
    }
}

/// Walk a single MIR block's ops once + return which heap imports the
/// block needs. Single-pass O(N) ; no allocation.
#[must_use]
pub fn scan_block_for_vec_heap_imports(block: &MirBlock) -> VecHeapImportSet {
    let mut set = VecHeapImportSet::empty();
    for op in &block.ops {
        match op.name.as_str() {
            "cssl.heap.alloc" => set = set.with_alloc(),
            "cssl.heap.realloc" => set = set.with_realloc(),
            "cssl.heap.dealloc" => set = set.with_free(),
            _ => {}
        }
        // Recurse into nested regions (scf.if grow-arm carries the
        // realloc op).
        for region in &op.regions {
            for inner in &region.blocks {
                set = set.merge(scan_block_for_vec_heap_imports(inner));
            }
        }
    }
    set
}

/// `true` iff a fn-level walk found ANY `cssl.vec.*` recognizer-mint
/// that survived the rewrite. Used as a defensive cgen-side audit : if
/// any op leaks past `vec_abi::expand_vec_module` the cgen path can fail
/// fast with a clear diagnostic instead of attempting to emit machine
/// code for an unrecognized op.
#[must_use]
pub fn block_has_unexpanded_vec_op(block: &MirBlock) -> bool {
    for op in &block.ops {
        if op.name.starts_with("cssl.vec.") {
            return true;
        }
        for region in &op.regions {
            for inner in &region.blocks {
                if block_has_unexpanded_vec_op(inner) {
                    return true;
                }
            }
        }
    }
    false
}

// ─────────────────────────────────────────────────────────────────────────
// § contract validators — defensive cross-checks on per-op shape.
// ─────────────────────────────────────────────────────────────────────────

/// Validate that a `memref.store` carrying a Vec field-marker has the
/// expected offset.
///
/// # Errors
/// Returns `Err` when the op is not a memref.store, the field marker
/// mismatches, or the offset is wrong for the named field.
pub fn validate_vec_field_store(op: &MirOp, expected_field: &str) -> Result<(), String> {
    if op.name != "memref.store" {
        return Err(format!(
            "validate_vec_field_store : expected memref.store, got `{}`",
            op.name
        ));
    }
    let field = op
        .attributes
        .iter()
        .find(|(k, _)| k == ATTR_FIELD)
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| "validate_vec_field_store : missing field attribute".to_string())?;
    if field != expected_field {
        return Err(format!(
            "validate_vec_field_store : expected field=`{expected_field}`, got `{field}`"
        ));
    }
    let expected_offset = match expected_field {
        ATTR_FIELD_DATA => VEC_DATA_OFFSET,
        ATTR_FIELD_LEN => VEC_LEN_OFFSET,
        ATTR_FIELD_CAP => VEC_CAP_OFFSET,
        ATTR_FIELD_ELEM => 0,
        _ => return Err(format!("unknown field marker `{expected_field}`")),
    };
    let actual_offset = read_offset(op).ok_or_else(|| {
        "validate_vec_field_store : missing or non-numeric offset attribute".to_string()
    })?;
    // ELEM offsets are runtime-computed via `cssl.ptr.offset` so we
    // accept any offset on field=elem (the load happens at +0 from the
    // computed slot-addr).
    if expected_field == ATTR_FIELD_ELEM {
        return Ok(());
    }
    if actual_offset != expected_offset {
        return Err(format!(
            "validate_vec_field_store : field=`{expected_field}` should have offset={expected_offset}, got {actual_offset}"
        ));
    }
    Ok(())
}

/// Validate a `cssl.heap.alloc` is a Vec triple-cell allocation : 24
/// bytes, 8-byte alignment, source_kind=vec_cell.
///
/// # Errors
/// Returns `Err` with a diagnostic when any contract slot mismatches.
pub fn validate_vec_cell_alloc(op: &MirOp) -> Result<(), String> {
    if op.name != MIR_ALLOC_OP_NAME {
        return Err(format!(
            "validate_vec_cell_alloc : expected `{MIR_ALLOC_OP_NAME}`, got `{}`",
            op.name
        ));
    }
    if !has_source_kind(op, SOURCE_KIND_VEC_CELL) {
        return Err(format!(
            "validate_vec_cell_alloc : missing source_kind=`{SOURCE_KIND_VEC_CELL}`"
        ));
    }
    let bytes = op
        .attributes
        .iter()
        .find(|(k, _)| k == "bytes")
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .ok_or_else(|| "validate_vec_cell_alloc : missing bytes attribute".to_string())?;
    if bytes != VEC_CELL_SIZE {
        return Err(format!(
            "validate_vec_cell_alloc : expected bytes={VEC_CELL_SIZE}, got {bytes}"
        ));
    }
    let align = op
        .attributes
        .iter()
        .find(|(k, _)| k == ATTR_ALIGNMENT)
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .ok_or_else(|| "validate_vec_cell_alloc : missing alignment attribute".to_string())?;
    if align != VEC_CELL_ALIGN {
        return Err(format!(
            "validate_vec_cell_alloc : expected alignment={VEC_CELL_ALIGN}, got {align}"
        ));
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// § tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::types as cl_types;
    use cssl_mir::{vec_abi, IntWidth, MirFunc, MirModule, MirOp, MirType, ValueId};

    /// Build a `cssl.vec.new` recognizer-mint as body_lower would emit.
    /// Build a `cssl.vec.new` recognizer-mint with result-id=99 so it
    /// does NOT collide with the freshly-minted ids from
    /// `vec_abi::FreshIdSeq` (which start at fn.next_value_id == 0 in
    /// these test fns). This keeps the alias-bitcast emit path live.
    fn mk_vec_new() -> MirOp {
        MirOp::std(vec_abi::OP_VEC_NEW)
            .with_result(ValueId(99), MirType::Opaque("Vec".to_string()))
            .with_attribute(vec_abi::ATTR_PAYLOAD_TY, "i32")
            .with_attribute(vec_abi::ATTR_ORIGIN, "vec_new")
    }

    fn mk_vec_push() -> MirOp {
        MirOp::std(vec_abi::OP_VEC_PUSH)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(100), MirType::Opaque("Vec".to_string()))
            .with_attribute(vec_abi::ATTR_PAYLOAD_TY, "i32")
            .with_attribute(vec_abi::ATTR_ORIGIN, "vec_push")
    }

    fn mk_vec_drop() -> MirOp {
        MirOp::std(vec_abi::OP_VEC_DROP)
            .with_operand(ValueId(0))
            .with_attribute(vec_abi::ATTR_PAYLOAD_TY, "i32")
            .with_attribute(vec_abi::ATTR_ORIGIN, "vec_drop")
    }

    /// Helper : build + expand a fn carrying the given vec ops. Pre-
    /// bumps `next_value_id` past 99 so the FreshIdSeq inside
    /// `expand_vec_func` does not collide with the synthetic result-ids
    /// (99..) that the test ops use.
    fn expanded_fn(ops: Vec<MirOp>, params: Vec<MirType>, results: Vec<MirType>) -> MirFunc {
        let mut func = MirFunc::new("test_fn", params, results);
        // Synthetic vec ops use result_id=99 (and reserved up to 199 for
        // multi-op chains). Bump next_value_id so freshly-minted SSA
        // ids don't shadow them.
        func.next_value_id = 200;
        for op in ops {
            func.push_op(op);
        }
        let _ = vec_abi::expand_vec_func(&mut func);
        func
    }

    // ─────────────────────────────────────────────────────────────────
    // § signature builders — alloc / realloc / free shapes match cssl-rt.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn realloc_signature_has_3_params_and_1_return() {
        let sig = build_realloc_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.params[0].value_type, cl_types::I64);
        assert_eq!(sig.returns[0].value_type, cl_types::I64);
    }

    #[test]
    fn alloc_signature_has_2_params_and_1_return() {
        let sig = build_alloc_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn free_signature_has_3_params_and_no_return() {
        let sig = build_free_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.returns.len(), 0);
    }

    // ─────────────────────────────────────────────────────────────────
    // § per-fn pre-scan — vec_push expanded fn should need alloc+realloc
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn scan_for_vec_new_reports_alloc_only() {
        // T-9 : `vec_new` expansion emits ONLY heap.alloc (no realloc /
        //   no dealloc). The pre-scan correctly flags alloc + nothing else.
        let func = expanded_fn(
            vec![mk_vec_new()],
            vec![],
            vec![MirType::Opaque("Vec".to_string())],
        );
        let entry = func.body.entry().unwrap();
        let set = scan_block_for_vec_heap_imports(entry);
        assert!(set.needs_alloc());
        assert!(!set.needs_realloc());
        assert!(!set.needs_free());
    }

    #[test]
    fn scan_for_vec_push_reports_realloc() {
        // The vec_push expansion has a realloc op nested inside the
        // scf.if grow-arm — the scan must recurse into op.regions to
        // pick it up.
        let func = expanded_fn(
            vec![mk_vec_push()],
            vec![
                MirType::Opaque("Vec".to_string()),
                MirType::Int(IntWidth::I32),
            ],
            vec![MirType::Opaque("Vec".to_string())],
        );
        let entry = func.body.entry().unwrap();
        let set = scan_block_for_vec_heap_imports(entry);
        assert!(set.needs_realloc(), "scan should find realloc in scf.if grow-arm");
    }

    #[test]
    fn scan_for_vec_drop_reports_free() {
        let func = expanded_fn(
            vec![mk_vec_drop()],
            vec![MirType::Opaque("Vec".to_string())],
            vec![],
        );
        let entry = func.body.entry().unwrap();
        let set = scan_block_for_vec_heap_imports(entry);
        assert!(set.needs_free(), "vec_drop must trip the free-import flag");
    }

    // ─────────────────────────────────────────────────────────────────
    // § predicate helpers — recognize source_kind + field markers.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn recognize_vec_cell_alloc_in_expanded_body() {
        // T-10 : the vec-cell heap.alloc op (24 bytes / 8 align) is
        //   identifiable post-rewrite via `is_vec_cell_alloc`.
        let func = expanded_fn(
            vec![mk_vec_new()],
            vec![],
            vec![MirType::Opaque("Vec".to_string())],
        );
        let entry = func.body.entry().unwrap();
        let cell = entry
            .ops
            .iter()
            .find(|o| is_vec_cell_alloc(o))
            .expect("missing vec_cell heap.alloc");
        assert!(validate_vec_cell_alloc(cell).is_ok());
    }

    #[test]
    fn recognize_data_len_cap_field_stores_in_vec_new_expansion() {
        let func = expanded_fn(
            vec![mk_vec_new()],
            vec![],
            vec![MirType::Opaque("Vec".to_string())],
        );
        let entry = func.body.entry().unwrap();
        let mut data_seen = false;
        let mut len_seen = false;
        let mut cap_seen = false;
        for op in &entry.ops {
            if is_data_store(op) {
                data_seen = true;
                assert!(validate_vec_field_store(op, ATTR_FIELD_DATA).is_ok());
            }
            if is_len_store(op) {
                len_seen = true;
                assert!(validate_vec_field_store(op, ATTR_FIELD_LEN).is_ok());
            }
            if is_cap_store(op) {
                cap_seen = true;
                assert!(validate_vec_field_store(op, ATTR_FIELD_CAP).is_ok());
            }
        }
        assert!(data_seen && len_seen && cap_seen);
    }

    #[test]
    fn recognize_vec_alias_bitcast_after_vec_new() {
        let func = expanded_fn(
            vec![mk_vec_new()],
            vec![],
            vec![MirType::Opaque("Vec".to_string())],
        );
        let entry = func.body.entry().unwrap();
        let alias = entry.ops.iter().find(|o| is_vec_alias(o));
        assert!(alias.is_some(), "expected vec_alias bitcast after vec_new");
    }

    // ─────────────────────────────────────────────────────────────────
    // § block_has_unexpanded_vec_op — defensive audit.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn block_has_unexpanded_vec_op_detects_pre_rewrite_op() {
        // Pre-rewrite : the body contains a `cssl.vec.new` op.
        let mut func = MirFunc::new("audit", vec![], vec![MirType::Opaque("Vec".to_string())]);
        func.push_op(mk_vec_new());
        let entry = func.body.entry().unwrap();
        assert!(block_has_unexpanded_vec_op(entry));
    }

    #[test]
    fn block_has_unexpanded_vec_op_passes_post_rewrite() {
        // Post-rewrite : every cssl.vec.* op is replaced with primitive
        // shape — the audit returns `false`.
        let func = expanded_fn(
            vec![mk_vec_new()],
            vec![],
            vec![MirType::Opaque("Vec".to_string())],
        );
        let entry = func.body.entry().unwrap();
        assert!(!block_has_unexpanded_vec_op(entry));
    }

    // ─────────────────────────────────────────────────────────────────
    // § attribute readers — pull back the canonical metadata.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn read_payload_ty_finds_i32_from_vec_new() {
        let op = mk_vec_new();
        assert_eq!(read_payload_ty(&op), Some("i32"));
    }

    #[test]
    fn read_offset_finds_8_on_len_field_load() {
        let func = expanded_fn(
            vec![mk_vec_new()],
            vec![],
            vec![MirType::Opaque("Vec".to_string())],
        );
        let len_store = func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| is_len_store(o))
            .expect("missing len store");
        assert_eq!(read_offset(len_store), Some(VEC_LEN_OFFSET));
    }

    // ─────────────────────────────────────────────────────────────────
    // § symbol contracts — sanity-check the const FFI strings.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn ffi_symbol_contracts_match_cssl_rt() {
        // Lock-step audit : if any of these literals drift, the cssl-rt
        // FFI declaration and this cgen path mismatch ⇒ link error.
        assert_eq!(HEAP_ALLOC_SYMBOL, "__cssl_alloc");
        assert_eq!(HEAP_REALLOC_SYMBOL, "__cssl_realloc");
        assert_eq!(HEAP_FREE_SYMBOL, "__cssl_free");
    }

    #[test]
    fn vec_layout_constants_match_mir_side() {
        // Lock-step audit between `vec_abi::VecLayout` + this module's
        // const surface. Drift = ABI mismatch.
        let layout = vec_abi::VecLayout::for_element(&MirType::Int(IntWidth::I32));
        assert_eq!(layout.data_offset, VEC_DATA_OFFSET);
        assert_eq!(layout.len_offset, VEC_LEN_OFFSET);
        assert_eq!(layout.cap_offset, VEC_CAP_OFFSET);
        assert_eq!(layout.total_size, VEC_CELL_SIZE);
        assert_eq!(layout.cell_alignment, VEC_CELL_ALIGN);
    }

    // ─────────────────────────────────────────────────────────────────
    // § end-to-end — full module round-trip from cssl.vec.* to primitive
    //   ops, then cgen-side audit reports ALL imports accurately.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn end_to_end_vec_pipeline_produces_recognizable_cgen_ops() {
        let mut module = MirModule::with_name("e2e_cgen");
        let mut func = MirFunc::new(
            "demo",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        // Bump next_value_id past synthetic result-ids 99/100/101 so
        // FreshIdSeq does not collide.
        func.next_value_id = 200;
        // Build a chain : vec_new → vec_push → vec_drop.
        // mk_vec_new() returns result-id=99 ; vec_push reads it as
        // operand + emits a new triple-id ; vec_drop frees that triple.
        func.push_op(mk_vec_new()); // result=99
        func.push_op(
            MirOp::std(vec_abi::OP_VEC_PUSH)
                .with_operand(ValueId(99))
                .with_operand(ValueId(0)) // param i32
                .with_result(ValueId(100), MirType::Opaque("Vec".to_string()))
                .with_attribute(vec_abi::ATTR_PAYLOAD_TY, "i32")
                .with_attribute(vec_abi::ATTR_ORIGIN, "vec_push"),
        );
        func.push_op(
            MirOp::std(vec_abi::OP_VEC_DROP)
                .with_operand(ValueId(100))
                .with_attribute(vec_abi::ATTR_PAYLOAD_TY, "i32")
                .with_attribute(vec_abi::ATTR_ORIGIN, "vec_drop"),
        );
        module.push_func(func);

        let report = vec_abi::expand_vec_module(&mut module);
        assert_eq!(report.vec_new_count, 1);
        assert_eq!(report.vec_push_count, 1);
        assert_eq!(report.vec_drop_count, 1);

        let func = &module.funcs[0];
        let entry = func.body.entry().unwrap();
        // Cgen-side audit : no vec.* ops survived + scan reports
        // alloc + realloc + free needed.
        assert!(!block_has_unexpanded_vec_op(entry));
        let imports = scan_block_for_vec_heap_imports(entry);
        assert!(imports.needs_alloc());
        assert!(imports.needs_realloc());
        assert!(imports.needs_free());
        // Sanity : at least one each of cell-alloc, data-realloc,
        // data-dealloc, cell-dealloc is present.
        let mut cell_alloc = 0;
        let mut data_realloc = 0;
        let mut data_dealloc = 0;
        let mut cell_dealloc = 0;
        // Walk top-level + recurse into scf.if regions.
        fn walk(
            block: &cssl_mir::MirBlock,
            cell_alloc: &mut u32,
            data_realloc: &mut u32,
            data_dealloc: &mut u32,
            cell_dealloc: &mut u32,
        ) {
            for op in &block.ops {
                if is_vec_cell_alloc(op) {
                    *cell_alloc += 1;
                }
                if is_vec_data_realloc(op) {
                    *data_realloc += 1;
                }
                if is_vec_data_dealloc(op) {
                    *data_dealloc += 1;
                }
                if is_vec_cell_dealloc(op) {
                    *cell_dealloc += 1;
                }
                for region in &op.regions {
                    for inner in &region.blocks {
                        walk(inner, cell_alloc, data_realloc, data_dealloc, cell_dealloc);
                    }
                }
            }
        }
        walk(
            entry,
            &mut cell_alloc,
            &mut data_realloc,
            &mut data_dealloc,
            &mut cell_dealloc,
        );
        assert_eq!(cell_alloc, 1, "expected exactly one vec_cell heap.alloc");
        assert_eq!(data_realloc, 1, "expected one vec_data realloc in grow-arm");
        assert_eq!(data_dealloc, 1, "expected one vec_data dealloc in vec_drop");
        assert_eq!(cell_dealloc, 1, "expected one vec_cell dealloc in vec_drop");
    }

    #[test]
    fn vec_heap_import_set_packs_three_flags_into_u8() {
        // Sawyer-style bitfield audit : the 3 flags fit in 1 byte +
        // merge produces the OR.
        let alloc = VecHeapImportSet::empty().with_alloc();
        let realloc = VecHeapImportSet::empty().with_realloc();
        let free = VecHeapImportSet::empty().with_free();
        let merged = alloc.merge(realloc).merge(free);
        assert!(merged.needs_alloc());
        assert!(merged.needs_realloc());
        assert!(merged.needs_free());
        assert!(merged.any());
    }
}
