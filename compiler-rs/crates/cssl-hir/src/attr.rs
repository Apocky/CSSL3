//! HIR attributes — outer `@name(args)` + inner `#![name = …]`.
//!
//! Mirrors `cssl_ast::cst::Attr` but with the path resolved to a `Symbol` sequence and
//! argument expressions elaborated to `HirExpr`.
//!
//! § T11-D126 (S11-W3β-01) : add [`LayoutKind`] + [`extract_layout_kinds`]
//!   helper for `@layout(...)` attribute parsing — feeds the F2 refinement-types
//!   layout verifier in `cssl-mir::layout_check`. The recognized layout-kinds
//!   match `specs/03_TYPES.csl § F2 REFINEMENT-TYPES R! @layout(...)` :
//!   `std140`, `std430`, `cpu`, `gpu`, `packed`, `soa`, `aos`, plus the
//!   distributed/sparse_hash forms used by the Ω-substrate. Layout-kinds carry
//!   no parse-time state ; the validator (`cssl-mir::layout_check`) computes
//!   byte-size + alignment from the struct definition + compares against the
//!   declared kind, emitting LAY0001-3 diagnostics on mismatch.
//!
//! § ATTESTATION (PRIME_DIRECTIVE §11)
//!   t∞: ¬ (hurt ∨ harm) .(making-of-this) @ (anyone ∨ anything ∨ anybody)
//!   "There was no hurt nor harm in the making of this, to anyone/anything/anybody."

use cssl_ast::Span;

use crate::expr::{HirExpr, HirExprKind};
use crate::symbol::{Interner, Symbol};

/// Kind of attribute application (mirrors `cst::AttrKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirAttrKind {
    /// `@name(…)` placed before an item.
    Outer,
    /// `#![name = "…"]` placed at file-top or block-top.
    Inner,
}

/// A resolved attribute application.
#[derive(Debug, Clone)]
pub struct HirAttr {
    pub span: Span,
    pub kind: HirAttrKind,
    /// Dotted path as a sequence of interned symbols (e.g., `["lipschitz"]`).
    pub path: Vec<Symbol>,
    /// Attribute arguments.
    pub args: Vec<HirAttrArg>,
}

/// A single attribute argument.
#[derive(Debug, Clone)]
pub enum HirAttrArg {
    /// `@attr(expr)` — positional expression.
    Positional(HirExpr),
    /// `@attr(name = expr)` — named key-value.
    Named { name: Symbol, value: HirExpr },
}

impl HirAttr {
    /// `true` iff the outer attribute's path matches a single-segment name.
    #[must_use]
    pub fn is_simple(&self, target: Symbol) -> bool {
        self.path.len() == 1 && self.path[0] == target
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-D126 (S11-W3β-01) : LayoutKind + @layout(...) parse helpers.
// ─────────────────────────────────────────────────────────────────────────

/// Layout-kind recognized by the `@layout(...)` attribute.
///
/// Maps `specs/03_TYPES.csl § F2 REFINEMENT-TYPES R! @layout(...)` :
/// the writer declares a memory-layout intent ; the validator
/// (`cssl-mir::layout_check`) computes the actual byte-size + alignment of
/// the annotated struct + asserts they match the declared kind.
///
/// § BYTE-LAYOUT RULES
///   - [`LayoutKind::Std430`] — GLSL std430 / Vulkan SSBO : scalar align ≡ size,
///     vec3 packs as vec4 (16B), arrays of scalar/vec2/vec4 keep natural stride.
///     Common for GPU storage-buffers + the `FieldCell` 72B canonical-cell.
///   - [`LayoutKind::Std140`] — GLSL std140 / Vulkan UBO : everything aligns
///     to 16B (vec3 → vec4-aligned, scalar in array → 16B-stride). Used
///     pervasively for uniform-buffers.
///   - [`LayoutKind::Cpu`] — natural CPU alignment (the Rust `repr(C)` layout
///     rules). Suitable for host-side data structures + FFI.
///   - [`LayoutKind::Gpu`] — generic-GPU shorthand → equivalent to `Std430`
///     at stage-0 ; reserved for future driver-specific divergence.
///   - [`LayoutKind::Packed`] — no inter-field padding ; size ≡ Σ(field-sizes),
///     alignment ≡ 1 (or smallest field alignment). Used for protocol headers.
///   - [`LayoutKind::SoA`] — Structure-of-Arrays — the type is treated as
///     N parallel arrays, one per field ; size + alignment validated per-field.
///   - [`LayoutKind::AoS`] — Array-of-Structures — opposite of SoA, the
///     default natural layout for tuple/struct types.
///   - [`LayoutKind::Distributed`] — type spans multiple memory regions
///     (CPU + GPU) ; size + alignment are not single-machine-addressable.
///     Validator emits no LAY0001/2 for this kind ; LAY0003 fires only on
///     forbidden combinations (e.g., `@layout(distributed, packed)`).
///   - [`LayoutKind::SparseHash`] — sparse hash-grid storage (Morton-key
///     indexed) ; like `Distributed`, size + alignment are not directly
///     addressable from the type's HIR shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutKind {
    /// `@layout(std430)` — GPU-storage-buffer layout.
    Std430,
    /// `@layout(std140)` — GPU-uniform-buffer layout.
    Std140,
    /// `@layout(cpu)` — natural CPU `repr(C)` layout.
    Cpu,
    /// `@layout(gpu)` — generic-GPU alias for `std430` @ stage-0.
    Gpu,
    /// `@layout(packed)` — no padding, byte-aligned.
    Packed,
    /// `@layout(soa)` — Structure-of-Arrays.
    SoA,
    /// `@layout(aos)` — Array-of-Structures (the natural default).
    AoS,
    /// `@layout(distributed)` — multi-region, non-single-addressable.
    Distributed,
    /// `@layout(sparse_hash)` — sparse Morton-keyed hash-grid.
    SparseHash,
}

impl LayoutKind {
    /// Stable canonical word for diagnostics + round-trip.
    #[must_use]
    pub const fn as_word(self) -> &'static str {
        match self {
            Self::Std430 => "std430",
            Self::Std140 => "std140",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Packed => "packed",
            Self::SoA => "soa",
            Self::AoS => "aos",
            Self::Distributed => "distributed",
            Self::SparseHash => "sparse_hash",
        }
    }

