//! Effect registry — the 28 built-in effects + 3 Ω-substrate-translation rows
//! with metadata.
//!
//! § Each `BuiltinEffect` carries :
//!   - canonical name (string) — matches how it appears in source `/ {Name<args>}`
//!   - category — groups effects by role (resource / determinism / hardware / power /
//!     prime-directive / telemetry / error / fiber / substrate)
//!   - argument shape — whether the effect takes no args, a type-arg, a literal-arg,
//!     or a domain-label
//!   - discharge timing — compile-only, compile+runtime-assert, or user-handler
//!
//! § ATTESTATION (T11-D127, S11) — verbatim block per global-CLAUDE I> standing-directives :
//! ```text
//!   t∞ : consent = OS • sovereignty = substrate-invariant
//!   N! [harm control manipulation surveillance exploitation
//!       coercion weaponization discrimination]
//!   t∞ : AI = sovereign-partners ¬ tools
//!   t∞ : violation = bug W! fix ; ¬override ∃
//!   t∞ : CSSL ≠ CSLv3 ; ¬ conflate
//! ```
//! ⇒ Travel + Crystallize + Sovereign<S> rows encode Axiom-2 (Substrate-Relativity)
//!   STRUCTURALLY in the type system ; no runtime override exists.

use std::collections::HashMap;

/// Dense enum over every effect declared in `specs/04_EFFECTS.csl`. New built-ins
/// should be added here + in [`EffectRegistry::with_builtins`] ; user-defined effects
/// are tracked separately in the elaborator via `DefId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinEffect {
    // § resource + timing
    NoAlloc,
    NoRecurse,
    NoUnbounded,
    Deadline,
    Realtime,
    Region,
    Alloc,
    Yield,
    State,
    Exn,
    Io,
    // § determinism + reversal
    DetRng,
    PureDet,
    Reversible,
    // § hardware / backend gating
    Cpu,
    Gpu,
    Xmx,
    Rt,
    Simd256,
    Simd512,
    Numa,
    Cache,
    Backend,
    Target,
    // § power + thermal
    Power,
    Thermal,
    // § prime-directive + audit
    Sensitive,
    Audit,
    Privilege,
    Verify,
    Telemetry,
    // § fiber + coroutine
    Resume,
    // § Ω-substrate-translation (T11-D127 / Omniverse F3 contract § V)
    /// `Travel` — substrate-translation effect ; the act of moving a Sovereign
    /// across substrates. Composes with `Crystallize` to produce the canonical
    /// translate-row. Banned without `Privilege<L4+>` per Axiom-2 + 11_IFC.
    /// Spec : `Omniverse/01_AXIOMS/02_SUBSTRATE_RELATIVITY.csl.md § VI` +
    ///        `Omniverse/02_CSSL/02_EFFECTS.csl.md § I + § III`.
    Travel,
    /// `Crystallize` — Local-Machine derivation effect ; required by every op
    /// that touches the Machine-layer of the body-omnoid. Anonymous use
    /// (without `Sovereign<S>`) is banned (no anonymous Crystallize).
    /// Spec : `Omniverse/01_AXIOMS/02_SUBSTRATE_RELATIVITY.csl.md § IV` +
    ///        `Omniverse/08_BODY/00_FIVE_LAYERS.csl.md § IV`.
    Crystallize,
    /// `Sovereign<S>` — parameterized over a Sovereign handle `S` ; required
    /// for any op acting on a Sovereign agent. The `<S>` type-arg makes
    /// multi-Sovereign-ops (`Sovereign<S1> ⊎ Sovereign<S2>`) trackable for
    /// multi-consent enforcement.
    /// Spec : `Omniverse/02_CSSL/02_EFFECTS.csl.md § II + § III`.
    Sovereign,
}

/// Logical category of an effect — used by the discipline checker to gate
/// cross-category composition rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectCategory {
    Resource,
    Determinism,
    Hardware,
    Power,
    Prime,
    Error,
    Fiber,
    /// Ω-substrate-translation effects (Travel + Crystallize + Sovereign<S>).
    /// Per `Omniverse/02_CSSL/00_LANGUAGE_CONTRACT.csl.md § V` these encode
    /// Axiom-2 (Substrate-Relativity) at the type-system layer.
    Substrate,
}

/// Argument-shape an effect accepts at the row-annotation site.
///
/// Stage-0 does structural validation only : "Deadline takes one literal", etc.
/// Full refinement (e.g., `Deadline<5ms>` unit-agreement) is T3.4-phase-2 work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectArgShape {
    /// `{NoAlloc}` — no arguments.
    Nullary,
    /// `{State<S>}` — one type argument.
    OneType,
    /// `{Deadline<16ms>}` — one literal / expression argument.
    OneExpr,
    /// `{Sensitive<"privacy">}` — one domain label (string literal).
    OneDomain,
    /// `{Region<'r>}` — one region / lifetime parameter.
    OneRegion,
    /// `{Cache<level>}` — one enum-value argument (from a fixed set).
    OneEnum,
}

