//! Typed memref load/store + pointer-arith MIR-op builders for Wave-A2.
//!
//! § SPEC  : `specs/02_IR.csl` § MEMORY-OPS · `specs/40_WAVE_CSSL_PLAN.csl`
//!           § WAVES § WAVE-A · A2 (typed-memref load/store).
//! § ROLE  : produce well-formed `memref.load.<T>` / `memref.store.<T>`
//!           ops that downstream Cranelift cgen lowers to real `load.<T>`
//!           / `store.<T>` instructions. Replaces the
//!           `panic("…stage-0 deferred — typed memref.load required")`
//!           placeholder bodies in `stdlib/vec.cssl` (`vec_load_at` /
//!           `vec_store_at` / `vec_end_of`) once an integration commit
//!           wires the recognizer in `body_lower::lower_call`.
//!
//! § DESIGN
//!   - `TypedMemrefElem` is a 6-variant enum (i8 / i16 / i32 / i64 /
//!     f32 / f64) covering every primitive cell-type Vec<T> needs at
//!     stage-0. Nested generics (Vec<Option<i32>>) monomorphize to the
//!     concrete primitive at call sites that have a numeric layout
//!     (struct / sum-type cells defer to a future slice).
//!   - All per-elem maps use direct `match` LUTs — never `HashMap` —
//!     per the Sawyer-efficiency mandate (no scratch allocations on
//!     hot-path lowering).
//!   - `build_typed_load` / `build_typed_store` produce `MirOp::std`-
//!     shaped ops with name `memref.load.<T>` / `memref.store.<T>`.
//!     The element-type is encoded in the op-name SUFFIX (recognized by
//!     `cgen_memref::lower_typed_memref_load`) and as the `elem_ty`
//!     attribute (machine-readable for downstream walkers).
//!   - `build_typed_end_of` emits the `data + len * sizeof(T)` pointer-
//!     arith triple : `muli` of `len * sizeof(T)` then `addi` of
//!     `data + offset`. Used by `vec_end_of` for `VecIter::end`.
//!   - Op-name suffix is the canonical primitive name (`i32` / `f64` /
//!     etc.) so the Cranelift cgen can dispatch on a string match.
//!     This keeps the MIR surface inspectable + diffable without
//!     introducing a new `CsslOp::*` variant — the integration commit
//!     decides whether to promote them to first-class variants once
//!     the recognizer is wired.
//!
//! § INTEGRATION-NOTE
//!   To wire this module into the crate's public surface, add
//!   `pub mod memref_typed;` to `cssl-mir/src/lib.rs` ALONGSIDE the
//!   existing `pub mod block;` / `pub mod body_lower;` declarations.
//!   This file deliberately does NOT modify lib.rs ; the integration
//!   commit owns that one-line change.

use crate::block::MirOp;
use crate::value::{FloatWidth, IntWidth, MirType, ValueId};

// ════════════════════════════════════════════════════════════════════════
// § TypedMemrefElem — 6-variant enum covering Vec<T> primitive cells.
// ════════════════════════════════════════════════════════════════════════

/// Primitive cell-type for typed-memref load/store ops.
///
/// At stage-0 Vec<T> only specializes for the 6 primitives below — composite
/// cells (Vec<MyStruct>) defer to a follow-up slice that lowers struct ABI
/// before memref typing can apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypedMemrefElem {
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

impl TypedMemrefElem {
    /// Canonical primitive name (`"i32"` / `"f64"` / etc.) used as the
    /// op-name suffix and as the `elem_ty` attribute value.
    ///
    /// LUT-style match — no allocation, no scratch storage.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    /// Element size in bytes — compile-time const-fold per monomorph instance.
    #[must_use]
    pub const fn sizeof(self) -> i64 {
        match self {
            Self::I8 => 1,
            Self::I16 => 2,
            Self::I32 | Self::F32 => 4,
            Self::I64 | Self::F64 => 8,
        }
    }

