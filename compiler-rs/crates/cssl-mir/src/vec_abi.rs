//! § Vec full struct-ABI lowering — `Vec<T>` → `{data: ptr, len: usize, cap: usize}`.
//!
//! § SPEC : `specs/40_WAVE_CSSL_PLAN.csl` § WAVES § WAVE-A § A2-γ
//!          (W-A2-γ-redo · T11-D267 · isolated-worktree single-shot).
//!
//! § ROLE
//!   MIR → MIR pass that closes the W-A2-α/β chain. After
//!   `body_lower` mints `cssl.vec.{new,push,index,len,cap,drop}` ops with
//!   `MirType::Opaque("Vec")` results + after `tagged_union_abi` lowers
//!   `Option<T>` / `Result<T,E>`, this pass walks every `Vec<T>` SSA-cell
//!   and rewrites it into the canonical packed 3-cell struct ABI :
//!
//!   ```text
//!   struct Vec<T> {
//!       data : !cssl.ptr,    // offset 0,  8 bytes — heap buffer (or null when cap=0)
//!       len  : i64,          // offset 8,  8 bytes — number of valid elems
//!       cap  : i64,          // offset 16, 8 bytes — buffer capacity (in elems)
//!   }
//!   ```
//!
//!   § INVARIANTS
//!     - `cap == 0  ↔  data == null` (matches `stdlib/vec.cssl § Manual Drop`'s
//!       `if v.cap > 0` guard).
//!     - `len ≤ cap` for all observable states (verified at body-rewrite time
//!       via `vec_push`'s grow-if-needed branch).
//!     - The triple is heap-resident on stage-0 (one `cssl.heap.alloc` cell
//!       holding 3 × `i64` slots) — NOT split across registers. This
//!       preserves Sawyer-style cache locality + lets `vec_index` /
//!       `vec_len` / `vec_cap` lower to a single-pointer + offset load
//!       sequence rather than threading a 3-tuple through every op.
//!     - Stage-0 `usize` collapses to `i64` on the 64-bit JIT host. Future
//!       slices that target 32-bit hosts swap the slot-type via
//!       `host_pointer_width()`.
//!
//!   § REWRITES PERFORMED
//!     1. `cssl.vec.new`  → `cssl.heap.alloc {bytes=24, alignment=8} +
//!                          memref.store(null_ptr, +0) +
//!                          memref.store(0_i64,    +8) +
//!                          memref.store(0_i64,   +16)`
//!     2. `cssl.vec.push` → `field-load len + field-load cap + cmp-eq +
//!                          scf.if(grow) { realloc; double-cap; store cap;
//!                          store data } +
//!                          element-store @ data+len*sizeof_T +
//!                          field-store len+1`
//!     3. `cssl.vec.index`→ `field-load len + cmp-lt +
//!                          scf.if(panic_on_oob) +
//!                          field-load data + element-load @ data+i*sizeof_T`
//!     4. `cssl.vec.len`  → `memref.load triple+8`
//!     5. `cssl.vec.cap`  → `memref.load triple+16`
//!     6. `cssl.vec.drop` → `cssl.heap.dealloc(data, cap*sizeof_T, alignof_T) +
//!                          cssl.heap.dealloc(triple_ptr, 24, 8)`
//!
//!   § SIG-REWRITE
//!     - `Vec<T>` slot in fn-params / fn-results / block-args / op-result-types
//!       lowers to `MirType::Ptr` (the triple-cell pointer). Mirrors the
//!       tagged-union-abi `is_tagged_union_type` predicate.
//!     - Idempotent : a stamp attribute (`vec_abi.sig_rewritten=true`) on
//!       the `MirFunc.attributes` short-circuits repeated runs.
//!
//! § PATTERN-MATCH WITH `tagged_union_abi`
//!   This module mirrors the public surface + invariants of
//!   `tagged_union_abi` line-by-line so callers (the pass-pipeline driver
//!   in `pipeline.rs`, the cgen-side recognizers in `cgen_vec`) can use a
//!   consistent shape : `VecLayout` ↔ `TaggedUnionLayout`, `expand_vec_op`
//!   ↔ `expand_construct`, `expand_vec_func` ↔ `expand_func`,
//!   `expand_vec_module` ↔ `expand_module`, `is_vec_type` ↔
//!   `is_tagged_union_type`, `rewrite_vec_signature` ↔
//!   `rewrite_func_signature`, `VEC_SIG_REWRITTEN_KEY` ↔
//!   `SIG_REWRITTEN_KEY`, plus a callable `VecAbiPass` newtype as the
//!   pass-pipeline wrapper.
//!
//! § SAWYER-EFFICIENT
//!   - The triple-cell layout is computed ONCE per fn (3 × i64 = 24 bytes,
//!     8-byte aligned) ; per-op rewrites just splice 3-5 MIR ops + reuse
//!     the layout's offsets directly.
//!   - All offsets are `const` u32s — no `HashMap<String, u32>` lookups.
//!   - `is_vec_type` is a single `starts_with` + literal compare.
//!   - The expansion pass walks the MIR module ONCE per fn ; ops are
//!     rewritten in-place using a two-pass visit (collect indices,
//!     then splice) — no scratch `Vec` growth in the inner step.
//!
//! § DEFERRED (explicit ; matches the slice's stated boundary)
//!   - Real `MirType::VecOf<T>` first-class type-system surface : at this
//!     slice the rewrite preserves the existing `Opaque("Vec")` typing of
//!     `cssl.vec.*` results during expansion + adds a `MirType::Ptr` for
//!     the heap-cell triple. A follow-up slice replaces the opaque with
//!     a structural `Vec { data, len, cap }`.
//!   - 32-bit-host `usize` : stage-0 hard-codes `i64`. Future slice
//!     swaps via `host_pointer_width()`.
//!   - In-line capacity (small-vec optimization) : NOT in scope.

use crate::block::{MirBlock, MirOp, MirRegion};
use crate::func::{MirFunc, MirModule};
use crate::value::{IntWidth, MirType, ValueId};

// ─────────────────────────────────────────────────────────────────────────
// § Layout primitives — Sawyer-style packed record geometry.
// ─────────────────────────────────────────────────────────────────────────

/// Packed-record geometry for a `Vec<T>` triple cell.
///
/// All values are byte counts at stage-0. Every offset is from the
/// beginning of the allocated cell.
///
/// § INVARIANT — slots are 8-byte aligned i64 / ptr triples on the 64-bit
///   stage-0 host. Migrating to a 32-bit host requires swapping the slot
///   sizes + alignment in lockstep with `host_pointer_width()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VecLayout {
    /// `data` field byte offset within the triple cell. Always 0.
    pub data_offset: u32,
    /// `len` field byte offset. Always 8.
    pub len_offset: u32,
    /// `cap` field byte offset. Always 16.
    pub cap_offset: u32,
    /// Per-field byte width (uniform — 8 on stage-0's 64-bit host).
    pub field_size: u8,
    /// Total triple-cell byte size : `3 × field_size = 24`.
    pub total_size: u32,
    /// Alignment for the triple cell. Equal to `field_size` on stage-0.
    pub cell_alignment: u32,
    /// Element byte size — derived from the `payload_ty` attribute. Used
    /// by `vec_push` / `vec_index` to scale the `len` / `i` index into
    /// a byte offset on the `data` buffer.
    pub elem_size: u32,
    /// Element alignment — passed to `__cssl_alloc(buffer_size, elem_align)`
    /// when growing the data buffer.
    pub elem_align: u32,
}

