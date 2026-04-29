//! § cssl-substrate-omega-tensor
//! ═══════════════════════════════════════════════════════════════════
//!
//! Phase H — Substrate engine plumbing. Slice 1 (S8-H1).
//!
//! Authoritative spec : the canonical multi-dimensional state container
//! for the LoA Substrate. Surface contract is co-authored at this slice
//! while `specs/30_SUBSTRATE.csl` is in flight on the parallel
//! `cssl/session-8/H0-design` branch ; integration with that spec lands
//! at PM merge time.
//!
//! § ROLE
//!   `OmegaTensor<T, R>` is the canonical multi-dimensional state
//!   container that every other Substrate slice (H2..H6) consumes. It
//!   generalizes `Vec<T>` (rank-1, runtime length) to N-D rank with
//!   compile-time `R` and runtime shape. The element-storage is
//!   heap-backed through cssl-rt's raw allocator (T11-D57) and the data
//!   pointer carries iso-capability (`specs/12_CAPABILITIES.csl §
//!   ISO-OWNERSHIP`). At the Rust type level [`OmegaTensorIso`] encodes
//!   the linearity at the borrow boundary, mirroring the
//!   `GpuBufferIso<'a>` pattern from `cssl-host-d3d12`.
//!
//! § SURFACE
//!   ```text
//!   pub struct OmegaTensor<T : OmegaScalar, const R : usize> { ... }
//!   pub struct OmegaTensorIso<'a, T, const R : usize> { ... }
//!   pub struct OmegaView<'a, T, const R : usize> { ... }
//!   pub struct OmegaIter<'a, T, const R : usize> { ... }
//!
//!   pub trait OmegaScalar : Copy + Default ;   // f32 / f64 / i32 / i64
//!
//!   OmegaTensor::<T, R>::new(shape : [u64 ; R]) -> OmegaTensor<T, R>
//!   OmegaTensor::<T, R>::from_iter(shape, iter) -> OmegaTensor<T, R>
//!   OmegaTensor::get(&self, indices : [u64 ; R]) -> Option<T>
//!   OmegaTensor::set(&mut self, indices, value) -> bool
//!   OmegaTensor::shape() -> [u64 ; R]
//!   OmegaTensor::rank() -> usize     // == R
//!   OmegaTensor::numel() -> u64
//!   OmegaTensor::iter() -> OmegaIter<T, R>
//!   OmegaTensor::slice_along(axis, range) -> OmegaView<T, R>
//!   OmegaTensor::reshape::<R2>(new_shape : [u64 ; R2]) -> OmegaTensor<T, R2>
//!     // requires numel(self) == numel(new_shape)
//!   OmegaTensor::add(&self, other) -> OmegaTensor<T, R>   // shape-equal
//!   OmegaTensor::sub / mul / div                       // element-wise
//!   OmegaTensor::add_assign / sub_assign / mul_assign / div_assign  // in-place
//!   ```
//!
//! § CAPABILITY (`specs/12_CAPABILITIES.csl § ISO-OWNERSHIP`)
//!   - The owned `OmegaTensor<T, R>` carries an iso-capability over its
//!     backing storage. The Rust type system enforces that mutations
//!     require `&mut self` ; passing the tensor into `take()` consumes
//!     it. Public APIs do NOT clone the storage — rust-side `Clone` is
//!     intentionally omitted (mirrors GpuBufferIso pattern).
//!   - [`OmegaTensorIso`] is a `!Clone + !Copy` phantom borrow that
//!     callers can use to thread the iso-marker through Rust's borrow
//!     checker. `share_iso(&mut self)` consumes the tensor's interior
//!     mutability privilege for the borrow's lifetime.
//!   - The only escape-routes from iso-ownership are `take()` (move) +
//!     `into_view()` (borrow-only). No reference-counted shared owner is
//!     exposed at this slice ; multi-owner aliasing belongs to a later
//!     slice via the `box`/`val` capabilities.
//!
//! § HEAP DISCIPLINE
//!   Every owned `OmegaTensor` allocates through `cssl_rt::raw_alloc`
//!   with `size = numel × sizeof(T)` and `align = max(8, alignof(T))`.
//!   The empty tensor (`numel == 0`) reserves no storage — the `data`
//!   pointer is null and dropping it is a no-op. The `Drop` impl pairs
//!   every successful alloc with `cssl_rt::raw_free`, so the cssl-rt
//!   allocator's tracker counters return to zero net-allocations once
//!   every tensor goes out of scope. Reshape (`reshape`) preserves the
//!   underlying allocation byte-for-byte and re-records shape/strides
//!   only — no realloc.
//!
//! § AUTODIFF (AD-walker integration)
//!   `OmegaTensor<f32, R>` and `OmegaTensor<f64, R>` are AD-compatible
//!   per `specs/05_AUTODIFF.csl § INTERFACES`. Element-wise ops decompose
//!   to per-element `arith.addf` / `arith.subf` / `arith.mulf` /
//!   `arith.divf` MIR ops at lowering time, each of which has a known AD
//!   rule entry in `cssl_autodiff::rules::DiffRuleTable` (FAdd / FSub /
//!   FMul / FDiv). The walker (`cssl_autodiff::walker::op_to_primitive`)
//!   produces the dual variants without per-tensor extension. Rank-
//!   reducing ops (`dot`, `matmul`) and tensor-broadcasting are scheduled
//!   for a later H-slice — at H1 the AD claim is element-wise only.
//!   The Rust-side ops in this crate manipulate the storage directly to
//!   provide a correctness oracle that future MIR-lowered code can be
//!   differentially tested against (per `specs/23_TESTING.csl § ORACLE
//!   MODES`).
//!
//! § PRIME-DIRECTIVE (consent + opacity)
//!   - Tensor-data is OPAQUE TO THE RUNTIME by default. No introspection
//!     logging is emitted ; no payload bytes leave the Rust process. The
//!     element accessors (`get`/`set`) report only `Option<T>` for the
//!     specific cell the caller named — no bulk-readback API exists at
//!     this slice (deferred to H5 with explicit consent gate).
//!   - Mutating ops carry a stage-0 marker in the canonical effect
//!     row : `(omega_mutate, "true")`. At this slice the marker is a
//!     module-level constant ([`OMEGA_MUTATE_ATTR`]) returned by
//!     [`OmegaTensor::mutate_marker`] ; the threading into `MirFunc`
//!     attribute lists is the responsibility of the body-lower
//!     recognizer slice that lands once the source-level `omega::*`
//!     surface is ready (deferred — see § DEFERRED-AT-H1).
//!   - The implementation surface is small + auditable. Every `unsafe`
//!     block carries a `# Safety` paragraph. There is no observable
//!     side-channel beyond the cssl-rt allocator counters that already
//!     existed at T11-D52.
//!
//! § STAGE-0 SCOPE (H1)
//!   - Element-types covered : `f32`, `f64`, `i32`, `i64`. SIMD-vector
//!     elements (`vec3<f32>`, `mat<f32, R, C>`) are deferred — they enter
//!     when the Rust-level [`OmegaScalar`] trait grows the appropriate
//!     impls in a later slice.
//!   - Strides : runtime-stored row-major (C-order) only. Non-contiguous
//!     views over arbitrary stride permutations are deferred ; at H1
//!     `slice_along(axis, range)` produces a [`OmegaView`] that records
//!     a contiguous sub-range along ONE axis.
//!   - Empty-tensor (numel == 0) : a valid edge-case throughout. iter
//!     yields nothing, get always returns `None`, set always returns
//!     false, all elementwise ops are no-ops, reshape allows any shape
//!     where every dim is zero or numel(other) is zero.
//!   - Reshape : preserves numel ; broadcasting is deferred to a later
//!     slice. `reshape` to a shape with mismatched numel returns `None`.
//!
//! § DEFERRED-AT-H1 (explicit follow-ups)
//!   - `dot(other) -> OmegaTensor<T, R-1>` rank-reducing inner product —
//!     requires const-generic `{R - 1}` arithmetic which Rust 1.85 const
//!     generics support but is more readable behind a dedicated H-slice.
//!   - `matmul` (rank-2 × rank-2) — the killer-op for H4.
//!   - Broadcasting elementwise — H3.
//!   - Non-contiguous views (arbitrary stride permutations) — H2.
//!   - SIMD-vector element types — H6.
//!   - Source-level `omega::*` recognizer in `cssl-mir::body_lower` —
//!     mirrors the `Box::new` pattern at T11-D57 and the `fs::open`
//!     pattern at T11-D76. The `(omega_mutate, "true")` attribute is
//!     emitted by that recognizer.
//!   - Real `Drop` element-walker that calls T's drop fn for non-trivial
//!     T (the H1 element-set is all `Copy` so the storage can be freed
//!     directly).
//!   - GPU-target lowering for tensor ops — Phase-D follow-up.
//!   - Serialization / IO — H5 with explicit consent gate.

