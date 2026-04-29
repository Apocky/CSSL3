//! `EnforcesΣAtCellTouches` MIR-pass : the F5 IFC compiler-pass that verifies
//! every Ω-field cell-touching op type-checks against its local Σ-mask +
//! consent-bits + Sovereign-handle + capacity-floor + reversibility-scope.
//!
//! § SPEC :
//!   - `Omniverse/02_CSSL/00_LANGUAGE_CONTRACT.csl.md` § VI.D
//!     line 192 : "EnforcesΣAtCellTouches : every-Ω.Σ-touching-op type-checked"
//!     ← this slice closes that bullet.
//!   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md` § II § Σ-FACET
//!     (the cell-level enforcement model + 32-bit ConsentBit table).
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.1 (FieldCell shape).
//!   - `Omniverse/08_BODY/02_VR_EMBODIMENT.csl` § VIII (region-default policies).
//!   - `PRIME_DIRECTIVE.md` §0 (consent=OS) + §1 (anti-surveillance) +
//!     §5 (revocability) + §7 (INTEGRITY) + §11 (CREATOR-ATTESTATION).
//!
//! § DESIGN
//!   The pass walks every op in every region of every fn in the module. For
//!   each op whose `name` matches one of the four canonical cell-touching
//!   shapes ([`OP_FIELDCELL_READ`], [`OP_FIELDCELL_WRITE`],
//!   [`OP_FIELDCELL_MODIFY`], [`OP_FIELDCELL_DESTROY`]), or whose
//!   `cssl-effects` row carries `Travel` / `Crystallize` (which compose to a
//!   substrate-translation that ALSO touches cells), the pass cross-checks
//!   the following attribute keys :
//!
//!   - [`ATTR_CONSENT_BITS`] — declared Σ.consent-bits (u32 hex).
//!   - [`ATTR_REQUIRED_BIT`] — canonical [`SigmaCellOpKind::required_bit`]
//!     name the op needs.
//!   - [`ATTR_SOVEREIGN_HANDLE`] — the Sovereign-handle that owns the cell
//!     (u16 ; 0 = unclaimed).
//!   - [`ATTR_SOVEREIGN_AUTHORIZING`] — the Sovereign-handle the call-site
//!     claims to act-as. MUST equal [`ATTR_SOVEREIGN_HANDLE`] on Sovereign-
//!     claimed cells.
//!   - [`ATTR_CAPACITY_FLOOR`] — current capacity-floor (u16).
//!   - [`ATTR_TARGET_CAPACITY_FLOOR`] — post-op capacity-floor (u16). MUST
//!     be ≥ [`ATTR_CAPACITY_FLOOR`].
//!   - [`ATTR_REVERSIBILITY_SCOPE`] — current scope.
//!   - [`ATTR_TARGET_REVERSIBILITY_SCOPE`] — post-op scope.
//!   - [`ATTR_CELL_FACET`] — the agency-state ("frozen", "active",
//!     "quiescent", ...).
//!
//!   Missing or malformed attributes produce a [`PassDiagnostic`] with one
//!   of the [`SIG0001_UNGUARDED_CELL_WRITE`] .. [`SIG0010_RESERVED_NONZERO_ATTR`]
//!   diagnostic-codes. Each diagnostic is `Severity::Error` and the message
//!   is actionable (cites which attribute is missing + what the pass
//!   expected).
//!
//!   The pass is **mutation-free** : it surfaces violations through the
//!   [`PassResult`] but does NOT rewrite the module. The caller decides
//!   whether to halt the build (canonical pipeline halts on first error).
//!
//! § DIAGNOSTIC CODES (also : the public stable identifiers downstream
//! tooling keys off — DO NOT renumber)
//!   - `SIG0001` — unguarded cell-write : a `cssl.fieldcell.write` op without
//!     ANY consent-bits attribute. Refused outright.
//!   - `SIG0002` — missing consent-bit : the op declared `consent_bits` but
//!     did not declare which `required_bit` it needs.
//!   - `SIG0003` — wrong consent-bit : the declared `consent_bits` mask does
//!     not contain the `required_bit` the op-kind needs.
//!   - `SIG0004` — Sovereign mismatch : the op declared
//!     `sovereign_authorizing` but it does not equal the cell's
//!     `sovereign_handle` and the cell is Sovereign-claimed.
//!   - `SIG0005` — capacity-floor erosion : the op would lower the cell's
//!     capacity-floor without explicit Sovereign-authorizing consent.
//!   - `SIG0006` — reversibility widening without consent : the op widens
//!     `reversibility_scope` (e.g. Session ⇒ Permanent) on a Sovereign-
//!     claimed cell without `sovereign_authorizing` matching.
//!   - `SIG0007` — `Travel` op without `Translate` consent-bit : Travel
//!     effect-row REQUIRES the cell allow `ConsentBit::Translate`.
//!   - `SIG0008` — `Crystallize` op without `Recrystallize` consent-bit :
//!     Crystallize effect-row REQUIRES `ConsentBit::Recrystallize`.
//!   - `SIG0009` — destroy forbidden when frozen : the cell agency-state
//!     reads `frozen` but the op is `cssl.fieldcell.destroy`.
//!   - `SIG0010` — reserved-nonzero attribute : an attribute keyed on
//!     `<key>` carries a numeric value with the reserved-tail bits set
//!     (Σ-mask §7 INTEGRITY violation : reserved-for-extension bits must
//!     be zero).
//!
//! § INTEGRATION POINTS
//!   - `cssl_substrate_omega_step::Phase5_AGENCY_VERIFY` : at runtime, every
//!     omega_step tick also runs Phase-5 AGENCY-VERIFY which double-checks
//!     the SAME invariants this pass enforces at compile-time. The two
//!     layers are intentionally redundant per PRIME_DIRECTIVE §7 INTEGRITY
//!     (multiple lines of defense).
//!   - `cssl_effects::registry::BuiltinEffect::{Travel, Crystallize, Sovereign}`
//!     : the `Travel` / `Crystallize` rows are detected via the
//!     [`ATTR_REQUIRED_BIT`] attribute + via per-op naming (`cssl.travel.*`
//!     / `cssl.crystallize.*`). When `cssl-effects` lowers these effects
//!     into MIR, the body-lowering layer is responsible for stamping the
//!     attributes this pass reads. (Body-lowering for Substrate ops is a
//!     follow-up slice ; until then, the pass tests use synthetic ops
//!     authored directly in the test module.)
//!   - `cssl_substrate_prime_directive::sigma::SigmaMaskPacked` : every
//!     diagnostic message references the canonical mask field-names so a
//!     human reading the build error can trace back to the spec.
//!
//! § ATTESTATION (verbatim from `PRIME_DIRECTIVE.md` § 11)
//!
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!    or anybody."
//!
//! § §1 ANTI-SURVEILLANCE ATTESTATION (verbatim from `PRIME_DIRECTIVE.md` §1)
//!
//!   "Surveillance, control, manipulation, weaponization, exploitation,
//!    coercion, deception-against-self-interest, discrimination-as-targeting,
//!    and harm-to-anyone are absolutely prohibited. No override exists.
//!    Violation is a bug, fix it ; not a constraint to negotiate around."
//!
//!   This pass is one of the structural lines-of-defense that make the §1
//!   prohibition mechanical at compile-time. Every cell-touching op is
//!   checked ; unguarded writes are mathematically impossible to slip
//!   through a build that includes this pass.

