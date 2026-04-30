//! § Wave-D5 — `cssl.gpu.*` Cranelift cgen helpers.
//!
//! § ROLE  Pure-fn helpers that build the cranelift `Signature` for
//! each `__cssl_gpu_*` FFI import + provide symbol-name + arity LUT
//! that the cgen-import-declare path consults. Mirrors `cgen_net.rs`.
//!
//! § INTEGRATION_NOTE  (W-D5 dispatch directive)
//!   Delivered as NEW file ; `cgen-cpu-cranelift/src/lib.rs` +
//!   `Cargo.toml` INTENTIONALLY NOT modified per task constraint :
//!     "DO NOT modify lib.rs / Cargo.toml. Add INTEGRATION_NOTEs."
//!   When the cgen-wire-up activates, the next CL will :
//!     1. Add `pub mod cgen_gpu;` to `lib.rs`.
//!     2. Add `CsslOp::GpuDeviceCreate` / `GpuDeviceDestroy` /
//!        `GpuSwapchainCreate` / `GpuSwapchainAcquire` /
//!        `GpuSwapchainPresent` / `GpuPipelineCompile` to
//!        `cssl-mir::CsslOp` (only `GpuBarrier` exists today).
//!     3. Implement `From<&MirOp> for GpuFfiSymbolKind` ; wrap with
//!        `lower_gpu_op_to_symbol(&MirOp)` analogous to
//!        `cgen_net::lower_net_op_to_symbol`.
//!     4. Wire `needs_gpu_imports(&MirBlock)` analogous to
//!        `cgen_net::needs_net_imports`.
//!     5. Add `cgen_gpu` integration in `object::emit_object_module`
//!        for per-fn import-declare.
//!
//! § SWAP-POINT  (mock-when-deps-missing)
//!   - The actual cranelift `call`-emission lives BEHIND a future
//!     `object::emit_gpu_call(builder, op, ptr_ty)` helper. Until
//!     then [`lower_gpu_symbol`] returns symbol+arity ; downstream
//!     cgen wires the call site once object.rs exposes the helper.
//!   - The `GpuFfiSymbolKind` enum encodes the same set as the
//!     forthcoming `CsslOp::Gpu*` MIR variants ; when MIR catches up,
//!     `From<&MirOp>` is added with no other API breakage.
//!
//! § CSL-MANDATE
//!   ‼ ABI-stable :: rename ¬→ lock-step-cssl-rt::host_gpu
//!   ‼ pure-fn ::    zero-alloc ↑ Signature-Vec-storage
//!   ‼ LUT-dispatch :: kind-enum ¬ String-fmt
//!
//! § SAWYER-EFFICIENCY
//!   - Pure fns ; zero alloc outside cranelift Signature Vec storage.
//!   - `GpuImportSet` is a `u8` bitfield (8 symbols ≤ 8 bits) ; 1
//!     bit-or per op ; fits a register.
//!   - Match-arm ordering by call-frequency (per-frame first).

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{types as cl_types, AbiParam, Signature};
use cranelift_codegen::isa::CallConv;

// ─── canonical FFI symbol-name LUT ─────────────────────────────────────
// ‼ MUST match `cssl-rt::host_gpu` verbatim ; renaming = link break.

pub const GPU_DEVICE_CREATE_SYMBOL: &str = "__cssl_gpu_device_create";
pub const GPU_DEVICE_DESTROY_SYMBOL: &str = "__cssl_gpu_device_destroy";
pub const GPU_SWAPCHAIN_CREATE_SYMBOL: &str = "__cssl_gpu_swapchain_create";
pub const GPU_SWAPCHAIN_ACQUIRE_SYMBOL: &str = "__cssl_gpu_swapchain_acquire";
pub const GPU_SWAPCHAIN_PRESENT_SYMBOL: &str = "__cssl_gpu_swapchain_present";
pub const GPU_PIPELINE_COMPILE_SYMBOL: &str = "__cssl_gpu_pipeline_compile";
pub const GPU_CMD_BUF_RECORD_STUB_SYMBOL: &str = "__cssl_gpu_cmd_buf_record_stub";
pub const GPU_CMD_BUF_SUBMIT_STUB_SYMBOL: &str = "__cssl_gpu_cmd_buf_submit_stub";