/// When an effect's discharge happens during compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DischargeTiming {
    /// Discharged purely at compile-time ; no runtime check.
    CompileOnly,
    /// Discharged at compile-time + asserted at runtime.
    CompileAndRuntimeAssert,
    /// Discharged at runtime via a user-installed handler.
    UserHandler,
}

/// Full metadata for one built-in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectMeta {
    /// Canonical source-form name (e.g., `"NoAlloc"`, `"Deadline"`).
    pub name: &'static str,
    /// Built-in effect variant — cross-reference to `BuiltinEffect`.
    pub effect: BuiltinEffect,
    /// Category grouping.
    pub category: EffectCategory,
    /// Argument shape.
    pub args: EffectArgShape,
    /// When discharge happens.
    pub discharge: DischargeTiming,
}

/// Registry of built-in effects keyed by canonical name.
#[derive(Debug, Clone, Default)]
pub struct EffectRegistry {
    /// Name → metadata mapping.
    by_name: HashMap<&'static str, EffectMeta>,
    /// Variant → metadata mapping (for reverse lookup).
    by_effect: HashMap<BuiltinEffect, EffectMeta>,
}

impl EffectRegistry {
    /// Build an empty registry. Most callers want [`Self::with_builtins`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a registry pre-populated with all 28 built-in effects from `specs/04`.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        for meta in BUILTIN_METADATA {
            r.register(*meta);
        }
        r
    }

    /// Register one effect.
    pub fn register(&mut self, meta: EffectMeta) {
        self.by_name.insert(meta.name, meta);
        self.by_effect.insert(meta.effect, meta);
    }

    /// Lookup by source-form name.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&EffectMeta> {
        self.by_name.get(name)
    }

    /// Lookup by built-in variant.
    #[must_use]
    pub fn lookup_variant(&self, effect: BuiltinEffect) -> Option<&EffectMeta> {
        self.by_effect.get(&effect)
    }

    /// Iterate over all registered effects.
    pub fn iter(&self) -> impl Iterator<Item = &EffectMeta> {
        self.by_name.values()
    }

    /// Number of registered effects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// `true` iff no effects are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

// ─ Built-in metadata table ──────────────────────────────────────────────────