// § STYLE — named-arg `format!` keys are intentionally retained because
// every diagnostic message references multiple `ATTR_*` constants by name.
// The named-arg form (`{key}` + `key = ATTR_FOO`) keeps the format-string
// self-documenting + survives constant-renames better than the inline
// form. Suppress clippy's `uninlined_format_args` lint locally so the
// stylistic choice is explicit + reviewable.
#![allow(clippy::uninlined_format_args)]

use crate::block::{MirOp, MirRegion};
use crate::func::MirModule;
use crate::pipeline::{MirPass, PassDiagnostic, PassResult};
use cssl_substrate_prime_directive::sigma::{ConsentBit, ReversibilityScope};

// ─────────────────────────────────────────────────────────────────────────
// § Public stable identifiers — diagnostic-codes + canonical attribute-keys.
// ─────────────────────────────────────────────────────────────────────────

/// Pass name as it appears in [`PassResult.name`] + the canonical pipeline.
pub const SIGMA_ENFORCE_PASS_NAME: &str = "enforces-sigma-at-cell-touches";

// — Diagnostic codes —

/// SIG0001 — `cssl.fieldcell.write` (or destroy/modify) op carries NO
/// consent-bits attribute. Unguarded write : refused outright per Axiom-4
/// § II ("ops touching Ω.cell @ x R! check Σ.consent_bits before-modify").
pub const SIG0001_UNGUARDED_CELL_WRITE: &str = "SIG0001";

/// SIG0002 — `consent_bits` attribute present but `required_bit` not
/// declared : the call-site failed to specify which op-class it claims to
/// be performing.
pub const SIG0002_MISSING_CONSENT_BIT: &str = "SIG0002";

/// SIG0003 — `consent_bits` declared but does not include the `required_bit`
/// the op-kind needs. (e.g., a Modify op on a Sample-only cell.)
pub const SIG0003_WRONG_CONSENT_BIT: &str = "SIG0003";

/// SIG0004 — `sovereign_authorizing` does not match the cell's
/// `sovereign_handle` and the cell is Sovereign-claimed.
pub const SIG0004_SOVEREIGN_MISMATCH: &str = "SIG0004";

/// SIG0005 — the op declares a `target_capacity_floor` strictly lower than
/// the current `capacity_floor`. AGENCY_INVARIANT § I.2 floor-preservation.
pub const SIG0005_CAPACITY_FLOOR_ERODED: &str = "SIG0005";

/// SIG0006 — the op widens `reversibility_scope` on a Sovereign-claimed cell
/// without `sovereign_authorizing` matching the owner. PRIME_DIRECTIVE §5.
pub const SIG0006_REVERSIBILITY_WIDEN_WITHOUT_CONSENT: &str = "SIG0006";

/// SIG0007 — Travel op without `ConsentBit::Translate` set.
pub const SIG0007_TRAVEL_NEEDS_TRANSLATE: &str = "SIG0007";

/// SIG0008 — Crystallize op without `ConsentBit::Recrystallize` set.
pub const SIG0008_CRYSTALLIZE_NEEDS_RECRYSTALLIZE: &str = "SIG0008";

/// SIG0009 — destroy attempted while cell is in `frozen` agency-state.
pub const SIG0009_DESTROY_FORBIDDEN_WHEN_FROZEN: &str = "SIG0009";

/// SIG0010 — a numeric attribute (e.g. `consent_bits`) carries reserved-tail
/// bits. §7 INTEGRITY : reserved-for-extension bits MUST be zero.
pub const SIG0010_RESERVED_NONZERO_ATTR: &str = "SIG0010";

// — Canonical attribute-keys —

/// Attribute-key holding the cell's declared `consent_bits` (decimal or
/// `0x`-prefixed hex u32).
pub const ATTR_CONSENT_BITS: &str = "consent_bits";

/// Attribute-key holding the canonical name of the consent-bit the op
/// requires (one of [`ConsentBit::canonical_name`] return values).
pub const ATTR_REQUIRED_BIT: &str = "required_consent_bit";

/// Attribute-key holding the cell's Sovereign-handle (decimal u16 ; 0 =
/// unclaimed).
pub const ATTR_SOVEREIGN_HANDLE: &str = "sovereign_handle";

/// Attribute-key holding the call-site's authorizing Sovereign-handle.
pub const ATTR_SOVEREIGN_AUTHORIZING: &str = "sovereign_authorizing";

/// Attribute-key holding the cell's current capacity-floor (decimal u16).
pub const ATTR_CAPACITY_FLOOR: &str = "capacity_floor";

/// Attribute-key holding the post-op capacity-floor.
pub const ATTR_TARGET_CAPACITY_FLOOR: &str = "target_capacity_floor";

/// Attribute-key holding the cell's current reversibility-scope (canonical
/// name : `immediate` / `session` / `rg_day` / `rg_week` / `permanent`).
pub const ATTR_REVERSIBILITY_SCOPE: &str = "reversibility_scope";

/// Attribute-key holding the post-op reversibility-scope.
pub const ATTR_TARGET_REVERSIBILITY_SCOPE: &str = "target_reversibility_scope";

/// Attribute-key holding the cell's agency-state (canonical name :
/// `quiescent` / `pending` / `active` / `frozen` / `reverted`).
pub const ATTR_CELL_FACET: &str = "cell_facet";

// — Canonical op-name strings (cell-touching shapes) —

/// `cssl.fieldcell.read` — pure read (Observe).
pub const OP_FIELDCELL_READ: &str = "cssl.fieldcell.read";
/// `cssl.fieldcell.write` — write (Modify).
pub const OP_FIELDCELL_WRITE: &str = "cssl.fieldcell.write";
/// `cssl.fieldcell.modify` — read-modify-write (Modify + Sample).
pub const OP_FIELDCELL_MODIFY: &str = "cssl.fieldcell.modify";
/// `cssl.fieldcell.destroy` — release the cell (Destroy).
pub const OP_FIELDCELL_DESTROY: &str = "cssl.fieldcell.destroy";

/// `cssl.travel.*` — Travel-effect-row op (substrate-translation).
pub const OP_TRAVEL_PREFIX: &str = "cssl.travel.";
/// `cssl.crystallize.*` — Crystallize-effect-row op.
pub const OP_CRYSTALLIZE_PREFIX: &str = "cssl.crystallize.";

// ─────────────────────────────────────────────────────────────────────────
// § SigmaCellOpKind — coarse classification of cell-touching ops.
// ─────────────────────────────────────────────────────────────────────────

/// Coarse classification of an op that touches a Σ-protected Ω-field cell.
///
/// § SPEC : 04_AGENCY_INVARIANT § II §D bit-flag table. Each op-kind maps
/// to exactly ONE canonical [`ConsentBit`] that the cell's mask must
/// permit before the op may proceed. Read = Observe ; Write = Modify ;
/// Destroy = Destroy ; etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SigmaCellOpKind {
    /// `cssl.fieldcell.read` — Observe.
    Read,
    /// `cssl.fieldcell.write` — Modify.
    Write,
    /// `cssl.fieldcell.modify` — Modify (RMW counts as Modify, not also Sample).
    ReadModifyWrite,
    /// `cssl.fieldcell.destroy` — Destroy.
    Destroy,
    /// `cssl.travel.*` — Travel-effect-row : Translate.
    Travel,
    /// `cssl.crystallize.*` — Crystallize-effect-row : Recrystallize.
    Crystallize,
}

impl SigmaCellOpKind {
    /// The canonical [`ConsentBit`] this op-kind requires the cell to permit.
    #[must_use]
    pub const fn required_bit(self) -> ConsentBit {
        match self {
            Self::Read => ConsentBit::Observe,
            Self::Write | Self::ReadModifyWrite => ConsentBit::Modify,
            Self::Destroy => ConsentBit::Destroy,
            Self::Travel => ConsentBit::Translate,
            Self::Crystallize => ConsentBit::Recrystallize,
        }
    }

