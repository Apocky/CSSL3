//! Tunable type-system : `TunableId`, `TunableKind`, `TunableValue`,
//! `TunableRange`, `TunableSpec`, `BudgetMode`, and the `TweakError` taxonomy.
//!
//! Per `_drafts/phase_j/07_l3_l4_inspect_hotreload_spec.md` § 4.2 + § 4.7, the
//! tunable registry is type-erased via the [`TunableValue`] enum (chosen over
//! `Box<dyn Any>` for predictable size, no allocation, and easy serialization
//! for replay). Every spec advertises its [`TunableKind`] so the registry can
//! type-check writes without touching the value's concrete representation.

use core::ops::Range;

use thiserror::Error;

// ─── TunableId ─────────────────────────────────────────────────────────────────

/// Stable handle to a registered tunable.
///
/// In production builds this is the BLAKE3-hash of the canonical name (see
/// spec § 4.2). For the T11-D164 stub, we keep the contract — opaque,
/// `Hash + Eq + Copy` — but compute it via FxHash so the crate has no
/// dependency on a crypto-grade hasher. The wire-format is intentionally
/// `u64` so a future spec migration can swap the hasher without breaking
/// downstream consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TunableId(pub u64);

impl TunableId {
    /// Compute the deterministic id of a canonical name.
    ///
    /// Uses a stable variant of the FNV-1a 64-bit hash so the value is
    /// reproducible across processes (BLAKE3 will replace this once the
    /// `cssl-audit` crate lands ; see spec § 4.2).
    #[must_use]
    pub const fn of(canonical_name: &str) -> Self {
        let bytes = canonical_name.as_bytes();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            i += 1;
        }
        Self(hash)
    }
}

// ─── TunableKind ───────────────────────────────────────────────────────────────

/// Type-tag advertised by every [`TunableSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TunableKind {
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
    /// 32-bit unsigned integer.
    U32,
    /// 64-bit unsigned integer.
    U64,
    /// 32-bit signed integer.
    I32,
    /// 64-bit signed integer.
    I64,
    /// Boolean flag.
    Bool,
    /// Closed enumeration of allowed string variants.
    StringEnum,
}

impl TunableKind {
    /// Stable kebab-case rendering for diagnostics + audit-log output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::F32 => "F32",
            Self::F64 => "F64",
            Self::U32 => "U32",
            Self::U64 => "U64",
            Self::I32 => "I32",
            Self::I64 => "I64",
            Self::Bool => "Bool",
            Self::StringEnum => "StringEnum",
        }
    }
}

// ─── TunableValue ──────────────────────────────────────────────────────────────

/// Type-erased payload carried through the tweak API.
///
/// Per the T11-D164 prompt's "type-erasure" landmine, we prefer a closed enum
/// over `Box<dyn Any>`. Every variant maps 1-to-1 with a [`TunableKind`] so
/// type-checking is a single match arm.
#[derive(Debug, Clone, PartialEq)]
pub enum TunableValue {
    /// 32-bit float payload.
    F32(f32),
    /// 64-bit float payload.
    F64(f64),
    /// 32-bit unsigned integer payload.
    U32(u32),
    /// 64-bit unsigned integer payload.
    U64(u64),
    /// 32-bit signed integer payload.
    I32(i32),
    /// 64-bit signed integer payload.
    I64(i64),
    /// Boolean flag payload.
    Bool(bool),
    /// String variant from the spec's allowed set.
    StringEnum(String),
}

impl TunableValue {
    /// Returns the [`TunableKind`] of this value.
    #[must_use]
    pub fn kind(&self) -> TunableKind {
        match self {
            Self::F32(_) => TunableKind::F32,
            Self::F64(_) => TunableKind::F64,
            Self::U32(_) => TunableKind::U32,
            Self::U64(_) => TunableKind::U64,
            Self::I32(_) => TunableKind::I32,
            Self::I64(_) => TunableKind::I64,
            Self::Bool(_) => TunableKind::Bool,
            Self::StringEnum(_) => TunableKind::StringEnum,
        }
    }

    /// Render the value as a stable string for audit + diagnostics.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::F32(v) => format!("{v}"),
            Self::F64(v) => format!("{v}"),
            Self::U32(v) => format!("{v}"),
            Self::U64(v) => format!("{v}"),
            Self::I32(v) => format!("{v}"),
            Self::I64(v) => format!("{v}"),
            Self::Bool(v) => format!("{v}"),
            Self::StringEnum(v) => v.clone(),
        }
    }
}

// ─── TunableRange ──────────────────────────────────────────────────────────────

