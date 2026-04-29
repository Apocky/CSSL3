//! Comptime-Value : the carrier for `@staged(comptime)` argument values that
//! flow through specialization + const-propagation + DCE.
//!
//! § DESIGN
//!   The HIR literal-kind only tells us "this is an Int / Float / Bool / …" —
//!   it does NOT carry the parsed numeric value. For specialization we need
//!   the actual value : `eval_scene::<MY_SCENE>` only specializes if we know
//!   `MY_SCENE = Sphere{ … }` at the call-site.
//!
//!   T11-D141 (comptime-eval) is the canonical producer of [`Value`] in flight ;
//!   while it lands we provide [`evaluate_comptime_block_mock`] as a deterministic
//!   test-double that walks a small subset of literal expressions and arithmetic
//!   without going through native compilation. The mock is sufficient for the
//!   specialization-walk tests in this slice — when D141 lands, the specializer
//!   swaps `Specializer::evaluator` to the real implementation without changing
//!   the public API.
//!
//! § VALUES TRACKED
//!   - [`Value::Int`]    — i64 value carried alongside its [`IntWidth`] hint.
//!   - [`Value::Float`]  — f64 value.
//!   - [`Value::Bool`]   — boolean.
//!   - [`Value::Str`]    — owned string.
//!   - [`Value::Unit`]   — `()`.
//!   - [`Value::Sym`]    — opaque symbolic placeholder (for tests that mock a
//!     non-numeric KAN-weight blob via a token).
//!   - [`Value::Tuple`]  — recursive composite for tuples + struct-blobs.
//!
//! § INVARIANT — every `Value` is fully-resolved : no [`Value::Unknown`] variant
//! exists ; "I don't know" is encoded as `Option::None` at the call-site, NOT
//! as a Value variant. This forces every specialization-decision to commit.

use core::fmt;
use std::hash::{Hash, Hasher};

/// Integer width hint stored alongside Value::Int. Mirrors
/// [`cssl_mir::value::IntWidth`] but kept independent so cssl-staging does not
/// pull a hard cssl-mir dep at the value layer (the SpecializationPass plugs
/// in at the pipeline layer + maps these to MirType::Int directly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompIntWidth {
    /// `i1` — boolean lane (used when a bool flows through arithmetic).
    I1,
    I8,
    I16,
    I32,
    I64,
}

impl CompIntWidth {
    /// Canonical name (matches MirType::Int rendering).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I1 => "i1",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
        }
    }

    /// Saturating cast of an i64 to this width's representable range.
    #[must_use]
    pub const fn saturate(self, v: i64) -> i64 {
        // Inlined integer-bound constants — `const fn` cannot call
        // `i64::from(i8::MIN)` so we hard-code the min/max boundary values.
        match self {
            Self::I1 => v & 1,
            Self::I8 => {
                if v < -128 {
                    -128
                } else if v > 127 {
                    127
                } else {
                    v
                }
            }
            Self::I16 => {
                if v < -32_768 {
                    -32_768
                } else if v > 32_767 {
                    32_767
                } else {
                    v
                }
            }
            Self::I32 => {
                if v < -2_147_483_648_i64 {
                    -2_147_483_648_i64
                } else if v > 2_147_483_647_i64 {
                    2_147_483_647_i64
                } else {
                    v
                }
            }
            Self::I64 => v,
        }
    }
}

/// Comptime-known value flowing through the specializer.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64, CompIntWidth),
    Float(f64),
    Bool(bool),
    Str(String),
    /// Symbolic blob — used by tests + the mock evaluator to model an opaque
    /// "KAN-weight" or "scene-config" payload that should still trigger
    /// specialization (distinct payload-hash → distinct mangled fn).
    Sym(String),
    Unit,
    /// Composite : tuple / struct flattened to a sequence.
    Tuple(Vec<Value>),
}

impl Value {
    /// `true` iff this value is structurally a "trivial" comptime-arg —
    /// `Unit` is treated as comptime-trivial because it carries zero
    /// specialization-information ; the specializer skips emitting a
    /// per-call-site clone for such args.
    #[must_use]
    pub const fn is_trivial(&self) -> bool {
        matches!(self, Self::Unit)
    }

    /// `true` iff the inner numeric is zero (used by const-prop branch fold +
    /// the `x * 0 = 0` strength-reduction).
    #[must_use]
    pub fn is_zero(&self) -> bool {
        match self {
            Self::Int(v, _) => *v == 0,
            Self::Float(v) => *v == 0.0,
            Self::Bool(b) => !*b,
            _ => false,
        }
    }

    /// `true` iff the inner is a one-equivalent (used by `x * 1 = x` fold).
    #[must_use]
    pub fn is_one(&self) -> bool {
        match self {
            Self::Int(v, _) => *v == 1,
            Self::Float(v) => *v == 1.0,
            Self::Bool(b) => *b,
            _ => false,
        }
    }

