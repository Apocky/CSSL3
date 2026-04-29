//! [`EngineError`] — workspace-unified error sum-type.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.2 + § 1.3 + § 1.7.
//!
//! § DESIGN
//!   - Closed-set aggregator over per-crate `*Error` types (§ 1.2).
//!   - Variants ordered by frequency for branch-prediction friendliness :
//!     Render > Wave > Anim > Physics > ... > Other. New variants appended
//!     at the tail to preserve wire-format compatibility.
//!   - `non_exhaustive` so adding variants is non-breaking for downstream.
//!   - The `From<T>` impls live in this crate so per-crate-error owners
//!     do NOT depend on `cssl-error` (avoids dep-cycle ; § 1.1 spec).
//!   - For per-crate errors that are NOT yet typed-into a dedicated variant
//!     (substrate / per-stage), use [`EngineError::CrateError`] which
//!     carries the crate-name + display-text + severity.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - § 1 surveillance : no raw-paths in error messages ; per-crate
//!     errors that touch paths use [`cssl_telemetry::PathHash`] internally.
//!   - § 7 INTEGRITY : the [`EngineError::PrimeDirective`] variant is ALWAYS
//!     [`Severity::Fatal`] ; this is enforced in [`Severable::severity`].

use core::fmt;

use thiserror::Error;

use crate::context::{ErrorContext, KindId, SourceLocation, SubsystemTag};
use crate::pd::PrimeDirectiveViolation;
use crate::severity::{Severable, Severity};

// ───────────────────────────────────────────────────────────────────────
// § IO error kind — typed wrapper around std::io::ErrorKind.
// ───────────────────────────────────────────────────────────────────────

/// Typed I/O error kind (subset of `std::io::ErrorKind` we classify).
///
/// § DESIGN
///   - Stable u8 discriminants : wire-format pin.
///   - `retryable()` predicate : the canonical "should caller retry?" hint.
///   - We deliberately do NOT impl `From<std::io::ErrorKind>` to keep the
///     coupling explicit (caller chooses the mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum IoErrorKind {
    /// File not found (e.g., `ENOENT`). NON-retryable.
    NotFound = 0,
    /// Permission denied (e.g., `EACCES`). NON-retryable.
    PermissionDenied = 1,
    /// Connection refused. RETRYABLE (server may come up).
    ConnectionRefused = 2,
    /// Connection reset. RETRYABLE.
    ConnectionReset = 3,
    /// Connection aborted. NON-retryable (peer terminated).
    ConnectionAborted = 4,
    /// Resource is currently busy. RETRYABLE.
    WouldBlock = 5,
    /// Operation timed out. RETRYABLE.
    TimedOut = 6,
    /// Unexpected EOF. NON-retryable.
    UnexpectedEof = 7,
    /// Invalid input data. NON-retryable.
    InvalidData = 8,
    /// Out of memory / disk full. NON-retryable.
    OutOfStorage = 9,
    /// Broken pipe. NON-retryable.
    BrokenPipe = 10,
    /// Other / unclassified.
    Other = 11,
}

impl IoErrorKind {
    /// Stable canonical name (snake_case).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::ConnectionRefused => "connection_refused",
            Self::ConnectionReset => "connection_reset",
            Self::ConnectionAborted => "connection_aborted",
            Self::WouldBlock => "would_block",
            Self::TimedOut => "timed_out",
            Self::UnexpectedEof => "unexpected_eof",
            Self::InvalidData => "invalid_data",
            Self::OutOfStorage => "out_of_storage",
            Self::BrokenPipe => "broken_pipe",
            Self::Other => "other",
        }
    }

    /// Should the caller retry this op?
    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::ConnectionRefused | Self::ConnectionReset | Self::WouldBlock | Self::TimedOut
        )
    }

    /// All variants in canonical order.
    #[must_use]
    pub const fn all() -> &'static [IoErrorKind] {
        &[
            Self::NotFound,
            Self::PermissionDenied,
            Self::ConnectionRefused,
            Self::ConnectionReset,
            Self::ConnectionAborted,
            Self::WouldBlock,
            Self::TimedOut,
            Self::UnexpectedEof,
            Self::InvalidData,
            Self::OutOfStorage,
            Self::BrokenPipe,
            Self::Other,
        ]
    }

    /// Map a `std::io::ErrorKind` into our typed enum. NON-exhaustive ; the
    /// caller chooses to use this mapping.
    #[must_use]
    pub fn from_std(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::ConnectionRefused => Self::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset => Self::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted => Self::ConnectionAborted,
            std::io::ErrorKind::WouldBlock => Self::WouldBlock,
            std::io::ErrorKind::TimedOut => Self::TimedOut,
            std::io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            std::io::ErrorKind::InvalidData => Self::InvalidData,
            std::io::ErrorKind::BrokenPipe => Self::BrokenPipe,
            std::io::ErrorKind::OutOfMemory => Self::OutOfStorage,
            _ => Self::Other,
        }
    }
}

