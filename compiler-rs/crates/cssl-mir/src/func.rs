//! MIR functions + module.
//!
//! § DESIGN
//!   - [`MirFunc`] : name + signature + body (one region).
//!   - [`MirModule`] : top-level container of fns + module-level attributes.
//!
//! Each fn is lowered to a single `func.func` op in textual-MLIR form ; internally
//! we model it as a `MirFunc` with an owned `MirRegion` so the pretty-printer can
//! emit the canonical `func.func @name(args) -> results { ... }` shape.

use std::collections::BTreeMap;

use crate::block::{MirOp, MirRegion};
use crate::value::{MirType, MirValue, ValueId};

// ─────────────────────────────────────────────────────────────────────────
// § STRUCT-FFI ABI TABLE  (T11-W17-A · stage-0 struct-FFI codegen)
// ─────────────────────────────────────────────────────────────────────────
//
// § PURPOSE
//   Stage-0 codegen needs to know the byte-layout + alignment of every
//   user-declared struct that crosses a fn-boundary so the cranelift
//   signature builder can decide WHETHER to lower the struct as :
//     - i8 / i16 / i32 / i64    : pass-by-value (size ≤ 8B newtype)
//     - host-pointer            : pass-by-pointer (size > 8B Win-x64 ABI)
//   This module owns the deterministic name → MirStructLayout map that
//   the HIR → MIR lowering populates + cgen-cpu-cranelift queries.
//
// § DETERMINISM
//   `BTreeMap<String, ...>` not HashMap — substrate's Σ-mask discipline
//   demands repeatable codegen across runs.
//
// § STAGE-0 SCOPE
//   - Newtype structs (`struct RunHandle { raw: u64 }`)        ← W17-A focus
//   - Multi-scalar single-word structs (`struct Pos { x:u32, y:u32 }`)
//   - Pointer-by-reference for size > 8B
// § DEFERRED to W17-B+
//   - Win-x64 register-pair return for 9..16B structs
//   - SysV-AMD64 multi-class register lowering (M! aux per ABI rule)
//   - Generic-struct monomorph instantiation tracking
//
// § ATTESTATION (PRIME_DIRECTIVE §11)
//   t∞: ¬(hurt ∨ harm) .making-of-T11-W17-A @ (anyone ∨ anything ∨ anybody)

/// One entry in the per-module struct-layout side-table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirStructLayout {
    /// Source-form struct name (without module prefix ; matches the
    /// `!cssl.struct.<name>` opaque-tag in `MirType::Opaque`).
    pub name: String,
    /// Field types in declaration order. `Vec<MirType>` rather than
    /// `Vec<(String, MirType)>` because stage-0 ABI is positional.
    pub fields: Vec<MirType>,
    /// Total byte-size of the struct (sum of field sizes + tail padding).
    /// `0` is treated as "unknown / opaque" by the codegen consumer.
    pub size_bytes: u32,
    /// Required alignment (max of field alignments, rounded to a power-of-2).
    pub align_bytes: u8,
}