    /// Natural alignment in bytes — mirrors `IntWidth::natural_alignment` /
    /// `FloatWidth::natural_alignment` so memref ops carry a consistent
    /// `alignment` attribute across the LUT.
    #[must_use]
    pub const fn align(self) -> u32 {
        match self {
            Self::I8 => 1,
            Self::I16 => 2,
            Self::I32 | Self::F32 => 4,
            Self::I64 | Self::F64 => 8,
        }
    }

    /// Map a `MirType` scalar to the matching typed-memref element kind.
    /// Returns `None` for non-primitive / non-supported types — caller
    /// must reject the op as unsupported at stage-0.
    ///
    /// Branch-free dispatch via inner-enum match.
    #[must_use]
    pub const fn from_mir_type(ty: &MirType) -> Option<Self> {
        match ty {
            MirType::Int(IntWidth::I8) => Some(Self::I8),
            MirType::Int(IntWidth::I16) => Some(Self::I16),
            MirType::Int(IntWidth::I32) => Some(Self::I32),
            MirType::Int(IntWidth::I64 | IntWidth::Index) => Some(Self::I64),
            // I1 (bool) is supported as I8-width store at stage-0 — same as the
            // Cranelift backend treatment of `b1` (1-byte store).
            MirType::Int(IntWidth::I1) | MirType::Bool => Some(Self::I8),
            MirType::Float(FloatWidth::F32) => Some(Self::F32),
            MirType::Float(FloatWidth::F64) => Some(Self::F64),
            // F16 / Bf16 lower as i16 cells at stage-0 (no native f16 store).
            MirType::Float(FloatWidth::F16 | FloatWidth::Bf16) => Some(Self::I16),
            _ => None,
        }
    }

    /// Reconstruct a canonical `MirType` for the result of a typed load.
    /// Used by the op-builder to wire the result-type into the produced
    /// `MirOp.results[0].ty`.
    #[must_use]
    pub const fn to_mir_type(self) -> MirType {
        match self {
            Self::I8 => MirType::Int(IntWidth::I8),
            Self::I16 => MirType::Int(IntWidth::I16),
            Self::I32 => MirType::Int(IntWidth::I32),
            Self::I64 => MirType::Int(IntWidth::I64),
            Self::F32 => MirType::Float(FloatWidth::F32),
            Self::F64 => MirType::Float(FloatWidth::F64),
        }
    }

    /// All 6 primitive variants in canonical order — used by tests.
    pub const ALL: [Self; 6] = [
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::F32,
        Self::F64,
    ];
}

// ════════════════════════════════════════════════════════════════════════
// § Op-name conventions.
// ════════════════════════════════════════════════════════════════════════

/// Op-name prefix for typed-memref load.
///
/// Full op-name = `"memref.load.<T>"` where `<T>` is the canonical primitive
/// name returned by `TypedMemrefElem::name`.
pub const TYPED_LOAD_PREFIX: &str = "memref.load.";

/// Op-name prefix for typed-memref store.
pub const TYPED_STORE_PREFIX: &str = "memref.store.";

/// Op-name for the pointer-arith `data + len * sizeof(T)` end-of operation.
/// Carries the `elem_ty` attribute so downstream walkers can recover the
/// per-instance sizeof T constant.
pub const PTR_ARITH_END_OF: &str = "memref.ptr.end_of";

/// Compose the full op-name for a typed-memref load — `"memref.load.i32"`,
/// `"memref.load.f64"`, etc.
#[must_use]
pub fn typed_load_op_name(elem: TypedMemrefElem) -> String {
    format!("{}{}", TYPED_LOAD_PREFIX, elem.name())
}

/// Compose the full op-name for a typed-memref store.
#[must_use]
pub fn typed_store_op_name(elem: TypedMemrefElem) -> String {
    format!("{}{}", TYPED_STORE_PREFIX, elem.name())
}

/// Recover the element-kind from a typed-load op-name suffix.
/// Returns `None` if the suffix is not a recognized primitive name —
/// caller falls through to the generic `memref.load` lowering.
#[must_use]
pub fn parse_typed_load_op_name(name: &str) -> Option<TypedMemrefElem> {
    let suffix = name.strip_prefix(TYPED_LOAD_PREFIX)?;
    parse_elem_name(suffix)
}