    /// Render as the canonical mangle-fragment :
    ///   - Int    : `i{width}_{value}`     — sign-encoded as `n` for negative.
    ///   - Float  : `f_{bits}`             — IEEE-754 bit-representation as u64.
    ///   - Bool   : `b_t` / `b_f`.
    ///   - Str    : `s_{first8-hex}`       — first 8 chars hashed (FNV-1a 64).
    ///   - Sym    : `m_{first8-hex}`       — same FNV-1a hash but `m` prefix.
    ///   - Unit   : `u`.
    ///   - Tuple  : `t{N}_{frag1}_{frag2}` — N = arity, fragments inline.
    ///
    /// Stable + deterministic across compiler runs.
    #[must_use]
    pub fn mangle_fragment(&self) -> String {
        match self {
            Self::Int(v, w) => {
                if *v < 0 {
                    format!("{}_n{}", w.as_str(), v.unsigned_abs())
                } else {
                    format!("{}_{v}", w.as_str())
                }
            }
            Self::Float(v) => format!("f_{:016x}", v.to_bits()),
            Self::Bool(b) => format!("b_{}", if *b { "t" } else { "f" }),
            Self::Str(s) => format!("s_{:016x}", fnv1a64(s.as_bytes())),
            Self::Sym(s) => format!("m_{:016x}", fnv1a64(s.as_bytes())),
            Self::Unit => "u".to_string(),
            Self::Tuple(elems) => {
                let mut out = format!("t{}", elems.len());
                for e in elems {
                    out.push('_');
                    out.push_str(&e.mangle_fragment());
                }
                out
            }
        }
    }

    /// Stable hash — mixes the discriminant with the payload so hash-mangling
    /// across multiple stage-args produces a single u64 fingerprint.
    #[must_use]
    pub fn stable_hash(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(a, wa), Self::Int(b, wb)) => a == b && wa == wb,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Sym(a), Self::Sym(b)) => a == b,
            (Self::Unit, Self::Unit) => true,
            (Self::Tuple(a), Self::Tuple(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, h: &mut H) {
        // Discriminant first ; then payload.
        core::mem::discriminant(self).hash(h);
        match self {
            Self::Int(v, w) => {
                v.hash(h);
                w.hash(h);
            }
            Self::Float(v) => v.to_bits().hash(h),
            Self::Bool(b) => b.hash(h),
            Self::Str(s) | Self::Sym(s) => s.hash(h),
            Self::Unit => {}
            Self::Tuple(elems) => {
                elems.len().hash(h);
                for e in elems {
                    e.hash(h);
                }
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v, w) => write!(f, "{v}_{w_str}", w_str = w.as_str()),
            Self::Float(v) => write!(f, "{v}f"),
            Self::Bool(b) => f.write_str(if *b { "true" } else { "false" }),
            Self::Str(s) => write!(f, "{s:?}"),
            Self::Sym(s) => write!(f, "<sym:{s}>"),
            Self::Unit => f.write_str("()"),
            Self::Tuple(elems) => {
                f.write_str("(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{e}")?;
                }
                f.write_str(")")
            }
        }
    }
}