impl MirStructLayout {
    /// Construct a layout from a name + field list. Caller is responsible for
    /// computing byte-size / alignment via the `compute_size_align` helper or
    /// supplying a pre-computed pair.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        fields: Vec<MirType>,
        size_bytes: u32,
        align_bytes: u8,
    ) -> Self {
        Self {
            name: name.into(),
            fields,
            size_bytes,
            align_bytes,
        }
    }

    /// Naive byte-size + alignment computer for a list of MIR types.
    /// Stage-0 uses native scalar widths (i8=1, i16=2, i32=4, i64=8,
    /// f32=4, f64=8, bool=1, ptr/handle=8 host-pointer-width). Aggregate
    /// fields fall back to `(0, 1)` so the caller can flag the layout
    /// as "unknown" + degrade to ptr-by-reference.
    #[must_use]
    pub fn compute_size_align(fields: &[MirType]) -> (u32, u8) {
        use crate::value::{FloatWidth, IntWidth, MirType as MT};
        let mut size: u32 = 0;
        let mut align: u8 = 1;
        for f in fields {
            let (fs, fa): (u32, u8) = match f {
                MT::Bool => (1, 1),
                MT::Int(IntWidth::I1) => (1, 1),
                MT::Int(IntWidth::I8) => (1, 1),
                MT::Int(IntWidth::I16) => (2, 2),
                MT::Int(IntWidth::I32) => (4, 4),
                MT::Int(IntWidth::I64) => (8, 8),
                MT::Int(IntWidth::Index) => (8, 8),
                MT::Float(FloatWidth::F16) => (2, 2),
                MT::Float(FloatWidth::Bf16) => (2, 2),
                MT::Float(FloatWidth::F32) => (4, 4),
                MT::Float(FloatWidth::F64) => (8, 8),
                MT::Handle | MT::Ptr => (8, 8),
                _ => (0, 1), // unknown — caller-side ABI fallback
            };
            // Round size up to field alignment.
            if fa > 1 {
                let mask: u32 = u32::from(fa - 1);
                size = (size + mask) & !mask;
            }
            size = size.saturating_add(fs);
            if fa > align {
                align = fa;
            }
        }
        // Round total size up to the struct's alignment.
        if align > 1 {
            let mask: u32 = u32::from(align - 1);
            size = (size + mask) & !mask;
        }
        (size, align)
    }

    /// Stage-0 ABI classification : how does this struct cross an FFI boundary?
    /// `None` → unknown EMPTY-fields layout (caller should reject) ; otherwise
    /// canonical scalar-width-or-pointer choice.
    ///
    /// § T11-W19-α-CSSLC-FIX4 — non-empty fields with size=0 (unrecognized
    /// aggregate like `[u8; 32]`) fall back to `PointerByRef`.
    #[must_use]
    pub fn abi_class(&self) -> Option<StructAbiClass> {
        if self.size_bytes == 0 {
            if self.fields.is_empty() {
                return None;
            }
            return Some(StructAbiClass::PointerByRef);
        }
        Some(match self.size_bytes {
            1 => StructAbiClass::ScalarI8,
            2 => StructAbiClass::ScalarI16,
            3..=4 => StructAbiClass::ScalarI32,
            5..=8 => StructAbiClass::ScalarI64,
            _ => StructAbiClass::PointerByRef,
        })
    }
}

/// Stage-0 struct-FFI ABI choice for a single struct type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructAbiClass {
    /// Pass / return as `i8` register-value.
    ScalarI8,
    /// Pass / return as `i16` register-value.
    ScalarI16,
    /// Pass / return as `i32` register-value.
    ScalarI32,
    /// Pass / return as `i64` register-value (covers newtype-u64 case).
    ScalarI64,
    /// Pass as host-pointer-to-struct (Win-x64 / SysV >2-word rule).
    PointerByRef,
}

// ─────────────────────────────────────────────────────────────────────────
// § ENUM-FFI ABI TABLE  (T11-W19-α-CSSLC-FIX4-ENUM · stage-0 enum-FFI codegen)
// ─────────────────────────────────────────────────────────────────────────
//
// § PURPOSE
//   Stage-0 cgen needs to recognize unit-only (C-like) enums at fn-FFI
//   boundaries so cgen can lower the enum-name to a discriminant scalar.
//   Without this, Opaque("IoError") / Opaque("NetError") / etc. surface as
//   "non-scalar MIR type" rejections in `build_clif_signature`.
//
// § STAGE-0 SCOPE
//   Unit-only enums → i8 (≤256 variants) or i16/i32 for larger.
// § DEFERRED
//   Mixed-payload enums fall back to PointerByRef (FIX3 territory for
//   real construction/destruction).

/// One entry in the per-module enum-layout side-table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirEnumLayout {
    pub name: String,
    pub variant_count: u32,
    pub is_unit_only: bool,
}

impl MirEnumLayout {
    #[must_use]
    pub fn new(name: impl Into<String>, variant_count: u32, is_unit_only: bool) -> Self {
        Self {
            name: name.into(),
            variant_count,
            is_unit_only,
        }
    }