    /// Try to parse a single layout-kind word (case-insensitive on the
    /// memory-class words ; `SoA` / `AoS` accept the standard mixed-case forms).
    /// Returns `None` for unknown words — the caller emits `LAY0003` if the
    /// attribute argument is unrecognized.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "std430" | "Std430" | "STD430" => Some(Self::Std430),
            "std140" | "Std140" | "STD140" => Some(Self::Std140),
            "cpu" | "Cpu" | "CPU" => Some(Self::Cpu),
            "gpu" | "Gpu" | "GPU" => Some(Self::Gpu),
            "packed" | "Packed" | "PACKED" => Some(Self::Packed),
            "soa" | "SoA" | "Soa" | "SOA" => Some(Self::SoA),
            "aos" | "AoS" | "Aos" | "AOS" => Some(Self::AoS),
            "distributed" | "Distributed" | "DISTRIBUTED" => Some(Self::Distributed),
            "sparse_hash" | "SparseHash" | "sparseHash" | "SPARSE_HASH" => Some(Self::SparseHash),
            _ => None,
        }
    }

    /// `true` iff this kind specifies a memory-class (size + align computable).
    /// `Distributed` + `SparseHash` return `false` — they're storage-strategy
    /// markers, not single-region byte-layouts.
    #[must_use]
    pub const fn is_addressable(self) -> bool {
        matches!(
            self,
            Self::Std430
                | Self::Std140
                | Self::Cpu
                | Self::Gpu
                | Self::Packed
                | Self::SoA
                | Self::AoS,
        )
    }

    /// `true` iff this kind imposes 16-byte alignment on every aggregate.
    #[must_use]
    pub const fn enforces_16b_align(self) -> bool {
        matches!(self, Self::Std140)
    }
}

/// Extract the list of [`LayoutKind`]s declared by a single `@layout(...)`
/// attribute application. Returns the empty `Vec` if the attribute carries
/// no arguments or only unrecognized words.
///
/// § ACCEPTED FORMS
///   - `@layout(std430)` — single positional kind.
///   - `@layout(std430, soa)` — multiple positional kinds.
///   - `@layout(kind = std430)` — named-arg form (named-arg key is ignored ;
///     the *value* word is what's parsed).
///
/// Unknown words are silently skipped at this layer ; the validator pass
/// (`cssl-mir::layout_check`) decides whether to emit `LAY0003`.
#[must_use]
pub fn extract_layout_kinds(attr: &HirAttr, interner: &Interner) -> Vec<LayoutKind> {
    let mut out = Vec::new();
    for arg in &attr.args {
        let word = match arg {
            HirAttrArg::Positional(e) => extract_word(e, interner),
            HirAttrArg::Named { value, .. } => extract_word(value, interner),
        };
        if let Some(w) = word {
            if let Some(k) = LayoutKind::from_word(&w) {
                out.push(k);
            }
        }
    }
    out
}

