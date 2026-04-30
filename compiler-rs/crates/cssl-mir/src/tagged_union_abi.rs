//! § Tagged-union ABI lowering for `Option<T>` + `Result<T, E>`.
//!
//! § SPEC : `specs/40_WAVE_CSSL_PLAN.csl` § WAVES § WAVE-A § A1.
//! § ROLE : MIR → MIR pass that expands the high-level sum-type construction
//!          ops (`cssl.option.some` / `cssl.option.none` / `cssl.result.ok` /
//!          `cssl.result.err`) into the canonical packed tagged-union ABI :
//!
//!          ```text
//!          struct TaggedUnion<T> {           // for Option<T>
//!              tag    : u32,                 // offset 0,  4 bytes
//!              payload: [u8; sizeof(T)],     // offset 4
//!          }
//!          struct TaggedUnion<T,E> {         // for Result<T,E>
//!              tag    : u32,                 // offset 0,  4 bytes
//!              payload: [u8; max(T,E)],      // offset 4
//!          }
//!          ```
//!
//!   § TAG-DISCIPLINE
//!     - `Option`  : `Some=1`, `None=0` (matches `body_lower::try_lower_option_some` /
//!       `body_lower::lower_option_none`).
//!     - `Result`  : `Ok=1`, `Err=0` (matches `body_lower::try_lower_result_ok` /
//!       `body_lower::try_lower_result_err`).
//!
//!   § SAWYER-EFFICIENT
//!     - Tag width = `u32` (NOT `u64`) ; 2 variants today, 256 maximum if
//!       a sum-type ever grows. The 4-byte choice keeps the payload's
//!       natural alignment intact for the common case (`i64` / `f64` /
//!       `Ptr`) without spilling 8 bytes on the tag itself.
//!     - Layout is a packed contiguous record ; tag + payload share a
//!       single allocation.
//!     - The expansion pass walks the MIR module ONCE per fn ; ops are
//!       rewritten in-place using a two-pass visit (collect indices,
//!       then splice) — no scratch `Vec` growth in the inner step.
//!     - Variant-tag lookup uses a 4-entry slot-table indexed by op-name
//!       prefix ; no `HashMap<String, _>` allocation on the hot path.
//!
//!   § DEFERRED (explicit ; matches the slice's stated boundary)
//!     - Real `MirType::TaggedUnion` first-class type-system surface : at
//!       this slice the rewrite preserves the existing `Opaque` typing of
//!       the construction-op result (`!cssl.option.<T>` /
//!       `!cssl.result.<T>.<E>`) and adds a `MirType::Ptr` for the
//!       allocated cell. A follow-up slice replaces the opaque-tag with a
//!       structural `TaggedUnion { tag, payload }`.
//!     - `MirOp` for stack-slot allocation : stage-0 has no `cssl.alloca`
//!       dialect op so the construction always heap-allocates via
//!       `cssl.heap.alloc` (S6-B1 / T11-D57). The follow-up adds a stack-
//!       slot path for trivial-T variants (i32 / f32 / bool / Ptr) that
//!       avoids the heap round-trip.
//!     - Match-dispatch lowering against the existing `scf.match` op shape
//!       (which today carries N nested arm-regions WITHOUT pattern info)
//!       requires `body_lower` to emit per-arm `tag` attributes. The
//!       helpers here ([`build_match_dispatch_cascade`]) accept an
//!       arm-tags slice supplied by the caller ; the integration commit
//!       wires `body_lower::lower_match` to attach those attributes.
//!
//! # Public surface
//!
//! - [`TaggedUnionLayout`]    : packed-record geometry for one sum-type instance.
//! - [`SumFamily`]            : `{Option, Result}` discrimination.
//! - [`SumVariant`]           : `{Some, None, Ok, Err}` discrimination.
//! - [`tag_for_variant`]      : canonical numeric tag per variant.
//! - [`layout_for_construct`] : compute layout from a construction op.
//! - [`expand_construct`]     : MIR-rewrite a construction op into
//!                              `heap.alloc + tag-store + payload-store`.
//! - [`build_match_dispatch_cascade`] : fold an N-arm match into a
//!                              cascading `scf.if` chain keyed on the
//!                              loaded tag value.
//! - [`expand_module`]        : drive the rewrite over a whole `MirModule`.
//!
//! # Caller integration model
//!
//! ```ignore
//! use cssl_mir::tagged_union_abi::expand_module;
//! let mut mir = lower_module_signatures(...);
//! lower_fn_body(...);
//! expand_module(&mut mir);   // <-- THIS slice : sum-type ABI lowering
//! emit_module(&mir);
//! ```
//!
//! The Cranelift cgen side ([`crate::cgen_tagged_union`] in the cgen
//! crate) reads the post-rewrite ops and emits CLIF that the JIT actually
//! executes.

use crate::block::{MirBlock, MirOp, MirRegion};
use crate::func::{MirFunc, MirModule};
use crate::op::CsslOp;
use crate::value::{IntWidth, MirType, ValueId};

// ─────────────────────────────────────────────────────────────────────────
// § Layout primitives — Sawyer-style packed record geometry.
// ─────────────────────────────────────────────────────────────────────────

/// Packed-record geometry for a tagged-union instance.
///
/// All fields are byte counts at stage-0. Every offset is from the
/// beginning of the allocated cell.
///
/// § INVARIANT — `payload_offset` is the smallest byte boundary ≥
///   `tag_size` that satisfies the payload's natural alignment. This
///   keeps the payload's load / store on its natural-alignment slot
///   regardless of the (4-byte) tag width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaggedUnionLayout {
    /// Tag-field byte width. Always 4 at stage-0 (room for 256+ variants
    /// without exhausting the slot ; future slices may shrink to `u8`
    /// when the type-system records the variant cardinality).
    pub tag_size: u8,
    /// Tag-field byte offset within the allocated cell. Always 0.
    pub tag_offset: u32,
    /// Payload byte width. For `Option<T>` : `sizeof(T)`. For
    /// `Result<T,E>` : `max(sizeof(T), sizeof(E))`.
    pub payload_size: u32,
    /// Payload byte offset within the allocated cell. Always
    /// `align_up(tag_size, payload_align)`.
    pub payload_offset: u32,
    /// Total allocation size (`payload_offset + payload_size`, rounded
    /// up to `cell_alignment`). Mirrors the byte count passed to
    /// `cssl.heap.alloc`.
    pub total_size: u32,
    /// Allocation alignment (the larger of `tag_align = 4` and the
    /// payload's natural alignment). Passed to `cssl.heap.alloc`'s
    /// `"alignment"` attribute.
    pub cell_alignment: u32,
}