/// FNV-1a 64-bit hash — used as a deterministic fingerprint inside
/// `mangle_fragment` so the mangled name is stable across runs without
/// pulling in cryptographic deps.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::{fnv1a64, CompIntWidth, Value};

    #[test]
    fn int_width_as_str_canonical() {
        assert_eq!(CompIntWidth::I1.as_str(), "i1");
        assert_eq!(CompIntWidth::I8.as_str(), "i8");
        assert_eq!(CompIntWidth::I16.as_str(), "i16");
        assert_eq!(CompIntWidth::I32.as_str(), "i32");
        assert_eq!(CompIntWidth::I64.as_str(), "i64");
    }

    #[test]
    fn int_width_saturate_clamps_to_range() {
        assert_eq!(CompIntWidth::I8.saturate(200), 127);
        assert_eq!(CompIntWidth::I8.saturate(-200), -128);
        assert_eq!(CompIntWidth::I16.saturate(40_000), 32_767);
        assert_eq!(CompIntWidth::I32.saturate(3_000_000_000), 2_147_483_647);
        assert_eq!(CompIntWidth::I64.saturate(i64::MAX), i64::MAX);
    }

    #[test]
    fn int_width_saturate_passes_inrange_unchanged() {
        assert_eq!(CompIntWidth::I32.saturate(42), 42);
        assert_eq!(CompIntWidth::I32.saturate(-42), -42);
    }

    #[test]
    fn int_width_saturate_i1_is_lsb() {
        assert_eq!(CompIntWidth::I1.saturate(0), 0);
        assert_eq!(CompIntWidth::I1.saturate(1), 1);
        assert_eq!(CompIntWidth::I1.saturate(2), 0);
        assert_eq!(CompIntWidth::I1.saturate(3), 1);
    }

    #[test]
    fn value_is_trivial_only_for_unit() {
        assert!(Value::Unit.is_trivial());
        assert!(!Value::Int(0, CompIntWidth::I32).is_trivial());
        assert!(!Value::Bool(false).is_trivial());
    }

    #[test]
    fn value_is_zero() {
        assert!(Value::Int(0, CompIntWidth::I32).is_zero());
        assert!(Value::Float(0.0).is_zero());
        assert!(Value::Bool(false).is_zero());
        assert!(!Value::Int(1, CompIntWidth::I32).is_zero());
        assert!(!Value::Bool(true).is_zero());
    }

    #[test]
    fn value_is_one() {
        assert!(Value::Int(1, CompIntWidth::I32).is_one());
        assert!(Value::Float(1.0).is_one());
        assert!(Value::Bool(true).is_one());
        assert!(!Value::Int(0, CompIntWidth::I32).is_one());
    }

    #[test]
    fn mangle_fragment_int_positive() {
        let f = Value::Int(42, CompIntWidth::I32).mangle_fragment();
        assert_eq!(f, "i32_42");
    }

    #[test]
    fn mangle_fragment_int_negative() {
        let f = Value::Int(-7, CompIntWidth::I32).mangle_fragment();
        assert_eq!(f, "i32_n7");
    }

    #[test]
    fn mangle_fragment_bool() {
        assert_eq!(Value::Bool(true).mangle_fragment(), "b_t");
        assert_eq!(Value::Bool(false).mangle_fragment(), "b_f");
    }

    #[test]
    fn mangle_fragment_unit() {
        assert_eq!(Value::Unit.mangle_fragment(), "u");
    }

    #[test]
    fn mangle_fragment_str_is_hash_based() {
        let a = Value::Str("hello".into()).mangle_fragment();
        let b = Value::Str("hello".into()).mangle_fragment();
        assert_eq!(a, b);
        assert!(a.starts_with("s_"));
        let c = Value::Str("world".into()).mangle_fragment();
        assert_ne!(a, c);
    }

    #[test]
    fn mangle_fragment_sym_distinct_from_str() {
        let s = Value::Str("hello".into()).mangle_fragment();
        let m = Value::Sym("hello".into()).mangle_fragment();
        assert_ne!(s, m);
        assert!(s.starts_with("s_"));
        assert!(m.starts_with("m_"));
    }

    #[test]
    fn mangle_fragment_tuple_recursive() {
        let v = Value::Tuple(vec![Value::Int(1, CompIntWidth::I32), Value::Bool(true)]);
        let f = v.mangle_fragment();
        assert!(f.starts_with("t2_"));
        assert!(f.contains("i32_1"));
        assert!(f.contains("b_t"));
    }

    #[test]
    fn mangle_fragment_float_is_bit_repr() {
        let a = Value::Float(1.5).mangle_fragment();
        let b = Value::Float(1.5).mangle_fragment();
        assert_eq!(a, b);
        assert!(a.starts_with("f_"));
        // Distinct floats produce distinct fragments.
        let c = Value::Float(2.5).mangle_fragment();
        assert_ne!(a, c);
    }

    #[test]
    fn value_equality_int_with_width() {
        let a = Value::Int(1, CompIntWidth::I32);
        let b = Value::Int(1, CompIntWidth::I32);
        let c = Value::Int(1, CompIntWidth::I64);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn value_equality_float_via_bits() {
        // NaN values with same bit-pattern compare equal under our scheme.
        let a = Value::Float(f64::NAN);
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn value_stable_hash_deterministic() {
        let a = Value::Int(42, CompIntWidth::I32);
        let b = Value::Int(42, CompIntWidth::I32);
        assert_eq!(a.stable_hash(), b.stable_hash());
    }

    #[test]
    fn value_stable_hash_differs_for_distinct_widths() {
        let a = Value::Int(1, CompIntWidth::I32);
        let b = Value::Int(1, CompIntWidth::I64);
        assert_ne!(a.stable_hash(), b.stable_hash());
    }

    #[test]
    fn fnv1a64_known_vector() {
        // Empty input yields the FNV-1a 64-bit offset basis.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        // The classic "foobar" test vector.
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn value_display_round_trip_int() {
        let v = Value::Int(7, CompIntWidth::I32);
        assert_eq!(format!("{v}"), "7_i32");
    }

    #[test]
    fn value_display_bool() {
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::Bool(false)), "false");
    }

    #[test]
    fn value_display_unit() {
        assert_eq!(format!("{}", Value::Unit), "()");
    }
}