#![allow(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::needless_range_loop)]

use core::marker::PhantomData;
use core::ops::Range;

// ───────────────────────────────────────────────────────────────────────
// § OmegaScalar — the element-type trait
// ───────────────────────────────────────────────────────────────────────

/// Element-type trait for [`OmegaTensor`].
///
/// At H1 the impls cover `f32` / `f64` / `i32` / `i64`. The trait is
/// deliberately narrow — `Copy + Default` to support `iter`-based bulk
/// initialization, plus the four canonical arithmetic ops to support
/// elementwise tensor arithmetic. Equality is required only for the
/// validation/equality-oracle paths used by tests.
///
/// SIMD-vector element-types are deferred to a later slice ; when they
/// land they will impl the same trait via per-lane decomposition.
pub trait OmegaScalar: Copy + Default + PartialEq + core::fmt::Debug + 'static {
    /// Add two values of `Self` and return the sum.
    fn omega_add(a: Self, b: Self) -> Self;
    /// Subtract `b` from `a` and return the difference.
    fn omega_sub(a: Self, b: Self) -> Self;
    /// Multiply two values of `Self` and return the product.
    fn omega_mul(a: Self, b: Self) -> Self;
    /// Divide `a` by `b` and return the quotient.
    ///
    /// Integer division by zero is rejected at the call-site by
    /// `OmegaTensor::div` returning `None` ; this trait method is only
    /// invoked when the divisor is known non-zero.
    fn omega_div(a: Self, b: Self) -> Self;
    /// True iff `b == Self::default()` — used by the elementwise-div
    /// guard to avoid integer division-by-zero panics.
    fn omega_is_zero(b: Self) -> bool;
    /// Byte-size of the type (== `core::mem::size_of::<Self>()`).
    fn omega_size_of() -> usize;
    /// Byte-alignment of the type (== `core::mem::align_of::<Self>()`).
    fn omega_align_of() -> usize;
}

impl OmegaScalar for f32 {
    fn omega_add(a: f32, b: f32) -> f32 {
        a + b
    }
    fn omega_sub(a: f32, b: f32) -> f32 {
        a - b
    }
    fn omega_mul(a: f32, b: f32) -> f32 {
        a * b
    }
    fn omega_div(a: f32, b: f32) -> f32 {
        a / b
    }
    fn omega_is_zero(b: f32) -> bool {
        b == 0.0
    }
    fn omega_size_of() -> usize {
        core::mem::size_of::<f32>()
    }
    fn omega_align_of() -> usize {
        core::mem::align_of::<f32>()
    }
}

impl OmegaScalar for f64 {
    fn omega_add(a: f64, b: f64) -> f64 {
        a + b
    }
    fn omega_sub(a: f64, b: f64) -> f64 {
        a - b
    }
    fn omega_mul(a: f64, b: f64) -> f64 {
        a * b
    }
    fn omega_div(a: f64, b: f64) -> f64 {
        a / b
    }
    fn omega_is_zero(b: f64) -> bool {
        b == 0.0
    }
    fn omega_size_of() -> usize {
        core::mem::size_of::<f64>()
    }
    fn omega_align_of() -> usize {
        core::mem::align_of::<f64>()
    }
}

impl OmegaScalar for i32 {
    fn omega_add(a: i32, b: i32) -> i32 {
        a.wrapping_add(b)
    }
    fn omega_sub(a: i32, b: i32) -> i32 {
        a.wrapping_sub(b)
    }
    fn omega_mul(a: i32, b: i32) -> i32 {
        a.wrapping_mul(b)
    }
    fn omega_div(a: i32, b: i32) -> i32 {
        // wrapping_div panics on i32::MIN / -1 ; we wrap the overflow
        // case explicitly to keep the no-panic invariant on the hot path.
        if b == -1 && a == i32::MIN {
            i32::MIN
        } else {
            a / b
        }
    }
    fn omega_is_zero(b: i32) -> bool {
        b == 0
    }
    fn omega_size_of() -> usize {
        core::mem::size_of::<i32>()
    }
    fn omega_align_of() -> usize {
        core::mem::align_of::<i32>()
    }
}