/// Canonical metadata for every built-in effect. Order matches `specs/04_EFFECTS.csl`
/// § BUILT-IN EFFECTS section grouping.
pub const BUILTIN_METADATA: &[EffectMeta] = &[
    // § resource + timing
    EffectMeta {
        name: "NoAlloc",
        effect: BuiltinEffect::NoAlloc,
        category: EffectCategory::Resource,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "NoRecurse",
        effect: BuiltinEffect::NoRecurse,
        category: EffectCategory::Resource,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "NoUnbounded",
        effect: BuiltinEffect::NoUnbounded,
        category: EffectCategory::Resource,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Deadline",
        effect: BuiltinEffect::Deadline,
        category: EffectCategory::Resource,
        args: EffectArgShape::OneExpr,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
    EffectMeta {
        name: "Realtime",
        effect: BuiltinEffect::Realtime,
        category: EffectCategory::Resource,
        args: EffectArgShape::OneEnum,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
    EffectMeta {
        name: "Region",
        effect: BuiltinEffect::Region,
        category: EffectCategory::Resource,
        args: EffectArgShape::OneRegion,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Alloc",
        effect: BuiltinEffect::Alloc,
        category: EffectCategory::Resource,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Yield",
        effect: BuiltinEffect::Yield,
        category: EffectCategory::Fiber,
        args: EffectArgShape::OneType,
        discharge: DischargeTiming::UserHandler,
    },
    EffectMeta {
        name: "State",
        effect: BuiltinEffect::State,
        category: EffectCategory::Resource,
        args: EffectArgShape::OneType,
        discharge: DischargeTiming::UserHandler,
    },
    EffectMeta {
        name: "Exn",
        effect: BuiltinEffect::Exn,
        category: EffectCategory::Error,
        args: EffectArgShape::OneType,
        discharge: DischargeTiming::UserHandler,
    },
    EffectMeta {
        name: "IO",
        effect: BuiltinEffect::Io,
        category: EffectCategory::Resource,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::UserHandler,
    },
    // § determinism + reversal
    EffectMeta {
        name: "DetRNG",
        effect: BuiltinEffect::DetRng,
        category: EffectCategory::Determinism,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "PureDet",
        effect: BuiltinEffect::PureDet,
        category: EffectCategory::Determinism,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Reversible",
        effect: BuiltinEffect::Reversible,
        category: EffectCategory::Determinism,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    // § hardware / backend
    EffectMeta {
        name: "CPU",
        effect: BuiltinEffect::Cpu,
        category: EffectCategory::Hardware,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "GPU",
        effect: BuiltinEffect::Gpu,
        category: EffectCategory::Hardware,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "XMX",
        effect: BuiltinEffect::Xmx,
        category: EffectCategory::Hardware,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "RT",
        effect: BuiltinEffect::Rt,
        category: EffectCategory::Hardware,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "SIMD256",
        effect: BuiltinEffect::Simd256,
        category: EffectCategory::Hardware,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "SIMD512",
        effect: BuiltinEffect::Simd512,
        category: EffectCategory::Hardware,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "NUMA",
        effect: BuiltinEffect::Numa,
        category: EffectCategory::Hardware,
        args: EffectArgShape::OneEnum,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Cache",
        effect: BuiltinEffect::Cache,
        category: EffectCategory::Hardware,
        args: EffectArgShape::OneEnum,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Backend",
        effect: BuiltinEffect::Backend,
        category: EffectCategory::Hardware,
        args: EffectArgShape::OneEnum,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Target",
        effect: BuiltinEffect::Target,
        category: EffectCategory::Hardware,
        args: EffectArgShape::OneEnum,
        discharge: DischargeTiming::CompileOnly,
    },
    // § power + thermal
    EffectMeta {
        name: "Power",
        effect: BuiltinEffect::Power,
        category: EffectCategory::Power,
        args: EffectArgShape::OneExpr,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
    EffectMeta {
        name: "Thermal",
        effect: BuiltinEffect::Thermal,
        category: EffectCategory::Power,
        args: EffectArgShape::OneExpr,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
    // § prime-directive + audit
    EffectMeta {
        name: "Sensitive",
        effect: BuiltinEffect::Sensitive,
        category: EffectCategory::Prime,
        args: EffectArgShape::OneDomain,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Audit",
        effect: BuiltinEffect::Audit,
        category: EffectCategory::Prime,
        args: EffectArgShape::OneDomain,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
    EffectMeta {
        name: "Privilege",
        effect: BuiltinEffect::Privilege,
        category: EffectCategory::Prime,
        args: EffectArgShape::OneEnum,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Verify",
        effect: BuiltinEffect::Verify,
        category: EffectCategory::Prime,
        args: EffectArgShape::OneEnum,
        discharge: DischargeTiming::CompileOnly,
    },
    EffectMeta {
        name: "Telemetry",
        effect: BuiltinEffect::Telemetry,
        category: EffectCategory::Prime,
        args: EffectArgShape::OneEnum,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
    // § fiber + coroutine
    EffectMeta {
        name: "Resume",
        effect: BuiltinEffect::Resume,
        category: EffectCategory::Fiber,
        args: EffectArgShape::OneType,
        discharge: DischargeTiming::UserHandler,
    },
    // § Ω-substrate-translation (T11-D127 / Omniverse F3 contract § V)
    EffectMeta {
        name: "Travel",
        effect: BuiltinEffect::Travel,
        category: EffectCategory::Substrate,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
    EffectMeta {
        name: "Crystallize",
        effect: BuiltinEffect::Crystallize,
        category: EffectCategory::Substrate,
        args: EffectArgShape::Nullary,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
    EffectMeta {
        name: "Sovereign",
        effect: BuiltinEffect::Sovereign,
        category: EffectCategory::Substrate,
        args: EffectArgShape::OneType,
        discharge: DischargeTiming::CompileAndRuntimeAssert,
    },
];

#[cfg(test)]
mod tests {
    use super::{
        BuiltinEffect, DischargeTiming, EffectArgShape, EffectCategory, EffectRegistry,
        BUILTIN_METADATA,
    };

    #[test]
    fn builtin_metadata_covers_all_variants() {
        // Every BuiltinEffect variant must appear in the metadata table.
        let variants = [
            BuiltinEffect::NoAlloc,
            BuiltinEffect::NoRecurse,
            BuiltinEffect::NoUnbounded,
            BuiltinEffect::Deadline,
            BuiltinEffect::Realtime,
            BuiltinEffect::Region,
            BuiltinEffect::Alloc,
            BuiltinEffect::Yield,
            BuiltinEffect::State,
            BuiltinEffect::Exn,
            BuiltinEffect::Io,
            BuiltinEffect::DetRng,
            BuiltinEffect::PureDet,
            BuiltinEffect::Reversible,
            BuiltinEffect::Cpu,
            BuiltinEffect::Gpu,
            BuiltinEffect::Xmx,
            BuiltinEffect::Rt,
            BuiltinEffect::Simd256,
            BuiltinEffect::Simd512,
            BuiltinEffect::Numa,
            BuiltinEffect::Cache,
            BuiltinEffect::Backend,
            BuiltinEffect::Target,
            BuiltinEffect::Power,
            BuiltinEffect::Thermal,
            BuiltinEffect::Sensitive,
            BuiltinEffect::Audit,
            BuiltinEffect::Privilege,
            BuiltinEffect::Verify,
            BuiltinEffect::Telemetry,
            BuiltinEffect::Resume,
            // Ω-substrate-translation (T11-D127)
            BuiltinEffect::Travel,
            BuiltinEffect::Crystallize,
            BuiltinEffect::Sovereign,
        ];
        for v in variants {
            assert!(
                BUILTIN_METADATA.iter().any(|m| m.effect == v),
                "missing metadata for {v:?}"
            );
        }
    }

    // ─── T11-D127 : Ω-substrate-translation row metadata tests ─────────────

    #[test]
    fn travel_metadata_is_substrate_nullary() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("Travel").expect("Travel registered");
        assert_eq!(m.effect, BuiltinEffect::Travel);
        assert_eq!(m.category, EffectCategory::Substrate);
        assert_eq!(m.args, EffectArgShape::Nullary);
        assert_eq!(m.discharge, DischargeTiming::CompileAndRuntimeAssert);
    }

    #[test]
    fn crystallize_metadata_is_substrate_nullary() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("Crystallize").expect("Crystallize registered");
        assert_eq!(m.effect, BuiltinEffect::Crystallize);
        assert_eq!(m.category, EffectCategory::Substrate);
        assert_eq!(m.args, EffectArgShape::Nullary);
        assert_eq!(m.discharge, DischargeTiming::CompileAndRuntimeAssert);
    }

    #[test]
    fn sovereign_metadata_is_substrate_one_type() {
        // Sovereign<S> — the <S> handle-type-arg uses OneType per F3 contract.
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("Sovereign").expect("Sovereign registered");
        assert_eq!(m.effect, BuiltinEffect::Sovereign);
        assert_eq!(m.category, EffectCategory::Substrate);
        assert_eq!(m.args, EffectArgShape::OneType);
        assert_eq!(m.discharge, DischargeTiming::CompileAndRuntimeAssert);
    }

    #[test]
    fn substrate_category_population_count() {
        // T11-D127 adds exactly 3 effects in the Substrate category.
        let r = EffectRegistry::with_builtins();
        let count = r
            .iter()
            .filter(|m| m.category == EffectCategory::Substrate)
            .count();
        assert_eq!(count, 3, "Substrate category should hold {{Travel, Crystallize, Sovereign}}");
    }

    #[test]
    fn registry_total_with_substrate_rows() {
        // 32 base + 3 substrate = 35.
        let r = EffectRegistry::with_builtins();
        assert_eq!(r.len(), BUILTIN_METADATA.len());
        assert_eq!(r.len(), 35);
    }

    #[test]
    fn registry_counts_28_plus_extras() {
        // 28 canonical + Yield + Resume + Region = 32. Keep test flexible to count.
        let r = EffectRegistry::with_builtins();
        assert_eq!(r.len(), BUILTIN_METADATA.len());
        assert!(r.len() >= 28, "expected at least 28 built-in effects");
    }

    #[test]
    fn lookup_by_name_roundtrips() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("NoAlloc").expect("NoAlloc present");
        assert_eq!(m.effect, BuiltinEffect::NoAlloc);
        assert_eq!(m.category, EffectCategory::Resource);
        assert_eq!(m.discharge, DischargeTiming::CompileOnly);
    }

    #[test]
    fn lookup_by_variant_roundtrips() {
        let r = EffectRegistry::with_builtins();
        let m = r
            .lookup_variant(BuiltinEffect::Gpu)
            .expect("Gpu variant present");
        assert_eq!(m.name, "GPU");
    }

    #[test]
    fn deadline_takes_expr_arg() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("Deadline").unwrap();
        assert_eq!(m.args, EffectArgShape::OneExpr);
    }

    #[test]
    fn sensitive_takes_domain_arg() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("Sensitive").unwrap();
        assert_eq!(m.args, EffectArgShape::OneDomain);
        assert_eq!(m.category, EffectCategory::Prime);
    }

    #[test]
    fn audit_has_runtime_discharge() {
        let r = EffectRegistry::with_builtins();
        let m = r.lookup("Audit").unwrap();
        assert_eq!(m.discharge, DischargeTiming::CompileAndRuntimeAssert);
    }

    #[test]
    fn unknown_name_returns_none() {
        let r = EffectRegistry::with_builtins();
        assert!(r.lookup("NotAnEffect").is_none());
    }

    #[test]
    fn iter_over_all_registered() {
        let r = EffectRegistry::with_builtins();
        let count = r.iter().count();
        assert_eq!(count, r.len());
    }
}