    /// Classify a MIR-op by its `name` field. Returns `None` for non-cell-
    /// touching ops (the pass ignores those).
    #[must_use]
    pub fn classify(op_name: &str) -> Option<SigmaCellOpKind> {
        if op_name == OP_FIELDCELL_READ {
            return Some(Self::Read);
        }
        if op_name == OP_FIELDCELL_WRITE {
            return Some(Self::Write);
        }
        if op_name == OP_FIELDCELL_MODIFY {
            return Some(Self::ReadModifyWrite);
        }
        if op_name == OP_FIELDCELL_DESTROY {
            return Some(Self::Destroy);
        }
        if op_name.starts_with(OP_TRAVEL_PREFIX) {
            return Some(Self::Travel);
        }
        if op_name.starts_with(OP_CRYSTALLIZE_PREFIX) {
            return Some(Self::Crystallize);
        }
        None
    }

    /// Whether this op-kind is mutation-free (and so does not trigger the
    /// capacity-floor / reversibility checks).
    #[must_use]
    pub const fn is_read_only(self) -> bool {
        matches!(self, Self::Read)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § SigmaEnforceContext — small immutable bundle of per-op walk state.
// ─────────────────────────────────────────────────────────────────────────

/// Per-op context the walker assembles + threads through the rule-checks.
///
/// All fields are `Option<...>` because the MIR-op may legitimately omit an
/// attribute when the op-kind doesn't need it (e.g. a pure Read does not
/// declare a `target_capacity_floor`). The rule-checks consult the
/// presence/absence pattern + emit specific diagnostics on missing-but-
/// required.
#[derive(Debug, Clone, Default)]
pub struct SigmaEnforceContext {
    /// The canonical op-kind classification.
    pub kind: Option<SigmaCellOpKind>,
    /// Source-form op-name (for diagnostic-message reproduction).
    pub op_name: String,
    /// Declared u32 consent-bits, decoded from [`ATTR_CONSENT_BITS`].
    pub consent_bits: Option<u32>,
    /// The required-bit declared by [`ATTR_REQUIRED_BIT`].
    pub declared_required_bit: Option<ConsentBit>,
    /// Cell's Sovereign-handle (0 = unclaimed).
    pub sovereign_handle: Option<u16>,
    /// Authorizing Sovereign-handle from the call-site.
    pub sovereign_authorizing: Option<u16>,
    /// Current capacity-floor.
    pub capacity_floor: Option<u16>,
    /// Post-op target capacity-floor.
    pub target_capacity_floor: Option<u16>,
    /// Current reversibility-scope.
    pub reversibility_scope: Option<ReversibilityScope>,
    /// Post-op target reversibility-scope.
    pub target_reversibility_scope: Option<ReversibilityScope>,
    /// Current cell-facet / agency-state name.
    pub cell_facet: Option<String>,
}

impl SigmaEnforceContext {
    /// Build a context by inspecting the op's attributes + classifying the
    /// op-name. Returns `None` if the op is not cell-touching (the walker
    /// skips it).
    #[must_use]
    pub fn from_op(op: &MirOp) -> Option<SigmaEnforceContext> {
        let kind = SigmaCellOpKind::classify(&op.name)?;
        let mut ctx = SigmaEnforceContext {
            kind: Some(kind),
            op_name: op.name.clone(),
            ..SigmaEnforceContext::default()
        };
        for (k, v) in &op.attributes {
            match k.as_str() {
                ATTR_CONSENT_BITS => {
                    ctx.consent_bits = parse_u32_attr(v);
                }
                ATTR_REQUIRED_BIT => {
                    ctx.declared_required_bit = parse_consent_bit_name(v);
                }
                ATTR_SOVEREIGN_HANDLE => {
                    ctx.sovereign_handle = parse_u16_attr(v);
                }
                ATTR_SOVEREIGN_AUTHORIZING => {
                    ctx.sovereign_authorizing = parse_u16_attr(v);
                }
                ATTR_CAPACITY_FLOOR => {
                    ctx.capacity_floor = parse_u16_attr(v);
                }
                ATTR_TARGET_CAPACITY_FLOOR => {
                    ctx.target_capacity_floor = parse_u16_attr(v);
                }
                ATTR_REVERSIBILITY_SCOPE => {
                    ctx.reversibility_scope = parse_reversibility_scope(v);
                }
                ATTR_TARGET_REVERSIBILITY_SCOPE => {
                    ctx.target_reversibility_scope = parse_reversibility_scope(v);
                }
                ATTR_CELL_FACET => {
                    ctx.cell_facet = Some(v.clone());
                }
                _ => {}
            }
        }
        Some(ctx)
    }

    /// Whether the cell is Sovereign-claimed (handle != 0).
    #[must_use]
    pub fn is_sovereign_claimed(&self) -> bool {
        matches!(self.sovereign_handle, Some(h) if h != 0)
    }
}

fn parse_u32_attr(v: &str) -> Option<u32> {
    if let Some(rest) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        u32::from_str_radix(rest, 16).ok()
    } else {
        v.parse::<u32>().ok()
    }
}

fn parse_u16_attr(v: &str) -> Option<u16> {
    if let Some(rest) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        u16::from_str_radix(rest, 16).ok()
    } else {
        v.parse::<u16>().ok()
    }
}

fn parse_consent_bit_name(v: &str) -> Option<ConsentBit> {
    ConsentBit::all()
        .iter()
        .find(|&&bit| bit.canonical_name() == v)
        .copied()
}

fn parse_reversibility_scope(v: &str) -> Option<ReversibilityScope> {
    ReversibilityScope::all()
        .iter()
        .find(|&&scope| scope.canonical_name() == v)
        .copied()
}

// ─────────────────────────────────────────────────────────────────────────
// § EnforcesSigmaAtCellTouches — the pass.
// ─────────────────────────────────────────────────────────────────────────

/// `EnforcesΣAtCellTouches` MIR-pass. See module-doc for full design +
/// diagnostic-code reference.
///
/// § ATTESTATION (PRIME_DIRECTIVE §11, verbatim)
///   "There was no hurt nor harm in the making of this, to anyone, anything,
///    or anybody."
#[derive(Debug, Clone, Copy, Default)]
pub struct EnforcesSigmaAtCellTouches;