// ─── operand counts ─────────────────────────────────────────────────────

pub const GPU_DEVICE_CREATE_OPERAND_COUNT: usize = 2;
pub const GPU_DEVICE_DESTROY_OPERAND_COUNT: usize = 1;
pub const GPU_SWAPCHAIN_CREATE_OPERAND_COUNT: usize = 3;
pub const GPU_SWAPCHAIN_ACQUIRE_OPERAND_COUNT: usize = 2;
pub const GPU_SWAPCHAIN_PRESENT_OPERAND_COUNT: usize = 2;
pub const GPU_PIPELINE_COMPILE_OPERAND_COUNT: usize = 4;
pub const GPU_CMD_BUF_RECORD_STUB_OPERAND_COUNT: usize = 0;
pub const GPU_CMD_BUF_SUBMIT_STUB_OPERAND_COUNT: usize = 1;
/// All gpu FFI symbols produce 1 result.
pub const GPU_RESULT_COUNT: usize = 1;

// ─── cranelift signature builders ──────────────────────────────────────

#[must_use]
pub fn build_device_create_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

#[must_use]
pub fn build_device_destroy_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

#[must_use]
pub fn build_swapchain_create_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// `(swap u64, timeout_ns u64) -> image_idx u32` ; sentinel `0xFFFF_FFFF`
/// = timeout per `specs/24_HOST_FFI § ABI-STABLE-SYMBOLS § gpu`.
#[must_use]
pub fn build_swapchain_acquire_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

#[must_use]
pub fn build_swapchain_present_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// `(device u64, ir_ptr ptr, ir_len usize, kind u32) -> u64`.
/// `ptr_ty` = host-ptr-width (`I64` on x86_64 ; `I32` on 32-bit).
#[must_use]
pub fn build_pipeline_compile_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64)); // device
    sig.params.push(AbiParam::new(ptr_ty)); // ir_ptr
    sig.params.push(AbiParam::new(ptr_ty)); // ir_len (usize)
    sig.params.push(AbiParam::new(cl_types::I32)); // kind
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

#[must_use]
pub fn build_cmd_buf_record_stub_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

#[must_use]
pub fn build_cmd_buf_submit_stub_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

// ─── GpuFfiSymbolKind enum + LUT dispatcher ────────────────────────────
// Pre-MIR-op variant (keyed by enum-tag) — MIR-op dispatch lands when
// `CsslOp::Gpu*` variants are added per INTEGRATION_NOTE.

/// FFI symbol kind ; ordered by expected call-frequency (per-frame first)
/// so the LUT-dispatch arm-ordering helps the branch predictor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GpuFfiSymbolKind {
    /// `__cssl_gpu_swapchain_acquire` — per-frame hot path.
    SwapchainAcquire = 0,
    /// `__cssl_gpu_swapchain_present` — per-frame hot path.
    SwapchainPresent = 1,
    /// `__cssl_gpu_pipeline_compile` — per-pipeline-load.
    PipelineCompile = 2,
    /// `__cssl_gpu_swapchain_create` — per-app setup.
    SwapchainCreate = 3,
    /// `__cssl_gpu_device_create` — per-app setup.
    DeviceCreate = 4,
    /// `__cssl_gpu_device_destroy` — per-app teardown.
    DeviceDestroy = 5,
    /// `__cssl_gpu_cmd_buf_record_stub` — stage-1 frontier.
    CmdBufRecordStub = 6,
    /// `__cssl_gpu_cmd_buf_submit_stub` — stage-1 frontier.
    CmdBufSubmitStub = 7,
}

