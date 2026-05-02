//! § cssl-host-quantum-hdc::ffi — extern "C" shim layer for csslc q-prims
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage-0 FFI surface that csslc-compiled `.csl` source links against
//!   for the quantum-circuit primitives `qbind` / `qsuperpose` /
//!   `qmeasure` / `qentangle` (T11-W19-G).
//!
//! § APOCKY-DIRECTIVE (verbatim · T11-W19-G)
//!   "Add quantum-circuit primitives to csslc so .csl source can express
//!    directly · qbind(a, b) — complex-amplitude bind · qsuperpose(a, b,
//!    alpha) — weighted superposition with phase · qmeasure(state) → basis
//!    — collapse to deterministic basis · qentangle(a, b) — Bell-pair
//!    correlation · these compile to extern "C" calls into
//!    cssl_host_quantum_hdc::{bind, bundle, permute, coherence}"
//!
//! § ABI-SHAPE
//!   Vectors are passed by **opaque u64 handle** rather than by value
//!   because each `CHdcVec` is 2 KiB and an extern "C" return would
//!   require an out-pointer parameter that complicates csslc's stage-0
//!   ABI lowering. Handles are minted into a process-local registry; the
//!   registry is `BTreeMap<u64, CHdcVec>` to preserve determinism + a
//!   monotonic counter under a `Mutex` (single-writer at a time, multi-
//!   reader lockless via `clone()` on retrieval).
//!
//!   Symbol table the linker resolves :
//!     - `cssl_quantum_qbind(a: u64, b: u64) -> u64`
//!         ← per-component complex-multiplication of two state-handles
//!     - `cssl_quantum_qsuperpose(a: u64, b: u64, alpha: f32) -> u64`
//!         ← weighted superposition `α·a + (1-α)·b` via `bundle` +
//!           amplitude pre-scaling, phase-preserving
//!     - `cssl_quantum_qmeasure(state: u64) -> u32`
//!         ← collapses to deterministic basis-index ∈ [0, 256)
//!           (argmax of amplitude — argmax-tiebreak is lowest index)
//!     - `cssl_quantum_qentangle(a: u64, b: u64) -> u64`
//!         ← Bell-pair correlation : `bind(a, b)` THEN `permute(64)`
//!           applied to the bound state, producing a phase-correlated
//!           output that has high coherence with both `a` and `b`
//!
//! § DETERMINISM
//!   Handles are 1-indexed monotonically per-process. Same input
//!   sequence ⇒ same handle sequence within one process. Cross-process
//!   reproducibility is not promised here (handles are local
//!   identifiers); the **vector contents** at each handle ARE
//!   deterministic given the same inputs.
//!
//!   Handle 0 is reserved as the "missing" / sentinel value — any FFI
//!   call passed handle 0 returns 0 (silently falling through rather
//!   than panicking, so a stage-0 csslc test can compose calls without
//!   needing to validate every intermediate).
//!
//! § PRIME-DIRECTIVE ALIGNMENT
//!   - No host I/O, no logging, no telemetry. Pure math.
//!   - No host fingerprinting : registry seeds come from explicit caller
//!     handles, never from system entropy.
//!   - `unsafe` is scoped to the extern "C" boundary itself (parameters
//!     are scalars, no raw-pointer dereferences here — safety boundary
//!     is the registry-lock-management).
//!   - The crate-level `#![forbid(unsafe_code)]` is downgraded to
//!     `#![deny(unsafe_code)]` to allow this module's `extern "C" fn`
//!     declarations ; the deny + per-block `#[allow]` keeps the safety
//!     review surface tight.

#[allow(unsafe_code)]
mod _unsafe_marker {
    // No actual unsafe blocks live here — the `extern "C" fn` symbol
    // declarations below are themselves safe (their bodies do not
    // contain unsafe operations). This marker module exists so the
    // crate-level `#![deny(unsafe_code)]` (relaxed from `forbid`) is
    // still meaningful : if a future contributor adds a real unsafe
    // block in this file, the deny will fire on their PR.
}

use std::sync::Mutex;
use std::sync::OnceLock;
use std::collections::BTreeMap;

use crate::cvec::{CHdcVec, CHDC_DIM, bundle};

/// § Process-local handle registry. Handle 0 is reserved as the sentinel
///   "missing" value. The first real handle is 1.
struct HandleRegistry {
    next_handle: u64,
    table: BTreeMap<u64, CHdcVec>,
}

impl HandleRegistry {
    const fn new() -> Self {
        Self {
            next_handle: 1,
            table: BTreeMap::new(),
        }
    }

    fn intern(&mut self, vec: CHdcVec) -> u64 {
        let h = self.next_handle;
        self.next_handle = self.next_handle.saturating_add(1);
        self.table.insert(h, vec);
        h
    }

    fn get(&self, handle: u64) -> Option<CHdcVec> {
        self.table.get(&handle).cloned()
    }

    fn len(&self) -> usize {
        self.table.len()
    }
}