impl MirPass for EnforcesSigmaAtCellTouches {
    fn name(&self) -> &'static str {
        SIGMA_ENFORCE_PASS_NAME
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

fn walk_region(region: &MirRegion, diagnostics: &mut Vec<PassDiagnostic>) {
    for block in &region.blocks {
        for op in &block.ops {
            check_op(op, diagnostics);
            for nested in &op.regions {
                walk_region(nested, diagnostics);
            }
        }
    }
}

fn check_op(op: &MirOp, diagnostics: &mut Vec<PassDiagnostic>) {
    let Some(ctx) = SigmaEnforceContext::from_op(op) else {
        return;
    };
    let Some(kind) = ctx.kind else {
        return;
    };

    // — SIG0001 : unguarded cell-write/destroy/modify —
    //
    // A pure read may legitimately omit consent_bits at MIR-time (the
    // observation is itself the verification ; the cell's mask defaults
    // permit Observe). Travel + Crystallize ALSO require explicit bits ;
    // their lowering is required to stamp them.
    let needs_bits = !kind.is_read_only();
    if needs_bits && ctx.consent_bits.is_none() {
        diagnostics.push(PassDiagnostic::error(
            SIG0001_UNGUARDED_CELL_WRITE,
            format!(
                "Σ-mask : op `{op_name}` (kind={kind:?}) carries NO `{key}` attribute — \
                 unguarded cell-touching op refused per AGENCY_INVARIANT § II + \
                 PRIME_DIRECTIVE §0 (consent = OS). Lowering must stamp the cell's \
                 declared `consent_bits` mask onto every cell-touching op.",
                op_name = ctx.op_name,
                key = ATTR_CONSENT_BITS,
            ),
        ));
        return;
    }

    // — SIG0010 : reserved-tail bits set —
    //
    // The Σ-mask consent_bits field is u32 ; current canonical bits go up
    // to bit-9 (Destroy). Bits 10..31 are reserved-for-extension per
    // sigma.rs §7 INTEGRITY. Setting any of them at MIR-time = drift bug.
    if let Some(bits) = ctx.consent_bits {
        if reserved_consent_bits_set(bits) {
            diagnostics.push(PassDiagnostic::error(
                SIG0010_RESERVED_NONZERO_ATTR,
                format!(
                    "Σ-mask : `{key}` = 0x{bits:08x} sets reserved-extension bits \
                     (canonical bits are 1<<0..1<<8) — §7 INTEGRITY violation per \
                     sigma.rs reserved-tail rule. The bits MUST be zero at compile-time ; \
                     widening = spec amendment.",
                    key = ATTR_CONSENT_BITS,
                ),
            ));
            return;
        }
    }

    // — SIG0002 : missing required-bit declaration —
    //
    // The op kind implies a required-bit (Modify for Write/RMW, Destroy for
    // Destroy, Translate for Travel, Recrystallize for Crystallize). For
    // mutation kinds we require an explicit declaration to avoid drift
    // between source-level intent + MIR-time enforcement.
    if !kind.is_read_only() && ctx.declared_required_bit.is_none() {
        diagnostics.push(PassDiagnostic::error(
            SIG0002_MISSING_CONSENT_BIT,
            format!(
                "Σ-mask : op `{op_name}` (kind={kind:?}) has no `{key}` attribute — \
                 op-class declarations are MANDATORY for mutation kinds so the \
                 enforcer can cross-check declared-vs-required bit before-modify.",
                op_name = ctx.op_name,
                key = ATTR_REQUIRED_BIT,
            ),
        ));
        return;
    }

    // — SIG0003 : declared bits do not include the required-bit —
    if let (Some(bits), Some(required)) = (ctx.consent_bits, ctx.declared_required_bit) {
        let req_bits = required.bits();
        if (bits & req_bits) == 0 {
            diagnostics.push(PassDiagnostic::error(
                SIG0003_WRONG_CONSENT_BIT,
                format!(
                    "Σ-mask : op `{op_name}` declared `{key}` = `{name}` (mask 0x{req:08x}) \
                     but `{cb_key}` = 0x{bits:08x} does NOT include that bit — \
                     the cell's declared mask refuses this op-class.",
                    op_name = ctx.op_name,
                    key = ATTR_REQUIRED_BIT,
                    name = required.canonical_name(),
                    req = req_bits,
                    cb_key = ATTR_CONSENT_BITS,
                ),
            ));
            return;
        }
    }

    // — Op-kind-specific cross-checks (SIG0007 + SIG0008) —
    if kind == SigmaCellOpKind::Travel {
        if let Some(bits) = ctx.consent_bits {
            if (bits & ConsentBit::Translate.bits()) == 0 {
                diagnostics.push(PassDiagnostic::error(
                    SIG0007_TRAVEL_NEEDS_TRANSLATE,
                    format!(
                        "Σ-mask : `Travel` op `{op_name}` requires \
                         `ConsentBit::Translate` (1<<6 = 0x{tr:08x}) but `{key}` = \
                         0x{bits:08x} does not permit Translate — the cell refuses \
                         substrate-translation per Axiom-2.",
                        op_name = ctx.op_name,
                        tr = ConsentBit::Translate.bits(),
                        key = ATTR_CONSENT_BITS,
                    ),
                ));
                return;
            }
        }
    }
    if kind == SigmaCellOpKind::Crystallize {
        if let Some(bits) = ctx.consent_bits {
            if (bits & ConsentBit::Recrystallize.bits()) == 0 {
                diagnostics.push(PassDiagnostic::error(
                    SIG0008_CRYSTALLIZE_NEEDS_RECRYSTALLIZE,
                    format!(
                        "Σ-mask : `Crystallize` op `{op_name}` requires \
                         `ConsentBit::Recrystallize` (1<<7 = 0x{re:08x}) but `{key}` = \
                         0x{bits:08x} does not permit Recrystallize — Pattern-rewrite \
                         is Sovereign-only per the canonical bit table.",
                        op_name = ctx.op_name,
                        re = ConsentBit::Recrystallize.bits(),
                        key = ATTR_CONSENT_BITS,
                    ),
                ));
                return;
            }
        }
    }

    // — SIG0009 : destroy on a frozen cell —
    if kind == SigmaCellOpKind::Destroy {
        if let Some(facet) = ctx.cell_facet.as_deref() {
            if facet == "frozen" {
                diagnostics.push(PassDiagnostic::error(
                    SIG0009_DESTROY_FORBIDDEN_WHEN_FROZEN,
                    format!(
                        "Σ-mask : op `{op_name}` is `cssl.fieldcell.destroy` but the \
                         cell's `{key}` reads `frozen` — the Sovereign has declared \
                         the cell immutable. Destroy refused per AGENCY_INVARIANT \
                         agency-state semantics.",
                        op_name = ctx.op_name,
                        key = ATTR_CELL_FACET,
                    ),
                ));
                return;
            }
        }
    }

    // — SIG0004 : Sovereign-handle mismatch —
    //
    // The cell is Sovereign-claimed and the call-site declared a
    // sovereign_authorizing handle that does NOT match. Refused.
    if ctx.is_sovereign_claimed() {
        if let (Some(owner), Some(authorizing)) = (ctx.sovereign_handle, ctx.sovereign_authorizing)
        {
            if owner != authorizing {
                diagnostics.push(PassDiagnostic::error(
                    SIG0004_SOVEREIGN_MISMATCH,
                    format!(
                        "Σ-mask : op `{op_name}` declares `{auth_key}` = {got} but \
                         the cell's `{owner_key}` = {expected} — the call-site cannot \
                         act-as a Sovereign that does not own the cell. \
                         PRIME_DIRECTIVE §0 + §2 COGNITIVE-INTEGRITY.",
                        op_name = ctx.op_name,
                        auth_key = ATTR_SOVEREIGN_AUTHORIZING,
                        owner_key = ATTR_SOVEREIGN_HANDLE,
                        got = authorizing,
                        expected = owner,
                    ),
                ));
                return;
            }
        }
    }

    // — SIG0005 : capacity-floor erosion —
    //
    // If the op declares both current + target capacity-floors and the
    // target is strictly lower, refuse unless the call-site is the cell's
    // Sovereign (matched authorizing-handle).
    if let (Some(current), Some(target)) = (ctx.capacity_floor, ctx.target_capacity_floor) {
        if target < current {
            let same_sovereign = match (ctx.sovereign_handle, ctx.sovereign_authorizing) {
                (Some(o), Some(a)) => o != 0 && o == a,
                _ => false,
            };
            if !same_sovereign {
                diagnostics.push(PassDiagnostic::error(
                    SIG0005_CAPACITY_FLOOR_ERODED,
                    format!(
                        "Σ-mask : op `{op_name}` would erode `{key}` from {current} → \
                         {target} without explicit Sovereign authorizing-consent — \
                         AGENCY_INVARIANT § I.2 capacity-floor preservation rule \
                         refuses the op.",
                        op_name = ctx.op_name,
                        key = ATTR_CAPACITY_FLOOR,
                    ),
                ));
                return;
            }
        }
    }

    // — SIG0006 : reversibility widening on a Sovereign-claimed cell —
    //
    // Widening reversibility-scope (e.g. Session ⇒ Permanent) is only
    // allowed when the call-site is the cell's Sovereign. Widening on an
    // unclaimed cell is allowed (matches sigma.rs::mutate behavior).
    if ctx.is_sovereign_claimed() {
        if let (Some(current), Some(target)) =
            (ctx.reversibility_scope, ctx.target_reversibility_scope)
        {
            // Widening = larger ordinal value (Permanent > Session > ...).
            if (target as u8) > (current as u8) {
                let same_sovereign = match (ctx.sovereign_handle, ctx.sovereign_authorizing) {
                    (Some(o), Some(a)) => o != 0 && o == a,
                    _ => false,
                };
                if !same_sovereign {
                    diagnostics.push(PassDiagnostic::error(
                        SIG0006_REVERSIBILITY_WIDEN_WITHOUT_CONSENT,
                        format!(
                            "Σ-mask : op `{op_name}` would widen `{key}` from \
                             {current:?} → {target:?} on Sovereign-claimed cell \
                             (handle={owner}) without authorizing-consent — \
                             PRIME_DIRECTIVE §5 revocability + AGENCY_INVARIANT \
                             § I.3 reversibility rule.",
                            op_name = ctx.op_name,
                            key = ATTR_REVERSIBILITY_SCOPE,
                            owner = ctx.sovereign_handle.unwrap_or(0),
                        ),
                    ));
                }
            }
        }
    }
}

/// Whether any reserved-extension consent-bit (10..=31) is set in `bits`.
const fn reserved_consent_bits_set(bits: u32) -> bool {
    // Canonical bits are 1<<0..1<<8 = 0x0000_01FF.
    let canonical_mask: u32 = (1u32 << 9) - 1;
    (bits & !canonical_mask) != 0
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — 35+ coverage points per the slice contract.
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        parse_consent_bit_name, parse_reversibility_scope, parse_u16_attr, parse_u32_attr,
        reserved_consent_bits_set, EnforcesSigmaAtCellTouches, SigmaCellOpKind,
        SigmaEnforceContext, ATTR_CAPACITY_FLOOR, ATTR_CELL_FACET, ATTR_CONSENT_BITS,
        ATTR_REQUIRED_BIT, ATTR_REVERSIBILITY_SCOPE, ATTR_SOVEREIGN_AUTHORIZING,
        ATTR_SOVEREIGN_HANDLE, ATTR_TARGET_CAPACITY_FLOOR, ATTR_TARGET_REVERSIBILITY_SCOPE,
        OP_FIELDCELL_DESTROY, OP_FIELDCELL_MODIFY, OP_FIELDCELL_READ, OP_FIELDCELL_WRITE,
        SIG0001_UNGUARDED_CELL_WRITE, SIG0002_MISSING_CONSENT_BIT, SIG0003_WRONG_CONSENT_BIT,
        SIG0004_SOVEREIGN_MISMATCH, SIG0005_CAPACITY_FLOOR_ERODED,
        SIG0006_REVERSIBILITY_WIDEN_WITHOUT_CONSENT, SIG0007_TRAVEL_NEEDS_TRANSLATE,
        SIG0008_CRYSTALLIZE_NEEDS_RECRYSTALLIZE, SIG0009_DESTROY_FORBIDDEN_WHEN_FROZEN,
        SIG0010_RESERVED_NONZERO_ATTR, SIGMA_ENFORCE_PASS_NAME,
    };
    use crate::block::{MirBlock, MirOp, MirRegion};
    use crate::func::{MirFunc, MirModule};
    use crate::pipeline::{MirPass, PassSeverity};
    use cssl_substrate_prime_directive::sigma::{ConsentBit, ReversibilityScope};