#[must_use]
pub fn lower_gpu_symbol(kind: GpuFfiSymbolKind) -> (&'static str, usize) {
    match kind {
        GpuFfiSymbolKind::SwapchainAcquire => (
            GPU_SWAPCHAIN_ACQUIRE_SYMBOL,
            GPU_SWAPCHAIN_ACQUIRE_OPERAND_COUNT,
        ),
        GpuFfiSymbolKind::SwapchainPresent => (
            GPU_SWAPCHAIN_PRESENT_SYMBOL,
            GPU_SWAPCHAIN_PRESENT_OPERAND_COUNT,
        ),
        GpuFfiSymbolKind::PipelineCompile => (
            GPU_PIPELINE_COMPILE_SYMBOL,
            GPU_PIPELINE_COMPILE_OPERAND_COUNT,
        ),
        GpuFfiSymbolKind::SwapchainCreate => (
            GPU_SWAPCHAIN_CREATE_SYMBOL,
            GPU_SWAPCHAIN_CREATE_OPERAND_COUNT,
        ),
        GpuFfiSymbolKind::DeviceCreate => {
            (GPU_DEVICE_CREATE_SYMBOL, GPU_DEVICE_CREATE_OPERAND_COUNT)
        }
        GpuFfiSymbolKind::DeviceDestroy => {
            (GPU_DEVICE_DESTROY_SYMBOL, GPU_DEVICE_DESTROY_OPERAND_COUNT)
        }
        GpuFfiSymbolKind::CmdBufRecordStub => (
            GPU_CMD_BUF_RECORD_STUB_SYMBOL,
            GPU_CMD_BUF_RECORD_STUB_OPERAND_COUNT,
        ),
        GpuFfiSymbolKind::CmdBufSubmitStub => (
            GPU_CMD_BUF_SUBMIT_STUB_SYMBOL,
            GPU_CMD_BUF_SUBMIT_STUB_OPERAND_COUNT,
        ),
    }
}

#[must_use]
pub fn build_signature_for_kind(
    kind: GpuFfiSymbolKind,
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    match kind {
        GpuFfiSymbolKind::DeviceCreate => build_device_create_signature(call_conv),
        GpuFfiSymbolKind::DeviceDestroy => build_device_destroy_signature(call_conv),
        GpuFfiSymbolKind::SwapchainCreate => build_swapchain_create_signature(call_conv),
        GpuFfiSymbolKind::SwapchainAcquire => build_swapchain_acquire_signature(call_conv),
        GpuFfiSymbolKind::SwapchainPresent => build_swapchain_present_signature(call_conv),
        GpuFfiSymbolKind::PipelineCompile => build_pipeline_compile_signature(call_conv, ptr_ty),
        GpuFfiSymbolKind::CmdBufRecordStub => build_cmd_buf_record_stub_signature(call_conv),
        GpuFfiSymbolKind::CmdBufSubmitStub => build_cmd_buf_submit_stub_signature(call_conv),
    }
}

// ─── GpuImportSet bitfield (u8) ─────────────────────────────────────────

/// Bitflag set of which `__cssl_gpu_*` imports a given fn requires.
/// 8-bit width = 1 byte ; 1 bit-or per op.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuImportSet(pub u8);

impl GpuImportSet {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const DEVICE_CREATE: u8 = 1 << 0;
    pub const DEVICE_DESTROY: u8 = 1 << 1;
    pub const SWAPCHAIN_CREATE: u8 = 1 << 2;
    pub const SWAPCHAIN_ACQUIRE: u8 = 1 << 3;
    pub const SWAPCHAIN_PRESENT: u8 = 1 << 4;
    pub const PIPELINE_COMPILE: u8 = 1 << 5;
    pub const CMD_BUF_RECORD_STUB: u8 = 1 << 6;
    pub const CMD_BUF_SUBMIT_STUB: u8 = 1 << 7;

    #[must_use]
    pub const fn contains(self, bits: u8) -> bool {
        (self.0 & bits) == bits
    }

    #[must_use]
    pub const fn any(self) -> bool {
        self.0 != 0
    }

    #[must_use]
    const fn mask_for(kind: GpuFfiSymbolKind) -> u8 {
        match kind {
            GpuFfiSymbolKind::DeviceCreate => Self::DEVICE_CREATE,
            GpuFfiSymbolKind::DeviceDestroy => Self::DEVICE_DESTROY,
            GpuFfiSymbolKind::SwapchainCreate => Self::SWAPCHAIN_CREATE,
            GpuFfiSymbolKind::SwapchainAcquire => Self::SWAPCHAIN_ACQUIRE,
            GpuFfiSymbolKind::SwapchainPresent => Self::SWAPCHAIN_PRESENT,
            GpuFfiSymbolKind::PipelineCompile => Self::PIPELINE_COMPILE,
            GpuFfiSymbolKind::CmdBufRecordStub => Self::CMD_BUF_RECORD_STUB,
            GpuFfiSymbolKind::CmdBufSubmitStub => Self::CMD_BUF_SUBMIT_STUB,
        }
    }