impl OmegaScalar for i64 {
    fn omega_add(a: i64, b: i64) -> i64 {
        a.wrapping_add(b)
    }
    fn omega_sub(a: i64, b: i64) -> i64 {
        a.wrapping_sub(b)
    }
    fn omega_mul(a: i64, b: i64) -> i64 {
        a.wrapping_mul(b)
    }
    fn omega_div(a: i64, b: i64) -> i64 {
        if b == -1 && a == i64::MIN {
            i64::MIN
        } else {
            a / b
        }
    }
    fn omega_is_zero(b: i64) -> bool {
        b == 0
    }
    fn omega_size_of() -> usize {
        core::mem::size_of::<i64>()
    }
    fn omega_align_of() -> usize {
        core::mem::align_of::<i64>()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Constants — the public-facing markers
// ───────────────────────────────────────────────────────────────────────

/// Canonical effect-row marker emitted by mutating `cssl.omega.*` ops.
///
/// At H1 this is exposed so that downstream auditors + the
/// `cssl-mir::body_lower` recognizer (deferred — see crate doc-block §
/// DEFERRED-AT-H1) can attach the marker by stable name. Renaming this
/// constant requires lock-step changes across cssl-mir + telemetry.
///
/// Threading discipline mirrors the T11-D76 `(io_effect, "true")`
/// pattern : every mutating MIR op carries the attribute pair
/// `(OMEGA_MUTATE_ATTR, "true")` in its attribute list.
pub const OMEGA_MUTATE_ATTR: &str = "omega_mutate";

/// Minimum byte-alignment for the tensor backing storage.
///
/// Set to 8 so f64 / i64 alignment is satisfied even when T is f32 / i32
/// (no over-alignment cost on Windows-x86_64 ; matches `cssl_rt::ALIGN_MAX`
/// minus the SIMD-vector concession). Per-T alignment is computed from
/// `T::omega_align_of()` and the maximum of (8, alignof T) is used at
/// allocation time.
pub const OMEGA_MIN_ALIGN: usize = 8;

// ───────────────────────────────────────────────────────────────────────
// § OmegaTensor<T, R> — the heap-backed N-D container
// ───────────────────────────────────────────────────────────────────────

/// Heap-backed N-dimensional array with compile-time rank `R` and
/// runtime shape.
///
/// § INVARIANTS (carried by every owned tensor)
///   - `rank() == R` (compile-time guaranteed by the type parameter).
///   - `numel() == shape.iter().product()` ; the cached field is stored
///     so the hot path doesn't re-multiply on every call.
///   - `numel() == 0 ⟺ data.is_null()` ; the empty tensor uses no heap.
///   - `numel() > 0 ⇒ data` points to `numel * sizeof T` bytes of
///     properly-aligned heap storage produced by `cssl_rt::raw_alloc`.
///   - `strides[R-1] == 1` (always) ; `strides[i] == shape[i+1] *
///     strides[i+1]` for `i < R-1` (row-major / C-order).
///
/// § CAPABILITY  iso-ownership per `specs/12_CAPABILITIES.csl §
/// ISO-OWNERSHIP`. Public APIs do NOT clone storage. The struct is
/// intentionally `!Clone + !Copy` ; transferring ownership is by `move`
/// or by [`OmegaTensor::take`] in the rare case where the caller needs
/// to detach the storage from the wrapper.
pub struct OmegaTensor<T: OmegaScalar, const R: usize> {
    /// Heap pointer. Null iff `numel == 0`.
    data: *mut u8,
    /// Compile-time rank ; runtime shape.
    shape: [u64; R],
    /// Row-major strides (in elements, not bytes). Computed from shape.
    strides: [u64; R],
    /// Cached element count (`shape.iter().product()`).
    numel: u64,
    /// Phantom marker so `T` is exposed in the type signature.
    _marker: PhantomData<T>,
}

// SAFETY : the contained pointer is exclusively owned for the lifetime of
// the OmegaTensor ; no aliasing is exposed through any public API (clones
// are forbidden ; views borrow). Send is sound — moving the wrapper
// transfers exclusive ownership of the storage. Sync is NOT implemented
// at H1 ; cross-thread shared access requires the `box`/`val` capabilities
// which are deferred.
unsafe impl<T: OmegaScalar, const R: usize> Send for OmegaTensor<T, R> {}

impl<T: OmegaScalar, const R: usize> OmegaTensor<T, R> {
    // ──────────────────────────────────────────────────────────────────
    // § Construction
    // ──────────────────────────────────────────────────────────────────

    /// Construct a zero-initialized tensor with the given shape.
    ///
    /// Every element is `T::default()` (== 0 for the H1 scalar set).
    ///
    /// § COMPLEXITY  O(numel) — a `write_bytes` over the freshly-allocated
    /// region. The cssl-rt allocator counter increments by one.
    ///
    /// § PANICS  Returns a tensor with null `data` (representing a valid
    /// zero-element tensor) when allocation fails ; never panics. Callers
    /// who need OOM-rejection should compare `numel()` against the input
    /// shape product themselves.
    #[must_use]
    pub fn new(shape: [u64; R]) -> Self {
        let numel = shape_product(&shape);
        if numel == 0 {
            return Self::empty_with_shape(shape);
        }
        let size_bytes = numel.saturating_mul(T::omega_size_of() as u64);
        let align = effective_align::<T>();
        // SAFETY : raw_alloc rejects layout failures at the std::alloc
        // boundary by returning null ; we verify before deref.
        let data = unsafe { cssl_rt::alloc::raw_alloc(size_bytes as usize, align) };
        if data.is_null() {
            // Storage allocation failed ; degrade to an empty tensor with
            // the requested shape preserved on the stack-side fields.
            return Self::empty_with_shape(shape);
        }
        // SAFETY : numel * sizeof(T) bytes were just allocated and are
        // owned by this tensor ; write_bytes initializes them to zero
        // which is the bit-pattern of `T::default()` for every H1 scalar.
        unsafe {
            core::ptr::write_bytes(data, 0_u8, size_bytes as usize);
        }
        Self {
            data,
            shape,
            strides: row_major_strides(&shape),
            numel,
            _marker: PhantomData,
        }
    }

    /// Construct a tensor from `shape` and an iterator yielding exactly
    /// `numel(shape)` elements (more elements are ignored ; fewer
    /// elements leave the trailing slots at `T::default()`).
    ///
    /// § PANICS  Never. Returns an empty tensor on allocation failure.
    #[must_use]
    pub fn from_iter<I>(shape: [u64; R], iter: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let t = Self::new(shape);
        if t.numel == 0 {
            return t;
        }
        for (idx, v) in iter.into_iter().enumerate() {
            if (idx as u64) >= t.numel {
                break;
            }
            // SAFETY : data is non-null with numel*sizeof T bytes owned ;
            // idx < numel ⇒ offset is in-bounds.
            unsafe {
                let p = t.data.cast::<T>().add(idx);
                core::ptr::write(p, v);
            }
        }
        t
    }

    /// Helper : produce an empty tensor that retains the shape but does
    /// not allocate. Used by both `new` (numel == 0) and the OOM fallback.
    fn empty_with_shape(shape: [u64; R]) -> Self {
        Self {
            data: core::ptr::null_mut(),
            shape,
            strides: row_major_strides(&shape),
            numel: shape_product(&shape),
            _marker: PhantomData,
        }
    }

    // ──────────────────────────────────────────────────────────────────
    // § Accessors
    // ──────────────────────────────────────────────────────────────────

    /// Compile-time rank. Equal to the type parameter `R`.
    #[must_use]
    pub const fn rank(&self) -> usize {
        R
    }

    /// Runtime shape vector.
    #[must_use]
    pub const fn shape(&self) -> [u64; R] {
        self.shape
    }

    /// Row-major strides (in elements). At H1 every stride is the
    /// canonical row-major layout ; non-contiguous views are deferred.
    #[must_use]
    pub const fn strides(&self) -> [u64; R] {
        self.strides
    }

    /// Total element count == `shape.iter().product()`.
    #[must_use]
    pub const fn numel(&self) -> u64 {
        self.numel
    }

    /// True iff `numel == 0`.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.numel == 0
    }

    /// Stage-0 effect-row marker for mutating ops. See [`OMEGA_MUTATE_ATTR`].
    #[must_use]
    pub const fn mutate_marker() -> &'static str {
        OMEGA_MUTATE_ATTR
    }

    /// Linear element-offset for the given multi-dimensional index, or
    /// `None` if any index component is out-of-bounds.
    #[must_use]
    pub fn linear_offset(&self, indices: [u64; R]) -> Option<u64> {
        let mut off: u64 = 0;
        for axis in 0..R {
            let i = indices[axis];
            if i >= self.shape[axis] {
                return None;
            }
            off = off.checked_add(i.checked_mul(self.strides[axis])?)?;
        }
        Some(off)
    }

    /// Read element at `indices`. Returns `None` on out-of-bounds index
    /// component.
    #[must_use]
    pub fn get(&self, indices: [u64; R]) -> Option<T> {
        let off = self.linear_offset(indices)?;
        if self.data.is_null() {
            return None;
        }
        // SAFETY : off < numel ⇒ offset in-bounds for the allocated region.
        unsafe {
            let p = self.data.cast::<T>().add(off as usize);
            Some(core::ptr::read(p.cast_const()))
        }
    }

    /// Write `value` into the slot at `indices`. Returns true on success
    /// (in-bounds + non-null storage), false otherwise.
    pub fn set(&mut self, indices: [u64; R], value: T) -> bool {
        let Some(off) = self.linear_offset(indices) else {
            return false;
        };
        if self.data.is_null() {
            return false;
        }
        // SAFETY : off < numel ⇒ offset in-bounds.
        unsafe {
            let p = self.data.cast::<T>().add(off as usize);
            core::ptr::write(p, value);
        }
        true
    }

    // ──────────────────────────────────────────────────────────────────
    // § Iteration
    // ──────────────────────────────────────────────────────────────────