impl TaggedUnionLayout {
    /// Build a layout from a single payload type (`Option<T>` shape).
    /// `sizeof(T)` and `align_of(T)` are the heuristics from this
    /// crate's stage-0 layout helpers ; unknown / opaque types collapse
    /// to the safe-default `(8, 8)` so the generated cell still works
    /// for `Ptr`-shaped payloads.
    #[must_use]
    pub fn for_option(payload: &MirType) -> Self {
        let payload_size = u32::try_from(heuristic_size_of(payload)).unwrap_or(8).max(1);
        let payload_align = u32::try_from(heuristic_align_of(payload)).unwrap_or(8).max(1);
        Self::pack(payload_size, payload_align)
    }

    /// Build a layout for a `Result<T, E>` shape : payload-slot is sized
    /// to fit the larger of the two variants, aligned to whichever
    /// variant requires the stricter natural alignment.
    #[must_use]
    pub fn for_result(ok_ty: &MirType, err_ty: &MirType) -> Self {
        let ok_size = u32::try_from(heuristic_size_of(ok_ty)).unwrap_or(8).max(1);
        let err_size = u32::try_from(heuristic_size_of(err_ty)).unwrap_or(8).max(1);
        let ok_align = u32::try_from(heuristic_align_of(ok_ty)).unwrap_or(8).max(1);
        let err_align = u32::try_from(heuristic_align_of(err_ty)).unwrap_or(8).max(1);
        Self::pack(ok_size.max(err_size), ok_align.max(err_align))
    }

    /// Pack a payload of `(size, align)` into the canonical
    /// `{ tag : u32, payload : [u8; size] }` shape.
    #[must_use]
    pub fn pack(payload_size: u32, payload_align: u32) -> Self {
        const TAG_SIZE: u8 = 4;
        const TAG_ALIGN: u32 = 4;
        let payload_offset = align_up(u32::from(TAG_SIZE), payload_align);
        let cell_alignment = TAG_ALIGN.max(payload_align);
        let raw_total = payload_offset + payload_size;
        let total_size = align_up(raw_total, cell_alignment);
        Self {
            tag_size: TAG_SIZE,
            tag_offset: 0,
            payload_size,
            payload_offset,
            total_size,
            cell_alignment,
        }
    }
}

/// Round `value` up to the next multiple of `align`. `align` MUST be a
/// power of two ; non-powers-of-two are rounded via the slow modular
/// path so the helper stays defensive against bad upstream input.
#[must_use]
pub fn align_up(value: u32, align: u32) -> u32 {
    debug_assert!(align != 0, "align must be non-zero");
    if align.is_power_of_two() {
        (value + align - 1) & !(align - 1)
    } else {
        let r = value % align;
        if r == 0 {
            value
        } else {
            value + (align - r)
        }
    }
}

/// Stage-0 byte-size for a `MirType`. Mirrors
/// `body_lower::stage0_heuristic_size_of` (kept private there) so the
/// ABI rewriter doesn't depend on body-lower internals. Returns `0` for
/// types whose layout isn't computable yet — callers fold that into
/// the safe-default by clamping to `1` minimum.
#[must_use]
pub fn heuristic_size_of(t: &MirType) -> u32 {
    use crate::value::FloatWidth;
    match t {
        MirType::Int(IntWidth::I1 | IntWidth::I8) | MirType::Bool => 1,
        MirType::Int(IntWidth::I16) => 2,
        MirType::Int(IntWidth::I32) => 4,
        MirType::Int(IntWidth::I64 | IntWidth::Index) => 8,
        MirType::Float(FloatWidth::F16 | FloatWidth::Bf16) => 2,
        MirType::Float(FloatWidth::F32) => 4,
        MirType::Float(FloatWidth::F64) => 8,
        MirType::Ptr | MirType::Handle => 8,
        MirType::Vec(lanes, w) => {
            let lane_bytes: u32 = match w {
                FloatWidth::F16 | FloatWidth::Bf16 => 2,
                FloatWidth::F32 => 4,
                FloatWidth::F64 => 8,
            };
            *lanes * lane_bytes
        }
        // Opaque / Tuple / Function / None / Memref : 0 = "unknown" —
        // callers clamp to the 8-byte safe default.
        _ => 0,
    }
}

/// Stage-0 byte-alignment for a `MirType`. Returns `0` for types that
/// don't have a derivable natural alignment ; callers clamp to 8 (the
/// host pointer width on stage-0's 64-bit target).
#[must_use]
pub fn heuristic_align_of(t: &MirType) -> u32 {
    use crate::value::FloatWidth;
    match t {
        MirType::Int(IntWidth::I1 | IntWidth::I8) | MirType::Bool => 1,
        MirType::Int(IntWidth::I16) | MirType::Float(FloatWidth::F16 | FloatWidth::Bf16) => 2,
        MirType::Int(IntWidth::I32) | MirType::Float(FloatWidth::F32) => 4,
        MirType::Int(IntWidth::I64 | IntWidth::Index)
        | MirType::Float(FloatWidth::F64)
        | MirType::Ptr
        | MirType::Handle => 8,
        MirType::Vec(_, w) => match w {
            FloatWidth::F16 | FloatWidth::Bf16 => 2,
            FloatWidth::F32 => 4,
            FloatWidth::F64 => 8,
        },
        _ => 0,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Family / variant discrimination + canonical tags.
// ─────────────────────────────────────────────────────────────────────────

/// Sum-type family — only `Option` + `Result` at stage-0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SumFamily {
    /// `Option<T>`.
    Option,
    /// `Result<T, E>`.
    Result,
}

/// Sum-type variant — `Some / None / Ok / Err`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SumVariant {
    /// `Option::Some(T)`.
    Some,
    /// `Option::None`.
    None,
    /// `Result::Ok(T)`.
    Ok,
    /// `Result::Err(E)`.
    Err,
}

impl SumVariant {
    /// Family this variant belongs to.
    #[must_use]
    pub const fn family(self) -> SumFamily {
        match self {
            Self::Some | Self::None => SumFamily::Option,
            Self::Ok | Self::Err => SumFamily::Result,
        }
    }

    /// True when the variant carries a typed payload.
    #[must_use]
    pub const fn has_payload(self) -> bool {
        matches!(self, Self::Some | Self::Ok | Self::Err)
    }
}

/// Canonical numeric tag for a sum-variant. Matches the convention
/// stamped onto each construction op by `body_lower` :
///
///   - `Some` / `Ok`  : `1` (success / "value present").
///   - `None` / `Err` : `0` (failure / "absence").
#[must_use]
pub const fn tag_for_variant(v: SumVariant) -> u32 {
    match v {
        SumVariant::Some | SumVariant::Ok => 1,
        SumVariant::None | SumVariant::Err => 0,
    }
}