impl fmt::Display for IoErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical_name())
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Per-crate error kind — opaque payload for crates that haven't yet
//   typed their error into a dedicated EngineError variant.
// ───────────────────────────────────────────────────────────────────────

/// Opaque carrier for a per-crate error that has NOT yet been promoted to
/// a dedicated [`EngineError`] variant.
///
/// § DESIGN
///   - Carries the crate-name (compile-time `&'static str`) + display-text +
///     severity hint. Suitable for the long tail of per-crate errors that
///     don't yet warrant their own variant.
///   - Replaceable : a follow-up slice can promote frequently-used crates
///     to dedicated variants without breaking existing call-sites (the
///     `From<T>` impl simply switches from `CrateError`-construction to the
///     dedicated variant).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrateErrorPayload {
    /// Source crate name (compile-time const ; no allocation).
    pub crate_name: &'static str,
    /// Display-text of the original error.
    pub message: String,
    /// Severity hint provided by the source crate.
    pub severity: Severity,
}

impl CrateErrorPayload {
    /// Construct a [`CrateErrorPayload`].
    #[must_use]
    pub fn new(crate_name: &'static str, message: impl Into<String>, severity: Severity) -> Self {
        Self {
            crate_name,
            message: message.into(),
            severity,
        }
    }

    /// Construct from any `Display`-able error with default severity (Error).
    pub fn from_display<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self {
            crate_name,
            message: err.to_string(),
            severity: Severity::Error,
        }
    }
}

impl fmt::Display for CrateErrorPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.crate_name, self.message)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § PanicReport — payload for Engine-caught panics.
// ───────────────────────────────────────────────────────────────────────

/// Structured payload for a caught panic. Constructed by the panic-hook
/// + frame-boundary catch helpers in [`crate::panic`].
#[derive(Debug, Clone)]
pub struct PanicReport {
    /// Display-text of the panic payload (extracted from `&dyn Any`).
    pub message: String,
    /// Subsystem-tag where the panic originated (caller-supplied).
    pub subsystem: SubsystemTag,
    /// Optional source location of the panic-site.
    pub source: Option<SourceLocation>,
    /// Optional stack-trace captured @ panic-site.
    pub stack: Option<crate::stack::StackTrace>,
    /// Frame number @ which the panic occurred.
    pub frame_n: u64,
    /// Whether the panic payload tagged itself as a PD-violation.
    pub pd_violation: bool,
}

impl PanicReport {
    /// Construct a minimal [`PanicReport`].
    #[must_use]
    pub fn new(message: impl Into<String>, subsystem: SubsystemTag) -> Self {
        Self {
            message: message.into(),
            subsystem,
            source: None,
            stack: None,
            frame_n: 0,
            pd_violation: false,
        }
    }

    /// Set the source location.
    #[must_use]
    pub fn with_source(mut self, source: SourceLocation) -> Self {
        self.source = Some(source);
        self
    }

    /// Attach a stack-trace.
    #[must_use]
    pub fn with_stack(mut self, stack: crate::stack::StackTrace) -> Self {
        self.stack = Some(stack);
        self
    }

    /// Set the frame number.
    #[must_use]
    pub fn with_frame_n(mut self, frame_n: u64) -> Self {
        self.frame_n = frame_n;
        self
    }

    /// Tag this panic as a PD-violation. Flowing through the panic-hook
    /// will fire the kill-switch.
    #[must_use]
    pub fn with_pd_violation(mut self, pd_violation: bool) -> Self {
        self.pd_violation = pd_violation;
        self
    }

    /// Returns `true` if this panic is tagged as a PD-violation.
    #[must_use]
    pub const fn is_pd_violation(&self) -> bool {
        self.pd_violation
    }
}