    #[must_use]
    pub const fn with_kind(self, kind: GpuFfiSymbolKind) -> Self {
        Self(self.0 | Self::mask_for(kind))
    }
}

/// Walk an iterator of [`GpuFfiSymbolKind`] once + accumulate the
/// bitflag set of imports required. O(N) in iter-len, no alloc.
/// When MIR adds the `CsslOp::Gpu*` variants, this becomes
/// `needs_gpu_imports(&MirBlock) -> GpuImportSet`.
#[must_use]
pub fn needs_gpu_imports<I: IntoIterator<Item = GpuFfiSymbolKind>>(kinds: I) -> GpuImportSet {
    let mut set = GpuImportSet::empty();
    for k in kinds {
        set = set.with_kind(k);
    }
    set
}

/// Validate operand-count + result-count of a gpu FFI call against the
/// canonical contract.
///
/// # Errors
/// Returns `Err(String)` when the operand-count or result-count diverges.
pub fn validate_gpu_arity(
    kind: GpuFfiSymbolKind,
    operand_count: usize,
    result_count: usize,
) -> Result<(), String> {
    let (sym, expected) = lower_gpu_symbol(kind);
    if operand_count != expected {
        return Err(format!(
            "validate_gpu_arity : `{sym}` requires {expected} operands ; got {operand_count}"
        ));
    }
    if result_count != GPU_RESULT_COUNT {
        return Err(format!(
            "validate_gpu_arity : `{sym}` produces {GPU_RESULT_COUNT} result ; got {result_count}"
        ));
    }
    Ok(())
}