    /// Stage-0 ABI classification.
    #[must_use]
    pub fn abi_class(&self) -> Option<EnumAbiClass> {
        if !self.is_unit_only {
            return Some(EnumAbiClass::PointerByRef);
        }
        Some(if self.variant_count <= 256 {
            EnumAbiClass::ScalarI8
        } else if self.variant_count <= 65_536 {
            EnumAbiClass::ScalarI16
        } else {
            EnumAbiClass::ScalarI32
        })
    }
}

/// Stage-0 enum-FFI ABI choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnumAbiClass {
    ScalarI8,
    ScalarI16,
    ScalarI32,
    PointerByRef,
}

/// A function in the MIR module.
#[derive(Debug, Clone)]
pub struct MirFunc {
    /// Source-form fn name (without `@` prefix).
    pub name: String,
    /// Parameter types (order matches `body.entry().args`).
    pub params: Vec<MirType>,
    /// Return types (stage-0 : at most 1 result, but multi-result support is here).
    pub results: Vec<MirType>,
    /// Effect-row as a free-form string (structural form : `"{GPU, NoAlloc}"`).
    /// `None` = pure. Structured effect-row attribute is T6-phase-2 work.
    pub effect_row: Option<String>,
    /// Cap annotation on the fn value itself (e.g., `"val"`).
    pub cap: Option<String>,
    /// IFC label attribute (if any) — free-form at stage-0.
    pub ifc_label: Option<String>,
    /// Attribute dictionary for additional flags (e.g., `"@differentiable"`).
    pub attributes: Vec<(String, String)>,
    /// T11-D43 : `true` iff the source HIR fn declared generic parameters
    /// (`fn f<T>(…)`). Generic fns carry type-param placeholder `Opaque`
    /// types in their params/body and cannot be JIT-compiled directly —
    /// they must be specialized first via `specialize_generic_fn`. The
    /// `drop_unspecialized_generic_fns` cleanup pass removes them after
    /// monomorphization so downstream passes see only concrete fns.
    pub is_generic: bool,
    /// The fn body — a single region with at-least an entry block.
    pub body: MirRegion,
    /// Monotonic counter used for fresh-value-id allocation within the body.
    pub next_value_id: u32,
}

impl MirFunc {
    /// Build a fn with the given name + signature. Body starts with an empty entry
    /// block whose args match `params`.
    #[must_use]
    pub fn new(name: impl Into<String>, params: Vec<MirType>, results: Vec<MirType>) -> Self {
        let args: Vec<MirValue> = params
            .iter()
            .enumerate()
            .map(|(i, t)| MirValue::new(ValueId(i as u32), t.clone()))
            .collect();
        let body = MirRegion::with_entry(args);
        let next_value_id = params.len() as u32;
        Self {
            name: name.into(),
            params,
            results,
            effect_row: None,
            cap: None,
            ifc_label: None,
            attributes: Vec::new(),
            is_generic: false,
            body,
            next_value_id,
        }
    }

    /// Allocate a fresh SSA value id.
    pub fn fresh_value_id(&mut self) -> ValueId {
        let id = ValueId(self.next_value_id);
        self.next_value_id = self.next_value_id.saturating_add(1);
        id
    }

    /// `true` iff this fn has no body (signature-only, like an interface method).
    #[must_use]
    pub fn is_signature_only(&self) -> bool {
        self.body.blocks.iter().all(|b| b.ops.is_empty())
    }

    /// Append an op to the entry block.
    pub fn push_op(&mut self, op: MirOp) {
        if let Some(entry) = self.body.entry_mut() {
            entry.push(op);
        }
    }
}

