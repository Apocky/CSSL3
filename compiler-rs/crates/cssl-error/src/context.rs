//! [`ErrorContext`] + [`SourceLocation`] + [`SubsystemTag`] + [`KindId`].
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.4 + § 2.7.
//!
//! § DESIGN
//!   - Every [`crate::EngineError`] carries an [`ErrorContext`] capturing :
//!     where (file-hash + line + col), when (frame_n), what-class (subsystem
//!     + KindId), how-bad (severity), and how-to-dedup (fingerprint).
//!   - [`SourceLocation::file_path_hash`] is a [`PathHash`] newtype : the
//!     constructor at the type-level forbids raw-`&Path` ingress (D130).
//!   - [`SubsystemTag`] is the canonical 25-variant catalog ; renaming a
//!     variant = §7-INTEGRITY violation per spec § 7.4 (wire-format pin).
//!   - [`KindId`] is a thin newtype around `u32` for cheap discriminant
//!     identification ; per-crate-error-types map their variants to a
//!     stable `KindId` for fingerprinting.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - § 1 surveillance : raw-paths NEVER leave the capture-site ; always
//!     hashed via [`cssl_telemetry::PathHasher`] BEFORE entering this module.
//!   - § 7 INTEGRITY : the `SubsystemTag` enum is hash-pinned in tests.

use core::fmt;

use cssl_telemetry::PathHash;

use crate::severity::Severity;

// ───────────────────────────────────────────────────────────────────────
// § SourceLocation — file-hash + line + column.
// ───────────────────────────────────────────────────────────────────────

/// Source location of an error : the (file_path_hash, line, column) triple.
///
/// § INVARIANTS
///   - `file_path_hash` is a [`PathHash`] newtype constructible only via
///     [`cssl_telemetry::PathHasher::hash_str`]. Raw-strings cannot enter.
///   - `line` + `column` are `u32` ; column is 1-indexed (matches `column!()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    /// 32-byte BLAKE3 hash of the source file path (D130-enforced).
    pub file_path_hash: PathHash,
    /// 1-indexed line number from `line!()`.
    pub line: u32,
    /// 1-indexed column from `column!()`.
    pub column: u32,
}

impl SourceLocation {
    /// Construct a [`SourceLocation`]. The constructor takes a [`PathHash`]
    /// directly ; callers must go through [`cssl_telemetry::PathHasher`] to
    /// produce one. Raw-`&str` ingress is structurally impossible.
    #[must_use]
    pub const fn new(file_path_hash: PathHash, line: u32, column: u32) -> Self {
        Self {
            file_path_hash,
            line,
            column,
        }
    }