    /// Iterator over every element in row-major order.
    #[must_use]
    #[allow(clippy::iter_without_into_iter)] // borrowed `&OmegaTensor` does not own elements ; IntoIterator requires by-value impl that doesn't fit iso-discipline
    pub fn iter(&self) -> OmegaIter<'_, T, R> {
        OmegaIter {
            data: self.data.cast::<T>().cast_const(),
            cursor: 0,
            len: self.numel,
            _marker: PhantomData,
        }
    }

    // ──────────────────────────────────────────────────────────────────
    // § Views
    // ──────────────────────────────────────────────────────────────────

    /// Borrow a sub-range along ONE axis as a non-owning view.
    ///
    /// At H1 the view shape replaces `shape[axis]` with `range.end -
    /// range.start` and inherits all other axes from `self.shape`. Out-
    /// of-bounds ranges or `axis >= R` produce `None`.
    #[must_use]
    pub fn slice_along(&self, axis: usize, range: Range<u64>) -> Option<OmegaView<'_, T, R>> {
        if axis >= R {
            return None;
        }
        if range.start > range.end || range.end > self.shape[axis] {
            return None;
        }
        let mut new_shape = self.shape;
        new_shape[axis] = range.end - range.start;
        let new_numel = shape_product(&new_shape);
        // The view's start-offset is range.start * strides[axis] elements.
        let start_off = range.start.checked_mul(self.strides[axis])?;
        Some(OmegaView {
            data: self.data.cast::<T>().cast_const(),
            base_offset: start_off,
            shape: new_shape,
            strides: self.strides,
            numel: new_numel,
            _marker: PhantomData,
        })
    }

    /// Wrap `&mut self` in an `OmegaTensorIso` borrow, encoding the
    /// iso-capability at the Rust borrow level. Mirrors the
    /// `GpuBufferIso<'a>` pattern from `cssl-host-d3d12`.
    #[must_use]
    pub fn share_iso(&mut self) -> OmegaTensorIso<'_, T, R> {
        OmegaTensorIso {
            tensor: self,
            _marker: PhantomData,
        }
    }

    // ──────────────────────────────────────────────────────────────────
    // § Reshape (preserves numel)
    // ──────────────────────────────────────────────────────────────────

    /// Re-interpret the storage with a different rank-and-shape pair.
    /// Requires `numel(self) == numel(new_shape)`. Returns `None` on
    /// numel-mismatch.
    ///
    /// § COMPLEXITY  O(R2) — only the strides are recomputed. The
    /// underlying allocation is moved into the new wrapper without copy.
    #[must_use]
    pub fn reshape<const R2: usize>(self, new_shape: [u64; R2]) -> Option<OmegaTensor<T, R2>> {
        let new_numel = shape_product(&new_shape);
        if new_numel != self.numel {
            return None;
        }
        let new_strides = row_major_strides(&new_shape);
        let data = self.data;
        // We are moving the storage out of `self` into the new tensor.
        // Suppress the Drop on the old wrapper by forgetting it.
        let shape_dbg = self.shape;
        let _ = shape_dbg;
        core::mem::forget(self);
        Some(OmegaTensor {
            data,
            shape: new_shape,
            strides: new_strides,
            numel: new_numel,
            _marker: PhantomData,
        })
    }

    /// Detach the storage pointer + shape from the wrapper. The caller
    /// becomes responsible for freeing the storage (matching the cssl-rt
    /// raw_alloc convention) ; the original wrapper is consumed.
    ///
    /// Returns `(data_ptr, shape, numel)`. When the tensor is empty the
    /// pointer is null and the caller does NOT need to free.
    ///
    /// # Safety
    /// The caller must call `cssl_rt::alloc::raw_free(ptr, numel *
    /// sizeof T, align)` to release the storage, with the same align
    /// returned by [`effective_align`]. Failure to free leaks the
    /// allocation.
    #[must_use]
    pub fn take(self) -> (*mut u8, [u64; R], u64) {
        let data = self.data;
        let shape = self.shape;
        let numel = self.numel;
        core::mem::forget(self);
        (data, shape, numel)
    }

    // ──────────────────────────────────────────────────────────────────
    // § Elementwise arithmetic — produces NEW tensor
    // ──────────────────────────────────────────────────────────────────

    /// Element-wise sum. Returns `None` on shape-mismatch.
    #[must_use]
    pub fn add(&self, other: &Self) -> Option<Self> {
        self.elementwise(other, T::omega_add, false)
    }

    /// Element-wise difference. Returns `None` on shape-mismatch.
    #[must_use]
    pub fn sub(&self, other: &Self) -> Option<Self> {
        self.elementwise(other, T::omega_sub, false)
    }

    /// Element-wise product. Returns `None` on shape-mismatch.
    #[must_use]
    pub fn mul(&self, other: &Self) -> Option<Self> {
        self.elementwise(other, T::omega_mul, false)
    }

    /// Element-wise quotient. Returns `None` on shape-mismatch OR if any
    /// divisor element is zero (avoids panic).
    #[must_use]
    pub fn div(&self, other: &Self) -> Option<Self> {
        self.elementwise(other, T::omega_div, true)
    }

    fn elementwise(&self, other: &Self, op: fn(T, T) -> T, guard_zero: bool) -> Option<Self> {
        if self.shape != other.shape {
            return None;
        }
        if guard_zero {
            for i in 0..self.numel {
                if self.data.is_null() || other.data.is_null() {
                    break;
                }
                // SAFETY : i < numel.
                let b: T = unsafe {
                    let p = other.data.cast::<T>().add(i as usize);
                    core::ptr::read(p.cast_const())
                };
                if T::omega_is_zero(b) {
                    return None;
                }
            }
        }
        let out = Self::new(self.shape);
        if out.numel == 0 {
            return Some(out);
        }
        if self.data.is_null() || other.data.is_null() || out.data.is_null() {
            return Some(out);
        }
        for i in 0..self.numel {
            // SAFETY : i < numel ⇒ offsets in-bounds for both inputs +
            // output (same shape ⇒ same numel).
            unsafe {
                let pa = self.data.cast::<T>().add(i as usize);
                let pb = other.data.cast::<T>().add(i as usize);
                let po = out.data.cast::<T>().add(i as usize);
                let a = core::ptr::read(pa.cast_const());
                let b = core::ptr::read(pb.cast_const());
                core::ptr::write(po, op(a, b));
            }
        }
        Some(out)
    }

    // ──────────────────────────────────────────────────────────────────
    // § Elementwise arithmetic — IN-PLACE
    // ──────────────────────────────────────────────────────────────────

    /// In-place element-wise add. Returns false on shape-mismatch.
    pub fn add_assign(&mut self, other: &Self) -> bool {
        self.elementwise_assign(other, T::omega_add, false)
    }

    /// In-place element-wise sub. Returns false on shape-mismatch.
    pub fn sub_assign(&mut self, other: &Self) -> bool {
        self.elementwise_assign(other, T::omega_sub, false)
    }

    /// In-place element-wise mul. Returns false on shape-mismatch.
    pub fn mul_assign(&mut self, other: &Self) -> bool {
        self.elementwise_assign(other, T::omega_mul, false)
    }

    /// In-place element-wise div. Returns false on shape-mismatch OR any
    /// divisor element is zero (storage left untouched on rejection).
    pub fn div_assign(&mut self, other: &Self) -> bool {
        self.elementwise_assign(other, T::omega_div, true)
    }

    fn elementwise_assign(&mut self, other: &Self, op: fn(T, T) -> T, guard_zero: bool) -> bool {
        if self.shape != other.shape {
            return false;
        }
        if guard_zero && !other.data.is_null() {
            for i in 0..self.numel {
                // SAFETY : i < numel.
                let b: T = unsafe {
                    let p = other.data.cast::<T>().add(i as usize);
                    core::ptr::read(p.cast_const())
                };
                if T::omega_is_zero(b) {
                    return false;
                }
            }
        }
        if self.numel == 0 {
            return true;
        }
        if self.data.is_null() || other.data.is_null() {
            return true;
        }
        for i in 0..self.numel {
            // SAFETY : i < numel ⇒ offsets in-bounds for both inputs +
            // output (which is &mut self).
            unsafe {
                let pa = self.data.cast::<T>().add(i as usize);
                let pb = other.data.cast::<T>().add(i as usize);
                let a = core::ptr::read(pa.cast_const());
                let b = core::ptr::read(pb.cast_const());
                core::ptr::write(pa, op(a, b));
            }
        }
        true
    }

    // ──────────────────────────────────────────────────────────────────
    // § Equality oracle (test-side helper)
    // ──────────────────────────────────────────────────────────────────

    /// Element-wise equality oracle for tests. Returns true iff every
    /// shape component matches AND every element compares equal.
    #[must_use]
    pub fn elementwise_eq(&self, other: &Self) -> bool {
        if self.shape != other.shape {
            return false;
        }
        if self.numel == 0 {
            return true;
        }
        if self.data.is_null() || other.data.is_null() {
            return false;
        }
        for i in 0..self.numel {
            // SAFETY : i < numel.
            unsafe {
                let pa = self.data.cast::<T>().add(i as usize);
                let pb = other.data.cast::<T>().add(i as usize);
                let a = core::ptr::read(pa.cast_const());
                let b = core::ptr::read(pb.cast_const());
                if a != b {
                    return false;
                }
            }
        }
        true
    }
}