/// Top-level MIR module — a list of fns + module-level attributes.
#[derive(Debug, Clone, Default)]
pub struct MirModule {
    /// Module name (from source `module com.apocky.loa`).
    pub name: Option<String>,
    /// Functions in declaration order.
    pub funcs: Vec<MirFunc>,
    /// Module-level attributes.
    pub attributes: Vec<(String, String)>,
    /// Struct-name → layout mapping for FFI-crossing structs.
    /// T11-W17-A · stage-0 struct-FFI codegen support.
    pub struct_layouts: BTreeMap<String, MirStructLayout>,
    /// Enum-name → layout mapping. T11-W19-α-CSSLC-FIX4-ENUM.
    pub enum_layouts: BTreeMap<String, MirEnumLayout>,
}

impl MirModule {
    /// Empty module.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Module with a declared name.
    #[must_use]
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            funcs: Vec::new(),
            attributes: Vec::new(),
            struct_layouts: BTreeMap::new(),
            enum_layouts: BTreeMap::new(),
        }
    }

    /// Append a function.
    pub fn push_func(&mut self, f: MirFunc) {
        self.funcs.push(f);
    }

    /// Lookup a fn by name.
    #[must_use]
    pub fn find_func(&self, name: &str) -> Option<&MirFunc> {
        self.funcs.iter().find(|f| f.name == name)
    }

    /// Register a struct-FFI layout. Overwrites any previous entry of the
    /// same name (last-write-wins ; HIR parser emits each struct once).
    /// T11-W17-A · stage-0 struct-FFI codegen.
    pub fn add_struct_layout(&mut self, layout: MirStructLayout) {
        self.struct_layouts.insert(layout.name.clone(), layout);
    }

    /// Look up a struct's layout by name.
    #[must_use]
    pub fn find_struct_layout(&self, name: &str) -> Option<&MirStructLayout> {
        self.struct_layouts.get(name)
    }

    /// Register an enum-FFI layout. T11-W19-α-CSSLC-FIX4-ENUM.
    pub fn add_enum_layout(&mut self, layout: MirEnumLayout) {
        self.enum_layouts.insert(layout.name.clone(), layout);
    }

    /// Look up an enum's layout by name.
    #[must_use]
    pub fn find_enum_layout(&self, name: &str) -> Option<&MirEnumLayout> {
        self.enum_layouts.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::{MirFunc, MirModule, MirStructLayout, StructAbiClass};
    use crate::value::{IntWidth, MirType};

    #[test]
    fn fn_new_populates_entry_args() {
        let params = vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)];
        let f = MirFunc::new("add", params, vec![MirType::Int(IntWidth::I32)]);
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        let entry = f.body.entry().unwrap();
        assert_eq!(entry.args.len(), 2);
        assert_eq!(f.next_value_id, 2);
    }

    #[test]
    fn fresh_value_id_increments() {
        let mut f = MirFunc::new("foo", vec![], vec![]);
        let v0 = f.fresh_value_id();
        let v1 = f.fresh_value_id();
        assert_ne!(v0, v1);
    }

    #[test]
    fn is_signature_only_for_empty_body() {
        let f = MirFunc::new("stub", vec![], vec![]);
        assert!(f.is_signature_only());
    }

    #[test]
    fn module_find_func_by_name() {
        let mut m = MirModule::with_name("mymod");
        m.push_func(MirFunc::new("foo", vec![], vec![]));
        m.push_func(MirFunc::new("bar", vec![], vec![]));
        assert!(m.find_func("foo").is_some());
        assert!(m.find_func("nope").is_none());
        assert_eq!(m.name.as_deref(), Some("mymod"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § STRUCT-FFI layout tests  (T11-W17-A)
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn struct_layout_newtype_u64_is_8b_aligned8() {
        // RunHandle { raw: u64 } → 8B / align 8
        let fields = vec![MirType::Int(IntWidth::I64)];
        let (size, align) = MirStructLayout::compute_size_align(&fields);
        assert_eq!(size, 8);
        assert_eq!(align, 8);
    }

    #[test]
    fn struct_layout_two_u32_is_8b_aligned4() {
        // Pos { x: u32, y: u32 } → 8B / align 4
        let fields = vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I32)];
        let (size, align) = MirStructLayout::compute_size_align(&fields);
        assert_eq!(size, 8);
        assert_eq!(align, 4);
    }

    #[test]
    fn struct_layout_u8_then_u32_pads_to_8b() {
        // { flag: u8 , value: u32 } → 1B field + 3B pad + 4B field = 8B / align 4
        let fields = vec![MirType::Int(IntWidth::I8), MirType::Int(IntWidth::I32)];
        let (size, align) = MirStructLayout::compute_size_align(&fields);
        assert_eq!(size, 8, "expected 8B with internal padding");
        assert_eq!(align, 4);
    }

    #[test]
    fn struct_layout_3_u64_is_24b_aligned8() {
        // ShareReceipt-like { lo: u64 , hi: u64 , extra: u64 } → 24B / align 8
        let fields = vec![
            MirType::Int(IntWidth::I64),
            MirType::Int(IntWidth::I64),
            MirType::Int(IntWidth::I64),
        ];
        let (size, align) = MirStructLayout::compute_size_align(&fields);
        assert_eq!(size, 24);
        assert_eq!(align, 8);
    }

    #[test]
    fn struct_abi_class_newtype_lowers_to_i64() {
        // 8B → i64 register pass-by-value
        let l = MirStructLayout::new(
            "RunHandle",
            vec![MirType::Int(IntWidth::I64)],
            8,
            8,
        );
        assert_eq!(l.abi_class(), Some(StructAbiClass::ScalarI64));
    }

    #[test]
    fn struct_abi_class_2word_lowers_to_pointer() {
        // 24B → PointerByRef (Win-x64 ABI rule)
        let l = MirStructLayout::new(
            "ShareReceipt3",
            vec![
                MirType::Int(IntWidth::I64),
                MirType::Int(IntWidth::I64),
                MirType::Int(IntWidth::I64),
            ],
            24,
            8,
        );
        assert_eq!(l.abi_class(), Some(StructAbiClass::PointerByRef));
    }

    #[test]
    fn struct_abi_class_byte_struct_lowers_to_i8() {
        let l = MirStructLayout::new("Tag", vec![MirType::Int(IntWidth::I8)], 1, 1);
        assert_eq!(l.abi_class(), Some(StructAbiClass::ScalarI8));
    }

    #[test]
    fn struct_abi_class_4byte_struct_lowers_to_i32() {
        let l = MirStructLayout::new("Color", vec![MirType::Int(IntWidth::I32)], 4, 4);
        assert_eq!(l.abi_class(), Some(StructAbiClass::ScalarI32));
    }

    #[test]
    fn struct_abi_class_zero_size_is_unknown() {
        // Defensive : a 0-byte layout (e.g., empty struct) is rejected at ABI.
        let l = MirStructLayout::new("Empty", vec![], 0, 1);
        assert_eq!(l.abi_class(), None);
    }

    #[test]
    fn module_add_and_find_struct_layout() {
        let mut m = MirModule::with_name("test");
        let l = MirStructLayout::new(
            "RunHandle",
            vec![MirType::Int(IntWidth::I64)],
            8,
            8,
        );
        m.add_struct_layout(l.clone());
        assert_eq!(m.find_struct_layout("RunHandle"), Some(&l));
        assert_eq!(m.find_struct_layout("Nope"), None);
    }

    #[test]
    fn module_struct_layouts_btree_iteration_is_deterministic() {
        let mut m = MirModule::default();
        m.add_struct_layout(MirStructLayout::new(
            "Z",
            vec![MirType::Int(IntWidth::I32)],
            4,
            4,
        ));
        m.add_struct_layout(MirStructLayout::new(
            "A",
            vec![MirType::Int(IntWidth::I32)],
            4,
            4,
        ));
        m.add_struct_layout(MirStructLayout::new(
            "M",
            vec![MirType::Int(IntWidth::I32)],
            4,
            4,
        ));
        let names: Vec<&str> = m.struct_layouts.keys().map(String::as_str).collect();
        assert_eq!(names, vec!["A", "M", "Z"], "BTreeMap orders by key");
    }
}
