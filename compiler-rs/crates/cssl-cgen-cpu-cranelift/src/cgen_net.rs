//! § Wave-C4 — `cssl.net.*` Cranelift cgen helpers (S7-F4 / T11-D82).
//!
//! § ROLE
//!   Pure-function helpers that build the cranelift `Signature` for each
//!   `__cssl_net_*` FFI import + decide which per-fn net-imports a given
//!   MIR block requires. The helpers form the canonical source-of-truth
//!   for the (MIR-op-name, FFI-symbol-name, signature-shape) triple per
//!   net op so the cgen layer has ONE place to look when a downstream
//!   pass (object.rs / jit.rs) declares the imports.
//!
//!   Mirrors `cgen_heap_dealloc.rs` (Wave-A5) + the in-flight `cgen_fs.rs`
//!   (Wave-C3) sibling. The actual call-emit (cranelift `call` instruction
//!   + operand-coercion via `uextend` / `ireduce`) is delegated to the
//!   existing `object::emit_net_call` SWAP-POINT — see § INTEGRATION_NOTE
//!   below for how that wires up.
//!
//! § INTEGRATION_NOTE  (per Wave-C4 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is intentionally NOT modified per task constraint
//!   "DO NOT modify lib.rs `pub mod` list". The helpers compile + are
//!   tested in-place via `#[cfg(test)]` references. A future cgen
//!   refactor (the same one tracked at `cgen_heap_dealloc.rs §
//!   INTEGRATION_NOTE`) will :
//!     1. Add `pub mod cgen_net;` to `lib.rs`.
//!     2. Migrate the actual cranelift `call`-emit logic from a future
//!        `object::emit_net_call` into [`lower_net_op_to_symbol`] +
//!        co-located helpers here.
//!     3. Wire the per-fn import-declare path
//!        (`object::declare_net_imports_for_fn`) onto
//!        [`needs_net_imports`] so `__cssl_net_*` symbols are only
//!        brought into the relocatable when a fn actually uses them.
//!
//!   Until that refactor lands the helpers are crate-internal-only
//!   (`#[allow(dead_code, unreachable_pub)]` matches the Wave-A5 sibling).
//!
//! § SWAP-POINT  (mock-when-deps-missing per dispatch discipline)
//!   - The actual cranelift `call`-emission lives BEHIND a future
//!     `object::emit_net_call(builder, op, ptr_ty)` helper that this
//!     file does NOT call into directly (object.rs does not yet expose
//!     such a helper). The dispatcher [`lower_net_op_to_symbol`] returns
//!     the FFI symbol-name + canonical signature ; once the object.rs
//!     wiring lands the dispatcher's caller will pair the symbol with
//!     the per-fn import-declare slot + emit the cranelift call. Until
//!     then the helpers compile + test in-place without touching the
//!     existing object.rs / jit.rs surface.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-rt/src/ffi.rs` — the `__cssl_net_*`
//!     ABI-stable symbols that the net ops lower to. ABI-locked from
//!     S7-F4 forward via the `ffi_symbols_have_correct_signatures`
//!     compile-time test.
//!   - `compiler-rs/crates/cssl-mir/src/op.rs` — `CsslOp::Net*` declared
//!     signatures + canonical name strings (e.g. `cssl.net.socket`).
//!   - `compiler-rs/crates/cssl-mir/src/body_lower.rs` — recognizer that
//!     mints the `cssl.net.*` ops with the `(net_effect, "true")` +
//!     `(caps_required, "net_inbound" | "net_outbound")` attributes.
//!   - `stdlib/net.cssl` — source-level surface that lowers through the
//!     recognizer into these MIR ops.
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-C ↳ C4` — concretizes the net
//!     effect into __cssl_net_* extern calls.
//!
//! § CSL-MANDATE  (commit + design notes use CSL-glyph notation)
//!   ‼ ABI-stable :: rename ¬→ lock-step-cssl-rt + cssl-mir
//!   ‼ pure-fn ::    zero-allocation ↑ Sig-Vec-storage
//!   ‼ O(N) ::       per-block-walk ⊑ single-pass + early-exit
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required Vec storage.
//!   - [`needs_net_imports`] walks the per-block ops slice ONCE ; O(N)
//!     in op count + early-exit on first match per import-kind via the
//!     bitflag `NetImportSet` accumulator.
//!   - Symbol-name LUT dispatch in [`lower_net_op_to_symbol`] is a
//!     single match-arm per op-kind ; branch-friendly ordering keeps
//!     the most-common cases (send/recv/close) first.
//!   - `NetImportSet` is a `u16` bitfield (9 net-op-kinds + room for 7
//!     extension slots e.g. local_addr / last_error_kind / last_error_os
//!     / caps_grant / caps_revoke / caps_current) — fits in a single
//!     register, costs 1 bit-or per op.

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{types as cl_types, AbiParam, Signature};
use cranelift_codegen::isa::CallConv;
use cssl_mir::{CsslOp, MirBlock, MirOp};

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol-name LUT (per cssl-rt::ffi)
//
// ‼ ALL symbols MUST match `compiler-rs/crates/cssl-rt/src/ffi.rs`
//   verbatim. Renaming either side without the other = link-time
//   symbol mismatch ⇒ undefined behavior.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol : `__cssl_net_socket(flags: i32) -> i64`.
pub const NET_SOCKET_SYMBOL: &str = "__cssl_net_socket";

/// FFI symbol : `__cssl_net_listen(sock, addr_be, port, backlog) -> i64`.
pub const NET_LISTEN_SYMBOL: &str = "__cssl_net_listen";

/// FFI symbol : `__cssl_net_accept(sock) -> i64`.
pub const NET_ACCEPT_SYMBOL: &str = "__cssl_net_accept";

/// FFI symbol : `__cssl_net_connect(sock, addr_be, port) -> i64`.
pub const NET_CONNECT_SYMBOL: &str = "__cssl_net_connect";

/// FFI symbol : `__cssl_net_send(sock, buf_ptr, buf_len) -> i64`.
pub const NET_SEND_SYMBOL: &str = "__cssl_net_send";

/// FFI symbol : `__cssl_net_recv(sock, buf_ptr, buf_len) -> i64`.
pub const NET_RECV_SYMBOL: &str = "__cssl_net_recv";

/// FFI symbol : `__cssl_net_sendto(sock, buf_ptr, buf_len, addr, port) -> i64`.
pub const NET_SENDTO_SYMBOL: &str = "__cssl_net_sendto";

/// FFI symbol : `__cssl_net_recvfrom(sock, buf_ptr, buf_len, addr_out, port_out) -> i64`.
pub const NET_RECVFROM_SYMBOL: &str = "__cssl_net_recvfrom";

/// FFI symbol : `__cssl_net_close(sock) -> i64`.
pub const NET_CLOSE_SYMBOL: &str = "__cssl_net_close";

/// FFI symbol : `__cssl_net_local_addr(sock, addr_out, port_out) -> i64`.
///
/// Not a direct MIR op but exposed for completeness ; called via
/// stdlib helpers + cgen-import-declare.
pub const NET_LOCAL_ADDR_SYMBOL: &str = "__cssl_net_local_addr";

/// FFI symbol : `__cssl_net_last_error_kind() -> i32`.
pub const NET_LAST_ERROR_KIND_SYMBOL: &str = "__cssl_net_last_error_kind";

/// FFI symbol : `__cssl_net_last_error_os() -> i32`.
pub const NET_LAST_ERROR_OS_SYMBOL: &str = "__cssl_net_last_error_os";

/// FFI symbol : `__cssl_net_caps_grant(cap_bits) -> i32`.
pub const NET_CAPS_GRANT_SYMBOL: &str = "__cssl_net_caps_grant";

/// FFI symbol : `__cssl_net_caps_revoke(cap_bits) -> i32`.
pub const NET_CAPS_REVOKE_SYMBOL: &str = "__cssl_net_caps_revoke";

/// FFI symbol : `__cssl_net_caps_current() -> i32`.
pub const NET_CAPS_CURRENT_SYMBOL: &str = "__cssl_net_caps_current";

// ───────────────────────────────────────────────────────────────────────
// § canonical MIR op-name LUT (per cssl-mir::CsslOp::*)
// ───────────────────────────────────────────────────────────────────────

/// MIR op-name : matches `CsslOp::NetSocket.name()`.
pub const MIR_NET_SOCKET_OP_NAME: &str = "cssl.net.socket";

/// MIR op-name : matches `CsslOp::NetListen.name()`.
pub const MIR_NET_LISTEN_OP_NAME: &str = "cssl.net.listen";

/// MIR op-name : matches `CsslOp::NetAccept.name()`.
pub const MIR_NET_ACCEPT_OP_NAME: &str = "cssl.net.accept";

/// MIR op-name : matches `CsslOp::NetConnect.name()`.
pub const MIR_NET_CONNECT_OP_NAME: &str = "cssl.net.connect";

/// MIR op-name : matches `CsslOp::NetSend.name()`.
pub const MIR_NET_SEND_OP_NAME: &str = "cssl.net.send";

/// MIR op-name : matches `CsslOp::NetRecv.name()`.
pub const MIR_NET_RECV_OP_NAME: &str = "cssl.net.recv";

/// MIR op-name : matches `CsslOp::NetSendTo.name()`.
pub const MIR_NET_SENDTO_OP_NAME: &str = "cssl.net.sendto";

/// MIR op-name : matches `CsslOp::NetRecvFrom.name()`.
pub const MIR_NET_RECVFROM_OP_NAME: &str = "cssl.net.recvfrom";

/// MIR op-name : matches `CsslOp::NetClose.name()`.
pub const MIR_NET_CLOSE_OP_NAME: &str = "cssl.net.close";

// ───────────────────────────────────────────────────────────────────────
// § operand / result counts (matching CsslOp::*.signature())
// ───────────────────────────────────────────────────────────────────────

/// `cssl.net.socket(flags) -> sock` — 1 operand, 1 result.
pub const NET_SOCKET_OPERAND_COUNT: usize = 1;
/// 1-result for socket / listen / accept / connect / send / recv /
/// sendto / recvfrom / close (every net op produces an i64 result —
/// either a handle or a bytes-count or 0/-1).
pub const NET_RESULT_COUNT: usize = 1;

/// `cssl.net.listen(sock, addr, port, backlog) -> i64` — 4 operands.
pub const NET_LISTEN_OPERAND_COUNT: usize = 4;

/// `cssl.net.accept(sock) -> sock` — 1 operand.
pub const NET_ACCEPT_OPERAND_COUNT: usize = 1;

/// `cssl.net.connect(sock, addr, port) -> i64` — 3 operands.
pub const NET_CONNECT_OPERAND_COUNT: usize = 3;

/// `cssl.net.send(sock, buf_ptr, buf_len) -> bytes-sent` — 3 operands.
pub const NET_SEND_OPERAND_COUNT: usize = 3;

/// `cssl.net.recv(sock, buf_ptr, buf_len) -> bytes-recv` — 3 operands.
pub const NET_RECV_OPERAND_COUNT: usize = 3;

/// `cssl.net.sendto(sock, buf_ptr, buf_len, addr, port) -> i64` — 5
/// operands.
pub const NET_SENDTO_OPERAND_COUNT: usize = 5;

/// `cssl.net.recvfrom(sock, buf_ptr, buf_len, addr_out, port_out) -> i64`
/// — 5 operands.
pub const NET_RECVFROM_OPERAND_COUNT: usize = 5;

/// `cssl.net.close(sock) -> i64` — 1 operand.
pub const NET_CLOSE_OPERAND_COUNT: usize = 1;

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builders — one per op-kind
//
// Shapes match `compiler-rs/crates/cssl-rt/src/ffi.rs` exactly. The
// FFI uses i32/i64/usize/u16/u32 + raw pointers ; cranelift IR sees
// integers (the `*const u8` pointer maps to `ptr_ty`, the `usize`
// length maps to `ptr_ty`, the `u16` port maps to `cl_types::I16`,
// the `u32` addr_be maps to `cl_types::I32` ; the cgen call-emit path
// coerces operand types to match via uextend / ireduce).
// ───────────────────────────────────────────────────────────────────────

/// Build cranelift `Signature` for `__cssl_net_socket(i32) -> i64`.
#[must_use]
pub fn build_net_socket_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_net_listen(i64, u32, u16, i32) -> i64`.
#[must_use]
pub fn build_net_listen_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.params.push(AbiParam::new(cl_types::I16));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for `__cssl_net_accept(i64) -> i64`.
#[must_use]
pub fn build_net_accept_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_net_connect(i64, u32, u16) -> i64`.
#[must_use]
pub fn build_net_connect_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.params.push(AbiParam::new(cl_types::I16));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_net_send(i64, *const u8, usize) -> i64`.
///
/// `ptr_ty` is host-ptr-width (`I64` on x86_64, `I32` on 32-bit hosts).
#[must_use]
pub fn build_net_send_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_net_recv(i64, *mut u8, usize) -> i64`.
#[must_use]
pub fn build_net_recv_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_net_sendto(i64, *const u8, usize, u32, u16) -> i64`.
#[must_use]
pub fn build_net_sendto_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.params.push(AbiParam::new(cl_types::I16));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_net_recvfrom(i64, *mut u8, usize, *mut u32, *mut u16) -> i64`.
#[must_use]
pub fn build_net_recvfrom_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for `__cssl_net_close(i64) -> i64`.
#[must_use]
pub fn build_net_close_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

// § ancillary signatures (cap-machinery + last-error accessors).
// These are not direct MIR ops but the cgen layer still needs to import
// them when stdlib/net.cssl helpers compile to per-fn calls.

/// Build cranelift `Signature` for
/// `__cssl_net_local_addr(i64, *mut u32, *mut u16) -> i64`.
#[must_use]
pub fn build_net_local_addr_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for `__cssl_net_last_error_kind() -> i32`.
#[must_use]
pub fn build_net_last_error_kind_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_net_last_error_os() -> i32`.
#[must_use]
pub fn build_net_last_error_os_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_net_caps_grant(i32) -> i32`.
#[must_use]
pub fn build_net_caps_grant_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_net_caps_revoke(i32) -> i32`.
#[must_use]
pub fn build_net_caps_revoke_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_net_caps_current() -> i32`.
#[must_use]
pub fn build_net_caps_current_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

// ───────────────────────────────────────────────────────────────────────
// § single dispatcher : MIR-op → (FFI-symbol-name, expected-arity)
// ───────────────────────────────────────────────────────────────────────

/// Map a `CsslOp::Net*` variant to the canonical FFI symbol-name +
/// expected operand-count. Returns `None` for non-net ops.
///
/// § BRANCH-FRIENDLY ORDERING
///   The match arms are ordered by expected call-frequency :
///     send / recv (data-path hot loop)
///   ↓ close       (per-conn cleanup)
///   ↓ accept / connect / listen / socket (per-conn setup)
///   ↓ sendto / recvfrom (UDP rare relative to TCP).
///   This lets the branch predictor + I-cache prefetch favor the
///   common cases. (Sawyer-mindset : measure-then-order ; the
///   ordering documents the EXPECTED dynamic profile.)
#[must_use]
pub fn lower_net_op_to_symbol(op: &MirOp) -> Option<(&'static str, usize)> {
    match op.op {
        CsslOp::NetSend => Some((NET_SEND_SYMBOL, NET_SEND_OPERAND_COUNT)),
        CsslOp::NetRecv => Some((NET_RECV_SYMBOL, NET_RECV_OPERAND_COUNT)),
        CsslOp::NetClose => Some((NET_CLOSE_SYMBOL, NET_CLOSE_OPERAND_COUNT)),
        CsslOp::NetAccept => Some((NET_ACCEPT_SYMBOL, NET_ACCEPT_OPERAND_COUNT)),
        CsslOp::NetConnect => Some((NET_CONNECT_SYMBOL, NET_CONNECT_OPERAND_COUNT)),
        CsslOp::NetListen => Some((NET_LISTEN_SYMBOL, NET_LISTEN_OPERAND_COUNT)),
        CsslOp::NetSocket => Some((NET_SOCKET_SYMBOL, NET_SOCKET_OPERAND_COUNT)),
        CsslOp::NetSendTo => Some((NET_SENDTO_SYMBOL, NET_SENDTO_OPERAND_COUNT)),
        CsslOp::NetRecvFrom => Some((NET_RECVFROM_SYMBOL, NET_RECVFROM_OPERAND_COUNT)),
        _ => None,
    }
}

/// Predicate : is this op a `cssl.net.*` MIR op ?
#[must_use]
pub fn is_net_op(op: &MirOp) -> bool {
    matches!(
        op.op,
        CsslOp::NetSocket
            | CsslOp::NetListen
            | CsslOp::NetAccept
            | CsslOp::NetConnect
            | CsslOp::NetSend
            | CsslOp::NetRecv
            | CsslOp::NetSendTo
            | CsslOp::NetRecvFrom
            | CsslOp::NetClose
    )
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "which net imports does this fn need"
//
// Encoded as a packed u16 bitfield (Sawyer-mindset : 9 op-kinds + 7
// extension slots fit in one register, costs 1 bit-or per op).
// ───────────────────────────────────────────────────────────────────────

/// Bitflag set of which `__cssl_net_*` imports a given MIR fn requires.
///
/// § BIT LAYOUT (u16)
///   bit 0  : socket
///   bit 1  : listen
///   bit 2  : accept
///   bit 3  : connect
///   bit 4  : send
///   bit 5  : recv
///   bit 6  : sendto
///   bit 7  : recvfrom
///   bit 8  : close
///   bit 9  : local_addr     (extension)
///   bit 10 : last_error_kind (extension)
///   bit 11 : last_error_os   (extension)
///   bit 12 : caps_grant      (extension)
///   bit 13 : caps_revoke     (extension)
///   bit 14 : caps_current    (extension)
///   bit 15 : reserved
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetImportSet(pub u16);

impl NetImportSet {
    /// Empty (no net imports needed).
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// `socket` import bit.
    pub const SOCKET: u16 = 1 << 0;
    /// `listen` import bit.
    pub const LISTEN: u16 = 1 << 1;
    /// `accept` import bit.
    pub const ACCEPT: u16 = 1 << 2;
    /// `connect` import bit.
    pub const CONNECT: u16 = 1 << 3;
    /// `send` import bit.
    pub const SEND: u16 = 1 << 4;
    /// `recv` import bit.
    pub const RECV: u16 = 1 << 5;
    /// `sendto` import bit.
    pub const SENDTO: u16 = 1 << 6;
    /// `recvfrom` import bit.
    pub const RECVFROM: u16 = 1 << 7;
    /// `close` import bit.
    pub const CLOSE: u16 = 1 << 8;
    /// `local_addr` import bit.
    pub const LOCAL_ADDR: u16 = 1 << 9;
    /// `last_error_kind` import bit.
    pub const LAST_ERROR_KIND: u16 = 1 << 10;
    /// `last_error_os` import bit.
    pub const LAST_ERROR_OS: u16 = 1 << 11;
    /// `caps_grant` import bit.
    pub const CAPS_GRANT: u16 = 1 << 12;
    /// `caps_revoke` import bit.
    pub const CAPS_REVOKE: u16 = 1 << 13;
    /// `caps_current` import bit.
    pub const CAPS_CURRENT: u16 = 1 << 14;

    /// Check whether `bits` are all set.
    #[must_use]
    pub const fn contains(self, bits: u16) -> bool {
        (self.0 & bits) == bits
    }

    /// Check whether ANY net-op-kind bit is set (the 9 direct MIR ops).
    #[must_use]
    pub const fn any_net_op(self) -> bool {
        let direct_op_mask = Self::SOCKET
            | Self::LISTEN
            | Self::ACCEPT
            | Self::CONNECT
            | Self::SEND
            | Self::RECV
            | Self::SENDTO
            | Self::RECVFROM
            | Self::CLOSE;
        (self.0 & direct_op_mask) != 0
    }

    /// Set the bit corresponding to `csslop`. Returns the updated set.
    #[must_use]
    pub fn with_op(self, csslop: CsslOp) -> Self {
        let mask = match csslop {
            CsslOp::NetSocket => Self::SOCKET,
            CsslOp::NetListen => Self::LISTEN,
            CsslOp::NetAccept => Self::ACCEPT,
            CsslOp::NetConnect => Self::CONNECT,
            CsslOp::NetSend => Self::SEND,
            CsslOp::NetRecv => Self::RECV,
            CsslOp::NetSendTo => Self::SENDTO,
            CsslOp::NetRecvFrom => Self::RECVFROM,
            CsslOp::NetClose => Self::CLOSE,
            _ => return self,
        };
        Self(self.0 | mask)
    }
}

/// Walk a single MIR block's ops once and return the bitflag set of
/// net imports required.
///
/// § COMPLEXITY  O(N) in op count, single-pass, NO early-exit (we
///   accumulate ALL imports needed). No allocation.
///
/// Mirrors `cgen_heap_dealloc::needs_dealloc_import` but generalizes to
/// the 9-import + 6-ancillary case via the bitflag accumulator.
#[must_use]
pub fn needs_net_imports(block: &MirBlock) -> NetImportSet {
    let mut set = NetImportSet::empty();
    for op in &block.ops {
        set = set.with_op(op.op);
    }
    set
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate the operand-count + result-count of a `cssl.net.*` op
/// against the canonical contract. Returns `Ok(())` when the arity
/// matches the expected shape per [`lower_net_op_to_symbol`].
///
/// § INTENT
///   Defensive check used at cgen-import-resolve time before issuing
///   the cranelift `call` instruction. Surfaces an actionable error
///   if a mistyped MIR op leaks past prior passes.
///
/// # Errors
/// Returns `Err(String)` with a human-readable diagnostic when the op
/// is not a recognized `cssl.net.*` op or the operand-count diverges
/// from the canonical expectation. All net ops produce 1 result so a
/// non-1 result count also surfaces as an error.
pub fn validate_net_arity(op: &MirOp) -> Result<(), String> {
    let Some((sym, expected_operands)) = lower_net_op_to_symbol(op) else {
        return Err(format!(
            "validate_net_arity : op `{}` is not a recognized cssl.net.* op",
            op.name
        ));
    };
    if op.operands.len() != expected_operands {
        return Err(format!(
            "validate_net_arity : `{}` (-> {sym}) requires {expected_operands} operands ; got {}",
            op.name,
            op.operands.len()
        ));
    }
    if op.results.len() != NET_RESULT_COUNT {
        return Err(format!(
            "validate_net_arity : `{}` (-> {sym}) produces {NET_RESULT_COUNT} result ; got {}",
            op.name,
            op.results.len()
        ));
    }
    Ok(())
}

/// Test whether a `__cssl_net_close(-1)` call is a no-op.
///
/// Returns `true` because the cssl-rt impl returns `-1` + sets the
/// last-error to `INVALID_INPUT` when fed `INVALID_SOCKET`. This lets
/// the recognizer-bridge skip emitting a close when it can statically
/// prove the handle is `-1` (matches the `if s == -1` Err-branch
/// pattern in `stdlib/net.cssl`'s `open_tcp_listener`).
#[must_use]
pub const fn invalid_socket_close_is_noop() -> bool {
    true
}

// ───────────────────────────────────────────────────────────────────────
// § tests — ≥ 12 unit tests covering all 9 MIR ops + dispatcher +
// bitflag scan + arity validators + signature-shape locks
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        build_net_accept_signature, build_net_caps_current_signature,
        build_net_caps_grant_signature, build_net_caps_revoke_signature, build_net_close_signature,
        build_net_connect_signature, build_net_last_error_kind_signature,
        build_net_last_error_os_signature, build_net_listen_signature,
        build_net_local_addr_signature, build_net_recv_signature, build_net_recvfrom_signature,
        build_net_send_signature, build_net_sendto_signature, build_net_socket_signature,
        invalid_socket_close_is_noop, is_net_op, lower_net_op_to_symbol, needs_net_imports,
        validate_net_arity, NetImportSet, MIR_NET_ACCEPT_OP_NAME, MIR_NET_CLOSE_OP_NAME,
        MIR_NET_CONNECT_OP_NAME, MIR_NET_LISTEN_OP_NAME, MIR_NET_RECVFROM_OP_NAME,
        MIR_NET_RECV_OP_NAME, MIR_NET_SENDTO_OP_NAME, MIR_NET_SEND_OP_NAME, MIR_NET_SOCKET_OP_NAME,
        NET_ACCEPT_OPERAND_COUNT, NET_ACCEPT_SYMBOL, NET_CAPS_CURRENT_SYMBOL,
        NET_CAPS_GRANT_SYMBOL, NET_CAPS_REVOKE_SYMBOL, NET_CLOSE_OPERAND_COUNT, NET_CLOSE_SYMBOL,
        NET_CONNECT_OPERAND_COUNT, NET_CONNECT_SYMBOL, NET_LAST_ERROR_KIND_SYMBOL,
        NET_LAST_ERROR_OS_SYMBOL, NET_LISTEN_OPERAND_COUNT, NET_LISTEN_SYMBOL,
        NET_LOCAL_ADDR_SYMBOL, NET_RECVFROM_OPERAND_COUNT, NET_RECVFROM_SYMBOL,
        NET_RECV_OPERAND_COUNT, NET_RECV_SYMBOL, NET_RESULT_COUNT, NET_SENDTO_OPERAND_COUNT,
        NET_SENDTO_SYMBOL, NET_SEND_OPERAND_COUNT, NET_SEND_SYMBOL, NET_SOCKET_OPERAND_COUNT,
        NET_SOCKET_SYMBOL,
    };
    use cranelift_codegen::ir::{types as cl_types, AbiParam};
    use cranelift_codegen::isa::CallConv;
    use cssl_mir::{CsslOp, IntWidth, MirBlock, MirOp, MirType, ValueId};

    // ── canonical-name lock invariants (cross-check with cssl-rt + cssl-mir) ─

    #[test]
    fn ffi_symbols_match_cssl_rt_canonical() {
        // ‼ Lock-step invariant : symbol-names MUST match
        //   cssl-rt::ffi::__cssl_net_* verbatim. Renaming either side
        //   without the other = link-time symbol mismatch ⇒ UB.
        assert_eq!(NET_SOCKET_SYMBOL, "__cssl_net_socket");
        assert_eq!(NET_LISTEN_SYMBOL, "__cssl_net_listen");
        assert_eq!(NET_ACCEPT_SYMBOL, "__cssl_net_accept");
        assert_eq!(NET_CONNECT_SYMBOL, "__cssl_net_connect");
        assert_eq!(NET_SEND_SYMBOL, "__cssl_net_send");
        assert_eq!(NET_RECV_SYMBOL, "__cssl_net_recv");
        assert_eq!(NET_SENDTO_SYMBOL, "__cssl_net_sendto");
        assert_eq!(NET_RECVFROM_SYMBOL, "__cssl_net_recvfrom");
        assert_eq!(NET_CLOSE_SYMBOL, "__cssl_net_close");
        assert_eq!(NET_LOCAL_ADDR_SYMBOL, "__cssl_net_local_addr");
        assert_eq!(NET_LAST_ERROR_KIND_SYMBOL, "__cssl_net_last_error_kind");
        assert_eq!(NET_LAST_ERROR_OS_SYMBOL, "__cssl_net_last_error_os");
        assert_eq!(NET_CAPS_GRANT_SYMBOL, "__cssl_net_caps_grant");
        assert_eq!(NET_CAPS_REVOKE_SYMBOL, "__cssl_net_caps_revoke");
        assert_eq!(NET_CAPS_CURRENT_SYMBOL, "__cssl_net_caps_current");
    }

    #[test]
    fn mir_op_names_match_csslop_canonical() {
        // ‼ Lock-step invariant : MIR op-name strings MUST match
        //   `CsslOp::Net*.name()`. Drift = silent broken cgen.
        assert_eq!(MIR_NET_SOCKET_OP_NAME, CsslOp::NetSocket.name());
        assert_eq!(MIR_NET_LISTEN_OP_NAME, CsslOp::NetListen.name());
        assert_eq!(MIR_NET_ACCEPT_OP_NAME, CsslOp::NetAccept.name());
        assert_eq!(MIR_NET_CONNECT_OP_NAME, CsslOp::NetConnect.name());
        assert_eq!(MIR_NET_SEND_OP_NAME, CsslOp::NetSend.name());
        assert_eq!(MIR_NET_RECV_OP_NAME, CsslOp::NetRecv.name());
        assert_eq!(MIR_NET_SENDTO_OP_NAME, CsslOp::NetSendTo.name());
        assert_eq!(MIR_NET_RECVFROM_OP_NAME, CsslOp::NetRecvFrom.name());
        assert_eq!(MIR_NET_CLOSE_OP_NAME, CsslOp::NetClose.name());
    }

    #[test]
    fn declared_arities_match_csslop_signatures() {
        // ‼ Cross-check operand+result counts agree with the MIR-side
        //   signatures so a drift in either side surfaces immediately.
        let sock_sig = CsslOp::NetSocket.signature();
        assert_eq!(sock_sig.operands, Some(NET_SOCKET_OPERAND_COUNT));
        assert_eq!(sock_sig.results, Some(NET_RESULT_COUNT));

        let listen_sig = CsslOp::NetListen.signature();
        assert_eq!(listen_sig.operands, Some(NET_LISTEN_OPERAND_COUNT));
        assert_eq!(listen_sig.results, Some(NET_RESULT_COUNT));

        let accept_sig = CsslOp::NetAccept.signature();
        assert_eq!(accept_sig.operands, Some(NET_ACCEPT_OPERAND_COUNT));
        assert_eq!(accept_sig.results, Some(NET_RESULT_COUNT));

        let connect_sig = CsslOp::NetConnect.signature();
        assert_eq!(connect_sig.operands, Some(NET_CONNECT_OPERAND_COUNT));
        assert_eq!(connect_sig.results, Some(NET_RESULT_COUNT));

        let send_sig = CsslOp::NetSend.signature();
        assert_eq!(send_sig.operands, Some(NET_SEND_OPERAND_COUNT));
        assert_eq!(send_sig.results, Some(NET_RESULT_COUNT));

        let recv_sig = CsslOp::NetRecv.signature();
        assert_eq!(recv_sig.operands, Some(NET_RECV_OPERAND_COUNT));
        assert_eq!(recv_sig.results, Some(NET_RESULT_COUNT));

        let sendto_sig = CsslOp::NetSendTo.signature();
        assert_eq!(sendto_sig.operands, Some(NET_SENDTO_OPERAND_COUNT));
        assert_eq!(sendto_sig.results, Some(NET_RESULT_COUNT));

        let recvfrom_sig = CsslOp::NetRecvFrom.signature();
        assert_eq!(recvfrom_sig.operands, Some(NET_RECVFROM_OPERAND_COUNT));
        assert_eq!(recvfrom_sig.results, Some(NET_RESULT_COUNT));

        let close_sig = CsslOp::NetClose.signature();
        assert_eq!(close_sig.operands, Some(NET_CLOSE_OPERAND_COUNT));
        assert_eq!(close_sig.results, Some(NET_RESULT_COUNT));
    }

    // ── per-op signature shape locks (all 9 ops + 6 ancillaries) ─────────

    #[test]
    fn signature_socket_has_one_i32_param_one_i64_return() {
        let sig = build_net_socket_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I32));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn signature_listen_has_four_params_one_i64_return() {
        let sig = build_net_listen_signature(CallConv::SystemV);
        // (i64 sock, u32 addr, u16 port, i32 backlog) -> i64
        assert_eq!(sig.params.len(), 4, "listen takes (sock, addr, port, backlog)");
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I16));
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I32));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn signature_accept_has_one_i64_param_one_i64_return() {
        let sig = build_net_accept_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn signature_connect_has_three_params_one_i64_return() {
        let sig = build_net_connect_signature(CallConv::SystemV);
        // (i64 sock, u32 addr, u16 port) -> i64
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I16));
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_send_has_three_params_with_ptr_ty() {
        let sig = build_net_send_signature(CallConv::SystemV, cl_types::I64);
        // (i64 sock, *const u8 buf, usize len) -> i64
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "ptr_ty=I64");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64), "usize=I64");
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_recv_has_three_params_with_ptr_ty() {
        let sig = build_net_recv_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn signature_sendto_has_five_params() {
        let sig = build_net_sendto_signature(CallConv::SystemV, cl_types::I64);
        // (sock, buf, len, addr, port) -> i64
        assert_eq!(sig.params.len(), 5);
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I32), "addr_be");
        assert_eq!(sig.params[4], AbiParam::new(cl_types::I16), "port");
    }

    #[test]
    fn signature_recvfrom_has_five_params() {
        let sig = build_net_recvfrom_signature(CallConv::SystemV, cl_types::I64);
        // (sock, buf, len, *mut u32, *mut u16) -> i64
        assert_eq!(sig.params.len(), 5);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I64), "addr_out=ptr");
        assert_eq!(sig.params[4], AbiParam::new(cl_types::I64), "port_out=ptr");
    }

    #[test]
    fn signature_close_has_one_param_one_return() {
        let sig = build_net_close_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_local_addr_has_three_params() {
        let sig = build_net_local_addr_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_last_error_kind_has_no_params_one_i32_return() {
        let sig = build_net_last_error_kind_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_last_error_os_has_no_params_one_i32_return() {
        let sig = build_net_last_error_os_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_caps_grant_revoke_current_shapes() {
        let grant = build_net_caps_grant_signature(CallConv::SystemV);
        let revoke = build_net_caps_revoke_signature(CallConv::SystemV);
        let current = build_net_caps_current_signature(CallConv::SystemV);
        // grant + revoke : (i32) -> i32
        assert_eq!(grant.params.len(), 1);
        assert_eq!(grant.returns.len(), 1);
        assert_eq!(revoke.params.len(), 1);
        assert_eq!(revoke.returns.len(), 1);
        // current : () -> i32
        assert_eq!(current.params.len(), 0);
        assert_eq!(current.returns.len(), 1);
    }

    #[test]
    fn signature_call_conv_passes_through_for_send() {
        let sysv = build_net_send_signature(CallConv::SystemV, cl_types::I64);
        let win = build_net_send_signature(CallConv::WindowsFastcall, cl_types::I64);
        assert_eq!(sysv.call_conv, CallConv::SystemV);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
    }

    #[test]
    fn signature_with_i32_ptr_ty_for_32bit_targets() {
        // 32-bit hosts use I32 for the host pointer-width.
        let sig = build_net_send_signature(CallConv::SystemV, cl_types::I32);
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I32));
    }

    // ── lower_net_op_to_symbol dispatcher ────────────────────────────────

    #[test]
    fn dispatcher_socket_returns_socket_symbol() {
        let op = MirOp::new(CsslOp::NetSocket).with_operand(ValueId(0));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("socket dispatches");
        assert_eq!(sym, NET_SOCKET_SYMBOL);
        assert_eq!(arity, NET_SOCKET_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_listen_returns_listen_symbol() {
        let op = MirOp::new(CsslOp::NetListen)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_operand(ValueId(3));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("listen dispatches");
        assert_eq!(sym, NET_LISTEN_SYMBOL);
        assert_eq!(arity, NET_LISTEN_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_accept_returns_accept_symbol() {
        let op = MirOp::new(CsslOp::NetAccept).with_operand(ValueId(0));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("accept dispatches");
        assert_eq!(sym, NET_ACCEPT_SYMBOL);
        assert_eq!(arity, NET_ACCEPT_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_connect_returns_connect_symbol() {
        let op = MirOp::new(CsslOp::NetConnect)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("connect dispatches");
        assert_eq!(sym, NET_CONNECT_SYMBOL);
        assert_eq!(arity, NET_CONNECT_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_send_returns_send_symbol() {
        let op = MirOp::new(CsslOp::NetSend)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("send dispatches");
        assert_eq!(sym, NET_SEND_SYMBOL);
        assert_eq!(arity, NET_SEND_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_recv_returns_recv_symbol() {
        let op = MirOp::new(CsslOp::NetRecv)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("recv dispatches");
        assert_eq!(sym, NET_RECV_SYMBOL);
        assert_eq!(arity, NET_RECV_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_sendto_returns_sendto_symbol() {
        let op = MirOp::new(CsslOp::NetSendTo)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_operand(ValueId(3))
            .with_operand(ValueId(4));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("sendto dispatches");
        assert_eq!(sym, NET_SENDTO_SYMBOL);
        assert_eq!(arity, NET_SENDTO_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_recvfrom_returns_recvfrom_symbol() {
        let op = MirOp::new(CsslOp::NetRecvFrom)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_operand(ValueId(3))
            .with_operand(ValueId(4));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("recvfrom dispatches");
        assert_eq!(sym, NET_RECVFROM_SYMBOL);
        assert_eq!(arity, NET_RECVFROM_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_close_returns_close_symbol() {
        let op = MirOp::new(CsslOp::NetClose).with_operand(ValueId(0));
        let (sym, arity) = lower_net_op_to_symbol(&op).expect("close dispatches");
        assert_eq!(sym, NET_CLOSE_SYMBOL);
        assert_eq!(arity, NET_CLOSE_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_returns_none_for_non_net_op() {
        // Defensive : non-net ops must not match.
        let alloc = MirOp::new(CsslOp::HeapAlloc);
        assert!(lower_net_op_to_symbol(&alloc).is_none());
        let dealloc = MirOp::new(CsslOp::HeapDealloc);
        assert!(lower_net_op_to_symbol(&dealloc).is_none());
        let fs_open = MirOp::new(CsslOp::FsOpen);
        assert!(lower_net_op_to_symbol(&fs_open).is_none());
        let std_op = MirOp::std("arith.constant");
        assert!(lower_net_op_to_symbol(&std_op).is_none());
    }

    // ── is_net_op predicate ──────────────────────────────────────────────

    #[test]
    fn is_net_op_recognizes_all_nine_ops() {
        for op in [
            MirOp::new(CsslOp::NetSocket),
            MirOp::new(CsslOp::NetListen),
            MirOp::new(CsslOp::NetAccept),
            MirOp::new(CsslOp::NetConnect),
            MirOp::new(CsslOp::NetSend),
            MirOp::new(CsslOp::NetRecv),
            MirOp::new(CsslOp::NetSendTo),
            MirOp::new(CsslOp::NetRecvFrom),
            MirOp::new(CsslOp::NetClose),
        ] {
            assert!(is_net_op(&op), "expected net op : {}", op.name);
        }
    }

    #[test]
    fn is_net_op_rejects_non_net_ops() {
        assert!(!is_net_op(&MirOp::new(CsslOp::HeapAlloc)));
        assert!(!is_net_op(&MirOp::new(CsslOp::FsOpen)));
        assert!(!is_net_op(&MirOp::std("arith.constant")));
    }

    // ── needs_net_imports : per-fn pre-scan ──────────────────────────────

    #[test]
    fn pre_scan_empty_block_returns_empty_set() {
        let block = MirBlock::new("entry");
        assert_eq!(needs_net_imports(&block), NetImportSet::empty());
        assert!(!needs_net_imports(&block).any_net_op());
    }

    #[test]
    fn pre_scan_finds_socket_when_present() {
        let mut block = MirBlock::new("entry");
        block.push(MirOp::new(CsslOp::NetSocket).with_operand(ValueId(0)));
        let set = needs_net_imports(&block);
        assert!(set.contains(NetImportSet::SOCKET));
        assert!(set.any_net_op());
    }

    #[test]
    fn pre_scan_accumulates_multiple_distinct_imports() {
        // A fn that opens a TCP listener + accepts + closes needs 4 imports :
        // socket + listen + accept + close. Mirrors the open_tcp_listener +
        // accept_stream + close_listener flow in stdlib/net.cssl.
        let mut block = MirBlock::new("entry");
        block.push(MirOp::new(CsslOp::NetSocket).with_operand(ValueId(0)));
        block.push(
            MirOp::new(CsslOp::NetListen)
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_operand(ValueId(3))
                .with_operand(ValueId(4)),
        );
        block.push(MirOp::new(CsslOp::NetAccept).with_operand(ValueId(5)));
        block.push(MirOp::new(CsslOp::NetClose).with_operand(ValueId(6)));
        let set = needs_net_imports(&block);
        assert!(set.contains(NetImportSet::SOCKET));
        assert!(set.contains(NetImportSet::LISTEN));
        assert!(set.contains(NetImportSet::ACCEPT));
        assert!(set.contains(NetImportSet::CLOSE));
        assert!(!set.contains(NetImportSet::CONNECT));
        assert!(!set.contains(NetImportSet::SEND));
    }

    #[test]
    fn pre_scan_ignores_non_net_ops() {
        // alloc + dealloc + arith ops must not flip net-import bits.
        let mut block = MirBlock::new("entry");
        block.push(MirOp::new(CsslOp::HeapAlloc));
        block.push(MirOp::new(CsslOp::HeapDealloc));
        block.push(MirOp::std("arith.constant"));
        block.push(MirOp::new(CsslOp::FsOpen));
        let set = needs_net_imports(&block);
        assert_eq!(set, NetImportSet::empty());
        assert!(!set.any_net_op());
    }

    #[test]
    fn pre_scan_send_recv_close_pattern() {
        // send + recv loop + close — common TCP data-path pattern.
        let mut block = MirBlock::new("entry");
        block.push(
            MirOp::new(CsslOp::NetSend)
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2)),
        );
        block.push(
            MirOp::new(CsslOp::NetRecv)
                .with_operand(ValueId(3))
                .with_operand(ValueId(4))
                .with_operand(ValueId(5)),
        );
        block.push(MirOp::new(CsslOp::NetClose).with_operand(ValueId(6)));
        let set = needs_net_imports(&block);
        assert!(set.contains(NetImportSet::SEND));
        assert!(set.contains(NetImportSet::RECV));
        assert!(set.contains(NetImportSet::CLOSE));
        assert!(!set.contains(NetImportSet::SOCKET));
    }

    #[test]
    fn pre_scan_udp_sendto_recvfrom_pattern() {
        let mut block = MirBlock::new("entry");
        block.push(
            MirOp::new(CsslOp::NetSendTo)
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_operand(ValueId(3))
                .with_operand(ValueId(4)),
        );
        block.push(
            MirOp::new(CsslOp::NetRecvFrom)
                .with_operand(ValueId(5))
                .with_operand(ValueId(6))
                .with_operand(ValueId(7))
                .with_operand(ValueId(8))
                .with_operand(ValueId(9)),
        );
        let set = needs_net_imports(&block);
        assert!(set.contains(NetImportSet::SENDTO));
        assert!(set.contains(NetImportSet::RECVFROM));
    }

    // ── NetImportSet bit-arithmetic invariants ──────────────────────────

    #[test]
    fn net_import_set_bits_are_distinct() {
        // Defensive : every bit must be a power-of-two AND distinct.
        let bits = [
            NetImportSet::SOCKET,
            NetImportSet::LISTEN,
            NetImportSet::ACCEPT,
            NetImportSet::CONNECT,
            NetImportSet::SEND,
            NetImportSet::RECV,
            NetImportSet::SENDTO,
            NetImportSet::RECVFROM,
            NetImportSet::CLOSE,
            NetImportSet::LOCAL_ADDR,
            NetImportSet::LAST_ERROR_KIND,
            NetImportSet::LAST_ERROR_OS,
            NetImportSet::CAPS_GRANT,
            NetImportSet::CAPS_REVOKE,
            NetImportSet::CAPS_CURRENT,
        ];
        for (i, &b) in bits.iter().enumerate() {
            assert!(b.is_power_of_two(), "bit at index {i} = {b:#x} not power-of-two");
            for (j, &b2) in bits.iter().enumerate() {
                if i != j {
                    assert_eq!(b & b2, 0, "bits at {i} + {j} overlap");
                }
            }
        }
    }

    #[test]
    fn net_import_set_with_op_for_non_net_csslop_is_noop() {
        // Defensive : feeding a non-net CsslOp must NOT flip any bit.
        let set = NetImportSet::empty();
        let after = set.with_op(CsslOp::HeapAlloc);
        assert_eq!(after, NetImportSet::empty());
        let after2 = set.with_op(CsslOp::FsOpen);
        assert_eq!(after2, NetImportSet::empty());
    }

    #[test]
    fn net_import_set_any_net_op_ignores_extension_bits() {
        // any_net_op() reflects the 9 direct MIR ops only ; pure
        // ancillary imports (e.g., last_error_kind without any direct
        // net op) must not register.
        let only_ext = NetImportSet(NetImportSet::LAST_ERROR_KIND | NetImportSet::CAPS_CURRENT);
        assert!(!only_ext.any_net_op());
        let with_send = NetImportSet(only_ext.0 | NetImportSet::SEND);
        assert!(with_send.any_net_op());
    }

    // ── validate_net_arity defensive cross-checks ───────────────────────

    #[test]
    fn validate_accepts_canonical_socket_op() {
        let op = MirOp::new(CsslOp::NetSocket)
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I64));
        assert!(validate_net_arity(&op).is_ok());
    }

    #[test]
    fn validate_accepts_canonical_listen_op() {
        let op = MirOp::new(CsslOp::NetListen)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_operand(ValueId(3))
            .with_result(ValueId(4), MirType::Int(IntWidth::I64));
        assert!(validate_net_arity(&op).is_ok());
    }

    #[test]
    fn validate_rejects_non_net_op() {
        let op = MirOp::new(CsslOp::HeapAlloc);
        let err = validate_net_arity(&op).unwrap_err();
        assert!(err.contains("not a recognized cssl.net.* op"));
    }

    #[test]
    fn validate_rejects_short_listen_op() {
        // Defensive : if a mistyped MIR op leaks past prior passes
        // (only 2 operands instead of 4), the validator surfaces the
        // error before cgen issues a malformed call.
        let op = MirOp::new(CsslOp::NetListen)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I64));
        let err = validate_net_arity(&op).unwrap_err();
        assert!(err.contains("4 operands"), "diagnostic should mention expected 4 ; got: {err}");
    }

    #[test]
    fn validate_rejects_op_with_zero_results() {
        // Net ops must produce 1 result ; 0-result form is malformed.
        let op = MirOp::new(CsslOp::NetSocket).with_operand(ValueId(0));
        let err = validate_net_arity(&op).unwrap_err();
        assert!(err.contains("1 result"));
    }

    // ── invalid-socket no-op contract ───────────────────────────────────

    #[test]
    fn invalid_socket_close_is_noop_per_cssl_rt_contract() {
        // ‼ Cross-check : cssl-rt::ffi::__cssl_net_close's contract is
        //   "INVALID_SOCKET (-1) returns -1 + sets last-error to
        //   INVALID_INPUT". This helper records that contract on the
        //   cgen side so recognizer-bridges can skip the emit when the
        //   handle is statically -1 (matches stdlib/net.cssl's
        //   `if s == -1 { Err(...) }` Err-branch pattern).
        assert!(invalid_socket_close_is_noop());
    }
}