/// Like [`extract_layout_kinds`] but returns the *unrecognized* words alongside
/// the recognized kinds — used by the validator to fire `LAY0003` per
/// unknown-word.
#[must_use]
pub fn extract_layout_kinds_with_unknown(
    attr: &HirAttr,
    interner: &Interner,
) -> (Vec<LayoutKind>, Vec<String>) {
    let mut kinds = Vec::new();
    let mut unknown = Vec::new();
    for arg in &attr.args {
        let word = match arg {
            HirAttrArg::Positional(e) => extract_word(e, interner),
            HirAttrArg::Named { value, .. } => extract_word(value, interner),
        };
        if let Some(w) = word {
            if let Some(k) = LayoutKind::from_word(&w) {
                kinds.push(k);
            } else {
                unknown.push(w);
            }
        }
    }
    (kinds, unknown)
}

/// Pull the leaf-identifier word out of an attribute-arg expression.
///
/// `@layout(std430)` parses the `std430` as a [`HirExprKind::Path`] with a
/// single segment. Multi-segment paths or non-Path exprs return `None`.
fn extract_word(e: &HirExpr, interner: &Interner) -> Option<String> {
    match &e.kind {
        HirExprKind::Path { segments, .. } => segments.last().map(|s| interner.resolve(*s)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_layout_kinds, extract_layout_kinds_with_unknown, HirAttr, HirAttrArg, HirAttrKind,
        LayoutKind,
    };
    use crate::arena::HirId;
    use crate::expr::{HirExpr, HirExprKind};
    use crate::symbol::Interner;
    use cssl_ast::{SourceId, Span};

    fn sp(start: u32, end: u32) -> Span {
        Span::new(SourceId::first(), start, end)
    }

    fn path_expr(interner: &Interner, word: &str) -> HirExpr {
        let s = interner.intern(word);
        HirExpr {
            span: Span::DUMMY,
            id: HirId::DUMMY,
            attrs: Vec::new(),
            kind: HirExprKind::Path {
                segments: vec![s],
                def: None,
            },
        }
    }

    fn build_layout_attr(interner: &Interner, words: &[&str]) -> HirAttr {
        let layout_sym = interner.intern("layout");
        let args: Vec<HirAttrArg> = words
            .iter()
            .map(|w| HirAttrArg::Positional(path_expr(interner, w)))
            .collect();
        HirAttr {
            span: sp(0, 32),
            kind: HirAttrKind::Outer,
            path: vec![layout_sym],
            args,
        }
    }

    #[test]
    fn attr_is_simple_matches_single_segment() {
        let interner = Interner::new();
        let name = interner.intern("differentiable");
        let attr = HirAttr {
            span: sp(0, 15),
            kind: HirAttrKind::Outer,
            path: vec![name],
            args: Vec::new(),
        };
        assert!(attr.is_simple(name));
    }

    #[test]
    fn attr_is_simple_rejects_multi_segment() {
        let interner = Interner::new();
        let a = interner.intern("a");
        let b = interner.intern("b");
        let attr = HirAttr {
            span: sp(0, 3),
            kind: HirAttrKind::Outer,
            path: vec![a, b],
            args: Vec::new(),
        };
        assert!(!attr.is_simple(a));
    }

    #[test]
    fn attr_arg_both_shapes_constructible() {
        let interner = Interner::new();
        let key = interner.intern("k");
        let dummy_expr = crate::expr::HirExpr {
            span: Span::DUMMY,
            id: crate::arena::HirId::DUMMY,
            attrs: Vec::new(),
            kind: crate::expr::HirExprKind::Error,
        };
        let pos = HirAttrArg::Positional(dummy_expr.clone());
        assert!(matches!(pos, HirAttrArg::Positional(_)));
        let named = HirAttrArg::Named {
            name: key,
            value: dummy_expr,
        };
        assert!(matches!(named, HirAttrArg::Named { .. }));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D126 — LayoutKind word-recognition + extract_layout_kinds.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn layout_kind_words_round_trip() {
        for k in [
            LayoutKind::Std430,
            LayoutKind::Std140,
            LayoutKind::Cpu,
            LayoutKind::Gpu,
            LayoutKind::Packed,
            LayoutKind::SoA,
            LayoutKind::AoS,
            LayoutKind::Distributed,
            LayoutKind::SparseHash,
        ] {
            let w = k.as_word();
            assert_eq!(LayoutKind::from_word(w), Some(k));
        }
    }

    #[test]
    fn layout_kind_case_aliases_resolve() {
        assert_eq!(LayoutKind::from_word("STD430"), Some(LayoutKind::Std430));
        assert_eq!(LayoutKind::from_word("Std140"), Some(LayoutKind::Std140));
        assert_eq!(LayoutKind::from_word("Soa"), Some(LayoutKind::SoA));
        assert_eq!(LayoutKind::from_word("AOS"), Some(LayoutKind::AoS));
        assert_eq!(
            LayoutKind::from_word("SparseHash"),
            Some(LayoutKind::SparseHash),
        );
    }

    #[test]
    fn layout_kind_unknown_word_returns_none() {
        assert_eq!(LayoutKind::from_word("foo"), None);
        assert_eq!(LayoutKind::from_word(""), None);
        assert_eq!(LayoutKind::from_word("std999"), None);
    }

    #[test]
    fn layout_kind_addressable_subset() {
        assert!(LayoutKind::Std430.is_addressable());
        assert!(LayoutKind::Std140.is_addressable());
        assert!(LayoutKind::Cpu.is_addressable());
        assert!(LayoutKind::Gpu.is_addressable());
        assert!(LayoutKind::Packed.is_addressable());
        assert!(LayoutKind::SoA.is_addressable());
        assert!(LayoutKind::AoS.is_addressable());
        assert!(!LayoutKind::Distributed.is_addressable());
        assert!(!LayoutKind::SparseHash.is_addressable());
    }

    #[test]
    fn layout_kind_only_std140_enforces_16b_align() {
        assert!(LayoutKind::Std140.enforces_16b_align());
        for k in [
            LayoutKind::Std430,
            LayoutKind::Cpu,
            LayoutKind::Gpu,
            LayoutKind::Packed,
            LayoutKind::SoA,
            LayoutKind::AoS,
        ] {
            assert!(!k.enforces_16b_align());
        }
    }

    #[test]
    fn extract_layout_kinds_single_word() {
        let interner = Interner::new();
        let a = build_layout_attr(&interner, &["std430"]);
        let kinds = extract_layout_kinds(&a, &interner);
        assert_eq!(kinds, vec![LayoutKind::Std430]);
    }

    #[test]
    fn extract_layout_kinds_multi_word_combo() {
        let interner = Interner::new();
        let a = build_layout_attr(&interner, &["std430", "soa"]);
        let kinds = extract_layout_kinds(&a, &interner);
        assert_eq!(kinds, vec![LayoutKind::Std430, LayoutKind::SoA]);
    }

    #[test]
    fn extract_layout_kinds_skips_unknown_silently() {
        let interner = Interner::new();
        let a = build_layout_attr(&interner, &["std430", "wibble", "soa"]);
        let kinds = extract_layout_kinds(&a, &interner);
        assert_eq!(kinds, vec![LayoutKind::Std430, LayoutKind::SoA]);
    }

    #[test]
    fn extract_layout_kinds_named_arg_form() {
        let interner = Interner::new();
        let layout_sym = interner.intern("layout");
        let kind_key = interner.intern("kind");
        let attr = HirAttr {
            span: sp(0, 32),
            kind: HirAttrKind::Outer,
            path: vec![layout_sym],
            args: vec![HirAttrArg::Named {
                name: kind_key,
                value: path_expr(&interner, "std140"),
            }],
        };
        let kinds = extract_layout_kinds(&attr, &interner);
        assert_eq!(kinds, vec![LayoutKind::Std140]);
    }

    #[test]
    fn extract_layout_kinds_with_unknown_reports_unknowns() {
        let interner = Interner::new();
        let a = build_layout_attr(&interner, &["std430", "wibble", "soa", "foo"]);
        let (kinds, unknown) = extract_layout_kinds_with_unknown(&a, &interner);
        assert_eq!(kinds, vec![LayoutKind::Std430, LayoutKind::SoA]);
        assert_eq!(unknown, vec!["wibble".to_string(), "foo".to_string()]);
    }

    #[test]
    fn extract_layout_kinds_empty_args_yields_empty() {
        let interner = Interner::new();
        let a = build_layout_attr(&interner, &[]);
        let kinds = extract_layout_kinds(&a, &interner);
        assert!(kinds.is_empty());
    }
}