/// Range constraint advertised by every [`TunableSpec`].
#[derive(Debug, Clone)]
pub enum TunableRange {
    /// Half-open `f32` range `[start, end)`.
    F32(Range<f32>),
    /// Half-open `f64` range `[start, end)`.
    F64(Range<f64>),
    /// Half-open `u32` range `[start, end)`.
    U32(Range<u32>),
    /// Half-open `u64` range `[start, end)`.
    U64(Range<u64>),
    /// Half-open `i32` range `[start, end)`.
    I32(Range<i32>),
    /// Half-open `i64` range `[start, end)`.
    I64(Range<i64>),
    /// No range constraint (boolean tunables).
    Bool,
    /// Closed enumeration : value must equal one of the listed variants.
    StringEnum(Vec<&'static str>),
}

impl TunableRange {
    /// Returns the kind of values this range accepts.
    #[must_use]
    pub fn kind(&self) -> TunableKind {
        match self {
            Self::F32(_) => TunableKind::F32,
            Self::F64(_) => TunableKind::F64,
            Self::U32(_) => TunableKind::U32,
            Self::U64(_) => TunableKind::U64,
            Self::I32(_) => TunableKind::I32,
            Self::I64(_) => TunableKind::I64,
            Self::Bool => TunableKind::Bool,
            Self::StringEnum(_) => TunableKind::StringEnum,
        }
    }

