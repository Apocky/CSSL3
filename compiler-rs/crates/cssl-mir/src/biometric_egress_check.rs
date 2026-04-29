//! `biometric-egress-check` MIR pass : refuses every `cssl.telemetry.record`
//! op whose operand carries a biometric / surveillance / coercion sensitive-
//! domain attribute.
//!
//! § SPEC :
//!   - `specs/22_TELEMETRY.csl` § OBSERVABILITY-FIRST-CLASS
//!   - `specs/11_IFC.csl` § PRIME-DIRECTIVE ENCODING
//!   - `PRIME_DIRECTIVE.md §1` (anti-surveillance)
//!   - `Omniverse/07_AESTHETIC/05_VR_RENDERING.csl` § II.A
//!     ("eye-track : raw-gaze on-device ⊗ R! NEVER-egress")
//!
//! § DESIGN
//!   The pass walks every op in every region of every fn in the module. For
//!   each op whose `op == CsslOp::TelemetryRecord`, it inspects two
//!   attribute keys :
//!     - `("sensitive_domain", "<domain-name>")` — set by the source-level
//!       `Sensitive<dom>` effect at HIR-lowering time.
//!     - `("ifc_principal", "<principal-name>")` — set by the IFC pass when
//!       the operand's confidentiality-set contains a non-User principal.
//!   If the recorded domain or principal matches the biometric-family
//!   (`biometric` / `gaze` / `face` / `body`) or the absolute-egress-banned
//!   set (`surveillance` / `coercion`), the pass emits a `BIO0001` (or
//!   `BIO0002`/`BIO0003`) **error** diagnostic — halting the pipeline.
//!
//!   The op is left in place ; the caller decides whether to scrub or fail.
//!   Stage-0 fail-the-build : `BiometricEgressCheck` is wired AFTER
//!   `IfcLoweringPass` in the canonical pipeline so IFC-attributes are
//!   present.
//!
//! § DIAGNOSTIC CODES
//!   - `BIO0001` : `cssl.telemetry.record` operand carries a biometric-
//!     family sensitive-domain. Non-overridable.
//!   - `BIO0002` : `cssl.telemetry.record` operand carries surveillance.
//!     Non-overridable.
//!   - `BIO0003` : `cssl.telemetry.record` operand carries coercion.
//!     Non-overridable.
//!   - `BIO0004` : `cssl.telemetry.record` operand carries weapon without
//!     `Privilege<Kernel>` attribute.
//!
//! § TEST-DOUBLE NOTE (W3β-04 mock)
//!   This pass treats the biometric-family as a first-class concept at the
//!   IFC layer (`cssl-ifc::SensitiveDomain::Biometric` etc.) — the W3β-04
//!   slice will additionally promote these to `cssl-effects::SensitiveDomain`
//!   variants so the effect-row layer can also reason about composition.
//!   This pass is **independent** of W3β-04 because it reads the
//!   `sensitive_domain` attribute directly off the MIR op (which is set by
//!   the lowering layer, not the effect-layer). When W3β-04 lands, the
//!   lowering layer will start setting these attributes for biometric-
//!   tagged values automatically.

use crate::func::MirModule;
use crate::op::CsslOp;
use crate::pipeline::{MirPass, PassDiagnostic, PassResult};

/// Diagnostic-code prefix for this pass.
pub const BIO0001_BIOMETRIC: &str = "BIO0001";
/// Diagnostic-code for surveillance refusal.
pub const BIO0002_SURVEILLANCE: &str = "BIO0002";
/// Diagnostic-code for coercion refusal.
pub const BIO0003_COERCION: &str = "BIO0003";
/// Diagnostic-code for weapon-without-Kernel refusal.
pub const BIO0004_WEAPON_NEEDS_KERNEL: &str = "BIO0004";

/// Attribute-key holding the operand's `Sensitive<dom>` domain-tag (if any).
pub const SENSITIVE_DOMAIN_KEY: &str = "sensitive_domain";
/// Attribute-key holding the operand's IFC confidentiality-principal (if non-User).
pub const IFC_PRINCIPAL_KEY: &str = "ifc_principal";
/// Attribute-key holding the call-site's privilege-level (if any).
pub const PRIVILEGE_KEY: &str = "privilege";