/// Recover the element-kind from a typed-store op-name suffix.
#[must_use]
pub fn parse_typed_store_op_name(name: &str) -> Option<TypedMemrefElem> {
    let suffix = name.strip_prefix(TYPED_STORE_PREFIX)?;
    parse_elem_name(suffix)
}

/// Reverse of `TypedMemrefElem::name` — LUT-style match.
#[must_use]
pub const fn parse_elem_name(s: &str) -> Option<TypedMemrefElem> {
    // `match` on a `&str` slice — Rust allows this since 1.0 for string
    // literal patterns.
    match s.as_bytes() {
        b"i8" => Some(TypedMemrefElem::I8),
        b"i16" => Some(TypedMemrefElem::I16),
        b"i32" => Some(TypedMemrefElem::I32),
        b"i64" => Some(TypedMemrefElem::I64),
        b"f32" => Some(TypedMemrefElem::F32),
        b"f64" => Some(TypedMemrefElem::F64),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Op-builders.
// ════════════════════════════════════════════════════════════════════════

/// Build a typed-memref load op : `%result = memref.load.<T> %ptr, %offset`.
///
/// § SHAPE
/// - operands : `[ptr, offset]` — both `i64` (offset is in BYTES, already
///   pre-multiplied by sizeof T at the call site or carries its own sizeof
///   computation).
/// - result   : single typed value of `elem.to_mir_type()`.
/// - attributes :
///     - `elem_ty`   = canonical primitive name (`"i32"` etc.)
///     - `sizeof`    = byte-size of the element (`"4"` for i32, `"8"` for i64)
///     - `alignment` = natural alignment in bytes
///
/// The Cranelift cgen reads the op-name suffix to dispatch the correct
/// `load.<T>` instruction.
#[must_use]
pub fn build_typed_load(
    elem: TypedMemrefElem,
    ptr: ValueId,
    offset: ValueId,
    result_id: ValueId,
) -> MirOp {
    let name = typed_load_op_name(elem);
    MirOp::std(name)
        .with_operand(ptr)
        .with_operand(offset)
        .with_result(result_id, elem.to_mir_type())
        .with_attribute("elem_ty", elem.name())
        .with_attribute("sizeof", format!("{}", elem.sizeof()))
        .with_attribute("alignment", format!("{}", elem.align()))
}

/// Build a typed-memref store op :
/// `memref.store.<T> %value, %ptr, %offset`.
///
/// § SHAPE
/// - operands : `[value, ptr, offset]` — value is typed `elem.to_mir_type()`,
///   ptr + offset are `i64`.
/// - result   : none (store is side-effect-only).
/// - attributes : `elem_ty` / `sizeof` / `alignment` (same as `build_typed_load`).
#[must_use]
pub fn build_typed_store(
    elem: TypedMemrefElem,
    value: ValueId,
    ptr: ValueId,
    offset: ValueId,
) -> MirOp {
    let name = typed_store_op_name(elem);
    MirOp::std(name)
        .with_operand(value)
        .with_operand(ptr)
        .with_operand(offset)
        .with_attribute("elem_ty", elem.name())
        .with_attribute("sizeof", format!("{}", elem.sizeof()))
        .with_attribute("alignment", format!("{}", elem.align()))
}

/// Build the `data + len * sizeof(T)` pointer-arith sequence used by
/// `vec_end_of`.
///
/// § SHAPE
/// Returns 3 ops in source-order :
///   1. `arith.constant`   `%c = sizeof(T)` (i64 const)
///   2. `arith.muli`       `%bytes = len * %c`
///   3. `arith.addi`       `%end   = data + %bytes`
///
/// The caller threads the produced result-id (the third op) as the
/// `VecIter.end` field. The intermediate value-ids are allocated by
/// the caller via `BodyLowerCtx::fresh_value_id` and passed in.
///
/// The `arith.addi`-result op carries `elem_ty` as an attribute so a
/// downstream walker can audit the sizeof multiplier without re-deriving
/// it from the constant value.
#[must_use]
pub fn build_typed_end_of(
    elem: TypedMemrefElem,
    data: ValueId,
    len: ValueId,
    sizeof_const_id: ValueId,
    bytes_id: ValueId,
    end_id: ValueId,
) -> Vec<MirOp> {
    let i64_ty = MirType::Int(IntWidth::I64);
    vec![
        // sizeof(T) constant.
        MirOp::std("arith.constant")
            .with_result(sizeof_const_id, i64_ty.clone())
            .with_attribute("value", format!("{}", elem.sizeof()))
            .with_attribute("elem_ty", elem.name()),
        // len * sizeof(T).
        MirOp::std("arith.muli")
            .with_operand(len)
            .with_operand(sizeof_const_id)
            .with_result(bytes_id, i64_ty.clone()),
        // data + bytes  →  end-of-buffer pointer (as i64).
        MirOp::std(PTR_ARITH_END_OF)
            .with_operand(data)
            .with_operand(bytes_id)
            .with_result(end_id, i64_ty)
            .with_attribute("elem_ty", elem.name())
            .with_attribute("sizeof", format!("{}", elem.sizeof())),
    ]
}

/// Build the `data + i * sizeof(T)` byte-offset sequence used by load/store
/// at index `i`. Returns the constant + muli pair ; the resulting bytes
/// value-id is the consumer's offset operand for `build_typed_load` /
/// `build_typed_store`.
///
/// § SHAPE
///   1. `arith.constant`   `%c = sizeof(T)` (i64)
///   2. `arith.muli`       `%bytes = i * %c`
///
/// Two ops returned ; caller passes `%bytes` as the offset operand into the
/// load or store op.
#[must_use]
pub fn build_index_offset(
    elem: TypedMemrefElem,
    index: ValueId,
    sizeof_const_id: ValueId,
    bytes_id: ValueId,
) -> Vec<MirOp> {
    let i64_ty = MirType::Int(IntWidth::I64);
    vec![
        MirOp::std("arith.constant")
            .with_result(sizeof_const_id, i64_ty.clone())
            .with_attribute("value", format!("{}", elem.sizeof()))
            .with_attribute("elem_ty", elem.name()),
        MirOp::std("arith.muli")
            .with_operand(index)
            .with_operand(sizeof_const_id)
            .with_result(bytes_id, i64_ty),
    ]
}

// ════════════════════════════════════════════════════════════════════════
// § Tests.
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::{
        build_index_offset, build_typed_end_of, build_typed_load, build_typed_store,
        parse_elem_name, parse_typed_load_op_name, parse_typed_store_op_name, typed_load_op_name,
        typed_store_op_name, TypedMemrefElem, PTR_ARITH_END_OF,
    };
    use crate::value::{FloatWidth, IntWidth, MirType, ValueId};

    // ── 1. sizeof + align LUT correctness across all 6 primitives ───────

    #[test]
    fn sizeof_lut_canonical_per_primitive() {
        assert_eq!(TypedMemrefElem::I8.sizeof(), 1);
        assert_eq!(TypedMemrefElem::I16.sizeof(), 2);
        assert_eq!(TypedMemrefElem::I32.sizeof(), 4);
        assert_eq!(TypedMemrefElem::I64.sizeof(), 8);
        assert_eq!(TypedMemrefElem::F32.sizeof(), 4);
        assert_eq!(TypedMemrefElem::F64.sizeof(), 8);
    }

    #[test]
    fn align_lut_matches_sizeof_for_natural_align() {
        for elem in TypedMemrefElem::ALL {
            assert_eq!(
                u32::try_from(elem.sizeof()).unwrap(),
                elem.align(),
                "natural-align mismatch for {}",
                elem.name()
            );
        }
    }

    // ── 2. MirType ↔ TypedMemrefElem round-trips ────────────────────────

    #[test]
    fn from_mir_type_handles_all_int_widths() {
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Int(IntWidth::I8)),
            Some(TypedMemrefElem::I8)
        );
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Int(IntWidth::I32)),
            Some(TypedMemrefElem::I32)
        );
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Int(IntWidth::I64)),
            Some(TypedMemrefElem::I64)
        );
        // Index lowers to I64 cells.
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Int(IntWidth::Index)),
            Some(TypedMemrefElem::I64)
        );
        // Bool / I1 lower to I8 cells.
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Bool),
            Some(TypedMemrefElem::I8)
        );
    }

    #[test]
    fn from_mir_type_handles_floats_with_f16_fallback() {
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Float(FloatWidth::F32)),
            Some(TypedMemrefElem::F32)
        );
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Float(FloatWidth::F64)),
            Some(TypedMemrefElem::F64)
        );
        // Half-floats lower to i16 cells (raw bit-pattern store).
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Float(FloatWidth::F16)),
            Some(TypedMemrefElem::I16)
        );
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Float(FloatWidth::Bf16)),
            Some(TypedMemrefElem::I16)
        );
    }

    #[test]
    fn from_mir_type_rejects_aggregates() {
        assert_eq!(TypedMemrefElem::from_mir_type(&MirType::None), None);
        assert_eq!(TypedMemrefElem::from_mir_type(&MirType::Handle), None);
        assert_eq!(TypedMemrefElem::from_mir_type(&MirType::Ptr), None);
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Tuple(vec![])),
            None
        );
        assert_eq!(
            TypedMemrefElem::from_mir_type(&MirType::Vec(3, FloatWidth::F32)),
            None
        );
    }

    #[test]
    fn to_mir_type_round_trips_for_every_primitive() {
        // I8 / I16 / I32 / I64 / F32 / F64 round-trip exactly.
        for elem in TypedMemrefElem::ALL {
            let ty = elem.to_mir_type();
            let back = TypedMemrefElem::from_mir_type(&ty);
            assert_eq!(back, Some(elem), "round-trip failed for {}", elem.name());
        }
    }

    // ── 3. Op-name compose/parse round-trip ─────────────────────────────

    #[test]
    fn op_name_compose_parse_round_trip_load() {
        for elem in TypedMemrefElem::ALL {
            let name = typed_load_op_name(elem);
            assert!(name.starts_with("memref.load."));
            assert_eq!(parse_typed_load_op_name(&name), Some(elem));
        }
    }

    #[test]
    fn op_name_compose_parse_round_trip_store() {
        for elem in TypedMemrefElem::ALL {
            let name = typed_store_op_name(elem);
            assert!(name.starts_with("memref.store."));
            assert_eq!(parse_typed_store_op_name(&name), Some(elem));
        }
    }

    #[test]
    fn parse_elem_name_rejects_unknown() {
        assert!(parse_elem_name("u128").is_none());
        assert!(parse_elem_name("").is_none());
        assert!(parse_elem_name("vec3xf32").is_none());
        // Wrong prefix — parse fns return None, not the inner kind.
        assert!(parse_typed_load_op_name("memref.store.i32").is_none());
        assert!(parse_typed_store_op_name("memref.load.i32").is_none());
    }

    // ── 4. Op-builder structural correctness per primitive ──────────────

    #[test]
    fn build_typed_load_i32_carries_canonical_attrs() {
        let op = build_typed_load(
            TypedMemrefElem::I32,
            ValueId(0),
            ValueId(1),
            ValueId(2),
        );
        assert_eq!(op.name, "memref.load.i32");
        assert_eq!(op.operands, vec![ValueId(0), ValueId(1)]);
        assert_eq!(op.results.len(), 1);
        assert_eq!(op.results[0].id, ValueId(2));
        assert_eq!(op.results[0].ty, MirType::Int(IntWidth::I32));
        assert!(op.attributes.iter().any(|(k, v)| k == "elem_ty" && v == "i32"));
        assert!(op.attributes.iter().any(|(k, v)| k == "sizeof" && v == "4"));
        assert!(op.attributes.iter().any(|(k, v)| k == "alignment" && v == "4"));
    }

    #[test]
    fn build_typed_load_f64_uses_f64_result_type_and_8byte_align() {
        let op = build_typed_load(
            TypedMemrefElem::F64,
            ValueId(7),
            ValueId(8),
            ValueId(9),
        );
        assert_eq!(op.name, "memref.load.f64");
        assert_eq!(op.results[0].ty, MirType::Float(FloatWidth::F64));
        assert!(op.attributes.iter().any(|(k, v)| k == "sizeof" && v == "8"));
        assert!(op.attributes.iter().any(|(k, v)| k == "alignment" && v == "8"));
    }

    #[test]
    fn build_typed_store_has_no_result() {
        let op = build_typed_store(
            TypedMemrefElem::I64,
            ValueId(5),
            ValueId(6),
            ValueId(7),
        );
        assert_eq!(op.name, "memref.store.i64");
        assert_eq!(op.operands, vec![ValueId(5), ValueId(6), ValueId(7)]);
        assert!(op.results.is_empty());
        assert!(op.attributes.iter().any(|(k, v)| k == "elem_ty" && v == "i64"));
    }

    // ── 5. Load+store round-trip per primitive : monomorphized symbols ─

    #[test]
    fn load_store_roundtrip_per_primitive_emits_distinct_ops() {
        // Each primitive produces a distinct op-name pair — this catches
        // monomorphization-collision bugs (e.g. accidentally emitting
        // `memref.load.i32` for a Vec<f32> push).
        let mut load_names: Vec<String> = TypedMemrefElem::ALL
            .iter()
            .map(|e| {
                let op = build_typed_load(*e, ValueId(0), ValueId(1), ValueId(2));
                op.name
            })
            .collect();
        let before = load_names.len();
        load_names.sort();
        load_names.dedup();
        assert_eq!(
            load_names.len(),
            before,
            "monomorphization produced collision in load op-names"
        );

        let mut store_names: Vec<String> = TypedMemrefElem::ALL
            .iter()
            .map(|e| {
                let op = build_typed_store(*e, ValueId(0), ValueId(1), ValueId(2));
                op.name
            })
            .collect();
        let before = store_names.len();
        store_names.sort();
        store_names.dedup();
        assert_eq!(
            store_names.len(),
            before,
            "monomorphization produced collision in store op-names"
        );
    }

    // ── 6. Pointer-arith end_of correctness ──────────────────────────────

    #[test]
    fn build_typed_end_of_emits_const_muli_addi_triplet_for_i32() {
        let ops = build_typed_end_of(
            TypedMemrefElem::I32,
            ValueId(0), // data
            ValueId(1), // len
            ValueId(2), // sizeof const id
            ValueId(3), // bytes id
            ValueId(4), // end id
        );
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0].name, "arith.constant");
        assert!(ops[0]
            .attributes
            .iter()
            .any(|(k, v)| k == "value" && v == "4"));
        assert_eq!(ops[1].name, "arith.muli");
        assert_eq!(ops[1].operands, vec![ValueId(1), ValueId(2)]);
        assert_eq!(ops[2].name, PTR_ARITH_END_OF);
        assert_eq!(ops[2].operands, vec![ValueId(0), ValueId(3)]);
        assert_eq!(ops[2].results[0].id, ValueId(4));
    }

    #[test]
    fn build_typed_end_of_uses_per_primitive_sizeof_constant() {
        // F64 → 8 ; I8 → 1 ; F32 → 4. Verifies the constant emitter reads
        // sizeof(T) per-instance rather than baking a single literal.
        let ops_f64 = build_typed_end_of(
            TypedMemrefElem::F64,
            ValueId(0),
            ValueId(1),
            ValueId(2),
            ValueId(3),
            ValueId(4),
        );
        assert!(ops_f64[0]
            .attributes
            .iter()
            .any(|(k, v)| k == "value" && v == "8"));

        let ops_i8 = build_typed_end_of(
            TypedMemrefElem::I8,
            ValueId(0),
            ValueId(1),
            ValueId(2),
            ValueId(3),
            ValueId(4),
        );
        assert!(ops_i8[0]
            .attributes
            .iter()
            .any(|(k, v)| k == "value" && v == "1"));
    }

    // ── 7. Index-offset helper for load/store at index `i` ──────────────

    #[test]
    fn build_index_offset_emits_const_plus_muli_pair() {
        let ops = build_index_offset(
            TypedMemrefElem::I32,
            ValueId(7), // index
            ValueId(8), // sizeof const id
            ValueId(9), // bytes id
        );
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].name, "arith.constant");
        assert!(ops[0]
            .attributes
            .iter()
            .any(|(k, v)| k == "value" && v == "4"));
        assert_eq!(ops[1].name, "arith.muli");
        assert_eq!(ops[1].operands, vec![ValueId(7), ValueId(8)]);
        assert_eq!(ops[1].results[0].id, ValueId(9));
    }

    #[test]
    fn build_index_offset_per_primitive_distinct_constants() {
        // Confirm sizeof varies as expected across primitives so vec_index
        // monomorphizes correctly.
        let mut const_values: Vec<String> = TypedMemrefElem::ALL
            .iter()
            .map(|e| {
                let ops = build_index_offset(*e, ValueId(0), ValueId(1), ValueId(2));
                ops[0]
                    .attributes
                    .iter()
                    .find(|(k, _)| k == "value")
                    .map(|(_, v)| v.clone())
                    .unwrap()
            })
            .collect();
        // Expected sizeof set : {1, 2, 4, 4, 8, 8} — dedup → {1, 2, 4, 8}.
        const_values.sort();
        const_values.dedup();
        assert_eq!(const_values, vec!["1", "2", "4", "8"]);
    }

    // ── 8. Monomorphization-correctness : op-names depend ONLY on T ─────

    #[test]
    fn monomorphization_correctness_independent_of_value_ids() {
        // Same primitive → same op-name regardless of value-ids passed in.
        let a = build_typed_load(
            TypedMemrefElem::I32,
            ValueId(0),
            ValueId(1),
            ValueId(2),
        );
        let b = build_typed_load(
            TypedMemrefElem::I32,
            ValueId(100),
            ValueId(200),
            ValueId(300),
        );
        assert_eq!(a.name, b.name);

        // Different primitive → distinct op-name.
        let c = build_typed_load(
            TypedMemrefElem::F32,
            ValueId(0),
            ValueId(1),
            ValueId(2),
        );
        assert_ne!(a.name, c.name);
    }

    #[test]
    fn all_typed_ops_carry_elem_ty_attribute() {
        // Every op produced by this module carries `elem_ty` so downstream
        // walkers can audit per-instance specialization without re-parsing
        // the op-name.
        for elem in TypedMemrefElem::ALL {
            let load = build_typed_load(elem, ValueId(0), ValueId(1), ValueId(2));
            assert!(load
                .attributes
                .iter()
                .any(|(k, v)| k == "elem_ty" && v == elem.name()));
            let store = build_typed_store(elem, ValueId(0), ValueId(1), ValueId(2));
            assert!(store
                .attributes
                .iter()
                .any(|(k, v)| k == "elem_ty" && v == elem.name()));
            let end_of_ops =
                build_typed_end_of(elem, ValueId(0), ValueId(1), ValueId(2), ValueId(3), ValueId(4));
            assert!(end_of_ops[2]
                .attributes
                .iter()
                .any(|(k, v)| k == "elem_ty" && v == elem.name()));
        }
    }
}

// INTEGRATION_NOTE :
//   To wire this module into the `cssl-mir` crate's public surface, add
//
//       pub mod memref_typed;
//
//   to `cssl-mir/src/lib.rs` (alongside the existing `pub mod block;` /
//   `pub mod body_lower;` declarations). This file deliberately does NOT
//   modify `lib.rs` ; the integration commit owns that one-line change.
//   The matching cgen integration is `cssl-cgen-cpu-cranelift/src/cgen_memref.rs`
//   — its INTEGRATION_NOTE adds `pub mod cgen_memref;` to that crate's
//   `lib.rs` analogously.
