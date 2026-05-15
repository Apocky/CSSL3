//! FFI-boundary struct ABI layout check.
//!
//! § item-09 — extends the σ-enforce gate to extern-C signatures by checking
//! every struct layout that crosses an FFI boundary for natural padding +
//! alignment compatibility before native codegen sees it.

use std::collections::BTreeSet;

use crate::func::{MirFunc, MirModule, MirStructLayout};
use crate::layout_check::LayoutCode;
use crate::pipeline::{MirPass, PassDiagnostic, PassResult};
use crate::value::MirType;

/// Stable pass-name for direct pipeline use. The canonical σ-enforce pass also
/// calls this checker so item-09 remains an extension of the σ gate.
pub const FFI_BOUNDARY_ABI_CHECK_PASS_NAME: &str = "ffi-boundary-abi-check";

/// Summary from [`check_ffi_boundary_layouts`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FfiBoundaryAbiReport {
    /// Number of distinct extern-C struct slots checked.
    pub checked_boundary_count: usize,
    /// Diagnostics emitted while checking boundary layouts.
    pub diagnostics: Vec<PassDiagnostic>,
}

impl FfiBoundaryAbiReport {
    /// `true` iff any diagnostic was emitted.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// Standalone MIR-pass wrapper for callers that want only item-09 ABI checks.
#[derive(Debug, Clone, Copy, Default)]
pub struct FfiBoundaryAbiCheckPass;

impl MirPass for FfiBoundaryAbiCheckPass {
    fn name(&self) -> &'static str {
        FFI_BOUNDARY_ABI_CHECK_PASS_NAME
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        let report = check_ffi_boundary_layouts(module);
        PassResult {
            name: self.name().to_string(),
            changed: false,
            diagnostics: report.diagnostics,
        }
    }
}

/// Check every struct-typed slot in extern-C MIR function signatures.
#[must_use]
pub fn check_ffi_boundary_layouts(module: &MirModule) -> FfiBoundaryAbiReport {
    let mut report = FfiBoundaryAbiReport::default();
    let mut seen = BTreeSet::new();

    for func in &module.funcs {
        if !is_ffi_boundary_func(func) {
            continue;
        }

        for (slot, ty) in func
            .params
            .iter()
            .enumerate()
            .map(|(idx, ty)| (format!("param[{idx}]"), ty))
            .chain(
                func.results
                    .iter()
                    .enumerate()
                    .map(|(idx, ty)| (format!("result[{idx}]"), ty)),
            )
        {
            let mut names = Vec::new();
            collect_struct_names(ty, module, &mut names);
            for struct_name in names {
                if !seen.insert((func.name.clone(), slot.clone(), struct_name.clone())) {
                    continue;
                }
                report.checked_boundary_count = report.checked_boundary_count.saturating_add(1);
                if let Some(layout) = module.find_struct_layout(&struct_name) {
                    check_layout(func, &slot, layout, &mut report);
                }
            }
        }
    }

    report
}

/// `true` iff the MIR fn is an extern/ABI boundary.
#[must_use]
pub fn is_ffi_boundary_func(func: &MirFunc) -> bool {
    func.attributes.iter().any(|(k, v)| {
        (k == "abi" && !v.is_empty()) || (k == "linkage" && v == "import")
    })
}

fn collect_struct_names(ty: &MirType, module: &MirModule, out: &mut Vec<String>) {
    match ty {
        MirType::Opaque(name) => {
            let candidate = canonical_struct_name(name);
            if module.find_struct_layout(candidate).is_some() {
                out.push(candidate.to_string());
            }
        }
        MirType::Tuple(elems) => {
            for elem in elems {
                collect_struct_names(elem, module, out);
            }
        }
        MirType::Function { params, results } => {
            for elem in params.iter().chain(results.iter()) {
                collect_struct_names(elem, module, out);
            }
        }
        MirType::Memref { elem, .. } => collect_struct_names(elem, module, out),
        MirType::Int(_)
        | MirType::Float(_)
        | MirType::Bool
        | MirType::None
        | MirType::Handle
        | MirType::Vec(_, _)
        | MirType::Ptr => {}
    }
}