/// Biometric-family domain names recognized by this pass.
pub const BIOMETRIC_DOMAINS: &[&str] = &["biometric", "gaze", "face", "body"];

/// Biometric-family principal names recognized by this pass (mirrors
/// `cssl_ifc::Principal::is_biometric_family`).
pub const BIOMETRIC_PRINCIPALS: &[&str] = &[
    "BiometricSubject",
    "GazeSubject",
    "FaceSubject",
    "BodySubject",
];

/// `biometric-egress-check` pass.
///
/// See module-doc for full design + diagnostic-code reference.
#[derive(Debug, Clone, Copy, Default)]
pub struct BiometricEgressCheck;

impl MirPass for BiometricEgressCheck {
    fn name(&self) -> &'static str {
        "biometric-egress-check"
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        let mut diagnostics: Vec<PassDiagnostic> = Vec::new();
        for func in &module.funcs {
            walk_region(&func.body, &mut diagnostics);
        }
        PassResult {
            name: self.name().to_string(),
            changed: false,
            diagnostics,
        }
    }
}

fn walk_region(region: &crate::block::MirRegion, diagnostics: &mut Vec<PassDiagnostic>) {
    for block in &region.blocks {
        for op in &block.ops {
            check_op(op, diagnostics);
            for nested in &op.regions {
                walk_region(nested, diagnostics);
            }
        }
    }
}