    /// Check that `value` lies inside `self`. Returns `Ok(())` if in-range,
    /// `Err(())` if out-of-range. The caller decides whether to clamp or
    /// hard-reject based on the spec's [`BudgetMode`].
    #[allow(clippy::result_unit_err)]
    pub fn check_in_range(&self, value: &TunableValue) -> Result<(), ()> {
        match (self, value) {
            (Self::F32(r), TunableValue::F32(v)) => {
                if r.contains(v) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            (Self::F64(r), TunableValue::F64(v)) => {
                if r.contains(v) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            (Self::U32(r), TunableValue::U32(v)) => {
                if r.contains(v) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            (Self::U64(r), TunableValue::U64(v)) => {
                if r.contains(v) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            (Self::I32(r), TunableValue::I32(v)) => {
                if r.contains(v) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            (Self::I64(r), TunableValue::I64(v)) => {
                if r.contains(v) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            (Self::Bool, TunableValue::Bool(_)) => Ok(()),
            (Self::StringEnum(allowed), TunableValue::StringEnum(s)) => {
                if allowed.iter().any(|a| *a == s.as_str()) {
                    Ok(())
                } else {
                    Err(())
                }
            }
            _ => Err(()),
        }
    }

    /// Clamp `value` into `self`. Returns `Some(clamped)` for ordered numeric
    /// kinds and `None` for `Bool` / `StringEnum` (those use `HardReject`).
    #[must_use]
    pub fn clamp(&self, value: &TunableValue) -> Option<TunableValue> {
        match (self, value) {
            (Self::F32(r), TunableValue::F32(v)) => {
                Some(TunableValue::F32(clamp_half_open_f32(*v, r)))
            }
            (Self::F64(r), TunableValue::F64(v)) => {
                Some(TunableValue::F64(clamp_half_open_f64(*v, r)))
            }
            (Self::U32(r), TunableValue::U32(v)) => Some(TunableValue::U32(
                (*v).clamp(r.start, r.end.saturating_sub(1)),
            )),
            (Self::U64(r), TunableValue::U64(v)) => Some(TunableValue::U64(
                (*v).clamp(r.start, r.end.saturating_sub(1)),
            )),
            (Self::I32(r), TunableValue::I32(v)) => Some(TunableValue::I32(
                (*v).clamp(r.start, r.end.saturating_sub(1)),
            )),
            (Self::I64(r), TunableValue::I64(v)) => Some(TunableValue::I64(
                (*v).clamp(r.start, r.end.saturating_sub(1)),
            )),
            _ => None,
        }
    }
}

fn clamp_half_open_f32(v: f32, r: &Range<f32>) -> f32 {
    if v < r.start {
        r.start
    } else if v >= r.end {
        // Subtract one ULP so the clamp stays inside the half-open range.
        f32::from_bits(r.end.to_bits().saturating_sub(1))
    } else {
        v
    }
}

fn clamp_half_open_f64(v: f64, r: &Range<f64>) -> f64 {
    if v < r.start {
        r.start
    } else if v >= r.end {
        f64::from_bits(r.end.to_bits().saturating_sub(1))
    } else {
        v
    }
}

// ─── BudgetMode + Stage ────────────────────────────────────────────────────────

/// How the registry reacts to an out-of-range write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetMode {
    /// Out-of-range : clamp + emit a warning audit entry. Default.
    WarnAndClamp,
    /// Out-of-range : reject with [`TweakError::BudgetExceeded`].
    /// Used for safety-critical tunables (spec § 8).
    HardReject,
}

/// Lifecycle stage of a pending mutation. Mirrors the spec's "validate →
/// stage → fence → apply" pattern (§ 4.4) restricted to tweaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The new value passed validation but has not been published.
    Pending,
    /// The new value has been published as the current value at frame N.
    Applied,
}

// ─── TunableSpec ───────────────────────────────────────────────────────────────

/// User-facing registration for a single tunable.
#[derive(Debug, Clone)]
pub struct TunableSpec {
    /// Dot-separated canonical path, e.g. `"render.fovea_detail_budget"`.
    pub canonical_name: &'static str,
    /// Type tag.
    pub kind: TunableKind,
    /// Range constraint.
    pub range: TunableRange,
    /// Default value; **must** lie within `range`.
    pub default: TunableValue,
    /// Out-of-range behavior.
    pub budget_mode: BudgetMode,
    /// Human-readable description shown in inspector UI / MCP listings.
    pub description: &'static str,
    /// Optional unit string ("ms", "Hz", "dB", ...).
    pub units: Option<&'static str>,
    /// When `true`, mutations apply at the *next* frame boundary instead of
    /// immediately. The L4 surface always defers ; the flag exists so a future
    /// `inspect` integration can still reset values immediately at startup.
    pub frame_boundary_defer: bool,
}

impl TunableSpec {
    /// Compute the [`TunableId`] for this spec.
    #[must_use]
    pub const fn id(&self) -> TunableId {
        TunableId::of(self.canonical_name)
    }
}

// ─── TweakError ────────────────────────────────────────────────────────────────

/// Error taxonomy from spec § 4.7. Variants are intentionally rich so the
/// inspector and MCP bridge can produce actionable error messages.
#[derive(Debug, Error, PartialEq)]
pub enum TweakError {
    /// Tried to read or write an id that has not been registered.
    #[error("unknown tunable: id = {0:?}")]
    UnknownTunable(TunableId),
    /// Wrote a value whose [`TunableKind`] does not match the spec.
    #[error("kind mismatch: expected {expected}, got {got}")]
    KindMismatch {
        /// Kind required by the spec.
        expected: &'static str,
        /// Kind of the value the caller supplied.
        got: &'static str,
    },
    /// Out-of-range write rejected because the spec uses [`BudgetMode::HardReject`].
    #[error("budget exceeded: spec = {spec_name}, mode = HardReject")]
    BudgetExceeded {
        /// Canonical name of the offending spec.
        spec_name: &'static str,
    },
    /// String value did not match any allowed variant.
    #[error("string-enum invalid: spec = {spec_name}, got = {got:?}")]
    StringEnumInvalid {
        /// Canonical name of the offending spec.
        spec_name: &'static str,
        /// Allowed variants per the spec.
        allowed: Vec<&'static str>,
        /// Value the caller supplied.
        got: String,
    },
    /// Caller did not present a `Cap<Tweak>` token.
    #[error("cap denied: needed {needed:?}")]
    CapDenied {
        /// Cap-tag the registry expected.
        needed: &'static str,
    },
    /// Tried to register a canonical name that is already taken.
    #[error("already registered: {canonical_name}")]
    AlreadyRegistered {
        /// Canonical name that collided.
        canonical_name: &'static str,
    },
    /// Registry has been frozen (post-startup) and refuses new registrations.
    #[error("registry closed for new registrations")]
    RegistryClosed,
    /// Replay-mode is active : the caller may not mutate tunables manually.
    /// (Spec § 4.7 + AP-10.)
    #[error("replay determinism hold: manual tweak rejected during replay")]
    ReplayDeterminismHold,
    /// The default value supplied at registration is itself out-of-range.
    #[error("default value out of range for spec {canonical_name}")]
    DefaultOutOfRange {
        /// Canonical name of the offending spec.
        canonical_name: &'static str,
    },
}

// ─── unit-tests : pure-types ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunable_id_is_stable() {
        let a = TunableId::of("render.fovea_detail_budget");
        let b = TunableId::of("render.fovea_detail_budget");
        assert_eq!(a, b);
        let c = TunableId::of("render.fovea_detail_budgeT");
        assert_ne!(a, c);
    }

    #[test]
    fn tunable_value_kind_roundtrip() {
        assert_eq!(TunableValue::F32(1.0).kind(), TunableKind::F32);
        assert_eq!(TunableValue::F64(1.0).kind(), TunableKind::F64);
        assert_eq!(TunableValue::U32(1).kind(), TunableKind::U32);
        assert_eq!(TunableValue::U64(1).kind(), TunableKind::U64);
        assert_eq!(TunableValue::I32(-1).kind(), TunableKind::I32);
        assert_eq!(TunableValue::I64(-1).kind(), TunableKind::I64);
        assert_eq!(TunableValue::Bool(true).kind(), TunableKind::Bool);
        assert_eq!(
            TunableValue::StringEnum("ACES".into()).kind(),
            TunableKind::StringEnum
        );
    }

    #[test]
    fn range_check_f32_in_range() {
        let r = TunableRange::F32(0.0..1.0);
        assert!(r.check_in_range(&TunableValue::F32(0.5)).is_ok());
        assert!(r.check_in_range(&TunableValue::F32(0.0)).is_ok());
        assert!(r.check_in_range(&TunableValue::F32(1.0)).is_err()); // half-open
        assert!(r.check_in_range(&TunableValue::F32(-0.1)).is_err());
    }

    #[test]
    fn range_check_kind_mismatch() {
        let r = TunableRange::F32(0.0..1.0);
        assert!(r.check_in_range(&TunableValue::F64(0.5)).is_err());
    }

    #[test]
    fn range_clamp_f32() {
        let r = TunableRange::F32(0.0..1.0);
        let clamped = r.clamp(&TunableValue::F32(2.0)).unwrap();
        // Clamped to less than r.end via ULP-decrement.
        match clamped {
            TunableValue::F32(v) => {
                assert!(v < 1.0);
                assert!(v > 0.99);
            }
            _ => panic!("wrong kind"),
        }
        let clamped_lo = r.clamp(&TunableValue::F32(-5.0)).unwrap();
        match clamped_lo {
            TunableValue::F32(v) => assert!((v - 0.0).abs() < f32::EPSILON),
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn range_clamp_u32_inclusive_end() {
        let r = TunableRange::U32(1..16);
        match r.clamp(&TunableValue::U32(99)).unwrap() {
            TunableValue::U32(v) => assert_eq!(v, 15),
            _ => panic!(),
        }
        match r.clamp(&TunableValue::U32(0)).unwrap() {
            TunableValue::U32(v) => assert_eq!(v, 1),
            _ => panic!(),
        }
    }

    #[test]
    fn string_enum_in_range_check() {
        let r = TunableRange::StringEnum(vec!["Reinhard", "Filmic", "ACES", "Hable"]);
        assert!(r
            .check_in_range(&TunableValue::StringEnum("ACES".into()))
            .is_ok());
        assert!(r
            .check_in_range(&TunableValue::StringEnum("Whatever".into()))
            .is_err());
    }

    #[test]
    fn bool_check_always_passes() {
        let r = TunableRange::Bool;
        assert!(r.check_in_range(&TunableValue::Bool(true)).is_ok());
        assert!(r.check_in_range(&TunableValue::Bool(false)).is_ok());
        assert!(r.check_in_range(&TunableValue::U32(0)).is_err());
    }

    #[test]
    fn tunable_value_render_stable() {
        assert_eq!(TunableValue::F32(1.5).render(), "1.5");
        assert_eq!(TunableValue::Bool(true).render(), "true");
        assert_eq!(
            TunableValue::StringEnum("ACES".into()).render(),
            "ACES".to_string()
        );
    }

    #[test]
    fn spec_id_matches_name() {
        let spec = TunableSpec {
            canonical_name: "x.y",
            kind: TunableKind::F32,
            range: TunableRange::F32(0.0..1.0),
            default: TunableValue::F32(0.5),
            budget_mode: BudgetMode::WarnAndClamp,
            description: "",
            units: None,
            frame_boundary_defer: true,
        };
        assert_eq!(spec.id(), TunableId::of("x.y"));
    }

    #[test]
    fn budget_mode_eq() {
        assert_eq!(BudgetMode::WarnAndClamp, BudgetMode::WarnAndClamp);
        assert_ne!(BudgetMode::WarnAndClamp, BudgetMode::HardReject);
    }

    #[test]
    fn stage_eq() {
        assert_eq!(Stage::Pending, Stage::Pending);
        assert_ne!(Stage::Pending, Stage::Applied);
    }

    #[test]
    fn tweak_error_display() {
        let e = TweakError::UnknownTunable(TunableId(42));
        let s = format!("{e}");
        assert!(s.contains("unknown"));
    }

    #[test]
    fn kind_as_str_unique() {
        let names: [&str; 8] = [
            TunableKind::F32.as_str(),
            TunableKind::F64.as_str(),
            TunableKind::U32.as_str(),
            TunableKind::U64.as_str(),
            TunableKind::I32.as_str(),
            TunableKind::I64.as_str(),
            TunableKind::Bool.as_str(),
            TunableKind::StringEnum.as_str(),
        ];
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 8);
    }
}