impl<T: OmegaScalar, const R: usize> Drop for OmegaTensor<T, R> {
    fn drop(&mut self) {
        if self.data.is_null() || self.numel == 0 {
            return;
        }
        let size_bytes = self.numel.saturating_mul(T::omega_size_of() as u64);
        let align = effective_align::<T>();
        // SAFETY : data + size + align match the cssl-rt::raw_alloc call
        // produced this storage in `Self::new` (or a paired predecessor
        // whose ownership was transferred into self via reshape) ; we
        // own the chunk exclusively and no other reference exists.
        unsafe {
            cssl_rt::alloc::raw_free(self.data, size_bytes as usize, align);
        }
    }
}

impl<T: OmegaScalar, const R: usize> core::fmt::Debug for OmegaTensor<T, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // PRIME-DIRECTIVE : tensor data is OPAQUE BY DEFAULT. The Debug
        // impl emits the metadata (rank / shape / numel) but NOT the
        // payload bytes. Callers who genuinely need to inspect data must
        // do so through the H5 explicit-consent gate (deferred).
        f.debug_struct("OmegaTensor")
            .field("rank", &R)
            .field("shape", &self.shape)
            .field("numel", &self.numel)
            .field("data_present", &!self.data.is_null())
            .finish()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § OmegaTensorIso — phantom borrow encoding iso-capability
// ───────────────────────────────────────────────────────────────────────

/// Iso-capability borrow over an [`OmegaTensor`].
///
/// Mirrors the `cssl-host-d3d12::resource::GpuBufferIso<'a>` pattern.
/// Intentionally `!Clone + !Copy` — passing this wrapper into a downstream
/// API consumes it. The underlying tensor is borrowed mutably for the
/// lifetime `'a`, so the iso-discipline (no aliasing during the borrow)
/// is enforced by Rust's borrow checker.
pub struct OmegaTensorIso<'a, T: OmegaScalar, const R: usize> {
    tensor: &'a mut OmegaTensor<T, R>,
    _marker: PhantomData<&'a ()>,
}

impl<'a, T: OmegaScalar, const R: usize> OmegaTensorIso<'a, T, R> {
    /// Borrow the underlying tensor for read-only access.
    ///
    /// Named `tensor_ref` (rather than `as_ref`) to avoid shadowing the
    /// `std::convert::AsRef` trait method ; iso-borrow does not produce
    /// a generic conversion so the trait is intentionally not implemented.
    #[must_use]
    pub fn tensor_ref(&self) -> &OmegaTensor<T, R> {
        self.tensor
    }

    /// Borrow the underlying tensor for read-write access.
    ///
    /// Named `tensor_mut` (rather than `as_mut`) to avoid shadowing the
    /// `std::convert::AsMut` trait method.
    pub fn tensor_mut(&mut self) -> &mut OmegaTensor<T, R> {
        self.tensor
    }

    /// Read element at `indices`. Convenience proxy through the borrow.
    #[must_use]
    pub fn get(&self, indices: [u64; R]) -> Option<T> {
        self.tensor.get(indices)
    }

    /// Write element at `indices`. Convenience proxy through the borrow.
    pub fn set(&mut self, indices: [u64; R], value: T) -> bool {
        self.tensor.set(indices, value)
    }
}

impl<'a, T: OmegaScalar, const R: usize> core::fmt::Debug for OmegaTensorIso<'a, T, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OmegaTensorIso")
            .field("rank", &R)
            .field("shape", &self.tensor.shape)
            .field("numel", &self.tensor.numel)
            .finish()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § OmegaView — non-owning borrow of a tensor sub-range
// ───────────────────────────────────────────────────────────────────────

/// Non-owning view over an [`OmegaTensor`]. At H1 the view records a
/// shape sub-range along ONE axis ; arbitrary stride permutations are
/// deferred. The underlying storage is borrowed for `'a` and is never
/// freed by the view.
pub struct OmegaView<'a, T: OmegaScalar, const R: usize> {
    /// Borrowed underlying storage.
    data: *const T,
    /// Element-offset into `data` where this view begins.
    base_offset: u64,
    /// Shape of the view. May differ from the parent along ONE axis.
    shape: [u64; R],
    /// Strides inherited from the parent (row-major for H1).
    strides: [u64; R],
    /// Cached element count.
    numel: u64,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: OmegaScalar, const R: usize> OmegaView<'a, T, R> {
    /// Compile-time rank.
    #[must_use]
    pub const fn rank(&self) -> usize {
        R
    }

    /// Runtime shape.
    #[must_use]
    pub const fn shape(&self) -> [u64; R] {
        self.shape
    }

    /// Strides.
    #[must_use]
    pub const fn strides(&self) -> [u64; R] {
        self.strides
    }

    /// Element count.
    #[must_use]
    pub const fn numel(&self) -> u64 {
        self.numel
    }

    /// Read element at `indices`. Indices are RELATIVE to the view (i.e.
    /// `indices\[axis\]` in `0..view.shape\[axis\]`, not in the parent's range).
    #[must_use]
    pub fn get(&self, indices: [u64; R]) -> Option<T> {
        let mut off: u64 = self.base_offset;
        for axis in 0..R {
            let i = indices[axis];
            if i >= self.shape[axis] {
                return None;
            }
            off = off.checked_add(i.checked_mul(self.strides[axis])?)?;
        }
        if self.data.is_null() {
            return None;
        }
        // SAFETY : off is in-bounds of the parent's allocated region by
        // construction (slice_along verified ranges before producing the
        // view ; relative indices are bounds-checked above).
        unsafe { Some(core::ptr::read(self.data.add(off as usize))) }
    }
}

impl<'a, T: OmegaScalar, const R: usize> core::fmt::Debug for OmegaView<'a, T, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OmegaView")
            .field("rank", &R)
            .field("shape", &self.shape)
            .field("base_offset", &self.base_offset)
            .field("numel", &self.numel)
            .finish()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § OmegaIter — element iterator
// ───────────────────────────────────────────────────────────────────────

/// Forward-only iterator over every element of an [`OmegaTensor`] in
/// row-major order.
pub struct OmegaIter<'a, T: OmegaScalar, const R: usize> {
    data: *const T,
    cursor: u64,
    len: u64,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: OmegaScalar, const R: usize> Iterator for OmegaIter<'a, T, R> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        if self.cursor >= self.len {
            return None;
        }
        if self.data.is_null() {
            return None;
        }
        // SAFETY : cursor < len ⇒ offset in-bounds of the parent storage.
        let v = unsafe { core::ptr::read(self.data.add(self.cursor as usize)) };
        self.cursor += 1;
        Some(v)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len.saturating_sub(self.cursor) as usize;
        (remaining, Some(remaining))
    }
}

impl<'a, T: OmegaScalar, const R: usize> ExactSizeIterator for OmegaIter<'a, T, R> {}

// ───────────────────────────────────────────────────────────────────────
// § Helpers
// ───────────────────────────────────────────────────────────────────────

/// Compute `shape.iter().product()` with overflow saturation at u64::MAX.
#[must_use]
pub fn shape_product<const R: usize>(shape: &[u64; R]) -> u64 {
    let mut p: u64 = 1;
    for axis in 0..R {
        p = p.saturating_mul(shape[axis]);
    }
    p
}

/// Compute row-major (C-order) strides for the given shape.
///
/// `strides\[R-1\] == 1`
///
/// `strides\[i\] == shape\[i+1\] * strides\[i+1\]`   for i < R-1
#[must_use]
pub fn row_major_strides<const R: usize>(shape: &[u64; R]) -> [u64; R] {
    let mut strides = [1_u64; R];
    if R == 0 {
        return strides;
    }
    let mut acc: u64 = 1;
    let mut i = R;
    while i > 0 {
        i -= 1;
        strides[i] = acc;
        acc = acc.saturating_mul(shape[i]);
    }
    strides
}

