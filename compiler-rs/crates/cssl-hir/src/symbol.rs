//! String interner backed by `lasso::ThreadedRodeo`.
//!
//! § DESIGN
//!   Identifiers lexed from source are re-sliced via `SourceFile::slice(span)` at
//!   elaboration time and interned into a `Symbol(u32)` handle. All HIR nodes carry
//!   `Symbol` references instead of byte-slice spans when they need textual identity.
//!
//! § WHY LASSO
//!   - Stage0 uses single-threaded `Rodeo` — ample for module-at-a-time compilation.
//!   - Hash-based interning in `O(1)` amortized for common cases.
//!   - `Rodeo::resolve(spur)` gives a `&str` back for diagnostic rendering.
//!   - Stage1 parallel compilation can upgrade to `ThreadedRodeo` when the Windows-GNU
//!     toolchain supports `parking_lot_core`'s `dlltool.exe` dependency (or switch
//!     to MSVC toolchain). API stays stable — `Symbol` is backend-agnostic.
//!
//! § DECISION : `DECISIONS.md` T3-D2.

use core::cell::RefCell;
use core::fmt;

use lasso::{Key, Rodeo, Spur};

/// Interned identifier handle. `u32`-backed under the hood (via `lasso::Spur`).
///
/// Comparable, hashable, and `Copy` — cheap to thread through HIR nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Symbol(Spur);

impl Symbol {
    /// Return the underlying `Spur`. Only for low-level interop with `lasso`.
    #[must_use]
    pub const fn spur(self) -> Spur {
        self.0
    }
}

/// Wrapper around a single-threaded `Rodeo` providing the `Symbol` type safely.
///
/// Uses `RefCell` interior mutability so `Interner::intern` is `&self` — matches the
/// common-case pattern where the interner is shared across a walk via `&Interner`.
#[derive(Debug, Default)]
pub struct Interner {
    rodeo: RefCell<Rodeo>,
}

impl Interner {
    /// Build an empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a string, returning a stable `Symbol`. If the string is already present,
    /// returns the existing handle; otherwise inserts and allocates a fresh one.
    pub fn intern(&self, text: &str) -> Symbol {
        Symbol(self.rodeo.borrow_mut().get_or_intern(text))
    }

    /// Intern a static string (no allocation on repeated interns of the same input).
    pub fn intern_static(&self, text: &'static str) -> Symbol {
        Symbol(self.rodeo.borrow_mut().get_or_intern_static(text))
    }

    /// Resolve a `Symbol` back to its original string. Returns an owned `String` since
    /// the `Rodeo` lives behind a `RefCell` — the returned data is copied out.
    #[must_use]
    pub fn resolve(&self, sym: Symbol) -> String {
        self.rodeo.borrow().resolve(&sym.0).to_string()
    }

    /// Number of distinct strings currently interned.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rodeo.borrow().len()
    }

    /// `true` iff no strings have been interned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rodeo.borrow().is_empty()
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Best-effort display using the numeric Spur value — interner not available here.
        write!(f, "sym#{}", self.0.into_usize())
    }
}

#[cfg(test)]
mod tests {
    use super::Interner;

    #[test]
    fn intern_gives_stable_symbol() {
        let i = Interner::new();
        let a = i.intern("foo");
        let b = i.intern("foo");
        assert_eq!(
            a, b,
            "interning the same string twice returns equal symbols"
        );
    }

    #[test]
    fn distinct_strings_give_distinct_symbols() {
        let i = Interner::new();
        let a = i.intern("foo");
        let b = i.intern("bar");
        assert_ne!(a, b);
    }

    #[test]
    fn resolve_returns_original_string() {
        let i = Interner::new();
        let sym = i.intern("hello");
        assert_eq!(i.resolve(sym), "hello");
    }

    #[test]
    fn len_tracks_unique_strings() {
        let i = Interner::new();
        assert!(i.is_empty());
        let _ = i.intern("a");
        let _ = i.intern("b");
        let _ = i.intern("a"); // duplicate
        assert_eq!(i.len(), 2);
    }

    #[test]
    fn static_intern_works() {
        let i = Interner::new();
        let s = i.intern_static("fn");
        assert_eq!(i.resolve(s), "fn");
    }
}