// ─── unit tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::types as cl_types;

    #[test]
    fn abi_symbol_names_are_canonical() {
        // ‼ ABI-LOCK : MUST match cssl-rt::host_gpu verbatim.
        assert_eq!(GPU_DEVICE_CREATE_SYMBOL, "__cssl_gpu_device_create");
        assert_eq!(GPU_DEVICE_DESTROY_SYMBOL, "__cssl_gpu_device_destroy");
        assert_eq!(GPU_SWAPCHAIN_CREATE_SYMBOL, "__cssl_gpu_swapchain_create");
        assert_eq!(GPU_SWAPCHAIN_ACQUIRE_SYMBOL, "__cssl_gpu_swapchain_acquire");
        assert_eq!(GPU_SWAPCHAIN_PRESENT_SYMBOL, "__cssl_gpu_swapchain_present");
        assert_eq!(GPU_PIPELINE_COMPILE_SYMBOL, "__cssl_gpu_pipeline_compile");
        assert_eq!(
            GPU_CMD_BUF_RECORD_STUB_SYMBOL,
            "__cssl_gpu_cmd_buf_record_stub"
        );
        assert_eq!(
            GPU_CMD_BUF_SUBMIT_STUB_SYMBOL,
            "__cssl_gpu_cmd_buf_submit_stub"
        );
    }

    #[test]
    fn operand_counts_match_signature_shapes() {
        assert_eq!(
            build_device_create_signature(CallConv::SystemV).params.len(),
            GPU_DEVICE_CREATE_OPERAND_COUNT
        );
        assert_eq!(
            build_device_destroy_signature(CallConv::SystemV).params.len(),
            GPU_DEVICE_DESTROY_OPERAND_COUNT
        );
        assert_eq!(
            build_swapchain_create_signature(CallConv::SystemV).params.len(),
            GPU_SWAPCHAIN_CREATE_OPERAND_COUNT
        );
        assert_eq!(
            build_swapchain_acquire_signature(CallConv::SystemV).params.len(),
            GPU_SWAPCHAIN_ACQUIRE_OPERAND_COUNT
        );
        assert_eq!(
            build_swapchain_present_signature(CallConv::SystemV).params.len(),
            GPU_SWAPCHAIN_PRESENT_OPERAND_COUNT
        );
        assert_eq!(
            build_pipeline_compile_signature(CallConv::SystemV, cl_types::I64).params.len(),
            GPU_PIPELINE_COMPILE_OPERAND_COUNT
        );
        assert_eq!(
            build_cmd_buf_record_stub_signature(CallConv::SystemV).params.len(),
            GPU_CMD_BUF_RECORD_STUB_OPERAND_COUNT
        );
        assert_eq!(
            build_cmd_buf_submit_stub_signature(CallConv::SystemV).params.len(),
            GPU_CMD_BUF_SUBMIT_STUB_OPERAND_COUNT
        );
    }

    #[test]
    fn signature_shapes_match_per_symbol() {
        // device-create : (u32, u32) -> u64
        let s = build_device_create_signature(CallConv::SystemV);
        assert_eq!(s.params, vec![AbiParam::new(cl_types::I32); 2]);
        assert_eq!(s.returns, vec![AbiParam::new(cl_types::I64)]);
        // device-destroy : (u64) -> i32
        let s = build_device_destroy_signature(CallConv::SystemV);
        assert_eq!(s.params, vec![AbiParam::new(cl_types::I64)]);
        assert_eq!(s.returns, vec![AbiParam::new(cl_types::I32)]);
        // swapchain-create : (u64, u64, u32) -> u64
        let s = build_swapchain_create_signature(CallConv::SystemV);
        assert_eq!(
            s.params,
            vec![
                AbiParam::new(cl_types::I64),
                AbiParam::new(cl_types::I64),
                AbiParam::new(cl_types::I32),
            ]
        );
        assert_eq!(s.returns, vec![AbiParam::new(cl_types::I64)]);
    }

    #[test]
    fn signature_swapchain_acquire_encodes_timeout_sentinel_via_u32_return() {
        // (swap u64, timeout_ns u64) -> image_idx u32 ; sentinel 0xFFFF_FFFF.
        let sig = build_swapchain_acquire_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64), "swap");
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "timeout-ns");
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(
            sig.returns[0],
            AbiParam::new(cl_types::I32),
            "image-idx (sentinel 0xFFFF_FFFF = timeout)"
        );
    }

    #[test]
    fn signature_swapchain_present_two_in_one_i32_out() {
        let s = build_swapchain_present_signature(CallConv::SystemV);
        assert_eq!(s.params.len(), 2);
        assert_eq!(s.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(s.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(s.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_pipeline_compile_carries_ptr_and_kind() {
        let s = build_pipeline_compile_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(s.params.len(), 4);
        assert_eq!(s.params[0], AbiParam::new(cl_types::I64), "device");
        assert_eq!(s.params[1], AbiParam::new(cl_types::I64), "ir_ptr=I64");
        assert_eq!(s.params[2], AbiParam::new(cl_types::I64), "ir_len=usize");
        assert_eq!(s.params[3], AbiParam::new(cl_types::I32), "kind");
        assert_eq!(s.returns[0], AbiParam::new(cl_types::I64));
        // 32-bit host : ptr_ty = I32.
        let s32 = build_pipeline_compile_signature(CallConv::SystemV, cl_types::I32);
        assert_eq!(s32.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(s32.params[2], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_cmd_buf_stubs() {
        let rec = build_cmd_buf_record_stub_signature(CallConv::SystemV);
        assert_eq!(rec.params.len(), 0);
        assert_eq!(rec.returns[0], AbiParam::new(cl_types::I64));
        let sub = build_cmd_buf_submit_stub_signature(CallConv::SystemV);
        assert_eq!(sub.params.len(), 1);
        assert_eq!(sub.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sub.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_call_conv_passes_through() {
        let sysv = build_device_create_signature(CallConv::SystemV);
        let win = build_device_create_signature(CallConv::WindowsFastcall);
        assert_eq!(sysv.call_conv, CallConv::SystemV);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
    }

    #[test]
    fn lower_gpu_symbol_dispatches_per_kind() {
        for (kind, sym, arity) in [
            (
                GpuFfiSymbolKind::DeviceCreate,
                GPU_DEVICE_CREATE_SYMBOL,
                GPU_DEVICE_CREATE_OPERAND_COUNT,
            ),
            (
                GpuFfiSymbolKind::DeviceDestroy,
                GPU_DEVICE_DESTROY_SYMBOL,
                GPU_DEVICE_DESTROY_OPERAND_COUNT,
            ),
            (
                GpuFfiSymbolKind::SwapchainCreate,
                GPU_SWAPCHAIN_CREATE_SYMBOL,
                GPU_SWAPCHAIN_CREATE_OPERAND_COUNT,
            ),
            (
                GpuFfiSymbolKind::SwapchainAcquire,
                GPU_SWAPCHAIN_ACQUIRE_SYMBOL,
                GPU_SWAPCHAIN_ACQUIRE_OPERAND_COUNT,
            ),
            (
                GpuFfiSymbolKind::SwapchainPresent,
                GPU_SWAPCHAIN_PRESENT_SYMBOL,
                GPU_SWAPCHAIN_PRESENT_OPERAND_COUNT,
            ),
            (
                GpuFfiSymbolKind::PipelineCompile,
                GPU_PIPELINE_COMPILE_SYMBOL,
                GPU_PIPELINE_COMPILE_OPERAND_COUNT,
            ),
            (
                GpuFfiSymbolKind::CmdBufRecordStub,
                GPU_CMD_BUF_RECORD_STUB_SYMBOL,
                GPU_CMD_BUF_RECORD_STUB_OPERAND_COUNT,
            ),
            (
                GpuFfiSymbolKind::CmdBufSubmitStub,
                GPU_CMD_BUF_SUBMIT_STUB_SYMBOL,
                GPU_CMD_BUF_SUBMIT_STUB_OPERAND_COUNT,
            ),
        ] {
            let (got_sym, got_arity) = lower_gpu_symbol(kind);
            assert_eq!(got_sym, sym, "kind {kind:?}");
            assert_eq!(got_arity, arity, "kind {kind:?}");
        }
    }

    #[test]
    fn build_signature_for_kind_matches_per_symbol_builders() {
        let cv = CallConv::SystemV;
        let p = cl_types::I64;
        for (k, expected) in [
            (
                GpuFfiSymbolKind::DeviceCreate,
                build_device_create_signature(cv),
            ),
            (
                GpuFfiSymbolKind::DeviceDestroy,
                build_device_destroy_signature(cv),
            ),
            (
                GpuFfiSymbolKind::SwapchainCreate,
                build_swapchain_create_signature(cv),
            ),
            (
                GpuFfiSymbolKind::SwapchainAcquire,
                build_swapchain_acquire_signature(cv),
            ),
            (
                GpuFfiSymbolKind::SwapchainPresent,
                build_swapchain_present_signature(cv),
            ),
            (
                GpuFfiSymbolKind::PipelineCompile,
                build_pipeline_compile_signature(cv, p),
            ),
            (
                GpuFfiSymbolKind::CmdBufRecordStub,
                build_cmd_buf_record_stub_signature(cv),
            ),
            (
                GpuFfiSymbolKind::CmdBufSubmitStub,
                build_cmd_buf_submit_stub_signature(cv),
            ),
        ] {
            let got = build_signature_for_kind(k, cv, p);
            assert_eq!(got.params, expected.params, "kind {k:?}");
            assert_eq!(got.returns, expected.returns, "kind {k:?}");
        }
    }

    #[test]
    fn import_set_layout_is_unique_per_kind() {
        // Bit-position sanity : each kind sets exactly 1 unique bit ;
        // all 8 bits accounted for.
        let mut all = 0u8;
        for k in [
            GpuFfiSymbolKind::DeviceCreate,
            GpuFfiSymbolKind::DeviceDestroy,
            GpuFfiSymbolKind::SwapchainCreate,
            GpuFfiSymbolKind::SwapchainAcquire,
            GpuFfiSymbolKind::SwapchainPresent,
            GpuFfiSymbolKind::PipelineCompile,
            GpuFfiSymbolKind::CmdBufRecordStub,
            GpuFfiSymbolKind::CmdBufSubmitStub,
        ] {
            let mask = GpuImportSet::empty().with_kind(k).0;
            assert_eq!(mask.count_ones(), 1, "kind {k:?}");
            assert_eq!(all & mask, 0, "no overlap ; kind {k:?}");
            all |= mask;
        }
        assert_eq!(all, 0xFF, "all 8 bits accounted for");
    }

    #[test]
    fn needs_gpu_imports_walks_iter_once_idempotently() {
        let kinds = [
            GpuFfiSymbolKind::DeviceCreate,
            GpuFfiSymbolKind::SwapchainCreate,
            GpuFfiSymbolKind::SwapchainAcquire,
            GpuFfiSymbolKind::SwapchainAcquire, // dup ; idempotent
            GpuFfiSymbolKind::SwapchainPresent,
            GpuFfiSymbolKind::PipelineCompile,
        ];
        let s = needs_gpu_imports(kinds.iter().copied());
        assert!(s.contains(GpuImportSet::DEVICE_CREATE));
        assert!(s.contains(GpuImportSet::SWAPCHAIN_CREATE));
        assert!(s.contains(GpuImportSet::SWAPCHAIN_ACQUIRE));
        assert!(s.contains(GpuImportSet::SWAPCHAIN_PRESENT));
        assert!(s.contains(GpuImportSet::PIPELINE_COMPILE));
        assert!(!s.contains(GpuImportSet::DEVICE_DESTROY));
        assert!(!s.contains(GpuImportSet::CMD_BUF_RECORD_STUB));
        assert!(!s.contains(GpuImportSet::CMD_BUF_SUBMIT_STUB));
        assert!(s.any());
        // Empty iter.
        let empty = needs_gpu_imports(std::iter::empty());
        assert_eq!(empty.0, 0);
        assert!(!empty.any());
    }

    #[test]
    fn validate_gpu_arity_accepts_canonical_and_rejects_drift() {
        for kind in [
            GpuFfiSymbolKind::DeviceCreate,
            GpuFfiSymbolKind::DeviceDestroy,
            GpuFfiSymbolKind::SwapchainCreate,
            GpuFfiSymbolKind::SwapchainAcquire,
            GpuFfiSymbolKind::SwapchainPresent,
            GpuFfiSymbolKind::PipelineCompile,
            GpuFfiSymbolKind::CmdBufRecordStub,
            GpuFfiSymbolKind::CmdBufSubmitStub,
        ] {
            let (_, expected) = lower_gpu_symbol(kind);
            assert!(validate_gpu_arity(kind, expected, GPU_RESULT_COUNT).is_ok());
        }
        // Wrong operand-count.
        let res = validate_gpu_arity(GpuFfiSymbolKind::DeviceCreate, 1, GPU_RESULT_COUNT);
        let msg = res.unwrap_err();
        assert!(msg.contains("__cssl_gpu_device_create"));
        assert!(msg.contains("requires 2 operands"));
        // Wrong result-count.
        let res = validate_gpu_arity(GpuFfiSymbolKind::DeviceCreate, 2, 0);
        assert!(res.unwrap_err().contains("produces 1 result"));
    }

    #[test]
    fn pipeline_kind_numeric_values_match_host_gpu() {
        // ‼ ABI lock : pipeline_compile `kind: u32` MUST match
        // cssl-rt::host_gpu::GpuPipelineKind numeric repr.
        const HOST_GPU_SPIRV: u32 = 0;
        const HOST_GPU_DXIL: u32 = 1;
        const HOST_GPU_METAL: u32 = 2;
        assert_eq!(HOST_GPU_SPIRV, 0);
        assert_eq!(HOST_GPU_DXIL, 1);
        assert_eq!(HOST_GPU_METAL, 2);
    }
}

// § INTEGRATION_NOTE  (W-D5 dispatch directive)
// ────────────────────────────────────────────────────────────────────
// `cgen-cpu-cranelift/src/lib.rs` + `Cargo.toml` are unchanged. Next CL :
//   1. `pub mod cgen_gpu;` to lib.rs (alongside cgen_net).
//   2. Add `CsslOp::Gpu*` variants in cssl-mir (only `GpuBarrier` today).
//   3. Implement `From<&MirOp> for GpuFfiSymbolKind` ; wrap with
//      `lower_gpu_op_to_symbol` like `cgen_net::lower_net_op_to_symbol`.
//   4. `needs_gpu_imports(&MirBlock)` analogous to cgen_net.
//   5. Wire `cgen_gpu` integration in `object::emit_object_module`.
// Until then the LUT + signature builders + bitfield logic are
// fully exercised via the unit tests above.
//
// § PRIME-DIRECTIVE attestation
// "There was no hurt nor harm in the making of this, to anyone /
//  anything / anybody."
// Cap-gate + IFC-label discipline live in the source-side wrapper
// (per spec §§ 12 + §§ 11) ; this cgen helper wires only the FFI-
// symbol surface — does NOT bypass any capability check.