    /// Sentinel "unknown" source-location ; uses [`PathHash::zero()`] as the
    /// hash. Reserved for cases where the source-loc cannot be captured
    /// (e.g., FFI-boundary panics with stripped frames).
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            file_path_hash: PathHash::zero(),
            line: 0,
            column: 0,
        }
    }

    /// Returns `true` if this is the sentinel unknown location.
    #[must_use]
    pub fn is_unknown(self) -> bool {
        self.line == 0 && self.column == 0 && self.file_path_hash == PathHash::zero()
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unknown() {
            write!(f, "<unknown>")
        } else {
            write!(f, "{}:{}:{}", self.file_path_hash, self.line, self.column)
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § KindId — stable per-crate-error variant identifier.
// ───────────────────────────────────────────────────────────────────────

/// Stable u32 identifier for an error-variant. Used in fingerprinting +
/// deduplication ; each per-crate-error type maps its variants to a stable
/// [`KindId`] so two errors with the same KindId at the same source-loc are
/// considered duplicates.
///
/// § ENCODING-SPACE  (canonical reservation)
///   - 0x0000_0000           : unknown / unset
///   - 0x0000_0001..0x000F_FFFF : foundation crates (telemetry, prime-directive)
///   - 0x0010_0000..0x00FF_FFFF : substrate crates
///   - 0x0100_0000..0x0FFF_FFFF : per-subsystem error-spaces
///   - 0xF000_0000..0xFFFF_FFFF : reserved / future
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KindId(pub u32);

impl KindId {
    /// "Unknown" sentinel ; used when caller doesn't classify.
    pub const UNKNOWN: Self = Self(0);

    /// Construct a [`KindId`] from a u32 discriminant.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the underlying u32.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for KindId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "kind#{:08x}", self.0)
    }
}

impl Default for KindId {
    fn default() -> Self {
        Self::UNKNOWN
    }
}

// ───────────────────────────────────────────────────────────────────────
// § SubsystemTag — canonical 25-variant subsystem catalog.
// ───────────────────────────────────────────────────────────────────────

/// Canonical subsystem catalog (spec § 2.7).
///
/// § STABILITY
///   - Adding a variant = ADDITIVE per spec § 7.4 ; existing wire-format
///     payloads continue to decode correctly.
///   - Renaming a variant = §7-INTEGRITY violation (wire-format encoded as
///     u8 ; old logs would mis-decode).
///   - The `Other` variant captures genuinely-unmapped subsystems ; a
///     clippy lint (Wave-Jε-3) discourages its use in core-loop modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum SubsystemTag {
    /// Top-level engine orchestrator.
    Engine = 0,
    /// `omega_step` driver / 1kHz tick.
    OmegaStep = 1,
    /// Render-v2 pipeline.
    Render = 2,
    /// Wave-physics + SDF + XPBD.
    Physics = 3,
    /// Wave-audio + wave-coupler.
    Audio = 4,
    /// Procedural anim + KAN-anim.
    Anim = 5,
    /// AI : behavior trees + utility-AI + companion.
    Ai = 6,
    /// UI surface (loa-game UI).
    Ui = 7,
    /// OpenXR / XR session.
    Xr = 8,
    /// Host-level (Level-Zero / Vulkan / D3D12 / Metal / WebGPU).
    Host = 9,
    /// Wave-solver + LBM.
    WaveSolver = 10,
    /// KAN substrate.
    Kan = 11,
    /// Gaze-collapse + foveation.
    Gaze = 12,
    /// Companion-perspective (consent-gated rendering).
    Companion = 13,
    /// Mise-en-abyme recursion.
    MiseEnAbyme = 14,
    /// Hot-reload pipeline.
    HotReload = 15,
    /// MCP IPC layer (Wave-Jθ).
    Mcp = 16,
    /// Telemetry ring + exporter.
    Telemetry = 17,
    /// Audit-chain.
    Audit = 18,
    /// PD enforcement.
    PrimeDirective = 19,
    /// Asset pipeline.
    Asset = 20,
    /// Code-generation (cgen-cpu-x64 / cgen-cpu-cranelift / cgen-gpu-*).
    Codegen = 21,
    /// Effects discipline (effect-row checker).
    Effects = 22,
    /// Work-graph / pipeline scheduler.
    WorkGraph = 23,
    /// Test-only emissions.
    Test = 24,
    /// Genuinely-unmapped subsystem ; prefer a typed variant.
    Other = 25,
}