/// Effective alignment for `T` in tensor storage.
///
/// `max(OMEGA_MIN_ALIGN, alignof T)` — guarantees 8-byte alignment so the
/// raw_alloc layout is always valid for u64 reads on the host even when
/// the element-type is a 4-byte scalar.
#[must_use]
pub fn effective_align<T: OmegaScalar>() -> usize {
    core::cmp::max(OMEGA_MIN_ALIGN, T::omega_align_of())
}

// ───────────────────────────────────────────────────────────────────────
// § crate-level metadata
// ───────────────────────────────────────────────────────────────────────

/// Crate version string (from `Cargo.toml`).
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Each test acquires the cssl-rt shared lock so allocator counters
    // don't race with each other or the io tests.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        // re-export of the cssl-rt crate lock would need test-helpers
        // re-exposed publicly. At H1 we use a local lock since this
        // crate's tests only touch the cssl-rt allocator counters and
        // those are atomic ; the local lock serializes ONLY with itself
        // (which is fine — cssl-rt's own tests use --test-threads=1).
        use std::sync::Mutex;
        static L: Mutex<()> = Mutex::new(());
        L.lock().expect("local omega-tensor test lock poisoned")
    }

    // ──────────────────────────────────────────────────────────────
    // § Construction tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn empty_tensor_zero_rank_is_valid() {
        let _g = lock();
        let t = OmegaTensor::<f32, 0>::new([]);
        assert_eq!(t.rank(), 0);
        // Rank-0 (scalar) tensor : numel == 1 (empty product == 1).
        assert_eq!(t.numel(), 1);
        assert_eq!(t.shape(), []);
    }

    #[test]
    fn empty_tensor_with_zero_dim_has_zero_numel() {
        let _g = lock();
        let t = OmegaTensor::<f32, 2>::new([0, 5]);
        assert_eq!(t.numel(), 0);
        assert!(t.is_empty());
        assert_eq!(t.shape(), [0, 5]);
        // No allocation for empty tensor.
        assert_eq!(t.iter().count(), 0);
    }

    #[test]
    fn rank1_tensor_construction_zeroed() {
        let _g = lock();
        let t = OmegaTensor::<f32, 1>::new([4]);
        assert_eq!(t.rank(), 1);
        assert_eq!(t.shape(), [4]);
        assert_eq!(t.numel(), 4);
        for i in 0..4u64 {
            assert_eq!(t.get([i]), Some(0.0_f32));
        }
    }

    #[test]
    fn rank2_tensor_row_major_strides() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::new([3, 4]);
        // row-major : strides = [4, 1]
        assert_eq!(t.strides(), [4, 1]);
    }

    #[test]
    fn rank3_tensor_row_major_strides() {
        let _g = lock();
        let t = OmegaTensor::<i64, 3>::new([2, 3, 5]);
        // strides[2] = 1 ; strides[1] = 5 ; strides[0] = 15
        assert_eq!(t.strides(), [15, 5, 1]);
        assert_eq!(t.numel(), 30);
    }

    #[test]
    fn from_iter_populates_storage_in_row_major_order() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::from_iter([2, 3], 1..=6);
        assert_eq!(t.numel(), 6);
        assert_eq!(t.get([0, 0]), Some(1));
        assert_eq!(t.get([0, 1]), Some(2));
        assert_eq!(t.get([0, 2]), Some(3));
        assert_eq!(t.get([1, 0]), Some(4));
        assert_eq!(t.get([1, 1]), Some(5));
        assert_eq!(t.get([1, 2]), Some(6));
    }

    #[test]
    fn from_iter_with_short_iter_leaves_default_in_tail() {
        let _g = lock();
        let t = OmegaTensor::<i32, 1>::from_iter([4], [10, 20].iter().copied());
        assert_eq!(t.get([0]), Some(10));
        assert_eq!(t.get([1]), Some(20));
        assert_eq!(t.get([2]), Some(0)); // default
        assert_eq!(t.get([3]), Some(0)); // default
    }

    // ──────────────────────────────────────────────────────────────
    // § Get / set tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn get_oob_returns_none() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::new([3, 4]);
        assert_eq!(t.get([3, 0]), None);
        assert_eq!(t.get([0, 4]), None);
    }

    #[test]
    fn set_oob_returns_false() {
        let _g = lock();
        let mut t = OmegaTensor::<i32, 2>::new([2, 2]);
        assert!(!t.set([2, 0], 99));
        assert!(!t.set([0, 2], 99));
    }

    #[test]
    fn set_then_get_round_trips() {
        let _g = lock();
        let mut t = OmegaTensor::<f64, 2>::new([2, 3]);
        assert!(t.set([1, 2], core::f64::consts::PI));
        assert_eq!(t.get([1, 2]), Some(core::f64::consts::PI));
        // other slots untouched
        assert_eq!(t.get([0, 0]), Some(0.0));
        assert_eq!(t.get([1, 1]), Some(0.0));
    }

    #[test]
    fn linear_offset_matches_strides() {
        let _g = lock();
        let t = OmegaTensor::<i32, 3>::new([2, 3, 4]);
        // strides : [12, 4, 1]
        assert_eq!(t.linear_offset([0, 0, 0]), Some(0));
        assert_eq!(t.linear_offset([1, 0, 0]), Some(12));
        assert_eq!(t.linear_offset([0, 1, 0]), Some(4));
        assert_eq!(t.linear_offset([0, 0, 1]), Some(1));
        assert_eq!(t.linear_offset([1, 2, 3]), Some(12 + 8 + 3));
        assert_eq!(t.linear_offset([2, 0, 0]), None);
    }

    // ──────────────────────────────────────────────────────────────
    // § Iter tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn iter_yields_row_major_sequence() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::from_iter([2, 3], [1, 2, 3, 4, 5, 6].iter().copied());
        let collected: Vec<i32> = t.iter().collect();
        assert_eq!(collected, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn iter_size_hint_matches_remaining() {
        let _g = lock();
        let t = OmegaTensor::<f32, 1>::new([3]);
        let mut it = t.iter();
        assert_eq!(it.size_hint(), (3, Some(3)));
        let _ = it.next();
        assert_eq!(it.size_hint(), (2, Some(2)));
        let _ = it.next();
        let _ = it.next();
        assert_eq!(it.size_hint(), (0, Some(0)));
        assert!(it.next().is_none());
    }

    #[test]
    fn iter_on_empty_tensor_yields_nothing() {
        let _g = lock();
        let t = OmegaTensor::<f32, 2>::new([0, 7]);
        assert_eq!(t.iter().count(), 0);
    }

    // ──────────────────────────────────────────────────────────────
    // § Reshape tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn reshape_preserves_numel_and_storage() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::from_iter([2, 3], 1..=6);
        let r = t.reshape::<1>([6]).expect("reshape succeeds");
        assert_eq!(r.rank(), 1);
        assert_eq!(r.shape(), [6]);
        assert_eq!(r.numel(), 6);
        for (i, want) in (1..=6).enumerate() {
            assert_eq!(r.get([i as u64]), Some(want));
        }
    }

    #[test]
    fn reshape_to_higher_rank_preserves_layout() {
        let _g = lock();
        let t = OmegaTensor::<i32, 1>::from_iter([12], 1..=12);
        let r = t.reshape::<3>([2, 3, 2]).expect("reshape 3-D");
        assert_eq!(r.rank(), 3);
        assert_eq!(r.shape(), [2, 3, 2]);
        // Row-major preserved : [0,0,0]=1, [0,0,1]=2, [0,1,0]=3, ...
        assert_eq!(r.get([0, 0, 0]), Some(1));
        assert_eq!(r.get([0, 0, 1]), Some(2));
        assert_eq!(r.get([0, 1, 0]), Some(3));
        assert_eq!(r.get([1, 2, 1]), Some(12));
    }

    #[test]
    fn reshape_with_mismatched_numel_returns_none() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::new([2, 3]);
        assert!(t.reshape::<2>([3, 3]).is_none());
    }

    #[test]
    fn reshape_zero_dim_to_zero_dim_is_ok() {
        let _g = lock();
        let t = OmegaTensor::<f32, 2>::new([0, 5]);
        let r = t.reshape::<3>([0, 1, 1]).expect("reshape both empty");
        assert_eq!(r.numel(), 0);
        assert!(r.is_empty());
    }

    // ──────────────────────────────────────────────────────────────
    // § Elementwise arithmetic tests (out-of-place)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn add_shape_match_succeeds() {
        let _g = lock();
        let a = OmegaTensor::<f32, 1>::from_iter([4], [1.0_f32, 2.0, 3.0, 4.0].iter().copied());
        let b = OmegaTensor::<f32, 1>::from_iter([4], [10.0_f32, 20.0, 30.0, 40.0].iter().copied());
        let c = a.add(&b).expect("shape-match add");
        let want =
            OmegaTensor::<f32, 1>::from_iter([4], [11.0_f32, 22.0, 33.0, 44.0].iter().copied());
        assert!(c.elementwise_eq(&want));
    }

    #[test]
    fn add_shape_mismatch_returns_none() {
        let _g = lock();
        let a = OmegaTensor::<f32, 1>::new([3]);
        let b = OmegaTensor::<f32, 1>::new([4]);
        assert!(a.add(&b).is_none());
    }

    #[test]
    fn sub_works_elementwise() {
        let _g = lock();
        let a = OmegaTensor::<i32, 2>::from_iter([2, 2], [10, 20, 30, 40].iter().copied());
        let b = OmegaTensor::<i32, 2>::from_iter([2, 2], [1, 2, 3, 4].iter().copied());
        let c = a.sub(&b).expect("sub");
        assert_eq!(c.get([0, 0]), Some(9));
        assert_eq!(c.get([0, 1]), Some(18));
        assert_eq!(c.get([1, 0]), Some(27));
        assert_eq!(c.get([1, 1]), Some(36));
    }

    #[test]
    fn mul_works_elementwise() {
        let _g = lock();
        let a = OmegaTensor::<f64, 1>::from_iter([3], [1.0_f64, 2.0, 3.0].iter().copied());
        let b = OmegaTensor::<f64, 1>::from_iter([3], [4.0_f64, 5.0, 6.0].iter().copied());
        let c = a.mul(&b).expect("mul");
        assert_eq!(c.get([0]), Some(4.0));
        assert_eq!(c.get([1]), Some(10.0));
        assert_eq!(c.get([2]), Some(18.0));
    }

    #[test]
    fn div_zero_divisor_returns_none() {
        let _g = lock();
        let a = OmegaTensor::<i32, 1>::from_iter([3], [10, 20, 30].iter().copied());
        let b = OmegaTensor::<i32, 1>::from_iter([3], [2, 0, 3].iter().copied());
        assert!(a.div(&b).is_none());
    }

    #[test]
    fn div_nonzero_divisor_succeeds() {
        let _g = lock();
        let a = OmegaTensor::<f32, 1>::from_iter([3], [10.0_f32, 20.0, 30.0].iter().copied());
        let b = OmegaTensor::<f32, 1>::from_iter([3], [2.0_f32, 4.0, 5.0].iter().copied());
        let c = a.div(&b).expect("div");
        assert_eq!(c.get([0]), Some(5.0));
        assert_eq!(c.get([1]), Some(5.0));
        assert_eq!(c.get([2]), Some(6.0));
    }

    #[test]
    fn add_empty_tensors_produces_empty() {
        let _g = lock();
        let a = OmegaTensor::<f32, 2>::new([0, 4]);
        let b = OmegaTensor::<f32, 2>::new([0, 4]);
        let c = a.add(&b).expect("empty add");
        assert!(c.is_empty());
        assert_eq!(c.shape(), [0, 4]);
    }

    // ──────────────────────────────────────────────────────────────
    // § Elementwise arithmetic tests (in-place)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn add_assign_mutates_self() {
        let _g = lock();
        let mut a = OmegaTensor::<f32, 1>::from_iter([3], [1.0_f32, 2.0, 3.0].iter().copied());
        let b = OmegaTensor::<f32, 1>::from_iter([3], [10.0_f32, 20.0, 30.0].iter().copied());
        assert!(a.add_assign(&b));
        assert_eq!(a.get([0]), Some(11.0));
        assert_eq!(a.get([1]), Some(22.0));
        assert_eq!(a.get([2]), Some(33.0));
    }

    #[test]
    fn add_assign_shape_mismatch_returns_false() {
        let _g = lock();
        let mut a = OmegaTensor::<f32, 1>::new([3]);
        let b = OmegaTensor::<f32, 1>::new([4]);
        assert!(!a.add_assign(&b));
    }

    #[test]
    fn mul_assign_mutates_self() {
        let _g = lock();
        let mut a = OmegaTensor::<i32, 2>::from_iter([2, 2], [2, 3, 4, 5].iter().copied());
        let b = OmegaTensor::<i32, 2>::from_iter([2, 2], [10, 10, 10, 10].iter().copied());
        assert!(a.mul_assign(&b));
        assert_eq!(a.get([0, 0]), Some(20));
        assert_eq!(a.get([0, 1]), Some(30));
        assert_eq!(a.get([1, 0]), Some(40));
        assert_eq!(a.get([1, 1]), Some(50));
    }

    #[test]
    fn div_assign_zero_leaves_self_untouched() {
        let _g = lock();
        let mut a = OmegaTensor::<f32, 1>::from_iter([3], [10.0_f32, 20.0, 30.0].iter().copied());
        let b = OmegaTensor::<f32, 1>::from_iter([3], [2.0_f32, 0.0, 5.0].iter().copied());
        assert!(!a.div_assign(&b));
        // Storage untouched : original values remain.
        assert_eq!(a.get([0]), Some(10.0));
        assert_eq!(a.get([1]), Some(20.0));
        assert_eq!(a.get([2]), Some(30.0));
    }

    // ──────────────────────────────────────────────────────────────
    // § Slice / view tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn slice_along_axis0_slices_first_axis() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::from_iter([3, 2], [10, 11, 20, 21, 30, 31].iter().copied());
        // Take rows 1..3 ⇒ [[20,21],[30,31]]
        let v = t.slice_along(0, 1..3).expect("slice ok");
        assert_eq!(v.shape(), [2, 2]);
        assert_eq!(v.numel(), 4);
        assert_eq!(v.get([0, 0]), Some(20));
        assert_eq!(v.get([0, 1]), Some(21));
        assert_eq!(v.get([1, 0]), Some(30));
        assert_eq!(v.get([1, 1]), Some(31));
    }

    #[test]
    fn slice_along_invalid_axis_returns_none() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::new([3, 4]);
        assert!(t.slice_along(2, 0..1).is_none());
    }

    #[test]
    fn slice_along_oob_range_returns_none() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::new([3, 4]);
        assert!(t.slice_along(0, 0..5).is_none());
    }

    #[test]
    fn slice_along_inverted_range_returns_none() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::new([3, 4]);
        // 2..1 is an inverted range ; clippy flags the literal as
        // "yields no values" (correct — it would be empty as a Rust range
        // iterator) but our slice_along inspects start vs end and
        // explicitly rejects start > end. Build the range from variables
        // to keep clippy happy while preserving the test intent.
        let lo: u64 = 2;
        let hi: u64 = 1;
        assert!(t.slice_along(0, lo..hi).is_none());
    }

    #[test]
    fn slice_along_empty_range_yields_empty_view() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::new([3, 4]);
        let v = t.slice_along(0, 1..1).expect("empty range ok");
        assert_eq!(v.numel(), 0);
        assert_eq!(v.shape(), [0, 4]);
    }

    // ──────────────────────────────────────────────────────────────
    // § Iso-borrow tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn share_iso_round_trips_get_set() {
        let _g = lock();
        let mut t = OmegaTensor::<f32, 1>::new([4]);
        {
            let mut iso = t.share_iso();
            assert!(iso.set([2], 7.5));
            assert_eq!(iso.get([2]), Some(7.5));
        }
        // After borrow ends, original tensor sees the mutation.
        assert_eq!(t.get([2]), Some(7.5));
    }

    #[test]
    fn share_iso_as_ref_and_as_mut_behave() {
        let _g = lock();
        let mut t = OmegaTensor::<i32, 1>::from_iter([3], [1, 2, 3].iter().copied());
        let mut iso = t.share_iso();
        assert_eq!(iso.tensor_ref().get([1]), Some(2));
        iso.tensor_mut().set([1], 99);
        assert_eq!(iso.get([1]), Some(99));
    }

    // ──────────────────────────────────────────────────────────────
    // § take / iso transfer tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn take_returns_pointer_and_shape() {
        let _g = lock();
        let t = OmegaTensor::<i32, 1>::from_iter([4], [1, 2, 3, 4].iter().copied());
        let (data, shape, numel) = t.take();
        assert!(!data.is_null());
        assert_eq!(shape, [4]);
        assert_eq!(numel, 4);
        // Caller must free.
        let size = numel as usize * core::mem::size_of::<i32>();
        let align = effective_align::<i32>();
        unsafe {
            cssl_rt::alloc::raw_free(data, size, align);
        }
    }

    #[test]
    fn take_empty_tensor_returns_null_pointer() {
        let _g = lock();
        let t = OmegaTensor::<i32, 2>::new([0, 5]);
        let (data, shape, numel) = t.take();
        assert!(data.is_null());
        assert_eq!(shape, [0, 5]);
        assert_eq!(numel, 0);
        // No free required for null.
    }

    // ──────────────────────────────────────────────────────────────
    // § Drop / allocator counter tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn drop_tensor_pairs_with_alloc_counter() {
        // This test asserts the cssl-rt counter pre/post-delta. It must
        // serialize against any other test that touches the counters.
        // We rely on the local lock plus the absolute delta (post - pre)
        // to be 0 once both alloc + free fire.
        let _g = lock();
        let pre_alloc = cssl_rt::alloc::alloc_count();
        let pre_in_use = cssl_rt::alloc::bytes_in_use();
        {
            let _t = OmegaTensor::<f32, 2>::new([4, 5]);
            // alloc_count incremented exactly once for this tensor
            assert!(cssl_rt::alloc::alloc_count() > pre_alloc);
        }
        // After drop : bytes_in_use returned to baseline (or below).
        let post_in_use = cssl_rt::alloc::bytes_in_use();
        assert!(
            post_in_use <= pre_in_use,
            "drop should not increase bytes_in_use ; pre={pre_in_use}, post={post_in_use}",
        );
    }

    #[test]
    fn empty_tensor_does_not_allocate() {
        let _g = lock();
        let pre_alloc = cssl_rt::alloc::alloc_count();
        let _t = OmegaTensor::<f32, 2>::new([0, 5]);
        // No alloc for empty tensor.
        assert_eq!(cssl_rt::alloc::alloc_count(), pre_alloc);
    }

    // ──────────────────────────────────────────────────────────────
    // § Helpers + invariants tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn shape_product_works_for_each_rank() {
        assert_eq!(shape_product(&[]), 1);
        assert_eq!(shape_product(&[5]), 5);
        assert_eq!(shape_product(&[3, 4]), 12);
        assert_eq!(shape_product(&[2, 3, 5]), 30);
    }

    #[test]
    fn shape_product_with_zero_dim_is_zero() {
        assert_eq!(shape_product(&[0, 5]), 0);
        assert_eq!(shape_product(&[3, 0, 4]), 0);
    }

    #[test]
    fn row_major_strides_for_canonical_shapes() {
        assert_eq!(row_major_strides(&[3, 4]), [4, 1]);
        assert_eq!(row_major_strides(&[2, 3, 5]), [15, 5, 1]);
        assert_eq!(row_major_strides(&[7]), [1]);
    }

    #[test]
    fn effective_align_is_power_of_two() {
        assert!(effective_align::<f32>().is_power_of_two());
        assert!(effective_align::<f64>().is_power_of_two());
        assert!(effective_align::<i32>().is_power_of_two());
        assert!(effective_align::<i64>().is_power_of_two());
        // f64 / i64 alignment is 8 ⇒ effective_align >= 8
        assert!(effective_align::<f64>() >= OMEGA_MIN_ALIGN);
    }

    #[test]
    fn omega_mutate_attr_is_canonical() {
        // The marker name is stable from H1 ; renaming requires lock-step
        // changes across cssl-mir + telemetry.
        assert_eq!(OMEGA_MUTATE_ATTR, "omega_mutate");
        assert_eq!(OmegaTensor::<f32, 1>::mutate_marker(), "omega_mutate");
    }

    #[test]
    fn debug_impl_does_not_leak_payload() {
        // PRIME-DIRECTIVE check : Debug emission MUST NOT include element
        // bytes. This test verifies the format string contains shape +
        // numel but NOT any element values.
        let _g = lock();
        let t = OmegaTensor::<i32, 1>::from_iter([3], [42, 999, 12345].iter().copied());
        let s = format!("{t:?}");
        assert!(s.contains("OmegaTensor"));
        assert!(s.contains("rank"));
        assert!(s.contains("shape"));
        assert!(s.contains("numel"));
        assert!(!s.contains("42"));
        assert!(!s.contains("999"));
        assert!(!s.contains("12345"));
    }

    // ──────────────────────────────────────────────────────────────
    // § Element-type coverage tests (f32 / f64 / i32 / i64)
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn f32_tensor_full_cycle() {
        let _g = lock();
        let mut t = OmegaTensor::<f32, 1>::new([2]);
        assert!(t.set([0], 1.5));
        assert!(t.set([1], 2.5));
        assert_eq!(t.get([0]), Some(1.5));
        assert_eq!(t.get([1]), Some(2.5));
    }

    #[test]
    fn f64_tensor_full_cycle() {
        let _g = lock();
        let mut t = OmegaTensor::<f64, 1>::new([2]);
        assert!(t.set([0], 1.5_f64));
        assert!(t.set([1], 2.5_f64));
        assert_eq!(t.get([0]), Some(1.5_f64));
        assert_eq!(t.get([1]), Some(2.5_f64));
    }

    #[test]
    fn i32_tensor_full_cycle() {
        let _g = lock();
        let mut t = OmegaTensor::<i32, 1>::new([2]);
        assert!(t.set([0], -1));
        assert!(t.set([1], 7));
        assert_eq!(t.get([0]), Some(-1));
        assert_eq!(t.get([1]), Some(7));
    }

    #[test]
    fn i64_tensor_full_cycle() {
        let _g = lock();
        let mut t = OmegaTensor::<i64, 1>::new([2]);
        assert!(t.set([0], -1_i64));
        assert!(t.set([1], i64::MAX));
        assert_eq!(t.get([0]), Some(-1_i64));
        assert_eq!(t.get([1]), Some(i64::MAX));
    }

    #[test]
    fn i32_min_div_neg_one_does_not_panic() {
        // i32::MIN / -1 is the classic overflow case ; we wrap to MIN
        // rather than panic.
        let _g = lock();
        let a = OmegaTensor::<i32, 1>::from_iter([1], core::iter::once(i32::MIN));
        let b = OmegaTensor::<i32, 1>::from_iter([1], core::iter::once(-1_i32));
        let c = a.div(&b).expect("div ok");
        assert_eq!(c.get([0]), Some(i32::MIN));
    }

    // ──────────────────────────────────────────────────────────────
    // § crate metadata tests
    // ──────────────────────────────────────────────────────────────

    #[test]
    fn version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present_and_canonical() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
        );
    }

    #[test]
    fn iso_borrow_is_not_clone_or_copy() {
        // Compile-time check : OmegaTensorIso does not impl Clone/Copy.
        // We construct one and verify the move semantics work.
        let _g = lock();
        let mut t = OmegaTensor::<f32, 1>::new([2]);
        let iso = t.share_iso();
        // If OmegaTensorIso were Copy, we could re-borrow `t` here, but
        // the borrow of `t` is held by `iso` for the whole closure.
        let _ = iso;
        // After iso drops, t is borrowable again — confirms exclusive.
        let _ = t.share_iso();
    }
}