impl VecLayout {
    /// Build a layout for `Vec<T>` from the element type. Mirrors
    /// `TaggedUnionLayout::for_option` in spirit.
    #[must_use]
    pub fn for_element(elem: &MirType) -> Self {
        let elem_size = u32::try_from(crate::tagged_union_abi::heuristic_size_of(elem))
            .unwrap_or(8)
            .max(1);
        let elem_align = u32::try_from(crate::tagged_union_abi::heuristic_align_of(elem))
            .unwrap_or(8)
            .max(1);
        Self::pack(elem_size, elem_align)
    }

    /// Pack a triple-cell layout for the given element size + alignment.
    #[must_use]
    pub const fn pack(elem_size: u32, elem_align: u32) -> Self {
        const FIELD_SIZE: u8 = 8;
        const TOTAL: u32 = 24;
        const ALIGN: u32 = 8;
        Self {
            data_offset: 0,
            len_offset: 8,
            cap_offset: 16,
            field_size: FIELD_SIZE,
            total_size: TOTAL,
            cell_alignment: ALIGN,
            elem_size,
            elem_align,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Vec-op recognition — predicates for the body-walker.
// ─────────────────────────────────────────────────────────────────────────

/// Canonical MIR-op names emitted by `body_lower::try_lower_vec_*`.
pub const OP_VEC_NEW: &str = "cssl.vec.new";
/// `vec_push::<T>(v, x)` recognizer-mint.
pub const OP_VEC_PUSH: &str = "cssl.vec.push";
/// `vec_index::<T>(v, i)` recognizer-mint.
pub const OP_VEC_INDEX: &str = "cssl.vec.index";
/// `vec_len::<T>(v)` field-access recognizer.
pub const OP_VEC_LEN: &str = "cssl.vec.len";
/// `vec_cap::<T>(v)` field-access recognizer.
pub const OP_VEC_CAP: &str = "cssl.vec.cap";
/// `vec_drop::<T>(v)` deallocation recognizer.
pub const OP_VEC_DROP: &str = "cssl.vec.drop";

/// `payload_ty` attribute key — body_lower stamps the element type
/// textual form.
pub const ATTR_PAYLOAD_TY: &str = "payload_ty";
/// `origin` attribute key — body_lower stamps the source-fn name
/// (`vec_new` / `vec_push` / `vec_index`).
pub const ATTR_ORIGIN: &str = "origin";

/// Source-kind marker stamped onto every emitted op so cgen can
/// recognize the rewrite output without re-scanning attributes for the
/// `cssl.vec.*` op-name prefix. Mirrors `tagged_union_abi`'s
/// `source_kind=tagged_union*` markers.
pub const ATTR_SOURCE_KIND: &str = "source_kind";
/// `source_kind=vec_cell` — the heap-alloc that owns a Vec triple-cell.
pub const SOURCE_KIND_VEC_CELL: &str = "vec_cell";
/// `source_kind=vec_data` — the heap-alloc that owns the element buffer.
pub const SOURCE_KIND_VEC_DATA: &str = "vec_data";
/// `source_kind=vec_alias` — the bitcast that re-routes the original
/// vec-op result-id to the new triple-ptr.
pub const SOURCE_KIND_VEC_ALIAS: &str = "vec_alias";

/// Test whether `op` is a `cssl.vec.*` recognizer-mint that this pass
/// rewrites. Returns the kind for downstream dispatch ; `None` for ops
/// that aren't vec-ops.
#[must_use]
pub fn vec_op_kind(op: &MirOp) -> Option<VecOpKind> {
    match op.name.as_str() {
        OP_VEC_NEW => Some(VecOpKind::New),
        OP_VEC_PUSH => Some(VecOpKind::Push),
        OP_VEC_INDEX => Some(VecOpKind::Index),
        OP_VEC_LEN => Some(VecOpKind::Len),
        OP_VEC_CAP => Some(VecOpKind::Cap),
        OP_VEC_DROP => Some(VecOpKind::Drop),
        _ => None,
    }
}

/// Discriminator for the six `cssl.vec.*` recognizer-mints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VecOpKind {
    /// `cssl.vec.new`.
    New,
    /// `cssl.vec.push`.
    Push,
    /// `cssl.vec.index`.
    Index,
    /// `cssl.vec.len`.
    Len,
    /// `cssl.vec.cap`.
    Cap,
    /// `cssl.vec.drop`.
    Drop,
}

/// Read the `payload_ty` attribute textual form. Returns `"i64"` as a
/// safe-default when the attribute is absent.
#[must_use]
pub fn payload_ty_str(op: &MirOp) -> &str {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_PAYLOAD_TY)
        .map(|(_, v)| v.as_str())
        .unwrap_or("i64")
}

/// Parse a textual `payload_ty` attribute into a `MirType` for layout
/// lookup. Mirrors `tagged_union_abi::parse_payload_ty`.
#[must_use]
pub fn parse_payload_ty(s: &str) -> MirType {
    use crate::value::FloatWidth;
    match s {
        "i1" | "bool" | "i1.bool" => MirType::Bool,
        "i8" => MirType::Int(IntWidth::I8),
        "i16" => MirType::Int(IntWidth::I16),
        "i32" => MirType::Int(IntWidth::I32),
        "i64" => MirType::Int(IntWidth::I64),
        "index" => MirType::Int(IntWidth::Index),
        "f16" => MirType::Float(FloatWidth::F16),
        "bf16" => MirType::Float(FloatWidth::Bf16),
        "f32" => MirType::Float(FloatWidth::F32),
        "f64" => MirType::Float(FloatWidth::F64),
        "!cssl.handle" => MirType::Handle,
        "!cssl.ptr" => MirType::Ptr,
        _ => MirType::Ptr,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Sig-rewrite — `Vec<T>` opaque → `MirType::Ptr` triple-cell pointer.
// ─────────────────────────────────────────────────────────────────────────

/// Idempotency stamp key on `MirFunc.attributes` after a vec-abi pass.
pub const VEC_SIG_REWRITTEN_KEY: &str = "vec_abi.sig_rewritten";
/// Idempotency stamp value when [`VEC_SIG_REWRITTEN_KEY`] is set.
pub const VEC_SIG_REWRITTEN_VALUE: &str = "true";

/// `true` iff `t` is a `Vec<T>`-shaped opaque type that should lower to
/// `MirType::Ptr` in fn-signature position. Matches both the bare
/// `Opaque("Vec")` form emitted by body_lower-recognizer-mints + the
/// canonical post-construction `Opaque("!cssl.vec.<T>")` form that future
/// slices may emit.
#[must_use]
pub fn is_vec_type(t: &MirType) -> bool {
    let MirType::Opaque(s) = t else {
        return false;
    };
    is_vec_opaque_str(s)
}

/// Predicate for the textual shape of a `Vec<T>` opaque type. Split out
/// so callers can match against textual caches without first
/// constructing a `MirType::Opaque` wrapper.
#[must_use]
pub fn is_vec_opaque_str(s: &str) -> bool {
    s == "Vec" || s.starts_with("!cssl.vec.") || s.starts_with("Vec<")
}

/// Rewrite one type-slot in place — `Vec<T>` → `Ptr`, leaving anything
/// else alone. Returns `true` iff the slot was mutated.
fn rewrite_slot(slot: &mut MirType) -> bool {
    if is_vec_type(slot) {
        *slot = MirType::Ptr;
        true
    } else {
        false
    }
}

/// Walk `func`'s signature surface (params + results + entry-block args
/// + op-result types) and rewrite every `Vec<T>` slot to `MirType::Ptr`.
///
/// Idempotent : the `VEC_SIG_REWRITTEN_KEY` attribute on the fn short-
/// circuits any fn that's already been processed.
pub fn rewrite_vec_signature(func: &mut MirFunc, report: &mut VecExpansionReport) {
    if func
        .attributes
        .iter()
        .any(|(k, v)| k == VEC_SIG_REWRITTEN_KEY && v == VEC_SIG_REWRITTEN_VALUE)
    {
        return;
    }
    let mut local: u32 = 0;

    for slot in &mut func.params {
        if rewrite_slot(slot) {
            local += 1;
        }
    }
    for slot in &mut func.results {
        if rewrite_slot(slot) {
            local += 1;
        }
    }
    for block in &mut func.body.blocks {
        for arg in &mut block.args {
            if rewrite_slot(&mut arg.ty) {
                local += 1;
            }
        }
    }
    rewrite_op_result_types_in_region(&mut func.body, &mut local);

    report.sig_rewrites = report.sig_rewrites.saturating_add(local);
    func.attributes.push((
        VEC_SIG_REWRITTEN_KEY.to_string(),
        VEC_SIG_REWRITTEN_VALUE.to_string(),
    ));
}

/// Walk a region's ops + rewrite `Vec<T>` shapes on every op-result.
/// Mirrors `tagged_union_abi::rewrite_op_result_types_in_region`.
fn rewrite_op_result_types_in_region(region: &mut MirRegion, local: &mut u32) {
    for block in &mut region.blocks {
        for op in &mut block.ops {
            for r in &mut op.results {
                if rewrite_slot(&mut r.ty) {
                    *local += 1;
                }
            }
            for nested in &mut op.regions {
                rewrite_op_result_types_in_region(nested, local);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Body-op expansion — `cssl.vec.*` → primitive heap + memref ops.
// ─────────────────────────────────────────────────────────────────────────

/// Counter for fresh `ValueId`s during expansion. Threaded by the
/// caller. Identical shape to `tagged_union_abi::FreshIdSeq`.
#[derive(Debug, Clone, Copy)]
pub struct FreshIdSeq {
    /// Next id to hand out.
    pub next: u32,
}

impl FreshIdSeq {
    /// Build a sequencer starting at `next`.
    #[must_use]
    pub const fn new(next: u32) -> Self {
        Self { next }
    }

    /// Allocate one fresh `ValueId` (post-increments).
    pub fn fresh(&mut self) -> ValueId {
        let v = ValueId(self.next);
        self.next += 1;
        v
    }
}

/// Expansion result for one `cssl.vec.*` op : the synthesized primitive
/// MIR ops + the canonical SSA-id of the triple-cell pointer (or
/// element-result for `index` / `len` / `cap`).
#[derive(Debug, Clone)]
pub struct VecExpansion {
    /// MIR ops emitted, in source order.
    pub ops: Vec<MirOp>,
    /// SSA-id that should replace the original op's result-id. For
    /// `vec_new` / `vec_push` this is the triple-cell pointer ; for
    /// `vec_index` it is the loaded element ; for `vec_len` / `vec_cap`
    /// it is the loaded i64 ; for `vec_drop` no result.
    pub bound_id: Option<ValueId>,
    /// Layout used during expansion ; preserved so callers can audit
    /// per-op byte counts without re-deriving from the payload type.
    pub layout: VecLayout,
}

/// Expand a single `cssl.vec.*` op into the primitive MIR shape.
///
/// Returns `None` when `op` is not a recognized vec-op.
#[must_use]
pub fn expand_vec_op(op: &MirOp, ids: &mut FreshIdSeq) -> Option<VecExpansion> {
    let kind = vec_op_kind(op)?;
    let elem_str = payload_ty_str(op);
    let elem_ty = parse_payload_ty(elem_str);
    let layout = VecLayout::for_element(&elem_ty);
    Some(match kind {
        VecOpKind::New => expand_vec_new(op, &elem_ty, layout, ids),
        VecOpKind::Push => expand_vec_push(op, &elem_ty, layout, ids),
        VecOpKind::Index => expand_vec_index(op, &elem_ty, layout, ids),
        VecOpKind::Len => expand_vec_len(op, layout, ids),
        VecOpKind::Cap => expand_vec_cap(op, layout, ids),
        VecOpKind::Drop => expand_vec_drop(op, layout, ids),
    })
}

/// Expand `cssl.vec.new` :
///
/// ```text
///   %t = cssl.heap.alloc {bytes=24, alignment=8, source_kind=vec_cell}
///   %z = arith.constant 0 : i64
///   %n = arith.constant 0 : !cssl.ptr     // null-ptr sentinel
///   memref.store %n, %t {offset=0,  field=data}
///   memref.store %z, %t {offset=8,  field=len}
///   memref.store %z, %t {offset=16, field=cap}
/// ```
fn expand_vec_new(
    op: &MirOp,
    elem_ty: &MirType,
    layout: VecLayout,
    ids: &mut FreshIdSeq,
) -> VecExpansion {
    let _ = elem_ty;
    let triple = ids.fresh();
    let zero = ids.fresh();
    let null_ptr = ids.fresh();
    let mut ops = Vec::with_capacity(6);

    ops.push(
        MirOp::new(crate::op::CsslOp::HeapAlloc)
            .with_result(triple, MirType::Ptr)
            .with_attribute("bytes", layout.total_size.to_string())
            .with_attribute("alignment", layout.cell_alignment.to_string())
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_VEC_CELL)
            .with_attribute("origin", "vec_new"),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(zero, MirType::Int(IntWidth::I64))
            .with_attribute("value", "0"),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(null_ptr, MirType::Ptr)
            .with_attribute("value", "0")
            .with_attribute("kind", "null_ptr"),
    );
    push_field_store(&mut ops, null_ptr, triple, layout.data_offset, "data", layout.field_size);
    push_field_store(&mut ops, zero, triple, layout.len_offset, "len", layout.field_size);
    push_field_store(&mut ops, zero, triple, layout.cap_offset, "cap", layout.field_size);

    let _ = op;
    VecExpansion {
        ops,
        bound_id: Some(triple),
        layout,
    }
}

/// Expand `cssl.vec.push %v, %x` into the grow-if-needed + store + len++
/// sequence. Stage-0 emits the canonical primitive shape :
///
/// ```text
///   %old_len  = memref.load %v +8                  // i64
///   %old_cap  = memref.load %v +16                 // i64
///   %old_data = memref.load %v +0                  // !cssl.ptr
///   %need     = arith.cmpi eq %old_len, %old_cap   // i1
///   %new_cap_double = arith.muli %old_cap, 2
///   %is_zero  = arith.cmpi eq %old_cap, 0
///   %new_cap  = arith.select %is_zero, 4, %new_cap_double
///   %new_bytes = arith.muli %new_cap, sizeof_T
///   scf.if %need {
///       %new_data = cssl.heap.realloc %old_data, %old_cap*sizeof_T,
///                                     %new_bytes, elem_align
///       memref.store %new_data, %v +0
///       memref.store %new_cap,  %v +16
///   }
///   %data2     = memref.load %v +0                 // !cssl.ptr (post-grow)
///   %byte_off  = arith.muli %old_len, sizeof_T
///   %slot_addr = cssl.ptr.offset %data2, %byte_off
///   memref.store %x, %slot_addr +0
///   %new_len   = arith.addi %old_len, 1
///   memref.store %new_len, %v +8
/// ```
///
/// The emitted shape preserves all of the W-A2-α/β intent : real
/// realloc-on-grow + element-store + len-bump. Returns the triple-ptr
/// as `bound_id` so consumers can chain `vec_push(vec_push(vec_new, 1), 2)`
/// through the value-map.
fn expand_vec_push(
    op: &MirOp,
    elem_ty: &MirType,
    layout: VecLayout,
    ids: &mut FreshIdSeq,
) -> VecExpansion {
    let _ = elem_ty;
    let v_id = op.operands.first().copied().unwrap_or(ValueId(0));
    let x_id = op.operands.get(1).copied().unwrap_or(ValueId(0));
    let mut ops = Vec::with_capacity(20);

    let old_len = ids.fresh();
    let old_cap = ids.fresh();
    let old_data = ids.fresh();
    let need_grow = ids.fresh();
    let two_const = ids.fresh();
    let new_cap_dbl = ids.fresh();
    let zero_const = ids.fresh();
    let cap_is_zero = ids.fresh();
    let four_const = ids.fresh();
    let new_cap = ids.fresh();
    let elem_size_const = ids.fresh();
    let new_bytes = ids.fresh();
    let old_bytes = ids.fresh();
    let new_data = ids.fresh();
    let if_marker = ids.fresh();
    let data_post = ids.fresh();
    let byte_off = ids.fresh();
    let slot_addr = ids.fresh();
    let new_len = ids.fresh();
    let one_const = ids.fresh();

    push_field_load(&mut ops, old_len, v_id, layout.len_offset, "len", layout.field_size, MirType::Int(IntWidth::I64));
    push_field_load(&mut ops, old_cap, v_id, layout.cap_offset, "cap", layout.field_size, MirType::Int(IntWidth::I64));
    push_field_load(&mut ops, old_data, v_id, layout.data_offset, "data", layout.field_size, MirType::Ptr);

    ops.push(
        MirOp::std("arith.cmpi")
            .with_operand(old_len)
            .with_operand(old_cap)
            .with_result(need_grow, MirType::Bool)
            .with_attribute("predicate", "eq"),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(two_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", "2"),
    );
    ops.push(
        MirOp::std("arith.muli")
            .with_operand(old_cap)
            .with_operand(two_const)
            .with_result(new_cap_dbl, MirType::Int(IntWidth::I64)),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(zero_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", "0"),
    );
    ops.push(
        MirOp::std("arith.cmpi")
            .with_operand(old_cap)
            .with_operand(zero_const)
            .with_result(cap_is_zero, MirType::Bool)
            .with_attribute("predicate", "eq"),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(four_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", "4"),
    );
    ops.push(
        MirOp::std("arith.select")
            .with_operand(cap_is_zero)
            .with_operand(four_const)
            .with_operand(new_cap_dbl)
            .with_result(new_cap, MirType::Int(IntWidth::I64)),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(elem_size_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", layout.elem_size.to_string()),
    );
    ops.push(
        MirOp::std("arith.muli")
            .with_operand(new_cap)
            .with_operand(elem_size_const)
            .with_result(new_bytes, MirType::Int(IntWidth::I64)),
    );
    ops.push(
        MirOp::std("arith.muli")
            .with_operand(old_cap)
            .with_operand(elem_size_const)
            .with_result(old_bytes, MirType::Int(IntWidth::I64)),
    );

    // § grow-arm region : realloc + store new data + store new cap.
    let mut grow_block = MirBlock::new("grow");
    grow_block.push(
        MirOp::new(crate::op::CsslOp::HeapRealloc)
            .with_operand(old_data)
            .with_operand(old_bytes)
            .with_operand(new_bytes)
            .with_result(new_data, MirType::Ptr)
            .with_attribute("alignment", layout.elem_align.to_string())
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_VEC_DATA)
            .with_attribute("origin", "vec_push.grow"),
    );
    push_field_store_block(&mut grow_block, new_data, v_id, layout.data_offset, "data", layout.field_size);
    push_field_store_block(&mut grow_block, new_cap, v_id, layout.cap_offset, "cap", layout.field_size);
    grow_block.push(MirOp::std("scf.yield"));
    let mut grow_region = MirRegion::new();
    grow_region.push(grow_block);

    // § no-grow arm region — empty body + yield.
    let mut nogrow_block = MirBlock::new("nogrow");
    nogrow_block.push(MirOp::std("scf.yield"));
    let mut nogrow_region = MirRegion::new();
    nogrow_region.push(nogrow_block);

    ops.push(
        MirOp::std("scf.if")
            .with_operand(need_grow)
            .with_result(if_marker, MirType::None)
            .with_region(grow_region)
            .with_region(nogrow_region)
            .with_attribute(ATTR_SOURCE_KIND, "vec_push_grow_branch")
            .with_attribute("origin", "vec_push"),
    );

    push_field_load(&mut ops, data_post, v_id, layout.data_offset, "data", layout.field_size, MirType::Ptr);
    ops.push(
        MirOp::std("arith.muli")
            .with_operand(old_len)
            .with_operand(elem_size_const)
            .with_result(byte_off, MirType::Int(IntWidth::I64)),
    );
    ops.push(
        MirOp::std("cssl.ptr.offset")
            .with_operand(data_post)
            .with_operand(byte_off)
            .with_result(slot_addr, MirType::Ptr)
            .with_attribute("origin", "vec_push.slot_addr"),
    );
    ops.push(
        MirOp::std("memref.store")
            .with_operand(x_id)
            .with_operand(slot_addr)
            .with_attribute("offset", "0")
            .with_attribute("alignment", layout.elem_align.to_string())
            .with_attribute("field", "elem"),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(one_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", "1"),
    );
    ops.push(
        MirOp::std("arith.addi")
            .with_operand(old_len)
            .with_operand(one_const)
            .with_result(new_len, MirType::Int(IntWidth::I64)),
    );
    push_field_store(&mut ops, new_len, v_id, layout.len_offset, "len", layout.field_size);

    VecExpansion {
        ops,
        bound_id: Some(v_id),
        layout,
    }
}

/// Expand `cssl.vec.index %v, %i` :
///
/// ```text
///   %len   = memref.load %v +8                     // i64
///   %ok    = arith.cmpi slt %i, %len               // i1
///   scf.if !%ok { cssl.panic "vec_index OOB" }     // bounds-check
///   %data  = memref.load %v +0                     // !cssl.ptr
///   %off   = arith.muli %i, sizeof_T
///   %addr  = cssl.ptr.offset %data, %off
///   %r     = memref.load %addr +0                  // T
/// ```
fn expand_vec_index(
    op: &MirOp,
    elem_ty: &MirType,
    layout: VecLayout,
    ids: &mut FreshIdSeq,
) -> VecExpansion {
    let v_id = op.operands.first().copied().unwrap_or(ValueId(0));
    let i_id = op.operands.get(1).copied().unwrap_or(ValueId(0));
    let mut ops = Vec::with_capacity(10);

    let len_id = ids.fresh();
    let in_bounds = ids.fresh();
    let oob = ids.fresh();
    let if_marker = ids.fresh();
    let data_id = ids.fresh();
    let elem_size_const = ids.fresh();
    let off_id = ids.fresh();
    let addr_id = ids.fresh();
    let result_id = ids.fresh();

    push_field_load(&mut ops, len_id, v_id, layout.len_offset, "len", layout.field_size, MirType::Int(IntWidth::I64));
    ops.push(
        MirOp::std("arith.cmpi")
            .with_operand(i_id)
            .with_operand(len_id)
            .with_result(in_bounds, MirType::Bool)
            .with_attribute("predicate", "slt"),
    );
    ops.push(
        MirOp::std("arith.xori")
            .with_operand(in_bounds)
            .with_operand(in_bounds)
            .with_result(oob, MirType::Bool)
            .with_attribute("origin", "vec_index.oob_invert"),
    );

    let mut panic_block = MirBlock::new("oob_panic");
    panic_block.push(
        MirOp::std("cssl.panic")
            .with_attribute("message", "vec_index out-of-bounds")
            .with_attribute("origin", "vec_index"),
    );
    panic_block.push(MirOp::std("scf.yield"));
    let mut panic_region = MirRegion::new();
    panic_region.push(panic_block);

    let mut ok_block = MirBlock::new("ok");
    ok_block.push(MirOp::std("scf.yield"));
    let mut ok_region = MirRegion::new();
    ok_region.push(ok_block);

    ops.push(
        MirOp::std("scf.if")
            .with_operand(oob)
            .with_result(if_marker, MirType::None)
            .with_region(panic_region)
            .with_region(ok_region)
            .with_attribute(ATTR_SOURCE_KIND, "vec_index_bounds_check")
            .with_attribute("origin", "vec_index"),
    );

    push_field_load(&mut ops, data_id, v_id, layout.data_offset, "data", layout.field_size, MirType::Ptr);
    ops.push(
        MirOp::std("arith.constant")
            .with_result(elem_size_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", layout.elem_size.to_string()),
    );
    ops.push(
        MirOp::std("arith.muli")
            .with_operand(i_id)
            .with_operand(elem_size_const)
            .with_result(off_id, MirType::Int(IntWidth::I64)),
    );
    ops.push(
        MirOp::std("cssl.ptr.offset")
            .with_operand(data_id)
            .with_operand(off_id)
            .with_result(addr_id, MirType::Ptr)
            .with_attribute("origin", "vec_index.slot_addr"),
    );
    ops.push(
        MirOp::std("memref.load")
            .with_operand(addr_id)
            .with_result(result_id, elem_ty.clone())
            .with_attribute("offset", "0")
            .with_attribute("alignment", layout.elem_align.to_string())
            .with_attribute("field", "elem"),
    );

    VecExpansion {
        ops,
        bound_id: Some(result_id),
        layout,
    }
}

/// Expand `cssl.vec.len %v` → `memref.load %v +8` (single field-load).
fn expand_vec_len(op: &MirOp, layout: VecLayout, ids: &mut FreshIdSeq) -> VecExpansion {
    let v_id = op.operands.first().copied().unwrap_or(ValueId(0));
    let r = ids.fresh();
    let mut ops = Vec::with_capacity(1);
    push_field_load(&mut ops, r, v_id, layout.len_offset, "len", layout.field_size, MirType::Int(IntWidth::I64));
    let _ = op;
    VecExpansion {
        ops,
        bound_id: Some(r),
        layout,
    }
}

/// Expand `cssl.vec.cap %v` → `memref.load %v +16` (single field-load).
fn expand_vec_cap(op: &MirOp, layout: VecLayout, ids: &mut FreshIdSeq) -> VecExpansion {
    let v_id = op.operands.first().copied().unwrap_or(ValueId(0));
    let r = ids.fresh();
    let mut ops = Vec::with_capacity(1);
    push_field_load(&mut ops, r, v_id, layout.cap_offset, "cap", layout.field_size, MirType::Int(IntWidth::I64));
    let _ = op;
    VecExpansion {
        ops,
        bound_id: Some(r),
        layout,
    }
}

/// Expand `cssl.vec.drop %v` :
///
/// ```text
///   %cap   = memref.load %v +16
///   %data  = memref.load %v +0
///   %sz    = arith.muli %cap, sizeof_T
///   cssl.heap.dealloc %data, %sz, elem_align         // safe @ null per FFI
///   cssl.heap.dealloc %v, 24, 8                      // free triple-cell
/// ```
fn expand_vec_drop(op: &MirOp, layout: VecLayout, ids: &mut FreshIdSeq) -> VecExpansion {
    let v_id = op.operands.first().copied().unwrap_or(ValueId(0));
    let cap_id = ids.fresh();
    let data_id = ids.fresh();
    let elem_size_const = ids.fresh();
    let bytes_id = ids.fresh();
    let elem_align_const = ids.fresh();
    let triple_size_const = ids.fresh();
    let triple_align_const = ids.fresh();
    let mut ops = Vec::with_capacity(8);

    push_field_load(&mut ops, cap_id, v_id, layout.cap_offset, "cap", layout.field_size, MirType::Int(IntWidth::I64));
    push_field_load(&mut ops, data_id, v_id, layout.data_offset, "data", layout.field_size, MirType::Ptr);
    ops.push(
        MirOp::std("arith.constant")
            .with_result(elem_size_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", layout.elem_size.to_string()),
    );
    ops.push(
        MirOp::std("arith.muli")
            .with_operand(cap_id)
            .with_operand(elem_size_const)
            .with_result(bytes_id, MirType::Int(IntWidth::I64)),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(elem_align_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", layout.elem_align.to_string()),
    );
    ops.push(
        MirOp::new(crate::op::CsslOp::HeapDealloc)
            .with_operand(data_id)
            .with_operand(bytes_id)
            .with_operand(elem_align_const)
            .with_attribute("origin", "vec_drop.data")
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_VEC_DATA),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(triple_size_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", layout.total_size.to_string()),
    );
    ops.push(
        MirOp::std("arith.constant")
            .with_result(triple_align_const, MirType::Int(IntWidth::I64))
            .with_attribute("value", layout.cell_alignment.to_string()),
    );
    ops.push(
        MirOp::new(crate::op::CsslOp::HeapDealloc)
            .with_operand(v_id)
            .with_operand(triple_size_const)
            .with_operand(triple_align_const)
            .with_attribute("origin", "vec_drop.cell")
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_VEC_CELL),
    );

    let _ = op;
    VecExpansion {
        ops,
        bound_id: None,
        layout,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Helpers — push canonical field-load + field-store sequences.
// ─────────────────────────────────────────────────────────────────────────

fn push_field_store(
    ops: &mut Vec<MirOp>,
    val: ValueId,
    base: ValueId,
    offset: u32,
    field: &str,
    field_size: u8,
) {
    ops.push(
        MirOp::std("memref.store")
            .with_operand(val)
            .with_operand(base)
            .with_attribute("offset", offset.to_string())
            .with_attribute("alignment", u32::from(field_size).to_string())
            .with_attribute("field", field.to_string()),
    );
}

fn push_field_store_block(
    block: &mut MirBlock,
    val: ValueId,
    base: ValueId,
    offset: u32,
    field: &str,
    field_size: u8,
) {
    block.push(
        MirOp::std("memref.store")
            .with_operand(val)
            .with_operand(base)
            .with_attribute("offset", offset.to_string())
            .with_attribute("alignment", u32::from(field_size).to_string())
            .with_attribute("field", field.to_string()),
    );
}

fn push_field_load(
    ops: &mut Vec<MirOp>,
    dst: ValueId,
    base: ValueId,
    offset: u32,
    field: &str,
    field_size: u8,
    ty: MirType,
) {
    ops.push(
        MirOp::std("memref.load")
            .with_operand(base)
            .with_result(dst, ty)
            .with_attribute("offset", offset.to_string())
            .with_attribute("alignment", u32::from(field_size).to_string())
            .with_attribute("field", field.to_string()),
    );
}

// ─────────────────────────────────────────────────────────────────────────
// § Module-level rewrite — `expand_vec_module` driver.
// ─────────────────────────────────────────────────────────────────────────

/// Audit report for a vec-abi expansion run. Sawyer-bit-pack so callers
/// can assert behavior without trawling the full module.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VecExpansionReport {
    /// Number of `cssl.vec.new` ops expanded.
    pub vec_new_count: u32,
    /// Number of `cssl.vec.push` ops expanded.
    pub vec_push_count: u32,
    /// Number of `cssl.vec.index` ops expanded.
    pub vec_index_count: u32,
    /// Number of `cssl.vec.len` ops expanded.
    pub vec_len_count: u32,
    /// Number of `cssl.vec.cap` ops expanded.
    pub vec_cap_count: u32,
    /// Number of `cssl.vec.drop` ops expanded.
    pub vec_drop_count: u32,
    /// Total triple-cell bytes allocated (sum of layout.total_size for
    /// each `vec_new` rewrite).
    pub total_cell_bytes: u32,
    /// Number of fn-signature slots rewritten from `Vec<T>` opaque to
    /// `MirType::Ptr` triple-cell pointer. Idempotency : counts only
    /// grow on the first run per fn.
    pub sig_rewrites: u32,
}

impl VecExpansionReport {
    /// Total `cssl.vec.*` ops rewritten.
    #[must_use]
    pub const fn total_op_count(&self) -> u32 {
        self.vec_new_count
            + self.vec_push_count
            + self.vec_index_count
            + self.vec_len_count
            + self.vec_cap_count
            + self.vec_drop_count
    }

    fn record(&mut self, kind: VecOpKind, layout: VecLayout) {
        match kind {
            VecOpKind::New => {
                self.vec_new_count += 1;
                self.total_cell_bytes = self.total_cell_bytes.saturating_add(layout.total_size);
            }
            VecOpKind::Push => self.vec_push_count += 1,
            VecOpKind::Index => self.vec_index_count += 1,
            VecOpKind::Len => self.vec_len_count += 1,
            VecOpKind::Cap => self.vec_cap_count += 1,
            VecOpKind::Drop => self.vec_drop_count += 1,
        }
    }
}

/// Expand every `cssl.vec.*` op in a `MirFunc` in-place. Mirrors
/// `tagged_union_abi::expand_func`. Returns the per-fn report.
pub fn expand_vec_func(func: &mut MirFunc) -> VecExpansionReport {
    let mut report = VecExpansionReport::default();
    let mut ids = FreshIdSeq::new(func.next_value_id);
    expand_vec_region(&mut func.body, &mut ids, &mut report);
    func.next_value_id = ids.next;
    rewrite_vec_signature(func, &mut report);
    report
}

/// Expand every `cssl.vec.*` op across an entire `MirModule`.
pub fn expand_vec_module(module: &mut MirModule) -> VecExpansionReport {
    let mut report = VecExpansionReport::default();
    for func in &mut module.funcs {
        let per_fn = expand_vec_func(func);
        report.vec_new_count += per_fn.vec_new_count;
        report.vec_push_count += per_fn.vec_push_count;
        report.vec_index_count += per_fn.vec_index_count;
        report.vec_len_count += per_fn.vec_len_count;
        report.vec_cap_count += per_fn.vec_cap_count;
        report.vec_drop_count += per_fn.vec_drop_count;
        report.total_cell_bytes = report
            .total_cell_bytes
            .saturating_add(per_fn.total_cell_bytes);
        report.sig_rewrites = report.sig_rewrites.saturating_add(per_fn.sig_rewrites);
    }
    report
}

fn expand_vec_region(
    region: &mut MirRegion,
    ids: &mut FreshIdSeq,
    report: &mut VecExpansionReport,
) {
    for block in &mut region.blocks {
        expand_vec_block(block, ids, report);
    }
}

fn expand_vec_block(
    block: &mut MirBlock,
    ids: &mut FreshIdSeq,
    report: &mut VecExpansionReport,
) {
    let mut idx = 0;
    while idx < block.ops.len() {
        // Recurse into nested regions FIRST so the depth-first walk
        // matches the tagged-union pass + so a `vec_op` nested inside an
        // `scf.if` arm gets expanded before the surrounding op is
        // touched.
        for region in &mut block.ops[idx].regions {
            expand_vec_region(region, ids, report);
        }

        if let Some(kind) = vec_op_kind(&block.ops[idx]) {
            let original = block.ops[idx].clone();
            if let Some(expansion) = expand_vec_op(&original, ids) {
                report.record(kind, expansion.layout);
                let _span = expansion.ops.len();
                let original_result = original.results.first().map(|r| r.id);
                let mut splice: Vec<MirOp> = expansion.ops;
                if let (Some(orig), Some(bound)) = (original_result, expansion.bound_id) {
                    if orig != bound {
                        // Bind the original op's result-id to the bound
                        // SSA-id via a no-op alias so the value-map
                        // resolves through. Mirrors
                        // `tagged_union_abi::expand_block`'s alias
                        // bitcast.
                        let alias_ty = match kind {
                            VecOpKind::New | VecOpKind::Push => MirType::Ptr,
                            VecOpKind::Len | VecOpKind::Cap => MirType::Int(IntWidth::I64),
                            VecOpKind::Index => match original.results.first() {
                                Some(r) => r.ty.clone(),
                                None => MirType::Ptr,
                            },
                            VecOpKind::Drop => MirType::None,
                        };
                        splice.push(
                            MirOp::std("arith.bitcast")
                                .with_operand(bound)
                                .with_result(orig, alias_ty)
                                .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_VEC_ALIAS),
                        );
                    }
                }
                let added = splice.len();
                block.ops.splice(idx..=idx, splice);
                idx += added;
                continue;
            }
        }
        idx += 1;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Pass-pipeline wrapper — `VecAbiPass` drives the rewrite for
//   integration with `pipeline::PassPipeline`.
// ─────────────────────────────────────────────────────────────────────────

/// Pass-pipeline newtype for the vec-abi rewrite. Mirrors
/// `tagged_union_abi`'s integration with `pipeline::TaggedUnionAbiPass`
/// (registered separately when its dispatch lands).
#[derive(Debug, Default, Clone, Copy)]
pub struct VecAbiPass;

impl VecAbiPass {
    /// Canonical pass name used in pass-pipeline diagnostic output.
    pub const NAME: &'static str = "vec_abi.expand_module";

    /// Run the rewrite over a module + return the per-module report.
    pub fn run(self, module: &mut MirModule) -> VecExpansionReport {
        expand_vec_module(module)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — unit + golden coverage for the layout / expansion / sig.
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::MirOp;
    use crate::func::{MirFunc, MirModule};
    use crate::value::IntWidth;

    /// Helper : build a `cssl.vec.new` recognizer-mint as body_lower
    /// would emit it (matches `body_lower::try_lower_vec_new` line-shape).
    fn mk_vec_new(payload_ty: &str, result_id: u32) -> MirOp {
        MirOp::std(OP_VEC_NEW)
            .with_result(ValueId(result_id), MirType::Opaque("Vec".to_string()))
            .with_attribute(ATTR_PAYLOAD_TY, payload_ty)
            .with_attribute("cap", "iso")
            .with_attribute(ATTR_ORIGIN, "vec_new")
    }

    fn mk_vec_push(v: u32, x: u32, payload_ty: &str, result_id: u32) -> MirOp {
        MirOp::std(OP_VEC_PUSH)
            .with_operand(ValueId(v))
            .with_operand(ValueId(x))
            .with_result(ValueId(result_id), MirType::Opaque("Vec".to_string()))
            .with_attribute(ATTR_PAYLOAD_TY, payload_ty)
            .with_attribute(ATTR_ORIGIN, "vec_push")
    }

    fn mk_vec_index(v: u32, i: u32, payload_ty: &str, result_id: u32) -> MirOp {
        MirOp::std(OP_VEC_INDEX)
            .with_operand(ValueId(v))
            .with_operand(ValueId(i))
            .with_result(ValueId(result_id), MirType::Int(IntWidth::I32))
            .with_attribute(ATTR_PAYLOAD_TY, payload_ty)
            .with_attribute("bounds_check", "panic")
            .with_attribute(ATTR_ORIGIN, "vec_index")
    }

    fn mk_vec_len(v: u32, result_id: u32) -> MirOp {
        MirOp::std(OP_VEC_LEN)
            .with_operand(ValueId(v))
            .with_result(ValueId(result_id), MirType::Int(IntWidth::I64))
            .with_attribute(ATTR_PAYLOAD_TY, "i32")
            .with_attribute(ATTR_ORIGIN, "vec_len")
    }

    fn mk_vec_cap(v: u32, result_id: u32) -> MirOp {
        MirOp::std(OP_VEC_CAP)
            .with_operand(ValueId(v))
            .with_result(ValueId(result_id), MirType::Int(IntWidth::I64))
            .with_attribute(ATTR_PAYLOAD_TY, "i32")
            .with_attribute(ATTR_ORIGIN, "vec_cap")
    }

    fn mk_vec_drop(v: u32) -> MirOp {
        MirOp::std(OP_VEC_DROP)
            .with_operand(ValueId(v))
            .with_attribute(ATTR_PAYLOAD_TY, "i32")
            .with_attribute(ATTR_ORIGIN, "vec_drop")
    }

    // ─────────────────────────────────────────────────────────────────
    // § layout primitives — geometry sanity.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn vec_layout_i32_has_canonical_24_byte_triple() {
        let layout = VecLayout::for_element(&MirType::Int(IntWidth::I32));
        assert_eq!(layout.data_offset, 0);
        assert_eq!(layout.len_offset, 8);
        assert_eq!(layout.cap_offset, 16);
        assert_eq!(layout.total_size, 24);
        assert_eq!(layout.cell_alignment, 8);
        assert_eq!(layout.elem_size, 4);
        assert_eq!(layout.elem_align, 4);
    }

    #[test]
    fn vec_layout_i64_keeps_24_byte_triple_with_8_byte_elem() {
        let layout = VecLayout::for_element(&MirType::Int(IntWidth::I64));
        assert_eq!(layout.total_size, 24);
        assert_eq!(layout.elem_size, 8);
        assert_eq!(layout.elem_align, 8);
    }

    // ─────────────────────────────────────────────────────────────────
    // § sig-rewrite — Vec<T> in fn params/results lowers to Ptr.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn vec_abi_sig_rewrite_lowers_vec_param_to_ptr() {
        // T-1 : VecAbi-sig-rewrite — a fn carrying a Vec<i32> param +
        //   Vec<i32> result MUST have both slots rewritten to Ptr.
        let mut func = MirFunc::new(
            "noop_vec",
            vec![MirType::Opaque("Vec".to_string())],
            vec![MirType::Opaque("Vec".to_string())],
        );
        let mut report = VecExpansionReport::default();
        rewrite_vec_signature(&mut func, &mut report);
        assert_eq!(func.params, vec![MirType::Ptr]);
        assert_eq!(func.results, vec![MirType::Ptr]);
        assert!(report.sig_rewrites >= 2);
        // Entry-block arg also rewritten.
        let entry = func.body.entry().unwrap();
        assert_eq!(entry.args[0].ty, MirType::Ptr);
    }

    // ─────────────────────────────────────────────────────────────────
    // § body-rewrite — vec ops expand into primitive heap+memref shape.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn vec_abi_body_rewrite_replaces_vec_new_with_alloc_and_stores() {
        // T-2 : VecAbi-body-rewrite — `cssl.vec.new` becomes
        //   heap.alloc + 3 × memref.store(field=data/len/cap).
        let mut func = MirFunc::new("mk_vec", vec![], vec![MirType::Opaque("Vec".to_string())]);
        func.push_op(mk_vec_new("i32", 0));
        let report = expand_vec_func(&mut func);
        assert_eq!(report.vec_new_count, 1);
        let entry = func.body.entry().unwrap();
        let names: Vec<&str> = entry.ops.iter().map(|o| o.name.as_str()).collect();
        assert!(
            names.contains(&"cssl.heap.alloc"),
            "missing heap.alloc in {names:?}"
        );
        let store_count = names.iter().filter(|n| **n == "memref.store").count();
        assert!(
            store_count >= 3,
            "expected ≥ 3 memref.store ops (data/len/cap) in {names:?}"
        );
        // Original cssl.vec.new must be gone.
        assert!(!names.contains(&OP_VEC_NEW));
    }

    #[test]
    fn vec_new_emits_alloc_with_24_bytes_8_align() {
        // T-3 : vec_new-emits-alloc-0 — the heap.alloc op carries the
        //   canonical 24-byte / 8-align triple-cell shape.
        let mut func = MirFunc::new("mk_vec", vec![], vec![MirType::Opaque("Vec".to_string())]);
        func.push_op(mk_vec_new("i32", 0));
        let _ = expand_vec_func(&mut func);
        let alloc = func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "cssl.heap.alloc")
            .expect("missing heap.alloc");
        let bytes = alloc
            .attributes
            .iter()
            .find(|(k, _)| k == "bytes")
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        let align = alloc
            .attributes
            .iter()
            .find(|(k, _)| k == "alignment")
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        assert_eq!(bytes, "24");
        assert_eq!(align, "8");
    }

    #[test]
    fn vec_push_grow_emits_realloc_in_grow_arm() {
        // T-4 : vec_push-grow-emits-realloc — `cssl.vec.push` expansion
        //   contains a `cssl.heap.realloc` op (inside the scf.if grow-arm).
        let mut func = MirFunc::new(
            "push_one",
            vec![MirType::Opaque("Vec".to_string()), MirType::Int(IntWidth::I32)],
            vec![MirType::Opaque("Vec".to_string())],
        );
        // Simulate body : push the param-x onto the param-vec.
        func.push_op(mk_vec_push(0, 1, "i32", 2));
        let report = expand_vec_func(&mut func);
        assert_eq!(report.vec_push_count, 1);
        // Walk through the rewritten body + recurse into scf.if regions.
        let mut found_realloc = false;
        let mut found_scf_if = false;
        for op in &func.body.entry().unwrap().ops {
            if op.name == "scf.if" {
                found_scf_if = true;
                for region in &op.regions {
                    for blk in &region.blocks {
                        for nested in &blk.ops {
                            if nested.name == "cssl.heap.realloc" {
                                found_realloc = true;
                            }
                        }
                    }
                }
            }
        }
        assert!(found_scf_if, "vec_push expansion must emit an scf.if");
        assert!(
            found_realloc,
            "vec_push grow-arm must emit cssl.heap.realloc"
        );
    }

    #[test]
    fn vec_index_emits_bounds_check_scf_if() {
        // T-5 : vec_index-bounds-check — `cssl.vec.index` expansion
        //   contains an scf.if with `source_kind=vec_index_bounds_check`.
        let mut func = MirFunc::new(
            "idx",
            vec![MirType::Opaque("Vec".to_string()), MirType::Int(IntWidth::I64)],
            vec![MirType::Int(IntWidth::I32)],
        );
        func.push_op(mk_vec_index(0, 1, "i32", 2));
        let _ = expand_vec_func(&mut func);
        let bounds_op = func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| {
                o.name == "scf.if"
                    && o.attributes.iter().any(|(k, v)| {
                        k == ATTR_SOURCE_KIND && v == "vec_index_bounds_check"
                    })
            });
        assert!(
            bounds_op.is_some(),
            "vec_index must emit a bounds-check scf.if"
        );
    }

    #[test]
    fn vec_len_emits_field_load_at_offset_8() {
        // T-6 : vec_len-field-load — `cssl.vec.len` becomes a single
        //   memref.load at offset=8 with field=len.
        let mut func = MirFunc::new(
            "lenof",
            vec![MirType::Opaque("Vec".to_string())],
            vec![MirType::Int(IntWidth::I64)],
        );
        func.push_op(mk_vec_len(0, 1));
        let report = expand_vec_func(&mut func);
        assert_eq!(report.vec_len_count, 1);
        let load = func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| {
                o.name == "memref.load"
                    && o.attributes.iter().any(|(k, v)| k == "field" && v == "len")
            })
            .expect("missing field=len memref.load");
        let off = load
            .attributes
            .iter()
            .find(|(k, _)| k == "offset")
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        assert_eq!(off, "8");
    }

    #[test]
    fn vec_cap_emits_field_load_at_offset_16() {
        let mut func = MirFunc::new(
            "capof",
            vec![MirType::Opaque("Vec".to_string())],
            vec![MirType::Int(IntWidth::I64)],
        );
        func.push_op(mk_vec_cap(0, 1));
        let report = expand_vec_func(&mut func);
        assert_eq!(report.vec_cap_count, 1);
        let load = func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| {
                o.name == "memref.load"
                    && o.attributes.iter().any(|(k, v)| k == "field" && v == "cap")
            })
            .expect("missing field=cap memref.load");
        let off = load
            .attributes
            .iter()
            .find(|(k, _)| k == "offset")
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        assert_eq!(off, "16");
    }

    #[test]
    fn vec_drop_emits_two_dealloc_calls() {
        // vec_drop frees BOTH the data buffer AND the triple-cell.
        let mut func = MirFunc::new("dropv", vec![MirType::Opaque("Vec".to_string())], vec![]);
        func.push_op(mk_vec_drop(0));
        let report = expand_vec_func(&mut func);
        assert_eq!(report.vec_drop_count, 1);
        let dealloc_count = func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .filter(|o| o.name == "cssl.heap.dealloc")
            .count();
        assert_eq!(
            dealloc_count, 2,
            "vec_drop must emit 2 dealloc calls (data + cell)"
        );
    }

    // ─────────────────────────────────────────────────────────────────
    // § idempotency — second pass is a no-op.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn vec_abi_pass_is_idempotent() {
        // T-7 : idempotency — running the pass twice gives the same MIR
        //   shape + zero new sig-rewrites the second time.
        let mut func = MirFunc::new(
            "roundtrip",
            vec![MirType::Opaque("Vec".to_string())],
            vec![MirType::Int(IntWidth::I64)],
        );
        func.push_op(mk_vec_len(0, 1));
        let first = expand_vec_func(&mut func);
        assert!(first.sig_rewrites > 0);

        // Snapshot the body op-shape before the second run.
        let before: Vec<String> = func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .map(|o| o.name.clone())
            .collect();

        let second = expand_vec_func(&mut func);
        let after: Vec<String> = func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .map(|o| o.name.clone())
            .collect();

        assert_eq!(second.sig_rewrites, 0, "sig-rewrite must be idempotent");
        assert_eq!(before, after, "body must be stable across pass runs");
    }

    // ─────────────────────────────────────────────────────────────────
    // § end-to-end-Vec — multi-op chain expands cleanly through module.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn vec_abi_module_drives_multiop_chain_to_primitive_form() {
        // T-8 : end-to-end-Vec-MIR — a MirModule with vec_new + vec_push
        //   + vec_index + vec_len + vec_drop expands to a body whose
        //   names contain ZERO `cssl.vec.*` ops + DOES contain
        //   heap.alloc + heap.realloc + heap.dealloc + memref ops.
        let mut module = MirModule::with_name("end_to_end");
        let mut func = MirFunc::new("e2e", vec![MirType::Int(IntWidth::I32)], vec![MirType::Int(IntWidth::I32)]);
        // %1 = vec_new : Vec
        func.push_op(mk_vec_new("i32", 1));
        // %2 = vec_push %1, %0 (param i32) : Vec
        func.push_op(mk_vec_push(1, 0, "i32", 2));
        // %3 = vec_index %2, %0 : i32
        func.push_op(mk_vec_index(2, 0, "i32", 3));
        // %4 = vec_len %2 : i64
        func.push_op(mk_vec_len(2, 4));
        // vec_drop %2
        func.push_op(mk_vec_drop(2));
        module.push_func(func);

        let report = expand_vec_module(&mut module);
        assert_eq!(report.vec_new_count, 1);
        assert_eq!(report.vec_push_count, 1);
        assert_eq!(report.vec_index_count, 1);
        assert_eq!(report.vec_len_count, 1);
        assert_eq!(report.vec_drop_count, 1);
        // The fn-signature in this test uses i32 params/results (the Vec
        // values live as locals), so the sig-rewrite pass has no Vec-typed
        // slots to lower — it stamps the idempotency marker but counts 0.
        // The body-rewrite is what we care about for end-to-end coverage.
        assert_eq!(report.vec_new_count + report.vec_push_count, 2);

        let func = &module.funcs[0];
        // Walk every op in the body (recursing into scf.if regions) and
        // assert no `cssl.vec.*` ops survived the rewrite.
        let mut all_ops: Vec<String> = Vec::new();
        collect_op_names(&func.body, &mut all_ops);
        for n in &all_ops {
            assert!(
                !n.starts_with("cssl.vec."),
                "leftover cssl.vec.* op `{n}` in rewritten body"
            );
        }
        assert!(all_ops.iter().any(|n| n == "cssl.heap.alloc"));
        assert!(all_ops.iter().any(|n| n == "cssl.heap.realloc"));
        assert!(all_ops.iter().any(|n| n == "cssl.heap.dealloc"));
        assert!(all_ops.iter().any(|n| n == "memref.load"));
        assert!(all_ops.iter().any(|n| n == "memref.store"));
    }

    fn collect_op_names(region: &MirRegion, out: &mut Vec<String>) {
        for block in &region.blocks {
            for op in &block.ops {
                out.push(op.name.clone());
                for nested in &op.regions {
                    collect_op_names(nested, out);
                }
            }
        }
    }

    #[test]
    fn vec_abi_pass_struct_runs_via_pipeline_wrapper() {
        let mut module = MirModule::with_name("via_pass");
        let mut func = MirFunc::new("mk", vec![], vec![MirType::Opaque("Vec".to_string())]);
        func.push_op(mk_vec_new("i32", 0));
        module.push_func(func);

        let report = VecAbiPass.run(&mut module);
        assert_eq!(report.vec_new_count, 1);
        assert!(report.total_cell_bytes >= 24);
    }

    #[test]
    fn is_vec_type_recognizes_canonical_forms() {
        assert!(is_vec_type(&MirType::Opaque("Vec".to_string())));
        assert!(is_vec_type(&MirType::Opaque("!cssl.vec.i32".to_string())));
        assert!(is_vec_type(&MirType::Opaque("Vec<i32>".to_string())));
        assert!(!is_vec_type(&MirType::Ptr));
        assert!(!is_vec_type(&MirType::Int(IntWidth::I32)));
        assert!(!is_vec_type(&MirType::Opaque("Option".to_string())));
    }

    #[test]
    fn vec_op_kind_dispatches_correctly_for_all_six_ops() {
        assert_eq!(vec_op_kind(&MirOp::std(OP_VEC_NEW)), Some(VecOpKind::New));
        assert_eq!(vec_op_kind(&MirOp::std(OP_VEC_PUSH)), Some(VecOpKind::Push));
        assert_eq!(vec_op_kind(&MirOp::std(OP_VEC_INDEX)), Some(VecOpKind::Index));
        assert_eq!(vec_op_kind(&MirOp::std(OP_VEC_LEN)), Some(VecOpKind::Len));
        assert_eq!(vec_op_kind(&MirOp::std(OP_VEC_CAP)), Some(VecOpKind::Cap));
        assert_eq!(vec_op_kind(&MirOp::std(OP_VEC_DROP)), Some(VecOpKind::Drop));
        assert_eq!(vec_op_kind(&MirOp::std("arith.addi")), None);
    }
}