impl SubsystemTag {
    /// All variants in canonical (discriminant) order.
    #[must_use]
    pub const fn all() -> &'static [SubsystemTag] {
        &[
            Self::Engine,
            Self::OmegaStep,
            Self::Render,
            Self::Physics,
            Self::Audio,
            Self::Anim,
            Self::Ai,
            Self::Ui,
            Self::Xr,
            Self::Host,
            Self::WaveSolver,
            Self::Kan,
            Self::Gaze,
            Self::Companion,
            Self::MiseEnAbyme,
            Self::HotReload,
            Self::Mcp,
            Self::Telemetry,
            Self::Audit,
            Self::PrimeDirective,
            Self::Asset,
            Self::Codegen,
            Self::Effects,
            Self::WorkGraph,
            Self::Test,
            Self::Other,
        ]
    }

    /// Stable canonical name (snake_case).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::OmegaStep => "omega_step",
            Self::Render => "render",
            Self::Physics => "physics",
            Self::Audio => "audio",
            Self::Anim => "anim",
            Self::Ai => "ai",
            Self::Ui => "ui",
            Self::Xr => "xr",
            Self::Host => "host",
            Self::WaveSolver => "wave_solver",
            Self::Kan => "kan",
            Self::Gaze => "gaze",
            Self::Companion => "companion",
            Self::MiseEnAbyme => "mise_en_abyme",
            Self::HotReload => "hot_reload",
            Self::Mcp => "mcp",
            Self::Telemetry => "telemetry",
            Self::Audit => "audit",
            Self::PrimeDirective => "prime_directive",
            Self::Asset => "asset",
            Self::Codegen => "codegen",
            Self::Effects => "effects",
            Self::WorkGraph => "work_graph",
            Self::Test => "test",
            Self::Other => "other",
        }
    }

    /// Get the discriminant byte ; matches the wire-format.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Construct from u8 discriminant. Returns `None` on out-of-range.
    #[must_use]
    pub const fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Engine),
            1 => Some(Self::OmegaStep),
            2 => Some(Self::Render),
            3 => Some(Self::Physics),
            4 => Some(Self::Audio),
            5 => Some(Self::Anim),
            6 => Some(Self::Ai),
            7 => Some(Self::Ui),
            8 => Some(Self::Xr),
            9 => Some(Self::Host),
            10 => Some(Self::WaveSolver),
            11 => Some(Self::Kan),
            12 => Some(Self::Gaze),
            13 => Some(Self::Companion),
            14 => Some(Self::MiseEnAbyme),
            15 => Some(Self::HotReload),
            16 => Some(Self::Mcp),
            17 => Some(Self::Telemetry),
            18 => Some(Self::Audit),
            19 => Some(Self::PrimeDirective),
            20 => Some(Self::Asset),
            21 => Some(Self::Codegen),
            22 => Some(Self::Effects),
            23 => Some(Self::WorkGraph),
            24 => Some(Self::Test),
            25 => Some(Self::Other),
            _ => None,
        }
    }
}

impl fmt::Display for SubsystemTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_name())
    }
}

impl Default for SubsystemTag {
    fn default() -> Self {
        Self::Other
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Retryable hint.
// ───────────────────────────────────────────────────────────────────────

/// Hint to upstream callers : "is this error worth retrying?"
///
/// § DESIGN
///   - `Yes` : transient ; caller may retry with backoff.
///   - `No`  : terminal ; caller should propagate.
///   - `Maybe` : context-dependent ; caller decides per-call-site policy.
///
/// Encoded as u8 to keep [`ErrorContext`] memory-cheap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Retryable {
    /// Definitively retryable (e.g., transient I/O).
    Yes = 0,
    /// Definitively not retryable (e.g., type-error).
    No = 1,
    /// Caller-discretion (e.g., partial-state recoverable).
    Maybe = 2,
}

impl Retryable {
    /// Get the discriminant byte.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl Default for Retryable {
    fn default() -> Self {
        Self::No
    }
}

impl fmt::Display for Retryable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yes => f.write_str("retryable"),
            Self::No => f.write_str("terminal"),
            Self::Maybe => f.write_str("maybe-retryable"),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § ErrorContext — full error-site context-bundle.
// ───────────────────────────────────────────────────────────────────────

/// Full error-site context attached to every [`crate::EngineError`].
///
/// § DESIGN
///   - Self-contained : carries source-loc + frame-n + subsystem + kind +
///     severity + retryability + (optional) stack-trace + fingerprint.
///   - Lazily-computed : the fingerprint is derived from the other fields ;
///     [`ErrorContext::compute_fingerprint`] does the BLAKE3 derivation.
///   - Replay-safe : every field is deterministic given the same source-loc
///     + frame-n inputs (no wall-clock leak).
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Source-location capture-site.
    pub source: SourceLocation,
    /// Frame number @ which the error occurred.
    pub frame_n: u64,
    /// Subsystem-tag (matches log subsystem-catalog).
    pub subsystem: SubsystemTag,
    /// Crate name (compile-time const ; no runtime allocation).
    pub crate_name: &'static str,
    /// Severity classification.
    pub severity: Severity,
    /// Stable per-crate-error variant identifier.
    pub kind: KindId,
    /// Retryable hint.
    pub retryable: Retryable,
    /// Optional stack-trace (Some only when `debug-info` feature enabled).
    pub stack: Option<crate::stack::StackTrace>,
    /// BLAKE3 fingerprint for dedup.
    pub fingerprint: crate::fingerprint::ErrorFingerprint,
}