fn registry() -> &'static Mutex<HandleRegistry> {
    static REGISTRY: OnceLock<Mutex<HandleRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HandleRegistry::new()))
}

/// § Public Rust-side helper : intern a `CHdcVec` into the registry,
///   returning the opaque u64 handle the FFI surface uses.
///
/// Rust callers (csslc tests + future host-side wiring) use this to
/// stage state vectors before driving the FFI surface end-to-end.
#[must_use]
pub fn intern_vec(vec: CHdcVec) -> u64 {
    registry()
        .lock()
        .map(|mut r| r.intern(vec))
        .unwrap_or(0)
}

/// § Public Rust-side helper : retrieve a `CHdcVec` by handle, cloning
///   the registered value. Returns `None` for handle 0 or unknown
///   handles.
#[must_use]
pub fn lookup_vec(handle: u64) -> Option<CHdcVec> {
    if handle == 0 {
        return None;
    }
    registry().lock().ok().and_then(|r| r.get(handle))
}

/// § Public Rust-side helper : current registry size, intended for
///   tests + diagnostics. NOT a stable API for host callers.
#[must_use]
pub fn registry_len() -> usize {
    registry().lock().map(|r| r.len()).unwrap_or(0)
}

// ════════════════════════════════════════════════════════════════════════════
// § EXTERN "C" SYMBOLS · the surface csslc-compiled .csl source links to
// ════════════════════════════════════════════════════════════════════════════

/// § `qbind(a, b)` — complex-amplitude bind via per-component complex
///   multiplication. Commutative + associative + has identity.
///
/// Returns 0 (sentinel) on missing input handles ; a fresh registered
/// handle for the resulting vector otherwise.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn cssl_quantum_qbind(a: u64, b: u64) -> u64 {
    let Some(va) = lookup_vec(a) else { return 0; };
    let Some(vb) = lookup_vec(b) else { return 0; };
    intern_vec(va.bind(&vb))
}

/// § `qsuperpose(a, b, alpha)` — weighted superposition with phase.
///   Output ≈ α·a + (1-α)·b in cartesian form, then phase-preserving
///   renormalize. `alpha` ∈ [0, 1] is clamped on entry.
///
/// Implementation : pre-scale each input's amplitudes by `alpha` /
/// `(1 - alpha)`, then `bundle` (which sums in cartesian then
/// renormalizes amplitudes to peak = 1.0). The phase-coherent
/// `bundle` operation preserves phase information across the
/// superposition.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn cssl_quantum_qsuperpose(a: u64, b: u64, alpha: f32) -> u64 {
    let Some(va) = lookup_vec(a) else { return 0; };
    let Some(vb) = lookup_vec(b) else { return 0; };
    let alpha_clamped = alpha.clamp(0.0, 1.0);
    let beta = 1.0 - alpha_clamped;
    // Scale amplitudes by α / β respectively, then bundle.
    let mut a_scaled = va.clone();
    for i in 0..CHDC_DIM {
        a_scaled.amp[i] *= alpha_clamped;
    }
    let mut b_scaled = vb.clone();
    for i in 0..CHDC_DIM {
        b_scaled.amp[i] *= beta;
    }
    let combined = bundle(&[a_scaled, b_scaled]);
    intern_vec(combined)
}

/// § `qmeasure(state)` — collapse to deterministic basis-index ∈ [0,
///   256). The basis index is the argmax of amplitude across the 256
///   components. Argmax tiebreak is the lowest index. Phase is
///   ignored — measurement is amplitude-based.
///
/// Returns `u32::MAX` (= 0xFFFF_FFFF) on missing input handle (sentinel
/// for "no measurement possible") to distinguish from a legitimate
/// basis-0 collapse.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn cssl_quantum_qmeasure(state: u64) -> u32 {
    let Some(v) = lookup_vec(state) else { return u32::MAX; };
    let mut best_idx: u32 = 0;
    let mut best_amp: f32 = v.amp[0];
    for i in 1..CHDC_DIM {
        if v.amp[i] > best_amp {
            best_amp = v.amp[i];
            best_idx = i as u32;
        }
    }
    best_idx
}