fn canonical_struct_name(name: &str) -> &str {
    name.strip_prefix("!cssl.struct.").unwrap_or(name)
}

fn check_layout(
    func: &MirFunc,
    slot: &str,
    layout: &MirStructLayout,
    report: &mut FfiBoundaryAbiReport,
) {
    if layout.align_bytes == 0 || !layout.align_bytes.is_power_of_two() {
        push_lay0002(
            func,
            slot,
            layout,
            format!(
                "declared alignment {}B is not a non-zero power-of-two",
                layout.align_bytes
            ),
            report,
        );
        return;
    }

    let (natural_size, natural_align) = MirStructLayout::compute_size_align(&layout.fields);
    if natural_align > layout.align_bytes {
        push_lay0002(
            func,
            slot,
            layout,
            format!(
                "declared alignment {}B under-aligns natural field alignment {natural_align}B",
                layout.align_bytes
            ),
            report,
        );
        return;
    }

    let declared_align = u32::from(layout.align_bytes);
    if declared_align > 1 && layout.size_bytes % declared_align != 0 {
        push_lay0002(
            func,
            slot,
            layout,
            format!(
                "declared size {}B is not padded to declared alignment {}B",
                layout.size_bytes, layout.align_bytes
            ),
            report,
        );
        return;
    }

    if natural_size > layout.size_bytes {
        push_lay0002(
            func,
            slot,
            layout,
            format!(
                "declared size {}B is smaller than natural extern-C padded size {natural_size}B",
                layout.size_bytes
            ),
            report,
        );
        return;
    }

    if layout.abi_class().is_none() {
        report.diagnostics.push(PassDiagnostic::error(
            LayoutCode::SizeMismatch.as_str(),
            format!(
                "FFI-boundary : extern-C fn `{}` slot `{slot}` uses struct `{}` with zero-byte ABI layout",
                func.name, layout.name
            ),
        ));
    }
}