impl ErrorContext {
    /// Construct a minimal [`ErrorContext`] : just the bare-required fields.
    /// Convenience for sites that don't have stack/retryability info.
    #[must_use]
    pub fn minimal(
        source: SourceLocation,
        subsystem: SubsystemTag,
        crate_name: &'static str,
        kind: KindId,
        severity: Severity,
    ) -> Self {
        let frame_n = 0;
        let retryable = Retryable::default();
        let fingerprint =
            crate::fingerprint::ErrorFingerprint::compute(kind, &source, frame_n / 60);
        Self {
            source,
            frame_n,
            subsystem,
            crate_name,
            severity,
            kind,
            retryable,
            stack: None,
            fingerprint,
        }
    }

    /// Set the frame number ; recomputes the fingerprint with the new frame-bucket.
    #[must_use]
    pub fn with_frame_n(mut self, frame_n: u64) -> Self {
        self.frame_n = frame_n;
        self.fingerprint =
            crate::fingerprint::ErrorFingerprint::compute(self.kind, &self.source, frame_n / 60);
        self
    }

    /// Set the retryable hint.
    #[must_use]
    pub fn with_retryable(mut self, retryable: Retryable) -> Self {
        self.retryable = retryable;
        self
    }

    /// Attach an explicit stack-trace.
    #[must_use]
    pub fn with_stack(mut self, stack: crate::stack::StackTrace) -> Self {
        self.stack = Some(stack);
        self
    }

    /// Capture the current stack-trace if `debug-info` feature is enabled.
    /// No-op (returns `self` unchanged) in release builds.
    #[must_use]
    pub fn with_captured_stack(self) -> Self {
        if cfg!(feature = "debug-info") {
            self.with_stack(crate::stack::StackTrace::capture())
        } else {
            self
        }
    }

    /// Recompute the fingerprint after manual field-edit. Idempotent.
    pub fn refresh_fingerprint(&mut self) {
        self.fingerprint = crate::fingerprint::ErrorFingerprint::compute(
            self.kind,
            &self.source,
            self.frame_n / 60,
        );
    }
}