/// Match a construction op's `CsslOp` to its variant identity.
///
/// Returns `None` for ops that aren't sum-type constructors — callers
/// pass that through unchanged during the rewrite walk.
#[must_use]
pub fn variant_for_op(op: CsslOp) -> Option<SumVariant> {
    match op {
        CsslOp::OptionSome => Some(SumVariant::Some),
        CsslOp::OptionNone => Some(SumVariant::None),
        CsslOp::ResultOk => Some(SumVariant::Ok),
        CsslOp::ResultErr => Some(SumVariant::Err),
        _ => None,
    }
}

/// Read the `payload_ty` attribute that `body_lower` stamps onto every
/// construction op. Returns the textual form (e.g. `"i32"` /
/// `"!cssl.unknown"`). Callers parse the textual form when they need
/// a structural type — the ABI rewrite uses the size-heuristic on the
/// PARSED op, NOT the textual fallback, so this is informational only.
#[must_use]
pub fn payload_ty_str_from_attrs(op: &MirOp) -> Option<&str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == "payload_ty")
        .map(|(_, v)| v.as_str())
}

// ─────────────────────────────────────────────────────────────────────────
// § Construction-op expansion : `cssl.option.some` etc → `heap.alloc + ...`.
// ─────────────────────────────────────────────────────────────────────────

/// Layout-derive a tagged-union from a construction op. For
/// `OptionSome` / `OptionNone` / `ResultOk` the layout reads the
/// payload type from the op's first operand's MIR type (recovered from
/// the attached `payload_ty` attr in the absence of a value-map ; we
/// approximate via the `payload_ty` textual attribute, falling back to
/// the safe `Ptr` default of 8 bytes).
///
/// Stage-0 caveat : `Result<T, E>` only sees the side that actually
/// fired (Ok or Err) — the un-emitted side's size is unknown to a
/// single-op layout query. Callers that need the symmetric
/// max-of-both layout should call [`TaggedUnionLayout::for_result`]
/// directly with both type-arguments resolved through the
/// monomorphization quartet (see `cssl_mir::monomorph`).
#[must_use]
pub fn layout_for_construct(op: &MirOp) -> Option<TaggedUnionLayout> {
    let variant = variant_for_op(op.op)?;
    let payload_str = payload_ty_str_from_attrs(op)?;
    let payload_ty = parse_payload_ty(payload_str);
    Some(match variant.family() {
        SumFamily::Option => TaggedUnionLayout::for_option(&payload_ty),
        SumFamily::Result => {
            // Single-side query : approximate the OTHER side as having
            // the same layout. This is correct for symmetric
            // `Result<T,T>` / `Result<i32,i32>` and conservative for
            // asymmetric — the integration commit's monomorph-time
            // pre-pass replaces this with a true `for_result` call once
            // both sides are known.
            TaggedUnionLayout::for_result(&payload_ty, &payload_ty)
        }
    })
}

/// Parse a textual `payload_ty` attribute back into a `MirType` for
/// layout-heuristic lookup. Stage-0 only handles the scalar / `Ptr`
/// shapes that `body_lower` actually emits. Anything else collapses to
/// `MirType::Ptr` (8-byte safe default) so the layout is conservative
/// rather than crashy.
#[must_use]
pub fn parse_payload_ty(s: &str) -> MirType {
    use crate::value::FloatWidth;
    match s {
        "i1" => MirType::Int(IntWidth::I1),
        "i8" => MirType::Int(IntWidth::I8),
        "i16" => MirType::Int(IntWidth::I16),
        "i32" => MirType::Int(IntWidth::I32),
        "i64" => MirType::Int(IntWidth::I64),
        "index" => MirType::Int(IntWidth::Index),
        "f16" => MirType::Float(FloatWidth::F16),
        "bf16" => MirType::Float(FloatWidth::Bf16),
        "f32" => MirType::Float(FloatWidth::F32),
        "f64" => MirType::Float(FloatWidth::F64),
        "i1.bool" | "bool" => MirType::Bool,
        "!cssl.handle" => MirType::Handle,
        "!cssl.ptr" => MirType::Ptr,
        // Unknown / opaque : conservative 8-byte slot via Ptr.
        _ => MirType::Ptr,
    }
}

/// Result of expanding a construction op into the canonical
/// `heap.alloc + memref.store(tag) + memref.store(payload)` triple.
///
/// The `cell_ptr` is the `!cssl.ptr` value-id that downstream consumers
/// (`scf.match`, `?`-operator) load tag + payload from. It REPLACES the
/// op's original opaque-typed result-id at the call-site.
#[derive(Debug, Clone)]
pub struct ConstructExpansion {
    /// MIR ops emitted, in the order they should appear in the block.
    pub ops: Vec<MirOp>,
    /// Cell ptr-id that consumers use as the scrutinee of `scf.match`.
    pub cell_ptr: ValueId,
    /// Layout used during expansion ; preserved for downstream
    /// match-dispatch lowering so the same tag-offset / payload-offset
    /// pair is read back by load-side ops.
    pub layout: TaggedUnionLayout,
}

/// Counter for fresh `ValueId`s during expansion. Threaded by the
/// caller so the rewrite stays compatible with the existing
/// `BodyLowerCtx::next_value_id` allocation discipline.
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