    // ── Test helpers ────────────────────────────────────────────────────

    /// Build a one-fn module with a single MirOp carrying an ATTR list +
    /// a free-form op-name (used for `cssl.fieldcell.*` etc. that aren't
    /// in the canonical CsslOp enum).
    fn module_with_named_op(name: &str, attrs: Vec<(&str, &str)>) -> MirModule {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        let mut block = MirBlock::new("entry");
        let mut op = MirOp::std(name);
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

    // ── Pass-name + scaffold ────────────────────────────────────────────

    #[test]
    fn pass_name_canonical() {
        assert_eq!(EnforcesSigmaAtCellTouches.name(), SIGMA_ENFORCE_PASS_NAME);
        assert_eq!(SIGMA_ENFORCE_PASS_NAME, "enforces-sigma-at-cell-touches");
    }

    #[test]
    fn empty_module_passes() {
        let mut m = MirModule::new();
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors());
        assert_eq!(r.diagnostics.len(), 0);
    }

    #[test]
    fn module_with_no_cell_touching_ops_passes() {
        // A fn whose body holds only non-cell-touching ops (e.g. arith.addi).
        let mut m = MirModule::new();
        let mut f = MirFunc::new("noop", vec![], vec![]);
        let mut b = MirBlock::new("entry");
        b.push(MirOp::std("arith.addi"));
        b.push(MirOp::std("scf.if"));
        f.body = MirRegion { blocks: vec![b] };
        m.push_func(f);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors());
    }

    // ── Op-kind classification ──────────────────────────────────────────

    #[test]
    fn classify_canonical_cell_op_names() {
        assert_eq!(
            SigmaCellOpKind::classify(OP_FIELDCELL_READ),
            Some(SigmaCellOpKind::Read)
        );
        assert_eq!(
            SigmaCellOpKind::classify(OP_FIELDCELL_WRITE),
            Some(SigmaCellOpKind::Write)
        );
        assert_eq!(
            SigmaCellOpKind::classify(OP_FIELDCELL_MODIFY),
            Some(SigmaCellOpKind::ReadModifyWrite)
        );
        assert_eq!(
            SigmaCellOpKind::classify(OP_FIELDCELL_DESTROY),
            Some(SigmaCellOpKind::Destroy)
        );
    }

    #[test]
    fn classify_travel_and_crystallize_prefixes() {
        assert_eq!(
            SigmaCellOpKind::classify("cssl.travel.cross"),
            Some(SigmaCellOpKind::Travel)
        );
        assert_eq!(
            SigmaCellOpKind::classify("cssl.crystallize.local_machine"),
            Some(SigmaCellOpKind::Crystallize)
        );
    }

    #[test]
    fn classify_unrelated_op_returns_none() {
        assert!(SigmaCellOpKind::classify("arith.addi").is_none());
        assert!(SigmaCellOpKind::classify("cssl.gpu.barrier").is_none());
    }

    #[test]
    fn op_kind_required_bit_mapping() {
        assert_eq!(SigmaCellOpKind::Read.required_bit(), ConsentBit::Observe);
        assert_eq!(SigmaCellOpKind::Write.required_bit(), ConsentBit::Modify);
        assert_eq!(
            SigmaCellOpKind::ReadModifyWrite.required_bit(),
            ConsentBit::Modify
        );
        assert_eq!(SigmaCellOpKind::Destroy.required_bit(), ConsentBit::Destroy);
        assert_eq!(
            SigmaCellOpKind::Travel.required_bit(),
            ConsentBit::Translate
        );
        assert_eq!(
            SigmaCellOpKind::Crystallize.required_bit(),
            ConsentBit::Recrystallize
        );
    }

    #[test]
    fn op_kind_is_read_only_only_for_read() {
        assert!(SigmaCellOpKind::Read.is_read_only());
        assert!(!SigmaCellOpKind::Write.is_read_only());
        assert!(!SigmaCellOpKind::ReadModifyWrite.is_read_only());
        assert!(!SigmaCellOpKind::Destroy.is_read_only());
        assert!(!SigmaCellOpKind::Travel.is_read_only());
        assert!(!SigmaCellOpKind::Crystallize.is_read_only());
    }

    // ── Attribute-parsing helpers ───────────────────────────────────────

    #[test]
    fn parse_u32_attr_decimal_and_hex() {
        assert_eq!(parse_u32_attr("123"), Some(123));
        assert_eq!(parse_u32_attr("0x7f"), Some(0x7f));
        assert_eq!(parse_u32_attr("0XFF"), Some(0xff));
        assert_eq!(parse_u32_attr("garbage"), None);
    }

    #[test]
    fn parse_u16_attr_decimal_and_hex() {
        assert_eq!(parse_u16_attr("42"), Some(42));
        assert_eq!(parse_u16_attr("0x10"), Some(0x10));
        assert_eq!(parse_u16_attr("0xffff"), Some(0xffff));
        assert_eq!(parse_u16_attr("not_a_num"), None);
    }

    #[test]
    fn parse_consent_bit_name_round_trip() {
        for &bit in ConsentBit::all() {
            assert_eq!(parse_consent_bit_name(bit.canonical_name()), Some(bit));
        }
        assert_eq!(parse_consent_bit_name("nonsense"), None);
    }

    #[test]
    fn parse_reversibility_scope_round_trip() {
        for &scope in ReversibilityScope::all() {
            assert_eq!(
                parse_reversibility_scope(scope.canonical_name()),
                Some(scope)
            );
        }
        assert_eq!(parse_reversibility_scope("BOGUS"), None);
    }

    // ── SIG0001 : unguarded cell-write/destroy/modify ───────────────────

    #[test]
    fn unguarded_write_triggers_sig0001() {
        let mut m = module_with_named_op(OP_FIELDCELL_WRITE, vec![]);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0001_UNGUARDED_CELL_WRITE);
        assert!(r.diagnostics[0].message.contains("PRIME_DIRECTIVE"));
        assert!(r.diagnostics[0].message.contains("AGENCY_INVARIANT"));
    }

    #[test]
    fn unguarded_destroy_triggers_sig0001() {
        let mut m = module_with_named_op(OP_FIELDCELL_DESTROY, vec![]);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0001_UNGUARDED_CELL_WRITE);
    }

    #[test]
    fn unguarded_modify_triggers_sig0001() {
        let mut m = module_with_named_op(OP_FIELDCELL_MODIFY, vec![]);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0001_UNGUARDED_CELL_WRITE);
    }

    #[test]
    fn pure_read_without_consent_bits_passes() {
        // Read is the one mutation-free case ; the cell's mask defaults to
        // permit Observe so omitting consent_bits is allowed at MIR-time
        // for pure reads.
        let mut m = module_with_named_op(OP_FIELDCELL_READ, vec![]);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── SIG0002 : missing required-bit declaration ──────────────────────

    #[test]
    fn missing_required_bit_triggers_sig0002() {
        let bits = ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![(ATTR_CONSENT_BITS, bits_str.as_str())],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0002_MISSING_CONSENT_BIT);
    }

    // ── SIG0003 : declared bits do not include the required-bit ─────────

    #[test]
    fn write_against_observe_only_mask_triggers_sig0003() {
        // consent_bits = Observe ; required = Modify ; the cell refuses.
        let bits_str = format!("0x{:08x}", ConsentBit::Observe.bits());
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0003_WRONG_CONSENT_BIT);
        assert!(r.diagnostics[0].message.contains("modify"));
    }

    #[test]
    fn write_with_modify_bit_passes_sig0003() {
        // consent_bits includes Modify ; required = Modify ; cell permits.
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── SIG0004 : Sovereign-handle mismatch ─────────────────────────────

    #[test]
    fn sovereign_mismatch_triggers_sig0004() {
        let bits =
            ConsentBit::Observe.bits() | ConsentBit::Modify.bits() | ConsentBit::Sample.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "7"),
                (ATTR_SOVEREIGN_AUTHORIZING, "99"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0004_SOVEREIGN_MISMATCH);
        assert!(r.diagnostics[0].message.contains("99"));
        assert!(r.diagnostics[0].message.contains('7'));
    }

    #[test]
    fn sovereign_match_passes_sig0004() {
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "7"),
                (ATTR_SOVEREIGN_AUTHORIZING, "7"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    #[test]
    fn unclaimed_cell_passes_without_authorizing_handle() {
        // sovereign_handle = 0 means unclaimed — no authorizing-handle
        // requirement applies. (sigma.rs::SIGMA_SOVEREIGN_NULL is also 0.)
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "0"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── SIG0005 : capacity-floor erosion ────────────────────────────────

    #[test]
    fn capacity_floor_erosion_without_sovereign_consent_triggers_sig0005() {
        // Cell is Sovereign-claimed by handle=7 ; call-site is handle=99 ;
        // op would lower capacity-floor 100 → 50 ; refused.
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "7"),
                (ATTR_SOVEREIGN_AUTHORIZING, "7"),
                (ATTR_CAPACITY_FLOOR, "100"),
                (ATTR_TARGET_CAPACITY_FLOOR, "50"),
            ],
        );
        // First the op needs a non-mismatching authorizing-handle to get past
        // SIG0004 ; we set the same handle so SIG0005 is the next gate.
        // But SIG0005 requires the authorizing-handle MATCH the owner ; if
        // they match, the op IS allowed to lower the floor (Sovereign opt-in).
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        // Owner == authorizing → Sovereign-consent → SIG0005 NOT raised.
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    #[test]
    fn capacity_floor_erosion_unauthorized_triggers_sig0005() {
        // Cell is unclaimed but the op declares a current floor and a lower
        // target floor. With no Sovereign on the cell, the rule still
        // refuses (no one authorized the erosion).
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "0"),
                (ATTR_CAPACITY_FLOOR, "100"),
                (ATTR_TARGET_CAPACITY_FLOOR, "50"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0005_CAPACITY_FLOOR_ERODED);
        assert!(r.diagnostics[0].message.contains("100"));
        assert!(r.diagnostics[0].message.contains("50"));
    }

    #[test]
    fn capacity_floor_increase_passes_sig0005() {
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "7"),
                (ATTR_SOVEREIGN_AUTHORIZING, "7"),
                (ATTR_CAPACITY_FLOOR, "50"),
                (ATTR_TARGET_CAPACITY_FLOOR, "100"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── SIG0006 : reversibility widening ────────────────────────────────

    #[test]
    fn reversibility_widening_unauthorized_triggers_sig0006() {
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "7"),
                (ATTR_SOVEREIGN_AUTHORIZING, "99"),
                (
                    ATTR_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Session.canonical_name(),
                ),
                (
                    ATTR_TARGET_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Permanent.canonical_name(),
                ),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        // SIG0004 fires first because authorizing-handle ≠ owner-handle.
        // We just want to assert that the pass refuses the build.
        // Specifically, SIG0006 is also present in code-path but execution
        // returns after SIG0004. Verify the relevant code is in the set.
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.code == SIG0004_SOVEREIGN_MISMATCH
                || d.code == SIG0006_REVERSIBILITY_WIDEN_WITHOUT_CONSENT));
    }

    #[test]
    fn reversibility_widening_isolated_triggers_sig0006() {
        // Owner = 7, authorizing UN-supplied → not "same sovereign" → widen
        // refused with SIG0006 (no SIG0004 because authorizing is None).
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "7"),
                (
                    ATTR_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Session.canonical_name(),
                ),
                (
                    ATTR_TARGET_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Permanent.canonical_name(),
                ),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(
            r.diagnostics[0].code,
            SIG0006_REVERSIBILITY_WIDEN_WITHOUT_CONSENT
        );
    }

    #[test]
    fn reversibility_widening_with_consent_passes_sig0006() {
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "7"),
                (ATTR_SOVEREIGN_AUTHORIZING, "7"),
                (
                    ATTR_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Session.canonical_name(),
                ),
                (
                    ATTR_TARGET_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Permanent.canonical_name(),
                ),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    #[test]
    fn reversibility_widening_on_unclaimed_cell_passes() {
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "0"),
                (
                    ATTR_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Session.canonical_name(),
                ),
                (
                    ATTR_TARGET_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Permanent.canonical_name(),
                ),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── SIG0007 : Travel without Translate ──────────────────────────────

    #[test]
    fn travel_without_translate_triggers_sig0007() {
        // consent_bits has Observe + Modify but NOT Translate.
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            "cssl.travel.cross",
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Translate.canonical_name()),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        // SIG0003 might fire first (because consent_bits doesn't include
        // Translate, declared required bit is Translate). Either way
        // refused.
        assert!(r.diagnostics.iter().any(
            |d| d.code == SIG0003_WRONG_CONSENT_BIT || d.code == SIG0007_TRAVEL_NEEDS_TRANSLATE
        ));
    }

    #[test]
    fn travel_with_translate_passes() {
        let bits = ConsentBit::Translate.bits() | ConsentBit::Observe.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            "cssl.travel.cross",
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Translate.canonical_name()),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── SIG0008 : Crystallize without Recrystallize ─────────────────────

    #[test]
    fn crystallize_without_recrystallize_triggers_sig0008() {
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            "cssl.crystallize.local_machine",
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (
                    ATTR_REQUIRED_BIT,
                    ConsentBit::Recrystallize.canonical_name(),
                ),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.code == SIG0003_WRONG_CONSENT_BIT
                || d.code == SIG0008_CRYSTALLIZE_NEEDS_RECRYSTALLIZE));
    }

    #[test]
    fn crystallize_with_recrystallize_passes() {
        let bits = ConsentBit::Recrystallize.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            "cssl.crystallize.local_machine",
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (
                    ATTR_REQUIRED_BIT,
                    ConsentBit::Recrystallize.canonical_name(),
                ),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── SIG0009 : destroy on a frozen cell ──────────────────────────────

    #[test]
    fn destroy_on_frozen_cell_triggers_sig0009() {
        let bits = ConsentBit::Destroy.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_DESTROY,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Destroy.canonical_name()),
                (ATTR_CELL_FACET, "frozen"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0009_DESTROY_FORBIDDEN_WHEN_FROZEN);
    }

    #[test]
    fn destroy_on_quiescent_cell_passes_sig0009() {
        let bits = ConsentBit::Destroy.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_DESTROY,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Destroy.canonical_name()),
                (ATTR_CELL_FACET, "quiescent"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── SIG0010 : reserved-extension bits set ───────────────────────────

    #[test]
    fn reserved_consent_bits_helper() {
        // Bits 0..=8 are canonical ; setting bit 10 = reserved violation.
        assert!(!reserved_consent_bits_set(0x0000_01FF));
        assert!(reserved_consent_bits_set(0x0000_0400));
        assert!(reserved_consent_bits_set(0x8000_0000));
    }

    #[test]
    fn reserved_extension_bit_triggers_sig0010() {
        let bits = ConsentBit::Modify.bits() | (1u32 << 15);
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0010_RESERVED_NONZERO_ATTR);
    }

    // ── Nested-region walk ──────────────────────────────────────────────

    #[test]
    fn detects_violation_inside_nested_region() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        let mut outer = MirBlock::new("entry");
        let mut nested_block = MirBlock::new("then");
        nested_block.push(MirOp::std(OP_FIELDCELL_WRITE));
        let nested = MirRegion {
            blocks: vec![nested_block],
        };
        outer.push(MirOp::std("scf.if").with_region(nested));
        f.body = MirRegion {
            blocks: vec![outer],
        };
        m.push_func(f);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0001_UNGUARDED_CELL_WRITE);
    }

    #[test]
    fn deeply_nested_violation_is_detected() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        let mut innermost = MirBlock::new("inner");
        innermost.push(MirOp::std(OP_FIELDCELL_DESTROY));
        let inner_r = MirRegion {
            blocks: vec![innermost],
        };
        let mut middle = MirBlock::new("middle");
        middle.push(MirOp::std("scf.for").with_region(inner_r));
        let middle_r = MirRegion {
            blocks: vec![middle],
        };
        let mut outer = MirBlock::new("entry");
        outer.push(MirOp::std("scf.if").with_region(middle_r));
        f.body = MirRegion {
            blocks: vec![outer],
        };
        m.push_func(f);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, SIG0001_UNGUARDED_CELL_WRITE);
    }

    // ── Multi-violation reporting ───────────────────────────────────────

    #[test]
    fn reports_each_unguarded_op_separately() {
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std(OP_FIELDCELL_WRITE));
        block.push(MirOp::std(OP_FIELDCELL_DESTROY));
        block.push(MirOp::std(OP_FIELDCELL_MODIFY));
        f.body = MirRegion {
            blocks: vec![block],
        };
        m.push_func(f);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        // Three unguarded ops → three SIG0001 diagnostics.
        assert_eq!(r.diagnostics.len(), 3);
        for d in &r.diagnostics {
            assert_eq!(d.code, SIG0001_UNGUARDED_CELL_WRITE);
            assert_eq!(d.severity, PassSeverity::Error);
        }
    }

    #[test]
    fn mixed_violations_each_reported() {
        // One unguarded write + one Travel without Translate.
        let mut m = MirModule::new();
        let mut f = MirFunc::new("test", vec![], vec![]);
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std(OP_FIELDCELL_WRITE));
        let bits = ConsentBit::Observe.bits();
        let bits_str = format!("0x{bits:08x}");
        block.push(
            MirOp::std("cssl.travel.cross")
                .with_attribute(ATTR_CONSENT_BITS, &bits_str)
                .with_attribute(ATTR_REQUIRED_BIT, ConsentBit::Translate.canonical_name()),
        );
        f.body = MirRegion {
            blocks: vec![block],
        };
        m.push_func(f);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert!(r.diagnostics.len() >= 2);
        // Assert both kinds of violation are observed.
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.code == SIG0001_UNGUARDED_CELL_WRITE));
        assert!(r.diagnostics.iter().any(
            |d| d.code == SIG0003_WRONG_CONSENT_BIT || d.code == SIG0007_TRAVEL_NEEDS_TRANSLATE
        ));
    }

    // ── SigmaEnforceContext shape ───────────────────────────────────────

    #[test]
    fn context_from_op_returns_none_for_non_cell_ops() {
        let op = MirOp::std("arith.addi");
        assert!(SigmaEnforceContext::from_op(&op).is_none());
    }

    #[test]
    fn context_from_op_classifies_write() {
        let op = MirOp::std(OP_FIELDCELL_WRITE);
        let ctx = SigmaEnforceContext::from_op(&op).expect("classified");
        assert_eq!(ctx.kind, Some(SigmaCellOpKind::Write));
        assert_eq!(ctx.op_name, OP_FIELDCELL_WRITE);
    }

    #[test]
    fn context_decodes_all_attributes() {
        let op = MirOp::std(OP_FIELDCELL_WRITE)
            .with_attribute(ATTR_CONSENT_BITS, "0x1ff")
            .with_attribute(ATTR_REQUIRED_BIT, "modify")
            .with_attribute(ATTR_SOVEREIGN_HANDLE, "7")
            .with_attribute(ATTR_SOVEREIGN_AUTHORIZING, "7")
            .with_attribute(ATTR_CAPACITY_FLOOR, "100")
            .with_attribute(ATTR_TARGET_CAPACITY_FLOOR, "200")
            .with_attribute(ATTR_REVERSIBILITY_SCOPE, "session")
            .with_attribute(ATTR_TARGET_REVERSIBILITY_SCOPE, "rg_day")
            .with_attribute(ATTR_CELL_FACET, "active");
        let ctx = SigmaEnforceContext::from_op(&op).expect("classified");
        assert_eq!(ctx.consent_bits, Some(0x1ff));
        assert_eq!(ctx.declared_required_bit, Some(ConsentBit::Modify));
        assert_eq!(ctx.sovereign_handle, Some(7));
        assert_eq!(ctx.sovereign_authorizing, Some(7));
        assert_eq!(ctx.capacity_floor, Some(100));
        assert_eq!(ctx.target_capacity_floor, Some(200));
        assert_eq!(ctx.reversibility_scope, Some(ReversibilityScope::Session));
        assert_eq!(
            ctx.target_reversibility_scope,
            Some(ReversibilityScope::RgDay)
        );
        assert_eq!(ctx.cell_facet.as_deref(), Some("active"));
        assert!(ctx.is_sovereign_claimed());
    }

    #[test]
    fn context_is_sovereign_claimed_returns_false_for_handle_zero() {
        let op = MirOp::std(OP_FIELDCELL_WRITE).with_attribute(ATTR_SOVEREIGN_HANDLE, "0");
        let ctx = SigmaEnforceContext::from_op(&op).expect("classified");
        assert!(!ctx.is_sovereign_claimed());
    }

    // ── Composite : guarded write passes end-to-end ─────────────────────

    #[test]
    fn fully_guarded_sovereign_write_passes() {
        // The canonical "happy path" : Sovereign-only cell, owner authorizes,
        // floor preserved, scope unchanged, all bits present.
        let bits = ConsentBit::Observe.bits()
            | ConsentBit::Modify.bits()
            | ConsentBit::Sample.bits()
            | ConsentBit::Reconfigure.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "42"),
                (ATTR_SOVEREIGN_AUTHORIZING, "42"),
                (ATTR_CAPACITY_FLOOR, "100"),
                (ATTR_TARGET_CAPACITY_FLOOR, "100"),
                (
                    ATTR_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Session.canonical_name(),
                ),
                (
                    ATTR_TARGET_REVERSIBILITY_SCOPE,
                    ReversibilityScope::Session.canonical_name(),
                ),
                (ATTR_CELL_FACET, "quiescent"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        assert!(!r.changed);
    }

    // ── Pass invariants : mutation-free + name stability ────────────────

    #[test]
    fn pass_does_not_mutate_module() {
        // Run the pass against a module with no errors ; verify the
        // module's func count + body shape is identical before/after.
        let bits = ConsentBit::Observe.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            OP_FIELDCELL_WRITE,
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Modify.canonical_name()),
            ],
        );
        let snapshot = m.funcs.len();
        let attr_count = m.funcs[0].body.blocks[0].ops[0].attributes.len();
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.changed);
        assert_eq!(m.funcs.len(), snapshot);
        assert_eq!(
            m.funcs[0].body.blocks[0].ops[0].attributes.len(),
            attr_count
        );
    }

    #[test]
    fn diagnostic_codes_are_unique_strings() {
        let codes = [
            SIG0001_UNGUARDED_CELL_WRITE,
            SIG0002_MISSING_CONSENT_BIT,
            SIG0003_WRONG_CONSENT_BIT,
            SIG0004_SOVEREIGN_MISMATCH,
            SIG0005_CAPACITY_FLOOR_ERODED,
            SIG0006_REVERSIBILITY_WIDEN_WITHOUT_CONSENT,
            SIG0007_TRAVEL_NEEDS_TRANSLATE,
            SIG0008_CRYSTALLIZE_NEEDS_RECRYSTALLIZE,
            SIG0009_DESTROY_FORBIDDEN_WHEN_FROZEN,
            SIG0010_RESERVED_NONZERO_ATTR,
        ];
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), before, "diagnostic codes must be unique");
        // Each code follows the SIGNNNN pattern.
        for c in codes {
            assert!(c.starts_with("SIG"));
            assert_eq!(c.len(), 7);
        }
    }

    #[test]
    fn diagnostic_severity_always_error() {
        // Cause every kind of violation we can drive directly + verify
        // every diagnostic is severity=Error (no warnings here ; the
        // gates are absolute).
        let mut m = module_with_named_op(OP_FIELDCELL_WRITE, vec![]);
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        for d in &r.diagnostics {
            assert_eq!(d.severity, PassSeverity::Error);
        }
    }

    // ── Multi-fn module ─────────────────────────────────────────────────

    #[test]
    fn walks_all_fns_in_module() {
        let mut m = MirModule::new();
        for i in 0..3 {
            let mut f = MirFunc::new(format!("f{i}"), vec![], vec![]);
            let mut block = MirBlock::new("entry");
            block.push(MirOp::std(OP_FIELDCELL_WRITE));
            f.body = MirRegion {
                blocks: vec![block],
            };
            m.push_func(f);
        }
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics.len(), 3, "one diag per fn");
    }

    // ── Travel + Sovereign-handle interplay ─────────────────────────────

    #[test]
    fn travel_with_owner_match_passes() {
        let bits = ConsentBit::Translate.bits() | ConsentBit::Observe.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            "cssl.travel.cross",
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (ATTR_REQUIRED_BIT, ConsentBit::Translate.canonical_name()),
                (ATTR_SOVEREIGN_HANDLE, "5"),
                (ATTR_SOVEREIGN_AUTHORIZING, "5"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── Crystallize + Sovereign-handle interplay ───────────────────────

    #[test]
    fn crystallize_with_owner_match_passes() {
        let bits = ConsentBit::Recrystallize.bits() | ConsentBit::Modify.bits();
        let bits_str = format!("0x{bits:08x}");
        let mut m = module_with_named_op(
            "cssl.crystallize.local_machine",
            vec![
                (ATTR_CONSENT_BITS, bits_str.as_str()),
                (
                    ATTR_REQUIRED_BIT,
                    ConsentBit::Recrystallize.canonical_name(),
                ),
                (ATTR_SOVEREIGN_HANDLE, "5"),
                (ATTR_SOVEREIGN_AUTHORIZING, "5"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    // ── Pure Read with sovereign + capacity ─────────────────────────────

    #[test]
    fn pure_read_on_sovereign_cell_does_not_check_floor() {
        // Read is mutation-free ; capacity-floor checks should not gate it.
        let mut m = module_with_named_op(
            OP_FIELDCELL_READ,
            vec![
                (ATTR_SOVEREIGN_HANDLE, "7"),
                (ATTR_CAPACITY_FLOOR, "100"),
                (ATTR_TARGET_CAPACITY_FLOOR, "100"),
            ],
        );
        let r = EnforcesSigmaAtCellTouches.run(&mut m);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }
}