impl fmt::Display for PanicReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pd_violation {
            write!(
                f,
                "PD-VIOLATION-PANIC[{}] @ frame {} : {}",
                self.subsystem, self.frame_n, self.message
            )
        } else {
            write!(
                f,
                "PANIC[{}] @ frame {} : {}",
                self.subsystem, self.frame_n, self.message
            )
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § EngineError — the workspace-unified sum-type.
// ───────────────────────────────────────────────────────────────────────

/// Workspace-unified error sum-type.
///
/// § VARIANT-ORDERING
///   Variants are ordered by expected-frequency in the engine hot-loop
///   (Render > Audio > Physics > Anim > Codegen > Asset > Effects > Telemetry
///   > Audit > PathLog > Io > Crate > Panic > PrimeDirective > Other).
///
/// § NON-EXHAUSTIVE
///   Marked `non_exhaustive` so adding new variants is non-breaking for
///   downstream `match` expressions. Callers should always include a
///   wildcard arm.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EngineError {
    /// Render-pipeline error (e.g., shader-compile failure, GPU OOM).
    #[error("[render] {0}")]
    Render(CrateErrorPayload),

    /// Wave-physics / wave-solver error.
    #[error("[wave] {0}")]
    Wave(CrateErrorPayload),

    /// Wave-audio / audio-mix error.
    #[error("[audio] {0}")]
    Audio(CrateErrorPayload),

    /// Physics (SDF + XPBD) error.
    #[error("[physics] {0}")]
    Physics(CrateErrorPayload),

    /// Animation pipeline error (anim-procedural / blend-tree).
    #[error("[anim] {0}")]
    Anim(CrateErrorPayload),

    /// Code-generation error (cgen-cpu-x64 / cgen-cpu-cranelift / cgen-gpu-*).
    #[error("[codegen] {0}")]
    Codegen(CrateErrorPayload),

    /// Asset pipeline error.
    #[error("[asset] {0}")]
    Asset(CrateErrorPayload),

    /// Effects / effect-row-discipline error.
    #[error("[effects] {0}")]
    Effects(CrateErrorPayload),

    /// Work-graph / pipeline-scheduler error.
    #[error("[work_graph] {0}")]
    WorkGraph(CrateErrorPayload),

    /// AI behavior / utility / sensor error.
    #[error("[ai] {0}")]
    Ai(CrateErrorPayload),

    /// Gaze-collapse / foveation error.
    #[error("[gaze] {0}")]
    Gaze(CrateErrorPayload),

    /// Host-platform error (Level-Zero / Vulkan / D3D12 / Metal / WebGPU /
    /// OpenXR / window / input).
    #[error("[host] {0}")]
    Host(CrateErrorPayload),

    /// Network error.
    #[error("[network] {0}")]
    Network(CrateErrorPayload),

    /// Telemetry-ring failure (overflow / drop).
    #[error("[telemetry] {0}")]
    Telemetry(#[from] cssl_telemetry::RingError),

    /// Audit-chain failure.
    #[error("[audit] {0}")]
    Audit(#[from] cssl_telemetry::AuditError),

    /// Path-log discipline violation (raw-path detected ; D130).
    #[error("[pathlog] {0}")]
    PathLog(#[from] cssl_telemetry::PathLogError),

    /// I/O failure with retryability hint.
    #[error("[io] {kind} (retryable={retryable})")]
    Io {
        /// Typed I/O error kind.
        kind: IoErrorKind,
        /// Hint : may caller retry ?
        retryable: bool,
    },

    /// Untyped per-crate error ; carries crate-name + display-text.
    /// Promote to a dedicated variant when frequency warrants.
    #[error("{0}")]
    CrateError(CrateErrorPayload),

    /// Frame-boundary-caught panic with structured report.
    #[error("[panic] {0}")]
    Panic(PanicReport),

    /// PRIME-DIRECTIVE violation. ALWAYS [`Severity::Fatal`] ; fires
    /// kill-switch via [`crate::pd::halt_for_pd_violation`].
    #[error("[pd] {0}")]
    PrimeDirective(PrimeDirectiveViolation),

    /// Genuinely unmapped / third-party FFI error. Permitted but discouraged ;
    /// a follow-up slice should promote to a dedicated variant.
    #[error("[other] {0}")]
    Other(String),
}

impl EngineError {
    /// Stable [`KindId`] for fingerprinting + dedup.
    ///
    /// § ENCODING (canonical reservation)
    ///   1   Render
    ///   2   Wave
    ///   3   Audio
    ///   4   Physics
    ///   5   Anim
    ///   6   Codegen
    ///   7   Asset
    ///   8   Effects
    ///   9   WorkGraph
    ///   10  Ai
    ///   11  Gaze
    ///   12  Host
    ///   13  Network
    ///   14  Telemetry
    ///   15  Audit
    ///   16  PathLog
    ///   17  Io
    ///   18  CrateError
    ///   19  Panic
    ///   20  PrimeDirective
    ///   21  Other
    #[must_use]
    pub fn kind_id(&self) -> KindId {
        let id = match self {
            Self::Render(_) => 1,
            Self::Wave(_) => 2,
            Self::Audio(_) => 3,
            Self::Physics(_) => 4,
            Self::Anim(_) => 5,
            Self::Codegen(_) => 6,
            Self::Asset(_) => 7,
            Self::Effects(_) => 8,
            Self::WorkGraph(_) => 9,
            Self::Ai(_) => 10,
            Self::Gaze(_) => 11,
            Self::Host(_) => 12,
            Self::Network(_) => 13,
            Self::Telemetry(_) => 14,
            Self::Audit(_) => 15,
            Self::PathLog(_) => 16,
            Self::Io { .. } => 17,
            Self::CrateError(_) => 18,
            Self::Panic(_) => 19,
            Self::PrimeDirective(_) => 20,
            Self::Other(_) => 21,
        };
        KindId::new(id)
    }

    /// Subsystem-tag classification : maps each variant to its canonical
    /// [`SubsystemTag`]. Used for log-routing + audit-tagging.
    ///
    /// § ALLOW match-same-arms : `Network` + `Io` both legitimately route
    ///   to `Host` subsystem (catalog has no separate Network/Io tag) ;
    ///   `PathLog` correctly aliases to `Telemetry`. The duplicate arms are
    ///   intentional and document the spec-mapping verbatim.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub fn subsystem(&self) -> SubsystemTag {
        match self {
            Self::Render(_) => SubsystemTag::Render,
            Self::Wave(_) => SubsystemTag::WaveSolver,
            Self::Audio(_) => SubsystemTag::Audio,
            Self::Physics(_) => SubsystemTag::Physics,
            Self::Anim(_) => SubsystemTag::Anim,
            Self::Codegen(_) => SubsystemTag::Codegen,
            Self::Asset(_) => SubsystemTag::Asset,
            Self::Effects(_) => SubsystemTag::Effects,
            Self::WorkGraph(_) => SubsystemTag::WorkGraph,
            Self::Ai(_) => SubsystemTag::Ai,
            Self::Gaze(_) => SubsystemTag::Gaze,
            Self::Host(_) => SubsystemTag::Host,
            Self::Network(_) => SubsystemTag::Host,
            Self::Telemetry(_) => SubsystemTag::Telemetry,
            Self::Audit(_) => SubsystemTag::Audit,
            Self::PathLog(_) => SubsystemTag::Telemetry,
            Self::Io { .. } => SubsystemTag::Host,
            Self::CrateError(p) => crate_name_to_subsystem(p.crate_name),
            Self::Panic(r) => r.subsystem,
            Self::PrimeDirective(_) => SubsystemTag::PrimeDirective,
            Self::Other(_) => SubsystemTag::Other,
        }
    }

    /// Construct an [`ErrorContext`] for this error using the supplied
    /// source-loc + crate-name + frame-n. The kind + severity + subsystem
    /// are derived from the variant.
    #[must_use]
    pub fn make_context(
        &self,
        source: SourceLocation,
        crate_name: &'static str,
        frame_n: u64,
    ) -> ErrorContext {
        ErrorContext::minimal(
            source,
            self.subsystem(),
            crate_name,
            self.kind_id(),
            self.severity(),
        )
        .with_frame_n(frame_n)
    }

    /// Construct a `Render` variant from any `Display`-able error.
    pub fn render<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Render(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct a `Wave` variant from any `Display`-able error.
    pub fn wave<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Wave(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct an `Audio` variant from any `Display`-able error.
    pub fn audio<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Audio(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct a `Physics` variant from any `Display`-able error.
    pub fn physics<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Physics(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct an `Anim` variant from any `Display`-able error.
    pub fn anim<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Anim(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct a `Codegen` variant from any `Display`-able error.
    pub fn codegen<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Codegen(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct an `Asset` variant from any `Display`-able error.
    pub fn asset<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Asset(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct an `Effects` variant from any `Display`-able error.
    pub fn effects<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Effects(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct a `WorkGraph` variant from any `Display`-able error.
    pub fn work_graph<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::WorkGraph(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct an `Ai` variant from any `Display`-able error.
    pub fn ai<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Ai(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct a `Gaze` variant from any `Display`-able error.
    pub fn gaze<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Gaze(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct a `Host` variant from any `Display`-able error.
    pub fn host<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Host(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct a `Network` variant from any `Display`-able error.
    pub fn network<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::Network(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Construct an `Io` variant ; the `retryable` flag is derived from
    /// [`IoErrorKind::retryable`].
    #[must_use]
    pub fn io(kind: IoErrorKind) -> Self {
        Self::Io {
            kind,
            retryable: kind.retryable(),
        }
    }

    /// Construct from a `std::io::Error`, lifting via [`IoErrorKind::from_std`].
    #[must_use]
    pub fn from_io(err: std::io::Error) -> Self {
        Self::io(IoErrorKind::from_std(err.kind()))
    }

    /// Generic per-crate-error catcher : opaque payload variant.
    pub fn from_crate_err<E: fmt::Display>(crate_name: &'static str, err: E) -> Self {
        Self::CrateError(CrateErrorPayload::from_display(crate_name, err))
    }

    /// Generic per-crate-error catcher with explicit severity.
    pub fn from_crate_err_with_severity<E: fmt::Display>(
        crate_name: &'static str,
        err: E,
        severity: Severity,
    ) -> Self {
        Self::CrateError(CrateErrorPayload::new(
            crate_name,
            err.to_string(),
            severity,
        ))
    }

    /// Construct a free-form `Other(String)` ; permitted but lint-discouraged.
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    /// Returns `true` if this variant is a PRIME-DIRECTIVE violation.
    /// Used by the panic-hook + frame-catch helpers to detect halt-paths.
    #[must_use]
    pub const fn is_prime_directive_violation(&self) -> bool {
        matches!(self, Self::PrimeDirective(_))
    }

    /// Returns `true` if this variant is a panic-report.
    #[must_use]
    pub const fn is_panic(&self) -> bool {
        matches!(self, Self::Panic(_))
    }
}

/// Map a crate-name to its canonical subsystem-tag. Used for the opaque
/// [`EngineError::CrateError`] variant.
fn crate_name_to_subsystem(crate_name: &str) -> SubsystemTag {
    match crate_name {
        // Render-pipeline crates.
        n if n.starts_with("cssl-render") || n == "cssl-spectral-render" => SubsystemTag::Render,
        // Wave-related crates.
        n if n.starts_with("cssl-wave") => SubsystemTag::WaveSolver,
        // Audio.
        "cssl-host-audio" | "cssl-audio-mix" => SubsystemTag::Audio,
        // Anim.
        n if n.starts_with("cssl-anim") => SubsystemTag::Anim,
        // Physics.
        n if n.starts_with("cssl-physics") => SubsystemTag::Physics,
        // Codegen.
        n if n.starts_with("cssl-cgen") => SubsystemTag::Codegen,
        // Asset.
        "cssl-asset" => SubsystemTag::Asset,
        // Effects.
        "cssl-effects" => SubsystemTag::Effects,
        // Work-graph.
        "cssl-work-graph" => SubsystemTag::WorkGraph,
        // AI.
        n if n.starts_with("cssl-ai") => SubsystemTag::Ai,
        // Gaze.
        "cssl-gaze-collapse" => SubsystemTag::Gaze,
        // Host crates.
        n if n.starts_with("cssl-host") => SubsystemTag::Host,
        // Telemetry.
        "cssl-telemetry" => SubsystemTag::Telemetry,
        // PD enforcement.
        "cssl-substrate-prime-directive" => SubsystemTag::PrimeDirective,
        // Engine.
        "cssl-engine" | "loa-game" => SubsystemTag::Engine,
        _ => SubsystemTag::Other,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Severable impl for EngineError.
// ───────────────────────────────────────────────────────────────────────

impl Severable for EngineError {
    /// § INVARIANT
    ///   - `PrimeDirective(_) ⟶ Severity::Fatal` : ALWAYS, no override.
    ///   - `Audit(_) ⟶ Severity::Fatal` : audit-chain integrity is critical.
    ///   - `Panic(_) ⟶ Severity::Error` (default) : panic-catch keeps engine
    ///     running unless tagged as PD-violation (then Fatal).
    ///   - `Telemetry(RingError::Overflow) ⟶ Severity::Warning` : lossy by design.
    ///   - `PathLog(_) ⟶ Severity::Warning` : recoverable discipline-violation.
    ///   - All other variants ⟶ severity from carried payload (default Error).
    ///
    /// § ALLOW match-same-arms : `PrimeDirective` + `Audit` both yield `Fatal`
    ///   ; the duplicate body is intentional (each is a distinct invariant).
    #[allow(clippy::match_same_arms)]
    fn severity(&self) -> Severity {
        match self {
            Self::PrimeDirective(_) => Severity::Fatal,
            Self::Audit(_) => Severity::Fatal,
            Self::Panic(r) => {
                if r.is_pd_violation() {
                    Severity::Fatal
                } else {
                    Severity::Error
                }
            }
            Self::Telemetry(r) => r.severity(),
            Self::PathLog(p) => p.severity(),
            Self::Render(p)
            | Self::Wave(p)
            | Self::Audio(p)
            | Self::Physics(p)
            | Self::Anim(p)
            | Self::Codegen(p)
            | Self::Asset(p)
            | Self::Effects(p)
            | Self::WorkGraph(p)
            | Self::Ai(p)
            | Self::Gaze(p)
            | Self::Host(p)
            | Self::Network(p)
            | Self::CrateError(p) => p.severity,
            Self::Io { retryable, .. } => {
                if *retryable {
                    Severity::Warning
                } else {
                    Severity::Error
                }
            }
            Self::Other(_) => Severity::Error,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § From-impl convenience for std::io::Error.
// ───────────────────────────────────────────────────────────────────────

impl From<std::io::Error> for EngineError {
    fn from(err: std::io::Error) -> Self {
        Self::from_io(err)
    }
}

impl From<PrimeDirectiveViolation> for EngineError {
    fn from(v: PrimeDirectiveViolation) -> Self {
        Self::PrimeDirective(v)
    }
}

impl From<PanicReport> for EngineError {
    fn from(r: PanicReport) -> Self {
        Self::Panic(r)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        crate_name_to_subsystem, CrateErrorPayload, EngineError, IoErrorKind, PanicReport,
    };
    use crate::context::SubsystemTag;
    use crate::pd::PrimeDirectiveViolation;
    use crate::severity::{Severable, Severity};

    #[test]
    fn io_kind_canonical_names_unique() {
        let mut names: Vec<&str> = IoErrorKind::all()
            .iter()
            .map(|k| k.canonical_name())
            .collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }

    #[test]
    fn io_kind_retryable_classification() {
        assert!(IoErrorKind::ConnectionRefused.retryable());
        assert!(IoErrorKind::ConnectionReset.retryable());
        assert!(IoErrorKind::WouldBlock.retryable());
        assert!(IoErrorKind::TimedOut.retryable());
        assert!(!IoErrorKind::NotFound.retryable());
        assert!(!IoErrorKind::PermissionDenied.retryable());
        assert!(!IoErrorKind::InvalidData.retryable());
    }

    #[test]
    fn io_kind_from_std_maps_known_kinds() {
        assert_eq!(
            IoErrorKind::from_std(std::io::ErrorKind::NotFound),
            IoErrorKind::NotFound
        );
        assert_eq!(
            IoErrorKind::from_std(std::io::ErrorKind::PermissionDenied),
            IoErrorKind::PermissionDenied
        );
        assert_eq!(
            IoErrorKind::from_std(std::io::ErrorKind::TimedOut),
            IoErrorKind::TimedOut
        );
    }

    #[test]
    fn io_kind_from_std_unknown_maps_to_other() {
        // A kind we don't enumerate explicitly maps to Other.
        assert_eq!(
            IoErrorKind::from_std(std::io::ErrorKind::AddrInUse),
            IoErrorKind::Other
        );
    }

    #[test]
    fn engine_error_render_constructor() {
        let e = EngineError::render("cssl-render-v2", "shader-compile-failed");
        assert_eq!(e.subsystem(), SubsystemTag::Render);
        assert!(format!("{e}").contains("[render]"));
        assert!(format!("{e}").contains("shader-compile-failed"));
    }

    #[test]
    fn engine_error_kind_ids_unique() {
        let variants = vec![
            EngineError::render("c", "x"),
            EngineError::wave("c", "x"),
            EngineError::audio("c", "x"),
            EngineError::physics("c", "x"),
            EngineError::anim("c", "x"),
            EngineError::codegen("c", "x"),
            EngineError::asset("c", "x"),
            EngineError::effects("c", "x"),
            EngineError::work_graph("c", "x"),
            EngineError::ai("c", "x"),
            EngineError::gaze("c", "x"),
            EngineError::host("c", "x"),
            EngineError::network("c", "x"),
            EngineError::Telemetry(cssl_telemetry::RingError::Overflow),
            EngineError::Audit(cssl_telemetry::AuditError::SignatureInvalid),
            EngineError::PathLog(cssl_telemetry::PathLogError::RawPathInField {
                field: "x".into(),
            }),
            EngineError::io(IoErrorKind::NotFound),
            EngineError::from_crate_err("c", "x"),
            EngineError::Panic(PanicReport::new("p", SubsystemTag::Render)),
            EngineError::PrimeDirective(PrimeDirectiveViolation::new("PD0001", "test")),
            EngineError::other("misc"),
        ];
        let mut ids: Vec<u32> = variants.iter().map(|e| e.kind_id().as_u32()).collect();
        ids.sort_unstable();
        let original = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), original);
    }

    #[test]
    fn engine_error_pd_severity_is_fatal() {
        let e = EngineError::PrimeDirective(PrimeDirectiveViolation::new("PD0001", "harm"));
        assert_eq!(e.severity(), Severity::Fatal);
    }

    #[test]
    fn engine_error_audit_severity_is_fatal() {
        let e = EngineError::Audit(cssl_telemetry::AuditError::ChainBreak { seq: 7 });
        assert_eq!(e.severity(), Severity::Fatal);
    }

    #[test]
    fn engine_error_panic_default_is_error() {
        let e = EngineError::Panic(PanicReport::new("oops", SubsystemTag::Render));
        assert_eq!(e.severity(), Severity::Error);
    }

    #[test]
    fn engine_error_panic_pd_violation_is_fatal() {
        let r = PanicReport::new("oops", SubsystemTag::Render).with_pd_violation(true);
        let e = EngineError::Panic(r);
        assert_eq!(e.severity(), Severity::Fatal);
    }

    #[test]
    fn engine_error_telemetry_overflow_is_warning() {
        let e = EngineError::Telemetry(cssl_telemetry::RingError::Overflow);
        assert_eq!(e.severity(), Severity::Warning);
    }

    #[test]
    fn engine_error_path_log_is_warning() {
        let e = EngineError::PathLog(cssl_telemetry::PathLogError::RawPathInField {
            field: "x".into(),
        });
        assert_eq!(e.severity(), Severity::Warning);
    }

    #[test]
    fn engine_error_io_retryable_is_warning() {
        let e = EngineError::io(IoErrorKind::TimedOut);
        assert_eq!(e.severity(), Severity::Warning);
    }

    #[test]
    fn engine_error_io_terminal_is_error() {
        let e = EngineError::io(IoErrorKind::NotFound);
        assert_eq!(e.severity(), Severity::Error);
    }

    #[test]
    fn engine_error_from_io_lifts() {
        let std_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
        let e: EngineError = std_err.into();
        match e {
            EngineError::Io { kind, retryable } => {
                assert_eq!(kind, IoErrorKind::TimedOut);
                assert!(retryable);
            }
            _ => panic!("expected Io variant"),
        }
    }

    #[test]
    fn engine_error_from_telemetry_overflow() {
        let r: cssl_telemetry::RingError = cssl_telemetry::RingError::Overflow;
        let e: EngineError = r.into();
        assert_eq!(e.subsystem(), SubsystemTag::Telemetry);
    }

    #[test]
    fn engine_error_from_audit_signature_invalid() {
        let a: cssl_telemetry::AuditError = cssl_telemetry::AuditError::SignatureInvalid;
        let e: EngineError = a.into();
        assert_eq!(e.subsystem(), SubsystemTag::Audit);
    }

    #[test]
    fn engine_error_from_path_log_raw_path() {
        let p: cssl_telemetry::PathLogError =
            cssl_telemetry::PathLogError::RawPathInField { field: "x".into() };
        let e: EngineError = p.into();
        assert_eq!(e.subsystem(), SubsystemTag::Telemetry);
    }

    #[test]
    fn engine_error_from_pd_violation() {
        let v = PrimeDirectiveViolation::new("PD0001", "test");
        let e: EngineError = v.into();
        assert!(e.is_prime_directive_violation());
    }

    #[test]
    fn engine_error_from_panic_report() {
        let r = PanicReport::new("oops", SubsystemTag::Render);
        let e: EngineError = r.into();
        assert!(e.is_panic());
    }

    #[test]
    fn engine_error_from_crate_err_routes_to_subsystem() {
        let e = EngineError::from_crate_err("cssl-render-v2", "x");
        assert_eq!(e.subsystem(), SubsystemTag::Render);
    }

    #[test]
    fn engine_error_from_crate_err_with_severity() {
        let e = EngineError::from_crate_err_with_severity("cssl-anim", "x", Severity::Warning);
        assert_eq!(e.severity(), Severity::Warning);
    }

    #[test]
    fn engine_error_other_constructor() {
        let e = EngineError::other("misc");
        assert_eq!(e.subsystem(), SubsystemTag::Other);
        assert_eq!(e.severity(), Severity::Error);
    }

    #[test]
    fn engine_error_make_context_records_fields() {
        use crate::context::SourceLocation;
        let p = cssl_telemetry::PathHasher::from_seed([1u8; 32]).hash_str("/file.rs");
        let loc = SourceLocation::new(p, 7, 3);
        let e = EngineError::render("cssl-render-v2", "x");
        let ctx = e.make_context(loc, "cssl-render-v2", 100);
        assert_eq!(ctx.subsystem, SubsystemTag::Render);
        assert_eq!(ctx.frame_n, 100);
        assert_eq!(ctx.crate_name, "cssl-render-v2");
        assert_eq!(ctx.severity, Severity::Error);
    }

    #[test]
    fn crate_payload_display_includes_crate_name() {
        let p = CrateErrorPayload::new("cssl-test", "msg", Severity::Warning);
        let s = format!("{p}");
        assert!(s.contains("cssl-test"));
        assert!(s.contains("msg"));
    }

    #[test]
    fn crate_payload_from_display_default_severity_error() {
        let p = CrateErrorPayload::from_display("cssl-test", "msg");
        assert_eq!(p.severity, Severity::Error);
    }

    #[test]
    fn panic_report_display_humanized() {
        let r = PanicReport::new("oops", SubsystemTag::Render);
        let s = format!("{r}");
        assert!(s.contains("PANIC"));
        assert!(s.contains("oops"));
    }

    #[test]
    fn panic_report_pd_violation_display_distinguished() {
        let r = PanicReport::new("oops", SubsystemTag::Render).with_pd_violation(true);
        let s = format!("{r}");
        assert!(s.contains("PD-VIOLATION-PANIC"));
    }

    #[test]
    fn panic_report_builder_chain() {
        use crate::context::SourceLocation;
        let p = cssl_telemetry::PathHasher::from_seed([1u8; 32]).hash_str("/x.rs");
        let loc = SourceLocation::new(p, 1, 1);
        let r = PanicReport::new("x", SubsystemTag::Render)
            .with_source(loc)
            .with_frame_n(42)
            .with_pd_violation(true);
        assert!(r.source.is_some());
        assert_eq!(r.frame_n, 42);
        assert!(r.is_pd_violation());
    }

    #[test]
    fn crate_name_to_subsystem_render() {
        assert_eq!(
            crate_name_to_subsystem("cssl-render-v2"),
            SubsystemTag::Render
        );
        assert_eq!(
            crate_name_to_subsystem("cssl-spectral-render"),
            SubsystemTag::Render
        );
    }

    #[test]
    fn crate_name_to_subsystem_audio() {
        assert_eq!(
            crate_name_to_subsystem("cssl-host-audio"),
            SubsystemTag::Audio
        );
        assert_eq!(
            crate_name_to_subsystem("cssl-audio-mix"),
            SubsystemTag::Audio
        );
    }

    #[test]
    fn crate_name_to_subsystem_anim() {
        assert_eq!(crate_name_to_subsystem("cssl-anim"), SubsystemTag::Anim);
        assert_eq!(
            crate_name_to_subsystem("cssl-anim-procedural"),
            SubsystemTag::Anim
        );
    }

    #[test]
    fn crate_name_to_subsystem_physics() {
        assert_eq!(
            crate_name_to_subsystem("cssl-physics"),
            SubsystemTag::Physics
        );
        assert_eq!(
            crate_name_to_subsystem("cssl-physics-wave"),
            SubsystemTag::Physics
        );
    }

    #[test]
    fn crate_name_to_subsystem_codegen() {
        assert_eq!(
            crate_name_to_subsystem("cssl-cgen-cpu-x64"),
            SubsystemTag::Codegen
        );
        assert_eq!(
            crate_name_to_subsystem("cssl-cgen-gpu-spirv"),
            SubsystemTag::Codegen
        );
    }

    #[test]
    fn crate_name_to_subsystem_unknown_is_other() {
        assert_eq!(
            crate_name_to_subsystem("not-a-real-crate"),
            SubsystemTag::Other
        );
        assert_eq!(crate_name_to_subsystem(""), SubsystemTag::Other);
    }

    #[test]
    fn crate_name_to_subsystem_engine() {
        assert_eq!(crate_name_to_subsystem("cssl-engine"), SubsystemTag::Engine);
        assert_eq!(crate_name_to_subsystem("loa-game"), SubsystemTag::Engine);
    }

    #[test]
    fn engine_error_is_pd_predicate() {
        let v = PrimeDirectiveViolation::new("PD0001", "x");
        assert!(EngineError::PrimeDirective(v).is_prime_directive_violation());
        assert!(!EngineError::other("x").is_prime_directive_violation());
    }

    #[test]
    fn engine_error_is_panic_predicate() {
        assert!(EngineError::Panic(PanicReport::new("x", SubsystemTag::Render)).is_panic());
        assert!(!EngineError::other("x").is_panic());
    }

    #[test]
    fn engine_error_display_for_telemetry_overflow_humanized() {
        let e = EngineError::Telemetry(cssl_telemetry::RingError::Overflow);
        let s = format!("{e}");
        assert!(s.contains("[telemetry]"));
        assert!(s.contains("ring-buffer full"));
    }

    #[test]
    fn engine_error_display_for_io_humanized() {
        let e = EngineError::io(IoErrorKind::TimedOut);
        let s = format!("{e}");
        assert!(s.contains("[io]"));
        assert!(s.contains("timed_out"));
        assert!(s.contains("retryable=true"));
    }
}