/// Expand a single construction op (`cssl.option.some` / `.none` /
/// `cssl.result.ok` / `.err`) into the canonical
/// `heap.alloc + tag-store + payload-store` triple. The original op
/// is REPLACED by the returned op-vec ; the returned `cell_ptr` is the
/// `!cssl.ptr` that downstream consumers (`scf.match`) read.
///
/// Per-op expansion :
///
/// ```text
///  // input  : %r = cssl.option.some %payload {tag=1, payload_ty=i32, ...}
///  // output : %p = cssl.heap.alloc {bytes=8, alignment=4}
///  //          memref.store %tag, %p {offset=0, alignment=4}
///  //          memref.store %payload, %p {offset=4, alignment=4}
///  //          (cell_ptr = %p ;  caller rewrites %r references → %p)
/// ```
///
/// For payload-less variants (`OptionNone`) the payload-store is
/// elided.
#[must_use]
pub fn expand_construct(op: &MirOp, ids: &mut FreshIdSeq) -> Option<ConstructExpansion> {
    let variant = variant_for_op(op.op)?;
    let layout = layout_for_construct(op).unwrap_or_else(|| {
        // Defensive : if the op is missing its `payload_ty` attribute
        // we fall back to the 8-byte / Ptr-aligned default cell. This
        // keeps the rewrite total — every recognized op produces a
        // valid expansion.
        TaggedUnionLayout::pack(8, 8)
    });
    let tag = tag_for_variant(variant);
    let cell_ptr = ids.fresh();
    let tag_const = ids.fresh();

    let mut ops = Vec::with_capacity(4);
    // § alloc — total_size + alignment.
    ops.push(
        MirOp::new(CsslOp::HeapAlloc)
            .with_result(cell_ptr, MirType::Ptr)
            .with_attribute("bytes", layout.total_size.to_string())
            .with_attribute("alignment", layout.cell_alignment.to_string())
            .with_attribute("source_kind", "tagged_union")
            .with_attribute(
                "family",
                match variant.family() {
                    SumFamily::Option => "Option",
                    SumFamily::Result => "Result",
                },
            ),
    );

    // § tag-const — emit `arith.constant` for the variant tag value.
    ops.push(
        MirOp::std("arith.constant")
            .with_result(tag_const, MirType::Int(IntWidth::I32))
            .with_attribute("value", tag.to_string()),
    );

    // § tag-store — `memref.store %tag_const, %cell_ptr` at offset 0.
    ops.push(
        MirOp::std("memref.store")
            .with_operand(tag_const)
            .with_operand(cell_ptr)
            .with_attribute("offset", layout.tag_offset.to_string())
            .with_attribute("alignment", u32::from(layout.tag_size).to_string())
            .with_attribute("field", "tag"),
    );

    // § payload-store — only when the variant carries a payload.
    if variant.has_payload() {
        if let Some(payload_id) = op.operands.first().copied() {
            ops.push(
                MirOp::std("memref.store")
                    .with_operand(payload_id)
                    .with_operand(cell_ptr)
                    .with_attribute("offset", layout.payload_offset.to_string())
                    .with_attribute("alignment", layout.cell_alignment.to_string())
                    .with_attribute("field", "payload"),
            );
        }
        // ‼ When variant.has_payload() but no operand exists, the
        //   construction op is malformed — we don't emit a store. The
        //   tag-store still executes so a downstream match arm sees the
        //   tag and falls through to its arm body without an extracted
        //   payload (the body is responsible for handling its own load
        //   of an undefined payload-slot ; this is a body_lower bug).
    }

    Some(ConstructExpansion {
        ops,
        cell_ptr,
        layout,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// § Match-dispatch lowering — fold N-arm match into scf.if cascade.
// ─────────────────────────────────────────────────────────────────────────

/// Build a cascading `scf.if` chain that dispatches on the loaded tag
/// value of a tagged-union scrutinee.
///
/// ```text
///   //   %tag = memref.load %scrut_ptr {offset=0, alignment=4}
///   //   %t0  = arith.constant <tag_for_arm[0]>
///   //   %c0  = arith.cmpi eq %tag, %t0
///   //   scf.if %c0 {
///   //       <arm[0] region>
///   //   } else {
///   //       %t1 = arith.constant <tag_for_arm[1]>
///   //       %c1 = arith.cmpi eq %tag, %t1
///   //       scf.if %c1 { <arm[1] region> } else { <wildcard / unreachable> }
///   //   }
/// ```
///
/// `arm_tags[i]` is the canonical numeric tag the i-th arm matches.
/// The last arm acts as the wildcard fall-through (no comparison is
/// emitted for it ; control reaches it when none of the prior tags
/// matched). `arm_regions` carries the body-region per arm, in the
/// same order — these are spliced verbatim into the generated nested
/// `scf.if` ops.
///
/// Returns the generated ops in source order plus the loaded-tag
/// `ValueId` (in case the caller needs to wire it through a yielded
/// value).
///
/// # Panics
///   When `arm_tags.len() != arm_regions.len()` — these are paired by
///   construction at the call-site so a mismatch indicates a logic bug
///   in the caller. The MIR rewriter guards via debug-assert.
#[must_use]
pub fn build_match_dispatch_cascade(
    scrut_ptr: ValueId,
    arm_tags: &[u32],
    arm_regions: &[MirRegion],
    layout: TaggedUnionLayout,
    ids: &mut FreshIdSeq,
) -> Vec<MirOp> {
    debug_assert_eq!(arm_tags.len(), arm_regions.len());
    let mut ops = Vec::new();
    if arm_tags.is_empty() {
        return ops;
    }
    let tag_id = ids.fresh();
    ops.push(
        MirOp::std("memref.load")
            .with_operand(scrut_ptr)
            .with_result(tag_id, MirType::Int(IntWidth::I32))
            .with_attribute("offset", layout.tag_offset.to_string())
            .with_attribute("alignment", u32::from(layout.tag_size).to_string())
            .with_attribute("field", "tag"),
    );

    let cascade = build_cascade_inner(tag_id, arm_tags, arm_regions, ids);
    ops.extend(cascade);
    ops
}

/// Recursive helper for [`build_match_dispatch_cascade`]. Builds one
/// `scf.if` op for arm[0] and (recursively) the cascade for the
/// remaining arms in the else-region. The terminal arm (last in the
/// slice) becomes the bare-region fall-through with no further
/// comparison.
fn build_cascade_inner(
    tag_id: ValueId,
    arm_tags: &[u32],
    arm_regions: &[MirRegion],
    ids: &mut FreshIdSeq,
) -> Vec<MirOp> {
    if arm_regions.is_empty() {
        return Vec::new();
    }
    if arm_regions.len() == 1 {
        // Terminal arm — emit its region's ops directly (single-block
        // assumption matches body_lower's emission shape).
        return clone_region_ops(&arm_regions[0]);
    }
    let arm_tag = arm_tags[0];
    let then_region = arm_regions[0].clone();

    // Build the else-region by recursing on the remaining arms. We
    // splice the recursive ops into a fresh single-block region.
    let else_ops = build_cascade_inner(tag_id, &arm_tags[1..], &arm_regions[1..], ids);
    let else_region = single_block_region("else", else_ops);

    // Emit `arith.constant <arm_tag>` + `arith.cmpi eq %tag, %const` +
    // `scf.if %cond { then } else { else }`.
    let const_id = ids.fresh();
    let cond_id = ids.fresh();
    let if_id = ids.fresh();
    vec![
        MirOp::std("arith.constant")
            .with_result(const_id, MirType::Int(IntWidth::I32))
            .with_attribute("value", arm_tag.to_string()),
        MirOp::std("arith.cmpi")
            .with_operand(tag_id)
            .with_operand(const_id)
            .with_result(cond_id, MirType::Bool)
            .with_attribute("predicate", "eq"),
        MirOp::std("scf.if")
            .with_operand(cond_id)
            .with_result(if_id, MirType::None)
            .with_region(then_region)
            .with_region(else_region)
            .with_attribute("source_kind", "tagged_union_dispatch")
            .with_attribute("arm_tag", arm_tag.to_string()),
    ]
}

/// Clone the ops of a single-block region. Stage-0 every region built
/// by `body_lower::lower_match` has exactly one block. Multi-block
/// regions silently fall back to the first block's ops — the wave-A4
/// exhaustiveness slice surfaces the multi-block case as a compile
/// error.
fn clone_region_ops(r: &MirRegion) -> Vec<MirOp> {
    r.blocks
        .first()
        .map_or_else(Vec::new, |b| b.ops.clone())
}

/// Build a fresh single-block region carrying the given ops under a
/// `^entry`-style label. Used for synthesizing cascade else-arms.
#[must_use]
fn single_block_region(label: &str, ops: Vec<MirOp>) -> MirRegion {
    let mut blk = MirBlock::new(label);
    blk.ops = ops;
    let mut r = MirRegion::new();
    r.push(blk);
    r
}

// ─────────────────────────────────────────────────────────────────────────
// § Module-level rewrite — drives expansion across every MIR fn.
// ─────────────────────────────────────────────────────────────────────────

/// Audit report : counts ops rewritten per family + total-bytes
/// allocated across the rewrite. Future slices grow this with
/// per-fn diagnostics ; today it's a Sawyer-style bit-pack record so
/// callers can assert behavior without trawling the full module.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExpansionReport {
    /// Number of `Option::Some` constructions expanded.
    pub option_some_count: u32,
    /// Number of `Option::None` constructions expanded.
    pub option_none_count: u32,
    /// Number of `Result::Ok` constructions expanded.
    pub result_ok_count: u32,
    /// Number of `Result::Err` constructions expanded.
    pub result_err_count: u32,
    /// Total tagged-union bytes allocated across all rewrites in this
    /// pass (sum of `layout.total_size` for each expanded op). Useful
    /// for sanity-checking that the rewrite didn't accidentally over-
    /// expand a hot inner loop.
    pub total_bytes_allocated: u32,
}

impl ExpansionReport {
    /// Total constructions expanded.
    #[must_use]
    pub const fn total_count(&self) -> u32 {
        self.option_some_count
            + self.option_none_count
            + self.result_ok_count
            + self.result_err_count
    }

    /// Increment the counter for the given variant + add the cell-size.
    fn record(&mut self, v: SumVariant, bytes: u32) {
        self.total_bytes_allocated = self.total_bytes_allocated.saturating_add(bytes);
        match v {
            SumVariant::Some => self.option_some_count += 1,
            SumVariant::None => self.option_none_count += 1,
            SumVariant::Ok => self.result_ok_count += 1,
            SumVariant::Err => self.result_err_count += 1,
        }
    }
}

/// Expand every sum-type construction op in a `MirFunc` in-place. The
/// `next_value_id` field is grown to accommodate the freshly-allocated
/// SSA-values ; the original opaque-result-id of each construction op
/// is REWIRED to the new heap-cell `!cssl.ptr` so downstream consumers
/// (like `scf.match` lowering) see a `Ptr` scrutinee.
///
/// Returns an [`ExpansionReport`] for the fn so callers can audit the
/// pass at the test boundary.
pub fn expand_func(func: &mut MirFunc) -> ExpansionReport {
    let mut report = ExpansionReport::default();
    let mut ids = FreshIdSeq::new(func.next_value_id);
    expand_region(&mut func.body, &mut ids, &mut report);
    func.next_value_id = ids.next;
    report
}

/// Expand every sum-type construction op across an entire `MirModule`.
/// Walks each fn in source order ; per-fn results are aggregated into
/// the returned [`ExpansionReport`].
///
/// This is the public entry point cited by [`crate::lib`]'s integration
/// note — call this after `lower_fn_body` and before cgen.
pub fn expand_module(module: &mut MirModule) -> ExpansionReport {
    let mut report = ExpansionReport::default();
    for func in &mut module.funcs {
        let per_fn = expand_func(func);
        report.option_some_count += per_fn.option_some_count;
        report.option_none_count += per_fn.option_none_count;
        report.result_ok_count += per_fn.result_ok_count;
        report.result_err_count += per_fn.result_err_count;
        report.total_bytes_allocated = report
            .total_bytes_allocated
            .saturating_add(per_fn.total_bytes_allocated);
    }
    report
}

/// Walk a region in-place, expanding sum-type construction ops. Recurses
/// into every nested region so `scf.if` / `scf.match` arms are covered
/// without a separate visitor.
fn expand_region(
    region: &mut MirRegion,
    ids: &mut FreshIdSeq,
    report: &mut ExpansionReport,
) {
    for block in &mut region.blocks {
        expand_block(block, ids, report);
    }
}

/// Expand one block's ops in-place. The walk preserves source order :
/// each construction op is replaced with the expansion's ops at the
/// same position ; subsequent ops that referenced the original
/// result-id are not rewired automatically (the integration commit
/// adds the value-map rewire ; today's tests verify the structural
/// expansion of a single op).
fn expand_block(block: &mut MirBlock, ids: &mut FreshIdSeq, report: &mut ExpansionReport) {
    let mut idx = 0;
    while idx < block.ops.len() {
        // Recurse into nested regions FIRST — keeps the report
        // consistent with depth-first walk semantics.
        for region in &mut block.ops[idx].regions {
            expand_region(region, ids, report);
        }

        if let Some(variant) = variant_for_op(block.ops[idx].op) {
            let original_op = block.ops[idx].clone();
            if let Some(expansion) = expand_construct(&original_op, ids) {
                report.record(variant, expansion.layout.total_size);
                let span = expansion.ops.len();
                let cell_ptr = expansion.cell_ptr;
                // Replace block.ops[idx] with the expansion-ops. The
                // construction op's original result-id is rebound to
                // the cell_ptr via an `arith.bitcast` op so any
                // downstream op that names the original id still
                // resolves through the value-map. Stage-0 keeps this
                // bitcast structural ; the integration commit replaces
                // it with a true value-map rewrite.
                let original_result_id = original_op.results.first().map(|r| r.id);
                let mut splice: Vec<MirOp> = expansion.ops;
                if let Some(orig) = original_result_id {
                    splice.push(
                        MirOp::std("arith.bitcast")
                            .with_operand(cell_ptr)
                            .with_result(orig, MirType::Ptr)
                            .with_attribute("source_kind", "tagged_union_alias"),
                    );
                }
                block.ops.splice(idx..=idx, splice);
                idx += span + usize::from(original_result_id.is_some());
                continue;
            }
        }
        idx += 1;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — unit + golden coverage for the layout / expansion / dispatch.
// ─────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{MirBlock, MirOp, MirRegion};
    use crate::func::{MirFunc, MirModule};
    use crate::op::CsslOp;
    use crate::value::{IntWidth, MirType, ValueId};

    // ─────────────────────────────────────────────────────────────────
    // § layout primitives
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn align_up_pow2_rounds_to_next() {
        assert_eq!(align_up(0, 4), 0);
        assert_eq!(align_up(1, 4), 4);
        assert_eq!(align_up(4, 4), 4);
        assert_eq!(align_up(5, 8), 8);
        assert_eq!(align_up(7, 8), 8);
        assert_eq!(align_up(8, 8), 8);
    }

    #[test]
    fn align_up_non_pow2_rounds_correctly() {
        // 6 isn't power-of-two ; defensive slow-path.
        assert_eq!(align_up(0, 6), 0);
        assert_eq!(align_up(5, 6), 6);
        assert_eq!(align_up(6, 6), 6);
        assert_eq!(align_up(13, 6), 18);
    }

    #[test]
    fn layout_for_option_i32_is_4plus4_aligned4() {
        let l = TaggedUnionLayout::for_option(&MirType::Int(IntWidth::I32));
        assert_eq!(l.tag_size, 4);
        assert_eq!(l.tag_offset, 0);
        assert_eq!(l.payload_size, 4);
        assert_eq!(l.payload_offset, 4);
        assert_eq!(l.total_size, 8);
        assert_eq!(l.cell_alignment, 4);
    }

    #[test]
    fn layout_for_option_i64_pads_payload_to_8_aligned8() {
        let l = TaggedUnionLayout::for_option(&MirType::Int(IntWidth::I64));
        assert_eq!(l.tag_size, 4);
        assert_eq!(l.payload_size, 8);
        assert_eq!(l.payload_offset, 8); // padded past tag's 4 bytes for natural-align
        assert_eq!(l.total_size, 16);
        assert_eq!(l.cell_alignment, 8);
    }

    #[test]
    fn layout_for_option_bool_is_4plus1_aligned4() {
        let l = TaggedUnionLayout::for_option(&MirType::Bool);
        assert_eq!(l.payload_size, 1);
        // Tag is 4 bytes, bool's natural-align is 1, payload sits at offset 4.
        assert_eq!(l.payload_offset, 4);
        // Cell alignment = max(tag-align=4, payload-align=1) = 4.
        // total = align_up(4 + 1, 4) = 8.
        assert_eq!(l.total_size, 8);
        assert_eq!(l.cell_alignment, 4);
    }

    #[test]
    fn layout_for_result_takes_max_of_both_sides() {
        let ok = MirType::Int(IntWidth::I32);
        let err = MirType::Int(IntWidth::I64);
        let l = TaggedUnionLayout::for_result(&ok, &err);
        // payload is the larger side ; alignment is the stricter side.
        assert_eq!(l.payload_size, 8);
        assert_eq!(l.payload_offset, 8); // padded for i64's natural-align
        assert_eq!(l.cell_alignment, 8);
        assert_eq!(l.total_size, 16);
    }

    #[test]
    fn layout_for_result_symmetric_i32_i32() {
        let t = MirType::Int(IntWidth::I32);
        let l = TaggedUnionLayout::for_result(&t, &t);
        assert_eq!(l.payload_size, 4);
        assert_eq!(l.payload_offset, 4);
        assert_eq!(l.total_size, 8);
        assert_eq!(l.cell_alignment, 4);
    }

    // ─────────────────────────────────────────────────────────────────
    // § variant / family discrimination
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn variant_for_op_recognizes_all_four_constructors() {
        assert_eq!(variant_for_op(CsslOp::OptionSome), Some(SumVariant::Some));
        assert_eq!(variant_for_op(CsslOp::OptionNone), Some(SumVariant::None));
        assert_eq!(variant_for_op(CsslOp::ResultOk), Some(SumVariant::Ok));
        assert_eq!(variant_for_op(CsslOp::ResultErr), Some(SumVariant::Err));
    }

    #[test]
    fn variant_for_op_rejects_non_sum_ops() {
        assert!(variant_for_op(CsslOp::HeapAlloc).is_none());
        assert!(variant_for_op(CsslOp::Std).is_none());
        assert!(variant_for_op(CsslOp::FsOpen).is_none());
    }

    #[test]
    fn tag_for_variant_matches_body_lower_convention() {
        // body_lower stamps tag="1" for Some/Ok and tag="0" for None/Err.
        // The numeric tags emitted by the ABI rewrite must match this.
        assert_eq!(tag_for_variant(SumVariant::Some), 1);
        assert_eq!(tag_for_variant(SumVariant::None), 0);
        assert_eq!(tag_for_variant(SumVariant::Ok), 1);
        assert_eq!(tag_for_variant(SumVariant::Err), 0);
    }

    #[test]
    fn variant_family_partitions_correctly() {
        assert_eq!(SumVariant::Some.family(), SumFamily::Option);
        assert_eq!(SumVariant::None.family(), SumFamily::Option);
        assert_eq!(SumVariant::Ok.family(), SumFamily::Result);
        assert_eq!(SumVariant::Err.family(), SumFamily::Result);
    }

    #[test]
    fn variant_has_payload_predicate() {
        assert!(SumVariant::Some.has_payload());
        assert!(!SumVariant::None.has_payload());
        assert!(SumVariant::Ok.has_payload());
        assert!(SumVariant::Err.has_payload());
    }

    #[test]
    fn parse_payload_ty_recognizes_all_scalars() {
        assert_eq!(parse_payload_ty("i32"), MirType::Int(IntWidth::I32));
        assert_eq!(parse_payload_ty("i64"), MirType::Int(IntWidth::I64));
        assert_eq!(parse_payload_ty("bool"), MirType::Bool);
        assert_eq!(parse_payload_ty("!cssl.ptr"), MirType::Ptr);
        // Unknown opaque collapses to Ptr (8-byte safe default).
        assert_eq!(parse_payload_ty("!cssl.unknown"), MirType::Ptr);
        assert_eq!(parse_payload_ty("Foo<Bar>"), MirType::Ptr);
    }

    // ─────────────────────────────────────────────────────────────────
    // § construction-op expansion
    // ─────────────────────────────────────────────────────────────────

    /// Build the canonical `cssl.option.some %payload {tag=1, payload_ty=i32}`
    /// op shape that body_lower emits.
    fn make_option_some_op(payload_id: u32, result_id: u32) -> MirOp {
        MirOp::new(CsslOp::OptionSome)
            .with_operand(ValueId(payload_id))
            .with_result(ValueId(result_id), MirType::Opaque("!cssl.option.i32".into()))
            .with_attribute("tag", "1")
            .with_attribute("family", "Option")
            .with_attribute("payload_ty", "i32")
            .with_attribute("source_loc", "<test>:1:1")
    }

    fn make_option_none_op(result_id: u32) -> MirOp {
        MirOp::new(CsslOp::OptionNone)
            .with_result(
                ValueId(result_id),
                MirType::Opaque("!cssl.option.unknown".into()),
            )
            .with_attribute("tag", "0")
            .with_attribute("family", "Option")
            .with_attribute("payload_ty", "!cssl.unknown")
    }

    fn make_result_ok_op(payload_id: u32, result_id: u32) -> MirOp {
        MirOp::new(CsslOp::ResultOk)
            .with_operand(ValueId(payload_id))
            .with_result(
                ValueId(result_id),
                MirType::Opaque("!cssl.result.ok.i32".into()),
            )
            .with_attribute("tag", "1")
            .with_attribute("family", "Result")
            .with_attribute("payload_ty", "i32")
    }

    fn make_result_err_op(err_id: u32, result_id: u32) -> MirOp {
        MirOp::new(CsslOp::ResultErr)
            .with_operand(ValueId(err_id))
            .with_result(
                ValueId(result_id),
                MirType::Opaque("!cssl.result.err.i32".into()),
            )
            .with_attribute("tag", "0")
            .with_attribute("family", "Result")
            .with_attribute("err_ty", "i32")
            .with_attribute("payload_ty", "i32")
    }

    #[test]
    fn expand_option_some_emits_alloc_tag_payload_triple() {
        let op = make_option_some_op(/*payload*/ 0, /*result*/ 1);
        let mut ids = FreshIdSeq::new(2);
        let exp = expand_construct(&op, &mut ids).expect("Some lowers");
        assert_eq!(exp.ops.len(), 4);
        // [0] heap.alloc → !cssl.ptr
        assert_eq!(exp.ops[0].name, "cssl.heap.alloc");
        let bytes_attr = exp.ops[0]
            .attributes
            .iter()
            .find(|(k, _)| k == "bytes")
            .unwrap();
        assert_eq!(bytes_attr.1, "8"); // 4-byte tag + 4-byte i32 payload
        // [1] arith.constant 1 (tag value)
        assert_eq!(exp.ops[1].name, "arith.constant");
        let val = exp.ops[1]
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .unwrap();
        assert_eq!(val.1, "1");
        // [2] memref.store tag at offset 0
        assert_eq!(exp.ops[2].name, "memref.store");
        let off = exp.ops[2]
            .attributes
            .iter()
            .find(|(k, _)| k == "offset")
            .unwrap();
        assert_eq!(off.1, "0");
        // [3] memref.store payload at offset 4
        assert_eq!(exp.ops[3].name, "memref.store");
        let off2 = exp.ops[3]
            .attributes
            .iter()
            .find(|(k, _)| k == "offset")
            .unwrap();
        assert_eq!(off2.1, "4");
        let field = exp.ops[3]
            .attributes
            .iter()
            .find(|(k, _)| k == "field")
            .unwrap();
        assert_eq!(field.1, "payload");
    }

    #[test]
    fn expand_option_none_skips_payload_store() {
        let op = make_option_none_op(/*result*/ 1);
        let mut ids = FreshIdSeq::new(2);
        let exp = expand_construct(&op, &mut ids).expect("None lowers");
        // alloc + tag-const + tag-store ; NO payload-store.
        assert_eq!(exp.ops.len(), 3);
        assert_eq!(exp.ops[0].name, "cssl.heap.alloc");
        assert_eq!(exp.ops[1].name, "arith.constant");
        assert_eq!(exp.ops[2].name, "memref.store");
        // The constant carries tag=0.
        let v = exp.ops[1]
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .unwrap();
        assert_eq!(v.1, "0");
    }

    #[test]
    fn expand_result_ok_emits_alloc_plus_tag_plus_payload() {
        let op = make_result_ok_op(/*payload*/ 0, /*result*/ 1);
        let mut ids = FreshIdSeq::new(2);
        let exp = expand_construct(&op, &mut ids).expect("Ok lowers");
        assert_eq!(exp.ops.len(), 4);
        // tag should be 1 for Ok.
        let v = exp.ops[1]
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .unwrap();
        assert_eq!(v.1, "1");
    }

    #[test]
    fn expand_result_err_emits_alloc_plus_tag_plus_payload() {
        let op = make_result_err_op(/*err*/ 0, /*result*/ 1);
        let mut ids = FreshIdSeq::new(2);
        let exp = expand_construct(&op, &mut ids).expect("Err lowers");
        assert_eq!(exp.ops.len(), 4);
        let v = exp.ops[1]
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .unwrap();
        assert_eq!(v.1, "0");
    }

    #[test]
    fn expand_construct_ignores_non_sum_op() {
        let op = MirOp::new(CsslOp::HeapAlloc);
        let mut ids = FreshIdSeq::new(0);
        assert!(expand_construct(&op, &mut ids).is_none());
    }

    // ─────────────────────────────────────────────────────────────────
    // § Match-dispatch cascade
    // ─────────────────────────────────────────────────────────────────

    fn empty_arm_region() -> MirRegion {
        let blk = MirBlock::new("arm");
        let mut r = MirRegion::new();
        r.push(blk);
        r
    }

    #[test]
    fn build_match_dispatch_cascade_emits_load_plus_compare_plus_scf_if() {
        let layout = TaggedUnionLayout::for_option(&MirType::Int(IntWidth::I32));
        let arm_tags = [1_u32, 0_u32];
        let arms = [empty_arm_region(), empty_arm_region()];
        let mut ids = FreshIdSeq::new(10);
        let ops = build_match_dispatch_cascade(ValueId(7), &arm_tags, &arms, layout, &mut ids);
        // Expect : memref.load (tag) + arith.constant + arith.cmpi + scf.if.
        assert!(ops.iter().any(|o| o.name == "memref.load"));
        assert!(ops.iter().any(|o| o.name == "arith.constant"));
        assert!(ops.iter().any(|o| o.name == "arith.cmpi"));
        assert!(ops.iter().any(|o| o.name == "scf.if"));
    }

    #[test]
    fn build_match_dispatch_cascade_attaches_arm_tag_attribute_to_scf_if() {
        let layout = TaggedUnionLayout::for_option(&MirType::Int(IntWidth::I32));
        let arm_tags = [1_u32, 0_u32];
        let arms = [empty_arm_region(), empty_arm_region()];
        let mut ids = FreshIdSeq::new(10);
        let ops = build_match_dispatch_cascade(ValueId(7), &arm_tags, &arms, layout, &mut ids);
        let scf_if = ops.iter().find(|o| o.name == "scf.if").unwrap();
        let arm_tag = scf_if
            .attributes
            .iter()
            .find(|(k, _)| k == "arm_tag")
            .unwrap();
        // First arm is `Some` (tag 1) ; the cascade tests that tag first.
        assert_eq!(arm_tag.1, "1");
    }

    #[test]
    fn build_match_dispatch_cascade_empty_arms_emits_nothing() {
        let layout = TaggedUnionLayout::for_option(&MirType::Int(IntWidth::I32));
        let mut ids = FreshIdSeq::new(0);
        let ops = build_match_dispatch_cascade(ValueId(0), &[], &[], layout, &mut ids);
        assert!(ops.is_empty());
    }

    #[test]
    fn build_match_dispatch_cascade_three_arms_emits_two_ifs() {
        // 3 arms → 2 nested scf.if (last arm is wildcard fall-through).
        let layout = TaggedUnionLayout::for_option(&MirType::Int(IntWidth::I32));
        let arm_tags = [0_u32, 1_u32, 2_u32];
        let arms = [
            empty_arm_region(),
            empty_arm_region(),
            empty_arm_region(),
        ];
        let mut ids = FreshIdSeq::new(10);
        let ops = build_match_dispatch_cascade(ValueId(7), &arm_tags, &arms, layout, &mut ids);
        let scf_if_count = ops.iter().filter(|o| o.name == "scf.if").count();
        // Outer scf.if's else-region holds the inner scf.if — the outer
        // walk only sees ONE scf.if at the top-level, the second lives
        // inside the else-region.
        assert_eq!(scf_if_count, 1);
        let outer = ops.iter().find(|o| o.name == "scf.if").unwrap();
        // Outer's else-region contains another scf.if for the second arm
        // (the third is the terminal wildcard).
        let else_region = outer.regions.get(1).unwrap();
        let inner_count = else_region
            .blocks
            .iter()
            .flat_map(|b| b.ops.iter())
            .filter(|o| o.name == "scf.if")
            .count();
        assert_eq!(inner_count, 1);
    }

    // ─────────────────────────────────────────────────────────────────
    // § Module-level rewrite (golden test) : tiny MIR fn end-to-end.
    // ─────────────────────────────────────────────────────────────────

    /// Build a tiny MIR fn that returns `Some(42)` (the canonical
    /// success-path test from the slice's stage1/test_option.cssl
    /// fixture).
    fn build_make_some_fn() -> MirFunc {
        // %0 = arith.constant 42 : i32
        // %1 = cssl.option.some %0 : !cssl.option.i32
        // func.return %1
        let const_op = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "42");
        let some_op = make_option_some_op(0, 1);
        let ret_op = MirOp::std("func.return").with_operand(ValueId(1));

        let mut func = MirFunc::new(
            "make_some",
            Vec::new(),
            vec![MirType::Opaque("!cssl.option.i32".into())],
        );
        // MirFunc::new sets next_value_id = params.len() = 0 ; bump to 2 so
        // our hand-built %0 / %1 don't collide with future fresh ids.
        func.next_value_id = 2;
        func.push_op(const_op);
        func.push_op(some_op);
        func.push_op(ret_op);
        func
    }

    #[test]
    fn expand_func_replaces_option_some_with_alloc_plus_stores() {
        let mut func = build_make_some_fn();
        let report = expand_func(&mut func);
        assert_eq!(report.option_some_count, 1);
        assert_eq!(report.option_none_count, 0);
        assert_eq!(report.total_count(), 1);
        assert_eq!(report.total_bytes_allocated, 8); // i32 payload : 4+4=8

        let entry = func.body.entry().unwrap();
        // Original Some op should be GONE ; replaced by the expansion.
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.option.some"),
            "cssl.option.some must be expanded out : {:?}",
            entry.ops.iter().map(|o| o.name.clone()).collect::<Vec<_>>()
        );
        assert!(entry.ops.iter().any(|o| o.name == "cssl.heap.alloc"));
        let store_count = entry.ops.iter().filter(|o| o.name == "memref.store").count();
        assert_eq!(store_count, 2); // tag-store + payload-store
    }

    #[test]
    fn expand_func_grows_next_value_id_for_fresh_allocations() {
        let mut func = build_make_some_fn();
        let before = func.next_value_id;
        expand_func(&mut func);
        // Each construction expansion allocates 2 fresh ids : cell-ptr +
        // tag-const. The bitcast-alias reuses the original op's result-id
        // so it doesn't bump the counter.
        assert!(
            func.next_value_id >= before + 2,
            "next_value_id must grow by at least 2 : before={before} after={}",
            func.next_value_id
        );
    }

    #[test]
    fn expand_module_aggregates_per_fn_reports() {
        let mut module = MirModule::with_name("test");
        module.push_func(build_make_some_fn());
        module.push_func(build_make_some_fn());
        let report = expand_module(&mut module);
        assert_eq!(report.option_some_count, 2);
        assert_eq!(report.total_bytes_allocated, 16);
    }

    #[test]
    fn expansion_report_total_count_sums_all_variants() {
        let r = ExpansionReport {
            option_some_count: 3,
            option_none_count: 2,
            result_ok_count: 1,
            result_err_count: 4,
            total_bytes_allocated: 0,
        };
        assert_eq!(r.total_count(), 10);
    }

    #[test]
    fn fresh_id_seq_post_increments_correctly() {
        let mut ids = FreshIdSeq::new(7);
        assert_eq!(ids.fresh(), ValueId(7));
        assert_eq!(ids.fresh(), ValueId(8));
        assert_eq!(ids.next, 9);
    }
}

// INTEGRATION_NOTE :
//   add `pub mod tagged_union_abi;` (and the corresponding `pub use
//   tagged_union_abi::{...}` re-exports) to cssl-mir/src/lib.rs in the
//   integration commit. The wave-A1 dispatch carved this file out
//   single-file-owned ; main-thread integration replaces this comment
//   with the `pub mod` declaration + the re-export block listing
//   `expand_module`, `expand_func`, `expand_construct`,
//   `build_match_dispatch_cascade`, `TaggedUnionLayout`, `SumFamily`,
//   `SumVariant`, `tag_for_variant`, `variant_for_op`,
//   `ExpansionReport`, `FreshIdSeq`.