fn push_lay0002(
    func: &MirFunc,
    slot: &str,
    layout: &MirStructLayout,
    reason: String,
    report: &mut FfiBoundaryAbiReport,
) {
    report.diagnostics.push(PassDiagnostic::error(
        LayoutCode::AlignmentViolation.as_str(),
        format!(
            "FFI-boundary : extern-C fn `{}` slot `{slot}` uses struct `{}` with alignment violation — {reason}",
            func.name, layout.name
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::{
        check_ffi_boundary_layouts, FfiBoundaryAbiCheckPass, FFI_BOUNDARY_ABI_CHECK_PASS_NAME,
    };
    use crate::func::{MirFunc, MirModule, MirStructLayout};
    use crate::layout_check::LayoutCode;
    use crate::pipeline::MirPass;
    use crate::sigma_enforce::EnforcesSigmaAtCellTouches;
    use crate::value::{IntWidth, MirType};

    fn extern_c_fn(name: &str, params: Vec<MirType>, results: Vec<MirType>) -> MirFunc {
        let mut func = MirFunc::new(name, params, results);
        func.attributes.push(("linkage".to_string(), "import".to_string()));
        func.attributes.push(("abi".to_string(), "C".to_string()));
        func
    }

    #[test]
    fn pass_name_stable() {
        assert_eq!(FfiBoundaryAbiCheckPass.name(), FFI_BOUNDARY_ABI_CHECK_PASS_NAME);
    }

    #[test]
    fn a09_1_sigma_extends_to_extern_c_struct_signature() {
        let mut module = MirModule::new();
        module.add_struct_layout(MirStructLayout::new(
            "RunHandle",
            vec![MirType::Int(IntWidth::I64)],
            8,
            8,
        ));
        let opaque = MirType::Opaque("!cssl.struct.RunHandle".to_string());
        module.push_func(extern_c_fn(
            "host_roundtrip",
            vec![opaque.clone()],
            vec![opaque],
        ));

        let report = check_ffi_boundary_layouts(&module);
        assert_eq!(report.checked_boundary_count, 2);
        assert!(!report.has_errors(), "{:?}", report.diagnostics);

        let sigma = EnforcesSigmaAtCellTouches.run(&mut module);
        assert!(!sigma.has_errors(), "{:?}", sigma.diagnostics);
    }

    #[test]
    fn a09_2_alignment_violation_on_ffi_boundary_emits_lay0002() {
        let mut module = MirModule::new();
        module.add_struct_layout(MirStructLayout::new(
            "BadAlign",
            vec![MirType::Int(IntWidth::I64)],
            8,
            4,
        ));
        module.push_func(extern_c_fn(
            "take_bad_align",
            vec![MirType::Opaque("!cssl.struct.BadAlign".to_string())],
            vec![],
        ));

        let report = check_ffi_boundary_layouts(&module);
        assert!(report.has_errors());
        assert_eq!(report.diagnostics[0].code, LayoutCode::AlignmentViolation.as_str());
        assert!(report.diagnostics[0].message.contains("FFI-boundary"));
        assert!(report.diagnostics[0].message.contains("extern-C"));
    }

    #[test]
    fn a09_3_misaligned_extern_c_struct_fails_lay0002() {
        let mut module = MirModule::new();
        module.add_struct_layout(MirStructLayout::new(
            "PackedBad",
            vec![MirType::Int(IntWidth::I8), MirType::Int(IntWidth::I64)],
            9,
            1,
        ));
        module.push_func(extern_c_fn(
            "take_packed_bad",
            vec![MirType::Opaque("PackedBad".to_string())],
            vec![],
        ));

        let report = check_ffi_boundary_layouts(&module);
        assert!(report.has_errors());
        let diag = &report.diagnostics[0];
        assert_eq!(diag.code, LayoutCode::AlignmentViolation.as_str());
        assert!(diag.message.contains("FFI-boundary"));
        assert!(diag.message.contains("PackedBad"));
    }

    #[test]
    fn non_ffi_struct_signature_is_not_checked() {
        let mut module = MirModule::new();
        module.add_struct_layout(MirStructLayout::new(
            "BadAlign",
            vec![MirType::Int(IntWidth::I64)],
            8,
            4,
        ));
        module.push_func(MirFunc::new(
            "ordinary",
            vec![MirType::Opaque("BadAlign".to_string())],
            vec![],
        ));

        let report = check_ffi_boundary_layouts(&module);
        assert_eq!(report.checked_boundary_count, 0);
        assert!(!report.has_errors());
    }

    // ─────────────────────────────────────────────────────────────────────
    // § SHOTGUN-09 — robust test surface for item-09 (σ-extend FFI ABI).
    //
    // I> intent : pin (¬ just happy-path) ∀ invariants @ once :
    //   ✓ type-walker recursion .(Tuple Memref Function nested)
    //   ✓ boundary-detection edge-cases .(abi='' linkage=export missing)
    //   ✓ dedup +(aggregation +(order-stability))
    //   ✓ σ-enforce subsumption ← central-claim 'item-09
    //   ✓ idempotence +(canonical-pipeline integration)
    //   ✓ adversarial layouts +(property-grid)
    //   ✓ regression-canaries 'planned-work-ahead
    // ─────────────────────────────────────────────────────────────────────

    use crate::pipeline::{PassPipeline, PassSeverity};
    use crate::sigma_enforce::SIGMA_ENFORCE_PASS_NAME;

    /// Build a fn with arbitrary attribute set ; no body. Lets each test pin
    /// exactly the attribute combo under inspection.
    fn fn_with_attrs(
        name: &str,
        attrs: &[(&str, &str)],
        params: Vec<MirType>,
        results: Vec<MirType>,
    ) -> MirFunc {
        let mut f = MirFunc::new(name, params, results);
        for (k, v) in attrs {
            f.attributes.push(((*k).to_string(), (*v).to_string()));
        }
        f
    }

    fn opaque_struct(name: &str) -> MirType {
        MirType::Opaque(format!("!cssl.struct.{name}"))
    }

    fn good_layout(name: &str) -> MirStructLayout {
        // i64 ; natural (8,8) ; declared matches.
        MirStructLayout::new(name, vec![MirType::Int(IntWidth::I64)], 8, 8)
    }

    fn bad_align_layout(name: &str) -> MirStructLayout {
        // i64 ; declared align under-specified.
        MirStructLayout::new(name, vec![MirType::Int(IntWidth::I64)], 8, 4)
    }

    fn count_lay0002(diags: &[crate::pipeline::PassDiagnostic]) -> usize {
        diags
            .iter()
            .filter(|d| d.code == LayoutCode::AlignmentViolation.as_str())
            .count()
    }

    fn count_lay0001(diags: &[crate::pipeline::PassDiagnostic]) -> usize {
        diags
            .iter()
            .filter(|d| d.code == LayoutCode::SizeMismatch.as_str())
            .count()
    }

    // ── § STRUCTURAL — type-walker recursion ─────────────────────────────

    #[test]
    fn shotgun_tuple_param_carrying_struct_is_checked() {
        // Tuple<RunHandleBad, i32> as an FFI param ; struct-walker must
        // descend into the tuple element + flag the bad layout.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RunHandleBad"));
        m.push_func(extern_c_fn(
            "via_tuple",
            vec![MirType::Tuple(vec![
                opaque_struct("RunHandleBad"),
                MirType::Int(IntWidth::I32),
            ])],
            vec![],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 1);
    }

    #[test]
    fn shotgun_memref_elem_struct_is_checked() {
        // memref<?xRunHandleBad> as result ; walker must descend into elem.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RunHandleBad"));
        m.push_func(extern_c_fn(
            "via_memref",
            vec![],
            vec![MirType::Memref {
                shape: vec![None],
                elem: Box::new(opaque_struct("RunHandleBad")),
            }],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 1);
    }

    #[test]
    fn shotgun_function_typed_param_with_struct_in_inner_signature_is_checked() {
        // fn(RunHandleBad) -> i32 as a param : struct must be flushed via
        // the inner Function arms.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RunHandleBad"));
        m.push_func(extern_c_fn(
            "via_fn_ptr",
            vec![MirType::Function {
                params: vec![opaque_struct("RunHandleBad")],
                results: vec![MirType::Int(IntWidth::I32)],
            }],
            vec![],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 1);
    }

    #[test]
    fn shotgun_nested_tuple_of_memref_of_struct_is_checked() {
        // Tuple<memref<?xRunHandleBad>, i64> ← exercises 2-level recursion.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RunHandleBad"));
        m.push_func(extern_c_fn(
            "deep",
            vec![MirType::Tuple(vec![
                MirType::Memref {
                    shape: vec![None],
                    elem: Box::new(opaque_struct("RunHandleBad")),
                },
                MirType::Int(IntWidth::I64),
            ])],
            vec![],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 1);
    }

    // ── § BOUNDARY-DETECTION edge-cases ──────────────────────────────────

    #[test]
    fn shotgun_abi_attribute_alone_qualifies_as_boundary() {
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(fn_with_attrs(
            "abi_only",
            &[("abi", "C")],
            vec![opaque_struct("RH")],
            vec![],
        ));
        assert!(check_ffi_boundary_layouts(&m).has_errors());
    }

    #[test]
    fn shotgun_linkage_import_alone_qualifies_as_boundary() {
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(fn_with_attrs(
            "import_only",
            &[("linkage", "import")],
            vec![opaque_struct("RH")],
            vec![],
        ));
        assert!(check_ffi_boundary_layouts(&m).has_errors());
    }

    #[test]
    fn shotgun_empty_abi_string_is_not_a_boundary() {
        // R! defensive : empty abi="" must NOT be treated as FFI ; otherwise
        // any fn that touches an attribute pair (k="abi", v="") gets falsely
        // promoted. Pin the (¬ empty-string) condition.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(fn_with_attrs(
            "abi_blank",
            &[("abi", "")],
            vec![opaque_struct("RH")],
            vec![],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(r.checked_boundary_count, 0);
        assert!(!r.has_errors());
    }

    #[test]
    fn shotgun_no_attrs_is_not_a_boundary() {
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(MirFunc::new("plain", vec![opaque_struct("RH")], vec![]));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(r.checked_boundary_count, 0);
    }

    #[test]
    fn shotgun_linkage_export_currently_not_a_boundary_canary() {
        // ◐ regression-canary 'planned-work-ahead :
        //   item-09 currently treats only linkage=import + abi=* as FFI.
        //   When export-direction extension lands (post-09), THIS TEST WILL
        //   FAIL — that's the signal to update the canary AND the spec
        //   crossref. ¬ delete ; update intentionally.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(fn_with_attrs(
            "export_only",
            &[("linkage", "export")],
            vec![opaque_struct("RH")],
            vec![],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(
            r.checked_boundary_count, 0,
            "linkage=export was promoted to FFI ; if intentional, update spec + canary"
        );
    }

    #[test]
    fn shotgun_both_attrs_do_not_double_count_diagnostics() {
        // linkage=import + abi=C on the same fn ; one bad slot ⇒ exactly one
        // diagnostic .(¬ duplicated 'two-detection-paths)
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(extern_c_fn("both", vec![opaque_struct("RH")], vec![]));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 1);
    }

    // ── § DEDUP +(AGGREGATION +(ORDER-STABILITY)) ────────────────────────

    #[test]
    fn shotgun_same_struct_in_multiple_slots_yields_one_diagnostic_per_slot() {
        // Same broken struct in (param0, param1, result0) ⇒ 3 distinct
        // (slot)-keyed diagnostics ; pins per-slot dedup semantics.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(extern_c_fn(
            "three_slots",
            vec![opaque_struct("RH"), opaque_struct("RH")],
            vec![opaque_struct("RH")],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 3, "{:?}", r.diagnostics);
    }

    #[test]
    fn shotgun_same_struct_repeated_inside_one_slot_dedupes_to_one() {
        // Tuple<RH, RH> in ONE slot ⇒ dedup-key (fn, slot, struct) collapses
        // the two occurrences into a single diagnostic.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(extern_c_fn(
            "tuple_pair",
            vec![MirType::Tuple(vec![
                opaque_struct("RH"),
                opaque_struct("RH"),
            ])],
            vec![],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 1);
    }

    #[test]
    fn shotgun_aggregation_across_multiple_functions() {
        // 3 broken structs across 2 fns ⇒ 3 diagnostics. Pins that the pass
        // ¬ stop @ first-violation-per-module.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("A"));
        m.add_struct_layout(bad_align_layout("B"));
        m.add_struct_layout(bad_align_layout("C"));
        m.push_func(extern_c_fn(
            "f1",
            vec![opaque_struct("A"), opaque_struct("B")],
            vec![],
        ));
        m.push_func(extern_c_fn("f2", vec![opaque_struct("C")], vec![]));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 3);
    }

    #[test]
    fn shotgun_diagnostic_set_is_invariant_under_function_order() {
        // Same module, two fn-orderings ⇒ same diagnostic-code multiset.
        // Pins that the report's content is stable to caller-side reorders.
        fn build(order: &[&str]) -> Vec<String> {
            let mut m = MirModule::new();
            m.add_struct_layout(bad_align_layout("X"));
            m.add_struct_layout(bad_align_layout("Y"));
            for n in order {
                let s = if *n == "fx" { "X" } else { "Y" };
                m.push_func(extern_c_fn(n, vec![opaque_struct(s)], vec![]));
            }
            let mut codes: Vec<String> = check_ffi_boundary_layouts(&m)
                .diagnostics
                .into_iter()
                .map(|d| d.code)
                .collect();
            codes.sort();
            codes
        }
        assert_eq!(build(&["fx", "fy"]), build(&["fy", "fx"]));
    }

    // ── § σ-ENFORCE SUBSUMPTION ←  central-claim 'item-09 ─────────────────

    #[test]
    fn shotgun_sigma_enforce_subsumes_standalone_pass_on_ffi_only_module() {
        // Pure-FFI broken module ; standalone + σ-extended must produce the
        // SAME diagnostic-code multiset .(σ ⊑ standalone ; ¬ drop ¬ duplicate)
        let mut m1 = MirModule::new();
        m1.add_struct_layout(bad_align_layout("RH"));
        m1.push_func(extern_c_fn("g", vec![opaque_struct("RH")], vec![]));
        let mut m2 = m1.clone();

        let standalone: Vec<String> = check_ffi_boundary_layouts(&m1)
            .diagnostics
            .into_iter()
            .map(|d| d.code)
            .collect();
        let via_sigma: Vec<String> = EnforcesSigmaAtCellTouches
            .run(&mut m2)
            .diagnostics
            .into_iter()
            .filter(|d| d.code.starts_with("LAY"))
            .map(|d| d.code)
            .collect();

        assert!(!standalone.is_empty());
        assert_eq!(standalone, via_sigma);
    }

    #[test]
    fn shotgun_sigma_enforce_is_idempotent_on_ffi_violations() {
        // Run σ-enforce twice ; diagnostics MUST be byte-identical.
        // Pins (changed: false) +(¬ side-effects-on-module) for re-runs.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(extern_c_fn("g", vec![opaque_struct("RH")], vec![]));
        let r1 = EnforcesSigmaAtCellTouches.run(&mut m);
        let r2 = EnforcesSigmaAtCellTouches.run(&mut m);
        assert_eq!(r1.diagnostics, r2.diagnostics);
        assert!(!r1.changed && !r2.changed);
    }

    #[test]
    fn shotgun_check_ffi_boundary_layouts_is_pure_function() {
        // Same input ⇒ same output across 3 calls. Trivial but pins the
        // pure-fn contract for downstream callers (cgen, audit).
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(extern_c_fn("g", vec![opaque_struct("RH")], vec![]));
        let a = check_ffi_boundary_layouts(&m);
        let b = check_ffi_boundary_layouts(&m);
        let c = check_ffi_boundary_layouts(&m);
        assert_eq!(a.diagnostics, b.diagnostics);
        assert_eq!(b.diagnostics, c.diagnostics);
        assert_eq!(a.checked_boundary_count, c.checked_boundary_count);
    }

    // ── § ADVERSARIAL LAYOUTS ────────────────────────────────────────────

    #[test]
    fn shotgun_zero_byte_empty_struct_emits_lay0001_size_mismatch() {
        // size=0 + fields=[] ⇒ abi_class()==None ⇒ SizeMismatch (LAY0001),
        // ¬ AlignmentViolation. Pins routing of the (empty-struct)-case.
        let mut m = MirModule::new();
        m.add_struct_layout(MirStructLayout::new("Z", vec![], 0, 1));
        m.push_func(extern_c_fn("g", vec![opaque_struct("Z")], vec![]));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0001(&r.diagnostics), 1);
        assert_eq!(count_lay0002(&r.diagnostics), 0);
    }

    #[test]
    fn shotgun_non_power_of_two_alignment_emits_lay0002() {
        // align=3 ¬ PoT ⇒ LAY0002. Covers the (¬ power-of-2)-leg explicitly.
        let mut m = MirModule::new();
        m.add_struct_layout(MirStructLayout::new(
            "Bad",
            vec![MirType::Int(IntWidth::I8)],
            3,
            3,
        ));
        m.push_func(extern_c_fn("g", vec![opaque_struct("Bad")], vec![]));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 1);
    }

    #[test]
    fn shotgun_over_aligned_struct_with_padded_size_is_accepted() {
        // align=16 (PoT, > natural=8) ; size=16 (multiple-of-align, padded)
        // ; declared >= natural ⇒ accepted. Pins (over-alignment is OK).
        let mut m = MirModule::new();
        m.add_struct_layout(MirStructLayout::new(
            "OverAligned",
            vec![MirType::Int(IntWidth::I64), MirType::Int(IntWidth::I64)],
            16,
            16,
        ));
        m.push_func(extern_c_fn(
            "g",
            vec![opaque_struct("OverAligned")],
            vec![],
        ));
        let r = check_ffi_boundary_layouts(&m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    #[test]
    fn shotgun_size_not_multiple_of_alignment_emits_lay0002() {
        // size=7, align=4 ⇒ 7 % 4 != 0 ⇒ LAY0002.
        let mut m = MirModule::new();
        m.add_struct_layout(MirStructLayout::new(
            "Off",
            vec![MirType::Int(IntWidth::I32), MirType::Int(IntWidth::I16)],
            7,
            4,
        ));
        m.push_func(extern_c_fn("g", vec![opaque_struct("Off")], vec![]));
        let r = check_ffi_boundary_layouts(&m);
        assert_eq!(count_lay0002(&r.diagnostics), 1);
    }

    #[test]
    fn shotgun_missing_struct_layout_in_module_is_silent_skip_canary() {
        // ◐ canary : if a fn references a struct-name with no layout entry,
        //   current pass silently skips. Pin THAT behavior so a future
        //   tightening (E1004 'unknown layout') flips this test as a signal.
        let mut m = MirModule::new();
        m.push_func(extern_c_fn("g", vec![opaque_struct("Phantom")], vec![]));
        let r = check_ffi_boundary_layouts(&m);
        assert!(
            !r.has_errors(),
            "missing-layout currently silent ; if changed, update canary"
        );
    }

    #[test]
    fn shotgun_raw_and_canonical_struct_names_both_resolve() {
        // !cssl.struct.Foo + raw "Foo" should both resolve via
        // find_struct_layout. Pin that the name-strip logic handles both.
        for opaque_name in &["RH", "!cssl.struct.RH"] {
            let mut m = MirModule::new();
            m.add_struct_layout(bad_align_layout("RH"));
            m.push_func(extern_c_fn(
                "g",
                vec![MirType::Opaque((*opaque_name).to_string())],
                vec![],
            ));
            let r = check_ffi_boundary_layouts(&m);
            assert_eq!(
                count_lay0002(&r.diagnostics),
                1,
                "form `{opaque_name}` did not resolve"
            );
        }
    }

    // ── § PROPERTY GRID — small deterministic sweep ──────────────────────

    #[test]
    fn shotgun_property_grid_natural_align_exceeds_declared_always_fires() {
        // ∀ (declared_align ∈ {1,2,4}) ⊗ field=[i64] (natural 8) ⇒ LAY0002.
        // Catches future regressions in the under-alignment branch via a
        // tiny enumerated sweep (¬ proptest dep).
        for declared_align in [1u8, 2u8, 4u8] {
            let mut m = MirModule::new();
            m.add_struct_layout(MirStructLayout::new(
                "U",
                vec![MirType::Int(IntWidth::I64)],
                8,
                declared_align,
            ));
            m.push_func(extern_c_fn("g", vec![opaque_struct("U")], vec![]));
            let r = check_ffi_boundary_layouts(&m);
            assert_eq!(
                count_lay0002(&r.diagnostics),
                1,
                "declared_align={declared_align} did not fire LAY0002"
            );
        }
    }

    #[test]
    fn shotgun_property_grid_well_formed_layouts_never_fire() {
        // ∀ (size,align) ∈ {(1,1),(2,2),(4,4),(8,8),(16,8)} with matching
        // natural-fit fields ⇒ ¬ diagnostics. Companion to the negative grid.
        let cases: &[(u32, u8, Vec<MirType>)] = &[
            (1, 1, vec![MirType::Int(IntWidth::I8)]),
            (2, 2, vec![MirType::Int(IntWidth::I16)]),
            (4, 4, vec![MirType::Int(IntWidth::I32)]),
            (8, 8, vec![MirType::Int(IntWidth::I64)]),
            (
                16,
                8,
                vec![MirType::Int(IntWidth::I64), MirType::Int(IntWidth::I64)],
            ),
        ];
        for (size, align, fields) in cases {
            let mut m = MirModule::new();
            m.add_struct_layout(MirStructLayout::new(
                "G",
                fields.clone(),
                *size,
                *align,
            ));
            m.push_func(extern_c_fn("g", vec![opaque_struct("G")], vec![]));
            let r = check_ffi_boundary_layouts(&m);
            assert!(
                !r.has_errors(),
                "(size={size},align={align}) spurious : {:?}",
                r.diagnostics
            );
        }
    }

    // ── § CANONICAL-PIPELINE INTEGRATION ─────────────────────────────────

    #[test]
    fn shotgun_canonical_pipeline_surfaces_lay0002_only_through_sigma_stage() {
        // Item-09's central wiring claim : LAY0002 reaches the canonical
        // pipeline EXCLUSIVELY via the σ-enforce stage .(¬ a separate stage
        // ¬ a duplicate). Build a module with a single bad FFI struct, run
        // canonical, assert :
        //   1. ∃! result ⊗ name == SIGMA_ENFORCE_PASS_NAME +(LAY0002)
        //   2. ¬∃ other-result ⊗ LAY0002
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(extern_c_fn("g", vec![opaque_struct("RH")], vec![]));

        let results = PassPipeline::canonical().run_all(&mut m);

        let sigma_count: usize = results
            .iter()
            .filter(|r| r.name == SIGMA_ENFORCE_PASS_NAME)
            .map(|r| count_lay0002(&r.diagnostics))
            .sum();
        let elsewhere_count: usize = results
            .iter()
            .filter(|r| r.name != SIGMA_ENFORCE_PASS_NAME)
            .map(|r| count_lay0002(&r.diagnostics))
            .sum();

        assert_eq!(sigma_count, 1, "σ-enforce should surface 1 LAY0002");
        assert_eq!(
            elsewhere_count, 0,
            "no other canonical stage may emit LAY0002 (subsumption)"
        );
    }

    #[test]
    fn shotgun_canonical_pipeline_clean_module_has_no_lay_diagnostics() {
        // Companion : a well-formed FFI struct produces zero LAY-coded
        // diagnostics anywhere in the canonical pipeline. Pins that
        // item-09 ¬ false-positive in the integration path.
        let mut m = MirModule::new();
        m.add_struct_layout(good_layout("RH"));
        m.push_func(extern_c_fn("g", vec![opaque_struct("RH")], vec![]));

        let results = PassPipeline::canonical().run_all(&mut m);
        let any_lay = results.iter().any(|r| {
            r.diagnostics
                .iter()
                .any(|d| d.code == LayoutCode::AlignmentViolation.as_str()
                    || d.code == LayoutCode::SizeMismatch.as_str())
        });
        assert!(!any_lay, "spurious LAY-* in canonical clean run");
    }

    #[test]
    fn shotgun_severity_is_error_not_warning_or_info() {
        // Pin the severity of every emitted LAY0002 ; a downgrade to Warning
        // would silently allow broken FFI to ship. ¬ regress.
        let mut m = MirModule::new();
        m.add_struct_layout(bad_align_layout("RH"));
        m.push_func(extern_c_fn("g", vec![opaque_struct("RH")], vec![]));
        let r = check_ffi_boundary_layouts(&m);
        assert!(r
            .diagnostics
            .iter()
            .all(|d| d.severity == PassSeverity::Error));
    }
}