impl Default for ErrorContext {
    fn default() -> Self {
        Self::minimal(
            SourceLocation::unknown(),
            SubsystemTag::Other,
            "<unknown>",
            KindId::UNKNOWN,
            Severity::Error,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorContext, KindId, Retryable, SourceLocation, SubsystemTag};
    use crate::severity::Severity;
    use cssl_telemetry::PathHasher;

    fn h() -> PathHasher {
        PathHasher::from_seed([7u8; 32])
    }

    #[test]
    fn source_loc_round_trip() {
        let p = h().hash_str("/src/main.rs");
        let loc = SourceLocation::new(p, 42, 13);
        assert_eq!(loc.line, 42);
        assert_eq!(loc.column, 13);
        assert_eq!(loc.file_path_hash, p);
        assert!(!loc.is_unknown());
    }

    #[test]
    fn source_loc_unknown_sentinel() {
        let u = SourceLocation::unknown();
        assert!(u.is_unknown());
        assert_eq!(u.line, 0);
        assert_eq!(u.column, 0);
    }

    #[test]
    fn source_loc_display_known() {
        let p = h().hash_str("/test.rs");
        let loc = SourceLocation::new(p, 12, 3);
        let s = format!("{loc}");
        assert!(s.contains(":12:3"));
        assert!(!s.contains("<unknown>"));
    }

    #[test]
    fn source_loc_display_unknown() {
        let u = SourceLocation::unknown();
        assert_eq!(format!("{u}"), "<unknown>");
    }

    #[test]
    fn kind_id_default_unknown() {
        let k = KindId::default();
        assert_eq!(k, KindId::UNKNOWN);
        assert_eq!(k.as_u32(), 0);
    }

    #[test]
    fn kind_id_display_includes_hex() {
        let k = KindId::new(0xCAFE_BABE);
        let s = format!("{k}");
        assert!(s.contains("cafebabe"));
    }

    #[test]
    fn subsystem_tag_all_count() {
        assert_eq!(SubsystemTag::all().len(), 26);
    }

    #[test]
    fn subsystem_tag_names_unique() {
        let mut names: Vec<&str> = SubsystemTag::all()
            .iter()
            .map(|s| s.canonical_name())
            .collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    #[test]
    fn subsystem_tag_u8_round_trip() {
        for s in SubsystemTag::all() {
            let byte = s.as_u8();
            let parsed = SubsystemTag::from_u8(byte).expect("round-trip");
            assert_eq!(*s, parsed);
        }
    }

    #[test]
    fn subsystem_tag_u8_oob_none() {
        assert!(SubsystemTag::from_u8(26).is_none());
        assert!(SubsystemTag::from_u8(255).is_none());
    }

    #[test]
    fn subsystem_tag_default_is_other() {
        assert_eq!(SubsystemTag::default(), SubsystemTag::Other);
    }

    #[test]
    fn retryable_default_is_no() {
        assert_eq!(Retryable::default(), Retryable::No);
    }

    #[test]
    fn retryable_display_humanized() {
        assert_eq!(format!("{}", Retryable::Yes), "retryable");
        assert_eq!(format!("{}", Retryable::No), "terminal");
        assert_eq!(format!("{}", Retryable::Maybe), "maybe-retryable");
    }

    #[test]
    fn error_context_minimal_default_safe() {
        let p = h().hash_str("/file.rs");
        let loc = SourceLocation::new(p, 1, 1);
        let ctx = ErrorContext::minimal(
            loc,
            SubsystemTag::Render,
            "cssl-render-v2",
            KindId::new(7),
            Severity::Error,
        );
        assert_eq!(ctx.frame_n, 0);
        assert_eq!(ctx.subsystem, SubsystemTag::Render);
        assert_eq!(ctx.severity, Severity::Error);
        assert_eq!(ctx.retryable, Retryable::No);
        assert!(ctx.stack.is_none());
    }

    #[test]
    fn error_context_with_frame_n_recomputes_fingerprint() {
        let p = h().hash_str("/file.rs");
        let loc = SourceLocation::new(p, 10, 1);
        let ctx0 = ErrorContext::minimal(
            loc,
            SubsystemTag::Render,
            "cssl-render-v2",
            KindId::new(1),
            Severity::Error,
        );
        let fp0 = ctx0.fingerprint;
        // Same bucket : same fp.
        let ctx1 = ctx0.clone().with_frame_n(15);
        assert_eq!(ctx1.fingerprint, fp0);
        // Different bucket : different fp.
        let ctx2 = ctx0.with_frame_n(120);
        assert_ne!(ctx2.fingerprint, fp0);
    }

    #[test]
    fn error_context_with_retryable() {
        let p = h().hash_str("/f.rs");
        let loc = SourceLocation::new(p, 1, 1);
        let ctx = ErrorContext::minimal(
            loc,
            SubsystemTag::Engine,
            "cssl-engine",
            KindId::new(3),
            Severity::Warning,
        )
        .with_retryable(Retryable::Yes);
        assert_eq!(ctx.retryable, Retryable::Yes);
    }

    #[test]
    fn error_context_default_is_unknown() {
        let ctx = ErrorContext::default();
        assert!(ctx.source.is_unknown());
        assert_eq!(ctx.subsystem, SubsystemTag::Other);
        assert_eq!(ctx.kind, KindId::UNKNOWN);
        assert_eq!(ctx.severity, Severity::Error);
    }

    #[test]
    fn subsystem_tag_canonical_table_byte_pinned() {
        // Wire-format pin : byte-values frozen.
        assert_eq!(SubsystemTag::Engine.as_u8(), 0);
        assert_eq!(SubsystemTag::Telemetry.as_u8(), 17);
        assert_eq!(SubsystemTag::PrimeDirective.as_u8(), 19);
        assert_eq!(SubsystemTag::Other.as_u8(), 25);
    }
}