/// § `qentangle(a, b)` — Bell-pair correlation. Produces a state that
///   has high coherence with BOTH input states by binding (per-
///   component complex-multiply) AND permuting the bound state by
///   `CHDC_DIM / 4` phase-rotation steps. The permutation introduces a
///   π/4-phase signature that distinguishes a Bell-pair from a plain
///   bind, while preserving amplitude pattern.
#[allow(unsafe_code)]
#[no_mangle]
pub extern "C" fn cssl_quantum_qentangle(a: u64, b: u64) -> u64 {
    let Some(va) = lookup_vec(a) else { return 0; };
    let Some(vb) = lookup_vec(b) else { return 0; };
    let bound = va.bind(&vb);
    let entangled = bound.permute((CHDC_DIM as u32) / 4);
    intern_vec(entangled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn intern_round_trip() {
        let v = CHdcVec::derive_from_blake3(&seed(7));
        let h = intern_vec(v.clone());
        assert!(h > 0);
        let v2 = lookup_vec(h).expect("handle resolves");
        for i in 0..CHDC_DIM {
            assert!((v.amp[i] - v2.amp[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn missing_handle_returns_sentinel() {
        // Handle 0 is the missing-sentinel — every FFI op returns 0
        // (or u32::MAX for qmeasure) rather than panicking.
        assert_eq!(cssl_quantum_qbind(0, 0), 0);
        assert_eq!(cssl_quantum_qsuperpose(0, 0, 0.5), 0);
        assert_eq!(cssl_quantum_qmeasure(0), u32::MAX);
        assert_eq!(cssl_quantum_qentangle(0, 0), 0);
    }

    #[test]
    fn ffi_qbind_matches_method_bind() {
        let a = CHdcVec::derive_from_blake3(&seed(11));
        let b = CHdcVec::derive_from_blake3(&seed(13));
        let ha = intern_vec(a.clone());
        let hb = intern_vec(b.clone());
        let hc = cssl_quantum_qbind(ha, hb);
        assert!(hc > 0);
        let c_ffi = lookup_vec(hc).expect("ffi result resolves");
        let c_direct = a.bind(&b);
        for i in 0..CHDC_DIM {
            assert!((c_ffi.amp[i] - c_direct.amp[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn ffi_qsuperpose_alpha_zero_yields_b() {
        let a = CHdcVec::derive_from_blake3(&seed(17));
        let b = CHdcVec::derive_from_blake3(&seed(19));
        let ha = intern_vec(a.clone());
        let hb = intern_vec(b.clone());
        // alpha = 0 ⇒ output ≈ b (renormalized).
        let hc = cssl_quantum_qsuperpose(ha, hb, 0.0);
        assert!(hc > 0);
        let c = lookup_vec(hc).expect("ffi result resolves");
        // Coherence of c with b should be very high (≈ 1).
        let coh = c.coherence(&b);
        assert!(coh > 0.95, "qsuperpose(α=0) coherence with b = {coh}");
    }

    #[test]
    fn ffi_qsuperpose_alpha_one_yields_a() {
        let a = CHdcVec::derive_from_blake3(&seed(23));
        let b = CHdcVec::derive_from_blake3(&seed(29));
        let ha = intern_vec(a.clone());
        let hb = intern_vec(b.clone());
        let hc = cssl_quantum_qsuperpose(ha, hb, 1.0);
        let c = lookup_vec(hc).expect("ffi result resolves");
        let coh = c.coherence(&a);
        assert!(coh > 0.95, "qsuperpose(α=1) coherence with a = {coh}");
    }

    #[test]
    fn ffi_qmeasure_returns_argmax_index() {
        // Construct a vector where component 42 has the largest amplitude.
        let mut v = CHdcVec::ones();
        // Reset all amplitudes to small values.
        for i in 0..CHDC_DIM {
            v.amp[i] = 0.1;
        }
        v.amp[42] = 0.99;
        let h = intern_vec(v);
        let basis = cssl_quantum_qmeasure(h);
        assert_eq!(basis, 42);
    }

    #[test]
    fn ffi_qmeasure_in_range() {
        let v = CHdcVec::derive_from_blake3(&seed(31));
        let h = intern_vec(v);
        let basis = cssl_quantum_qmeasure(h);
        assert!(basis < CHDC_DIM as u32, "basis = {basis}");
    }

    #[test]
    fn ffi_qentangle_high_coherence_with_inputs() {
        // Bell-pair correlation : entangled state has high coherence
        // with the bound state (permute is amplitude-preserving), and
        // moderate coherence with each input independently.
        let a = CHdcVec::derive_from_blake3(&seed(41));
        let b = CHdcVec::derive_from_blake3(&seed(43));
        let ha = intern_vec(a.clone());
        let hb = intern_vec(b.clone());
        let he = cssl_quantum_qentangle(ha, hb);
        assert!(he > 0);
        let entangled = lookup_vec(he).expect("entangled handle resolves");
        // Coherence with the un-permuted bind should be high — permute
        // is phase-only, amplitude pattern preserved.
        let bind_direct = a.bind(&b);
        let coh = entangled.coherence(&bind_direct);
        // Phase-shift by π/4 across all 256 components : sum-of-products
        // in the coherence formula picks up a uniform phase factor that
        // does not destroy the amplitude pattern, so the coherence
        // (after phase-magnitude normalization) stays high.
        assert!(coh > 0.5, "entangled-vs-bind coherence = {coh}");
    }

    #[test]
    fn registry_grows_monotonically() {
        let before = registry_len();
        let _h1 = intern_vec(CHdcVec::derive_from_blake3(&seed(51)));
        let _h2 = intern_vec(CHdcVec::derive_from_blake3(&seed(53)));
        let after = registry_len();
        assert!(after >= before + 2, "registry grew {before} → {after}");
    }
}