fn check_op(op: &crate::block::MirOp, diagnostics: &mut Vec<PassDiagnostic>) {
    if op.op != CsslOp::TelemetryRecord {
        return;
    }
    let domain = op
        .attributes
        .iter()
        .find(|(k, _)| k == SENSITIVE_DOMAIN_KEY)
        .map(|(_, v)| v.as_str());
    let principal = op
        .attributes
        .iter()
        .find(|(k, _)| k == IFC_PRINCIPAL_KEY)
        .map(|(_, v)| v.as_str());
    let privilege = op
        .attributes
        .iter()
        .find(|(k, _)| k == PRIVILEGE_KEY)
        .map(|(_, v)| v.as_str());

    if let Some(d) = domain {
        if BIOMETRIC_DOMAINS.contains(&d) {
            diagnostics.push(PassDiagnostic::error(
                BIO0001_BIOMETRIC,
                format!(
                    "cssl.telemetry.record operand carries biometric-family domain `{d}` — \
                     PRIME-DIRECTIVE §1 anti-surveillance ; \
                     compile-time refusal, no Privilege<*> override exists",
                ),
            ));
            return;
        }
        if d == "surveillance" {
            diagnostics.push(PassDiagnostic::error(
                BIO0002_SURVEILLANCE,
                "cssl.telemetry.record operand carries `surveillance` domain — \
                 PRIME-DIRECTIVE §1 anti-surveillance ; \
                 compile-time refusal, no Privilege<*> override exists"
                    .to_string(),
            ));
            return;
        }
        if d == "coercion" {
            diagnostics.push(PassDiagnostic::error(
                BIO0003_COERCION,
                "cssl.telemetry.record operand carries `coercion` domain — \
                 PRIME-DIRECTIVE §1 absolute prohibition ; \
                 compile-time refusal, no Privilege<*> override exists"
                    .to_string(),
            ));
            return;
        }
        if d == "weapon" && privilege != Some("Kernel") {
            diagnostics.push(PassDiagnostic::error(
                BIO0004_WEAPON_NEEDS_KERNEL,
                format!(
                    "cssl.telemetry.record operand carries `weapon` domain without \
                     Privilege<Kernel> (privilege={:?}) — specs/11 PRIME-DIRECTIVE ENCODING",
                    privilege.unwrap_or("<none>"),
                ),
            ));
            return;
        }
    }
    if let Some(p) = principal {
        if BIOMETRIC_PRINCIPALS.contains(&p) {
            diagnostics.push(PassDiagnostic::error(
                BIO0001_BIOMETRIC,
                format!(
                    "cssl.telemetry.record operand's IFC confidentiality includes \
                     biometric-family principal `{p}` — \
                     PRIME-DIRECTIVE §1 anti-surveillance ; \
                     compile-time refusal, no Privilege<*> override exists",
                ),
            ));
        } else if p == "SurveillanceTarget" {
            diagnostics.push(PassDiagnostic::error(
                BIO0002_SURVEILLANCE,
                "cssl.telemetry.record operand's IFC confidentiality includes \
                 `SurveillanceTarget` — PRIME-DIRECTIVE §1 anti-surveillance"
                    .to_string(),
            ));
        } else if p == "CoercionTarget" {
            diagnostics.push(PassDiagnostic::error(
                BIO0003_COERCION,
                "cssl.telemetry.record operand's IFC confidentiality includes \
                 `CoercionTarget` — PRIME-DIRECTIVE §1 absolute prohibition"
                    .to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BiometricEgressCheck, BIO0001_BIOMETRIC, BIO0002_SURVEILLANCE, BIO0003_COERCION,
        BIO0004_WEAPON_NEEDS_KERNEL, IFC_PRINCIPAL_KEY, PRIVILEGE_KEY, SENSITIVE_DOMAIN_KEY,
    };
    use crate::block::{MirBlock, MirOp, MirRegion};
    use crate::func::{MirFunc, MirModule};
    use crate::op::CsslOp;
    use crate::pipeline::{MirPass, PassSeverity};

    fn module_with_record_op(attrs: Vec<(&str, &str)>) -> MirModule {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        let mut block = MirBlock::new("entry");
        let mut op = MirOp::new(CsslOp::TelemetryRecord);
        for (k, v) in attrs {
            op = op.with_attribute(k, v);
        }
        block.push(op);
        f.body = MirRegion {
            blocks: vec![block],
        };
        m.push_func(f);
        m
    }

    #[test]
    fn pass_name_canonical() {
        assert_eq!(BiometricEgressCheck.name(), "biometric-egress-check");
    }

    #[test]
    fn refuses_biometric_domain() {
        let mut m = module_with_record_op(vec![(SENSITIVE_DOMAIN_KEY, "biometric")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
        assert!(r.diagnostics[0].message.contains("biometric"));
        assert!(r.diagnostics[0].message.contains("PRIME-DIRECTIVE"));
    }

    #[test]
    fn refuses_gaze_domain() {
        let mut m = module_with_record_op(vec![(SENSITIVE_DOMAIN_KEY, "gaze")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
        assert!(r.diagnostics[0].message.contains("gaze"));
    }

    #[test]
    fn refuses_face_domain() {
        let mut m = module_with_record_op(vec![(SENSITIVE_DOMAIN_KEY, "face")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
        assert!(r.diagnostics[0].message.contains("face"));
    }

    #[test]
    fn refuses_body_domain() {
        let mut m = module_with_record_op(vec![(SENSITIVE_DOMAIN_KEY, "body")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
        assert!(r.diagnostics[0].message.contains("body"));
    }

    #[test]
    fn refuses_surveillance_domain() {
        let mut m = module_with_record_op(vec![(SENSITIVE_DOMAIN_KEY, "surveillance")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0002_SURVEILLANCE);
        assert!(r.diagnostics[0].message.contains("surveillance"));
    }

    #[test]
    fn refuses_coercion_domain() {
        let mut m = module_with_record_op(vec![(SENSITIVE_DOMAIN_KEY, "coercion")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0003_COERCION);
    }

    #[test]
    fn weapon_without_kernel_refused() {
        let mut m = module_with_record_op(vec![(SENSITIVE_DOMAIN_KEY, "weapon")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0004_WEAPON_NEEDS_KERNEL);
    }

    #[test]
    fn weapon_with_kernel_passes() {
        let mut m = module_with_record_op(vec![
            (SENSITIVE_DOMAIN_KEY, "weapon"),
            (PRIVILEGE_KEY, "Kernel"),
        ]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(!r.has_errors());
    }

    #[test]
    fn weapon_with_user_priv_still_refused() {
        let mut m = module_with_record_op(vec![
            (SENSITIVE_DOMAIN_KEY, "weapon"),
            (PRIVILEGE_KEY, "User"),
        ]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0004_WEAPON_NEEDS_KERNEL);
    }

    #[test]
    fn privacy_domain_passes() {
        let mut m = module_with_record_op(vec![(SENSITIVE_DOMAIN_KEY, "privacy")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(!r.has_errors());
    }

    #[test]
    fn no_sensitive_attribute_passes() {
        let mut m = module_with_record_op(vec![]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(!r.has_errors());
    }

    // === IFC-PRINCIPAL TRIGGERED ===

    #[test]
    fn refuses_gaze_subject_principal() {
        let mut m = module_with_record_op(vec![(IFC_PRINCIPAL_KEY, "GazeSubject")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
        assert!(r.diagnostics[0].message.contains("GazeSubject"));
    }

    #[test]
    fn refuses_face_subject_principal() {
        let mut m = module_with_record_op(vec![(IFC_PRINCIPAL_KEY, "FaceSubject")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
    }

    #[test]
    fn refuses_body_subject_principal() {
        let mut m = module_with_record_op(vec![(IFC_PRINCIPAL_KEY, "BodySubject")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
    }

    #[test]
    fn refuses_biometric_subject_principal() {
        let mut m = module_with_record_op(vec![(IFC_PRINCIPAL_KEY, "BiometricSubject")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
    }

    #[test]
    fn refuses_surveillance_target_principal() {
        let mut m = module_with_record_op(vec![(IFC_PRINCIPAL_KEY, "SurveillanceTarget")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0002_SURVEILLANCE);
    }

    #[test]
    fn refuses_coercion_target_principal() {
        let mut m = module_with_record_op(vec![(IFC_PRINCIPAL_KEY, "CoercionTarget")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, BIO0003_COERCION);
    }

    #[test]
    fn user_principal_passes() {
        let mut m = module_with_record_op(vec![(IFC_PRINCIPAL_KEY, "User")]);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(!r.has_errors());
    }

    // === NESTED REGIONS ===

    #[test]
    fn detects_biometric_op_inside_nested_region() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        // Outer block has a host op carrying a nested region.
        let mut outer_block = MirBlock::new("entry");
        let mut nested_block = MirBlock::new("then");
        nested_block
            .push(MirOp::new(CsslOp::TelemetryRecord).with_attribute(SENSITIVE_DOMAIN_KEY, "gaze"));
        let nested_region = MirRegion {
            blocks: vec![nested_block],
        };
        let host_op = MirOp::std("scf.if").with_region(nested_region);
        outer_block.push(host_op);
        f.body = MirRegion {
            blocks: vec![outer_block],
        };
        m.push_func(f);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors(), "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
    }

    // === NON-RECORD OPS IGNORED ===

    #[test]
    fn ignores_non_telemetry_record_ops() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        let mut block = MirBlock::new("entry");
        // Even a non-record op with a biometric attribute (forged) is ignored —
        // the pass only inspects TelemetryRecord ops.
        block.push(MirOp::new(CsslOp::GpuBarrier).with_attribute(SENSITIVE_DOMAIN_KEY, "gaze"));
        block.push(MirOp::new(CsslOp::TelemetryProbe));
        f.body = MirRegion {
            blocks: vec![block],
        };
        m.push_func(f);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // === MULTIPLE VIOLATIONS ===

    #[test]
    fn reports_each_violation_separately() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        let mut block = MirBlock::new("entry");
        block
            .push(MirOp::new(CsslOp::TelemetryRecord).with_attribute(SENSITIVE_DOMAIN_KEY, "gaze"));
        block.push(
            MirOp::new(CsslOp::TelemetryRecord)
                .with_attribute(SENSITIVE_DOMAIN_KEY, "surveillance"),
        );
        block
            .push(MirOp::new(CsslOp::TelemetryRecord).with_attribute(SENSITIVE_DOMAIN_KEY, "body"));
        f.body = MirRegion {
            blocks: vec![block],
        };
        m.push_func(f);
        let r = BiometricEgressCheck.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics.len(), 3);
        assert_eq!(r.diagnostics[0].code, BIO0001_BIOMETRIC);
        assert_eq!(r.diagnostics[1].code, BIO0002_SURVEILLANCE);
        assert_eq!(r.diagnostics[2].code, BIO0001_BIOMETRIC);
        for d in &r.diagnostics {
            assert_eq!(d.severity, PassSeverity::Error);
        }
    }

    // === EMPTY MODULE ===

    #[test]
    fn empty_module_passes() {
        let mut m = MirModule::new();
        let r = BiometricEgressCheck.run(&mut m);
        assert!(!r.has_errors());
        assert_eq!(r.diagnostics.len(), 0);
    }
}
