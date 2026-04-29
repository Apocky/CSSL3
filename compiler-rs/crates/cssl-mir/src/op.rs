//! CSSL-dialect operations — the ~25 custom `cssl.*` ops from `specs/15` § CSSL-DIALECT OPS.
//!
//! § DESIGN
//!   Each `CsslOp` variant represents one dialect op with its canonical source-form
//!   name. `OpSignature` records the expected operand / result arity for a given
//!   op variant ; the pretty-printer uses this to produce valid textual MLIR.
//!
//!   Non-custom ops (arith / scf / cf / func / memref / vector / linalg / affine /
//!   gpu / spirv / llvm) are represented as [`CsslOp::Std`] with a free-form name ;
//!   they're passed through the printer without schema validation at stage-0.

use core::fmt;

/// Canonical dialect-op variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CsslOp {
    // § AD + Jet (F1 + §§ 17)
    DiffPrimal,
    DiffFwd,
    DiffBwd,
    JetConstruct,
    JetProject,
    // § Effects + Handlers
    EffectPerform,
    EffectHandle,
    // § Regions + Handles (§§ 12)
    RegionEnter,
    RegionExit,
    HandlePack,
    HandleUnpack,
    HandleCheck,
    // § Staging + Macros (F4)
    StagedSplice,
    StagedQuote,
    StagedRun,
    MacroExpand,
    // § IFC + Verify (§§ 11 + §§ 20)
    IfcLabel,
    IfcDeclassify,
    VerifyAssert,
    // § Engine primitives
    SdfMarch,
    SdfNormal,
    // § GPU
    GpuBarrier,
    // § XMX cooperative matrix
    XmxCoopMatmul,
    // § Ray-tracing
    RtTraceRay,
    RtIntersect,
    // § Telemetry (R18)
    TelemetryProbe,
    /// `cssl.telemetry.record` — produces a labeled telemetry-slot.
    ///
    /// Per `specs/22_TELEMETRY.csl` § OBSERVABILITY-FIRST-CLASS + T11-D132
    /// (W3β-07) biometric-compile-refusal slice. Each `cssl.telemetry.record`
    /// op carries the operand's IFC label-attributes ; the
    /// `biometric_egress_check` MIR pass walks every such op + refuses the
    /// build at compile-time if any operand carries a biometric-family /
    /// surveillance / coercion sensitive-domain.
    ///
    /// Op-attributes :
    ///   - `(scope, "<TelemetryScope name>")` — required.
    ///   - `(kind, "<TelemetryKind name>")`   — required.
    ///   - `(sensitive_domain, "<domain>")`   — optional, present iff the
    ///     operand was tagged with `Sensitive<dom>`.
    ///   - `(ifc_principal, "<principal>")`   — optional, present iff the
    ///     operand's IFC label confidentiality contains a non-User principal.
    ///   - `(privilege, "<level>")`           — optional, present iff a
    ///     `Privilege<level>` cap was held at the call-site.
    TelemetryRecord,
    // § Heap allocation (S6-B1, T11-D57) — capability-aware allocator surface.
    // Lowered to the `__cssl_alloc` / `__cssl_free` / `__cssl_realloc` FFI
    // symbols exposed by `cssl-rt` (T11-D52, S6-A1). Per `specs/12_CAPABILITIES`
    // § ISO-OWNERSHIP, `cssl.heap.alloc` returns an `iso<T>` (linear, unique),
    // and `cssl.heap.dealloc` consumes the iso (no result).
    HeapAlloc,
    HeapDealloc,
    HeapRealloc,
    // § Sum-type constructors (S6-B2, T11-D60) — Option<T> + Result<T,E>.
    // Per `specs/03_TYPES.csl § BASE-TYPES § aggregate` (sum-types) and
    // `specs/04_EFFECTS.csl § ERROR HANDLING` (Result canonical, ?-op
    // propagates Err). These ops are emitted by the syntactic intrinsic
    // recognizer in `cssl_mir::body_lower` (mirrors B1's `Box::new`
    // recognition). Stage-0 representation : flat tagged-union — the op
    // result carries `tag` + `payload_ty` attributes, JIT execution of
    // these ops is deferred to a follow-up slice that adds a real
    // `MirType::TaggedUnion` ABI lowering. Until then : (a) ops parse +
    // walk + monomorphize, (b) the `?`-operator (HirExprKind::Try) lowers
    // through the existing `cssl.try` op (D60 verifies it remains green
    // when the operand is a Result-shape).
    OptionSome,
    OptionNone,
    ResultOk,
    ResultErr,
    // § String surface (S6-B4, T11-D71) — printf-style format builtin. Per
    // `specs/03_TYPES.csl § STRING-MODEL`. The op is emitted by the
    // syntactic intrinsic recognizer in `cssl_mir::body_lower` (mirrors B2
    // sum-type + B1 Box::new pattern). The format-string + spec-count +
    // arg-count are recorded as op-attributes so the (deferred) ABI lowering
    // pass can dispatch per-type without re-parsing the format string.
    //
    // ‼ NO trait dispatch at stage-0. Display / Debug are not yet traits —
    //   the per-type handler is dispatched by type-checker assertions in
    //   the ABI lowering pass. Until that lands the op is structural-only :
    //   parses, walks, monomorphizes ; runtime execution is the same
    //   deferred-ABI slice as B2/B3.
    StringFormat,
    // § File-system I/O surface (S6-B5, T11-D76) — per
    //   `specs/04_EFFECTS.csl § IO-EFFECT` + `specs/22_TELEMETRY.csl §
    //   FS-OPS` (the latter being a spec-gap closed by the slice's
    //   DECISIONS sub-entry — see DECISIONS T11-D76 § Spec gaps closed).
    //
    //   Each fs op is emitted by a syntactic recognizer in
    //   `cssl_mir::body_lower` (mirrors B1's `Box::new` pattern + B4's
    //   `format(...)` precedent). Each carries the
    //   `(io_effect, "true")` attribute as the stage-0 marker that the
    //   {IO} effect-row threading is ACTIVE on this op. Per the slice
    //   handoff REPORT BACK note, the canonical
    //   `MirEffectRow` structural attribute on the parent fn is a
    //   deferred follow-up (see DECISIONS T11-D76 § DEFERRED) ; the
    //   per-op marker is sufficient at stage-0 for downstream
    //   capability + audit walkers to detect IO-touching MIR.
    //
    //   Lowered to the `__cssl_fs_open / __cssl_fs_read / __cssl_fs_write
    //   / __cssl_fs_close` FFI symbols exposed by `cssl-rt` (T11-D76,
    //   S6-B5). Renaming either the MIR op or the FFI symbol requires
    //   lock-step changes per the dispatch-plan landmines.
    FsOpen,
    FsRead,
    FsWrite,
    FsClose,
    // § Network surface (S7-F4, T11-D82) — per
    //   `specs/04_EFFECTS.csl § NET-EFFECT` (mirror of § IO-EFFECT shape)
    //   + `specs/11_IFC.csl § PRIME-DIRECTIVE ENCODING § NET-CAP rules`.
    //
    //   Each net op is emitted by a syntactic recognizer in
    //   `cssl_mir::body_lower` (mirrors B5 file-IO + B4 string-format
    //   patterns). Each carries the `(net_effect, "true")` attribute as
    //   the stage-0 marker that the {Net} effect-row threading is ACTIVE
    //   on this op. The full structural `MirEffectRow` for {Net} is a
    //   deferred follow-up (matches the {IO} threading deferral from
    //   T11-D76 § DEFERRED) ; the per-op marker is sufficient at stage-0
    //   for downstream capability + audit walkers to detect
    //   network-touching MIR.
    //
    //   PRIME-DIRECTIVE attestation : the `caps_required` attribute on
    //   each connect-style op records whether `NET_CAP_OUTBOUND` /
    //   `NET_CAP_INBOUND` is required. Downstream IFC walkers cross-
    //   reference this against the host's granted cap-set (see
    //   `cssl-rt::net::caps_grant`) before allowing the call to fire.
    //
    //   Lowered to the `__cssl_net_socket / _listen / _accept /
    //   _connect / _send / _recv / _sendto / _recvfrom / _close /
    //   _local_addr` FFI symbols exposed by `cssl-rt` (T11-D82, S7-F4).
    //   Renaming either the MIR op or the FFI symbol requires
    //   lock-step changes per the dispatch-plan landmines.
    NetSocket,
    NetListen,
    NetAccept,
    NetConnect,
    NetSend,
    NetRecv,
    NetSendTo,
    NetRecvFrom,
    NetClose,
    /// Standard-dialect op — name stored separately. Used for `arith.*`, `scf.*`,
    /// `func.*`, `memref.*`, etc. that pass through without schema validation.
    Std,
}

impl CsslOp {
    /// Canonical source-form name (e.g., `"cssl.diff.primal"`). For [`Self::Std`],
    /// the caller supplies the name — this method returns `"cssl.std"` as a marker.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DiffPrimal => "cssl.diff.primal",
            Self::DiffFwd => "cssl.diff.fwd",
            Self::DiffBwd => "cssl.diff.bwd",
            Self::JetConstruct => "cssl.jet.construct",
            Self::JetProject => "cssl.jet.project",
            Self::EffectPerform => "cssl.effect.perform",
            Self::EffectHandle => "cssl.effect.handle",
            Self::RegionEnter => "cssl.region.enter",
            Self::RegionExit => "cssl.region.exit",
            Self::HandlePack => "cssl.handle.pack",
            Self::HandleUnpack => "cssl.handle.unpack",
            Self::HandleCheck => "cssl.handle.check",
            Self::StagedSplice => "cssl.staged.splice",
            Self::StagedQuote => "cssl.staged.quote",
            Self::StagedRun => "cssl.staged.run",
            Self::MacroExpand => "cssl.macro.expand",
            Self::IfcLabel => "cssl.ifc.label",
            Self::IfcDeclassify => "cssl.ifc.declassify",
            Self::VerifyAssert => "cssl.verify.assert",
            Self::SdfMarch => "cssl.sdf.march",
            Self::SdfNormal => "cssl.sdf.normal",
            Self::GpuBarrier => "cssl.gpu.barrier",
            Self::XmxCoopMatmul => "cssl.xmx.coop_matmul",
            Self::RtTraceRay => "cssl.rt.trace_ray",
            Self::RtIntersect => "cssl.rt.intersect",
            Self::TelemetryProbe => "cssl.telemetry.probe",
            Self::TelemetryRecord => "cssl.telemetry.record",
            Self::HeapAlloc => "cssl.heap.alloc",
            Self::HeapDealloc => "cssl.heap.dealloc",
            Self::HeapRealloc => "cssl.heap.realloc",
            Self::OptionSome => "cssl.option.some",
            Self::OptionNone => "cssl.option.none",
            Self::ResultOk => "cssl.result.ok",
            Self::ResultErr => "cssl.result.err",
            Self::StringFormat => "cssl.string.format",
            Self::FsOpen => "cssl.fs.open",
            Self::FsRead => "cssl.fs.read",
            Self::FsWrite => "cssl.fs.write",
            Self::FsClose => "cssl.fs.close",
            Self::NetSocket => "cssl.net.socket",
            Self::NetListen => "cssl.net.listen",
            Self::NetAccept => "cssl.net.accept",
            Self::NetConnect => "cssl.net.connect",
            Self::NetSend => "cssl.net.send",
            Self::NetRecv => "cssl.net.recv",
            Self::NetSendTo => "cssl.net.sendto",
            Self::NetRecvFrom => "cssl.net.recvfrom",
            Self::NetClose => "cssl.net.close",
            Self::Std => "cssl.std",
        }
    }

    /// Category grouping from `specs/15`.
    #[must_use]
    pub const fn category(self) -> OpCategory {
        match self {
            Self::DiffPrimal | Self::DiffFwd | Self::DiffBwd => OpCategory::AutoDiff,
            Self::JetConstruct | Self::JetProject => OpCategory::Jet,
            Self::EffectPerform | Self::EffectHandle => OpCategory::Effect,
            Self::RegionEnter | Self::RegionExit => OpCategory::Region,
            Self::HandlePack | Self::HandleUnpack | Self::HandleCheck => OpCategory::Handle,
            Self::StagedSplice | Self::StagedQuote | Self::StagedRun => OpCategory::Staged,
            Self::MacroExpand => OpCategory::Macro,
            Self::IfcLabel | Self::IfcDeclassify => OpCategory::Ifc,
            Self::VerifyAssert => OpCategory::Verify,
            Self::SdfMarch | Self::SdfNormal => OpCategory::Sdf,
            Self::GpuBarrier => OpCategory::Gpu,
            Self::XmxCoopMatmul => OpCategory::Xmx,
            Self::RtTraceRay | Self::RtIntersect => OpCategory::Rt,
            Self::TelemetryProbe | Self::TelemetryRecord => OpCategory::Telemetry,
            Self::HeapAlloc | Self::HeapDealloc | Self::HeapRealloc => OpCategory::Heap,
            Self::OptionSome | Self::OptionNone | Self::ResultOk | Self::ResultErr => {
                OpCategory::SumType
            }
            Self::StringFormat => OpCategory::String,
            Self::FsOpen | Self::FsRead | Self::FsWrite | Self::FsClose => OpCategory::FileIo,
            Self::NetSocket
            | Self::NetListen
            | Self::NetAccept
            | Self::NetConnect
            | Self::NetSend
            | Self::NetRecv
            | Self::NetSendTo
            | Self::NetRecvFrom
            | Self::NetClose => OpCategory::Net,
            Self::Std => OpCategory::Std,
        }
    }

    /// Canonical signature — expected operand / result counts.
    /// Variadic ops surface as `None` in the relevant slot.
    #[must_use]
    pub const fn signature(self) -> OpSignature {
        match self {
            // AD : primal takes N operands → N results ; fwd/bwd take primal + tangent(s).
            Self::DiffPrimal => OpSignature {
                operands: None,
                results: None,
            },
            Self::DiffFwd | Self::DiffBwd => OpSignature {
                operands: None,
                results: None,
            },
            // Jet : construct N-tangent → 1 Jet ; project Jet + index → 1 tangent.
            Self::JetConstruct => OpSignature {
                operands: None,
                results: Some(1),
            },
            Self::JetProject => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            // Effect : perform takes args → 1 result ; handle takes body → 1 result.
            Self::EffectPerform | Self::EffectHandle => OpSignature {
                operands: None,
                results: Some(1),
            },
            // Region : enter takes no operands → 1 token ; exit takes token → no result.
            Self::RegionEnter => OpSignature {
                operands: Some(0),
                results: Some(1),
            },
            Self::RegionExit => OpSignature {
                operands: Some(1),
                results: Some(0),
            },
            // Handle : pack (idx, gen) → u64 ; unpack u64 → (idx, gen) ; check u64 + gen → bool.
            Self::HandlePack => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            Self::HandleUnpack => OpSignature {
                operands: Some(1),
                results: Some(2),
            },
            Self::HandleCheck => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            // Staged : splice / quote / run all variadic.
            Self::StagedSplice | Self::StagedQuote | Self::StagedRun => OpSignature {
                operands: None,
                results: Some(1),
            },
            Self::MacroExpand => OpSignature {
                operands: None,
                results: Some(1),
            },
            // IFC : label (value, label-attr) → value ; declassify similar.
            Self::IfcLabel | Self::IfcDeclassify => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            // Verify : assert condition → no result.
            Self::VerifyAssert => OpSignature {
                operands: Some(1),
                results: Some(0),
            },
            // SDF : march (scene, ray) → hit ; normal (scene, point) → vec3.
            Self::SdfMarch | Self::SdfNormal => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            // GPU barrier : no operands, no results.
            Self::GpuBarrier => OpSignature {
                operands: Some(0),
                results: Some(0),
            },
            // XMX coop-matmul : (a, b, c) → d.
            Self::XmxCoopMatmul => OpSignature {
                operands: Some(3),
                results: Some(1),
            },
            // RT : trace_ray (ray, scene) → hit ; intersect custom op (variadic).
            Self::RtTraceRay => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            Self::RtIntersect => OpSignature {
                operands: None,
                results: Some(1),
            },
            // Telemetry probe : no operands, no results (side-effect only).
            Self::TelemetryProbe => OpSignature {
                operands: Some(0),
                results: Some(0),
            },
            // Telemetry record : 1 operand (the labeled value to log) → 0 results.
            // The `biometric_egress_check` MIR pass refuses any record op
            // whose operand carries a biometric / surveillance / coercion
            // sensitive-domain attribute.
            Self::TelemetryRecord => OpSignature {
                operands: Some(1),
                results: Some(0),
            },
            // Heap (S6-B1) — see `specs/02_IR.csl` § HEAP-OPS.
            //   alloc   : (size : i64, align : i64)                          -> iso<ptr>
            //   dealloc : (ptr : iso<ptr>, size : i64, align : i64)          -> ()
            //   realloc : (ptr : iso<ptr>, old_size, new_size, align : i64)  -> iso<ptr>
            Self::HeapAlloc => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            Self::HeapDealloc => OpSignature {
                operands: Some(3),
                results: Some(0),
            },
            Self::HeapRealloc => OpSignature {
                operands: Some(4),
                results: Some(1),
            },
            // Sum-type constructors (S6-B2) — see `specs/03_TYPES.csl`.
            //   option.some : (payload : T)        -> Option<T>     (tag = 1)
            //   option.none : ()                   -> Option<T>     (tag = 0)
            //   result.ok   : (payload : T)        -> Result<T, E>  (tag = 1)
            //   result.err  : (err-payload : E)    -> Result<T, E>  (tag = 0)
            Self::OptionSome | Self::ResultOk | Self::ResultErr => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            Self::OptionNone => OpSignature {
                operands: Some(0),
                results: Some(1),
            },
            // String (S6-B4) — see `specs/03_TYPES.csl § STRING-MODEL`.
            //   format : (fmt-handle, *args : variadic) -> String
            //     The format-string is lowered into the first operand as a
            //     value of type `!cssl.string` ; subsequent operands carry
            //     the positional arguments. The op result is `!cssl.string`.
            //     Arity is variadic in the operand slot because the number of
            //     positional arguments depends on the source-level call.
            Self::StringFormat => OpSignature {
                operands: None,
                results: Some(1),
            },
            // File-system I/O (S6-B5) — see `specs/04_EFFECTS.csl § IO-EFFECT`.
            //   open  : (path-bytes : !cssl.string, flags : i32) -> i64    // handle ; -1 on err
            //   read  : (handle : i64, buf-ptr : ptr, buf-len : i64) -> i64 // bytes-read ; -1 on err
            //   write : (handle : i64, buf-ptr : ptr, buf-len : i64) -> i64 // bytes-written ; -1 on err
            //   close : (handle : i64) -> i64                                // 0=ok ; -1 on err
            //
            //   The path operand for FsOpen is the source-level
            //   `String` / `&str` lifted to a `!cssl.string` value at
            //   stage-0 ; the cgen layer extracts (ptr, len) from the
            //   String fat-pointer / StrSlice value when wiring to
            //   `__cssl_fs_open`. See body_lower's `try_lower_fs_open`
            //   for the recognized call-shape.
            Self::FsOpen => OpSignature {
                operands: Some(2),
                results: Some(1),
            },
            Self::FsRead | Self::FsWrite => OpSignature {
                operands: Some(3),
                results: Some(1),
            },
            Self::FsClose => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            // Network surface (S7-F4) — see `specs/04_EFFECTS.csl § NET-EFFECT`.
            //   socket   : (flags : i32) -> i64                                     // sock-handle
            //   listen   : (sock, addr : u32, port : u16, backlog : i32) -> i64     // 0 / -1
            //   accept   : (sock) -> i64                                            // new-sock
            //   connect  : (sock, addr : u32, port : u16) -> i64                    // 0 / -1
            //   send     : (sock, buf-ptr, buf-len) -> i64                          // bytes-sent
            //   recv     : (sock, buf-ptr, buf-len) -> i64                          // bytes-recv
            //   sendto   : (sock, buf-ptr, buf-len, addr, port) -> i64
            //   recvfrom : (sock, buf-ptr, buf-len, *addr-out, *port-out) -> i64
            //   close    : (sock) -> i64                                            // 0 / -1
            Self::NetSocket => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            Self::NetListen => OpSignature {
                operands: Some(4),
                results: Some(1),
            },
            Self::NetAccept => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            Self::NetConnect => OpSignature {
                operands: Some(3),
                results: Some(1),
            },
            Self::NetSend | Self::NetRecv => OpSignature {
                operands: Some(3),
                results: Some(1),
            },
            Self::NetSendTo => OpSignature {
                operands: Some(5),
                results: Some(1),
            },
            Self::NetRecvFrom => OpSignature {
                operands: Some(5),
                results: Some(1),
            },
            Self::NetClose => OpSignature {
                operands: Some(1),
                results: Some(1),
            },
            // Std : free-form.
            Self::Std => OpSignature {
                operands: None,
                results: None,
            },
        }
    }

    /// All `cssl.*` dialect ops (excluding `Std`).
    pub const ALL_CSSL: [Self; 48] = [
        Self::DiffPrimal,
        Self::DiffFwd,
        Self::DiffBwd,
        Self::JetConstruct,
        Self::JetProject,
        Self::EffectPerform,
        Self::EffectHandle,
        Self::RegionEnter,
        Self::RegionExit,
        Self::HandlePack,
        Self::HandleUnpack,
        Self::HandleCheck,
        Self::StagedSplice,
        Self::StagedQuote,
        Self::StagedRun,
        Self::MacroExpand,
        Self::IfcLabel,
        Self::IfcDeclassify,
        Self::VerifyAssert,
        Self::SdfMarch,
        Self::SdfNormal,
        Self::GpuBarrier,
        Self::XmxCoopMatmul,
        Self::RtTraceRay,
        Self::RtIntersect,
        Self::TelemetryProbe,
        Self::TelemetryRecord,
        Self::HeapAlloc,
        Self::HeapDealloc,
        Self::HeapRealloc,
        Self::OptionSome,
        Self::OptionNone,
        Self::ResultOk,
        Self::ResultErr,
        Self::StringFormat,
        Self::FsOpen,
        Self::FsRead,
        Self::FsWrite,
        Self::FsClose,
        Self::NetSocket,
        Self::NetListen,
        Self::NetAccept,
        Self::NetConnect,
        Self::NetSend,
        Self::NetRecv,
        Self::NetSendTo,
        Self::NetRecvFrom,
        Self::NetClose,
    ];
}

impl fmt::Display for CsslOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Category grouping for dialect ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpCategory {
    AutoDiff,
    Jet,
    Effect,
    Region,
    Handle,
    Staged,
    Macro,
    Ifc,
    Verify,
    Sdf,
    Gpu,
    Xmx,
    Rt,
    Telemetry,
    /// Heap allocation (S6-B1) — see `specs/02_IR.csl` § HEAP-OPS.
    Heap,
    /// Sum-type constructors (S6-B2) — `Option<T>` + `Result<T,E>`.
    /// See `specs/03_TYPES.csl § BASE-TYPES` (sum-types) +
    /// `specs/04_EFFECTS.csl § ERROR HANDLING` (Result + ?-op).
    SumType,
    /// String surface ops (S6-B4) — printf-style format. See
    /// `specs/03_TYPES.csl § STRING-MODEL`.
    String,
    /// File-system I/O ops (S6-B5) — `cssl.fs.{open,read,write,close}`.
    /// See `specs/04_EFFECTS.csl § IO-EFFECT` +
    /// `specs/22_TELEMETRY.csl § FS-OPS` (the latter spec-gap closed by
    /// DECISIONS T11-D76).
    FileIo,
    /// Network I/O ops (S7-F4) —
    /// `cssl.net.{socket,listen,accept,connect,send,recv,sendto,recvfrom,close}`.
    /// See `specs/04_EFFECTS.csl § NET-EFFECT` +
    /// `specs/11_IFC.csl § PRIME-DIRECTIVE ENCODING § NET-CAP rules`
    /// (the latter spec-gap closed by DECISIONS T11-D82).
    Net,
    Std,
}

/// Expected operand / result count for an op. `None` = variadic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpSignature {
    pub operands: Option<usize>,
    pub results: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::{CsslOp, OpCategory};

    #[test]
    fn all_ops_have_unique_names() {
        let mut names: Vec<&'static str> = CsslOp::ALL_CSSL.iter().map(|o| o.name()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "op names must be unique");
    }

    #[test]
    fn all_ops_start_with_cssl_prefix() {
        for op in CsslOp::ALL_CSSL {
            assert!(op.name().starts_with("cssl."), "{}", op.name());
        }
    }

    #[test]
    fn all_48_cssl_ops_tracked() {
        // S6-B1 (T11-D57) brought the count to 29 (HeapAlloc/Dealloc/Realloc).
        // S6-B2 (T11-D60) adds 4 sum-type constructors :
        //   OptionSome, OptionNone, ResultOk, ResultErr  →  total 33.
        // S6-B4 (T11-D71) adds StringFormat → total 34.
        // S6-B5 (T11-D76) adds 4 fs ops (Open/Read/Write/Close) → total 38.
        // S7-F4 (T11-D82) adds 9 net ops (Socket/Listen/Accept/Connect/
        //   Send/Recv/SendTo/RecvFrom/Close) → total 47.
        // T11-D132 (W3β-07) adds TelemetryRecord (biometric-egress check
        //   target op) → total 48.
        assert_eq!(CsslOp::ALL_CSSL.len(), 48);
    }

    #[test]
    fn heap_alloc_signature_is_2_to_1() {
        // (size : i64, align : i64) → iso<ptr>
        let sig = CsslOp::HeapAlloc.signature();
        assert_eq!(sig.operands, Some(2));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn heap_dealloc_signature_is_3_to_0() {
        // (ptr, size, align) → ()  (consumes iso<ptr>, no result)
        let sig = CsslOp::HeapDealloc.signature();
        assert_eq!(sig.operands, Some(3));
        assert_eq!(sig.results, Some(0));
    }

    #[test]
    fn heap_realloc_signature_is_4_to_1() {
        // (ptr, old_size, new_size, align) → iso<ptr>
        let sig = CsslOp::HeapRealloc.signature();
        assert_eq!(sig.operands, Some(4));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn heap_op_names_match_cssl_rt_ffi_symbols() {
        // ‼ Naming-match invariant : the MIR op-name suffixes mirror the
        // cssl-rt FFI symbol stems (`alloc / free / realloc`). Renaming
        // either side requires lock-step changes — see HANDOFF_SESSION_6
        // landmines + cssl-rt::ffi.
        assert_eq!(CsslOp::HeapAlloc.name(), "cssl.heap.alloc");
        assert_eq!(CsslOp::HeapDealloc.name(), "cssl.heap.dealloc");
        assert_eq!(CsslOp::HeapRealloc.name(), "cssl.heap.realloc");
    }

    #[test]
    fn heap_ops_in_heap_category() {
        assert_eq!(CsslOp::HeapAlloc.category(), OpCategory::Heap);
        assert_eq!(CsslOp::HeapDealloc.category(), OpCategory::Heap);
        assert_eq!(CsslOp::HeapRealloc.category(), OpCategory::Heap);
    }

    #[test]
    fn category_mapping_is_exhaustive() {
        // Every ALL_CSSL variant should produce a non-Std category.
        for op in CsslOp::ALL_CSSL {
            assert_ne!(op.category(), OpCategory::Std, "{op:?}");
        }
    }

    #[test]
    fn handle_pack_signature_is_2_to_1() {
        let sig = CsslOp::HandlePack.signature();
        assert_eq!(sig.operands, Some(2));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn region_enter_returns_one_token() {
        let sig = CsslOp::RegionEnter.signature();
        assert_eq!(sig.operands, Some(0));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn telemetry_probe_is_void_in_void_out() {
        let sig = CsslOp::TelemetryProbe.signature();
        assert_eq!(sig.operands, Some(0));
        assert_eq!(sig.results, Some(0));
    }

    #[test]
    fn std_is_free_form() {
        let sig = CsslOp::Std.signature();
        assert!(sig.operands.is_none());
        assert!(sig.results.is_none());
    }

    #[test]
    fn display_matches_name() {
        assert_eq!(format!("{}", CsslOp::SdfMarch), "cssl.sdf.march");
        assert_eq!(format!("{}", CsslOp::GpuBarrier), "cssl.gpu.barrier");
    }

    // ── S6-B2 (T11-D60) sum-type op coverage ────────────────────────────

    #[test]
    fn option_some_signature_is_1_to_1() {
        // (payload : T) → Option<T>
        let sig = CsslOp::OptionSome.signature();
        assert_eq!(sig.operands, Some(1));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn option_none_signature_is_0_to_1() {
        // () → Option<T>     (tag = 0 carried as attribute)
        let sig = CsslOp::OptionNone.signature();
        assert_eq!(sig.operands, Some(0));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn result_ok_signature_is_1_to_1() {
        let sig = CsslOp::ResultOk.signature();
        assert_eq!(sig.operands, Some(1));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn result_err_signature_is_1_to_1() {
        let sig = CsslOp::ResultErr.signature();
        assert_eq!(sig.operands, Some(1));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn sum_type_op_names_are_canonical() {
        // ‼ The `cssl.option.*` / `cssl.result.*` names are part of the MIR
        // public surface : downstream tooling (cssl-staging, body_lower
        // recognizers, future trait-dispatch glue) keys off these literals.
        // Renaming requires a lock-step update across all consumers.
        assert_eq!(CsslOp::OptionSome.name(), "cssl.option.some");
        assert_eq!(CsslOp::OptionNone.name(), "cssl.option.none");
        assert_eq!(CsslOp::ResultOk.name(), "cssl.result.ok");
        assert_eq!(CsslOp::ResultErr.name(), "cssl.result.err");
    }

    #[test]
    fn sum_type_ops_in_sum_type_category() {
        assert_eq!(CsslOp::OptionSome.category(), OpCategory::SumType);
        assert_eq!(CsslOp::OptionNone.category(), OpCategory::SumType);
        assert_eq!(CsslOp::ResultOk.category(), OpCategory::SumType);
        assert_eq!(CsslOp::ResultErr.category(), OpCategory::SumType);
    }

    // ── S6-B4 (T11-D71) string-format op coverage ───────────────────────

    #[test]
    fn string_format_signature_is_variadic_to_1() {
        // (fmt-handle, *args...) → !cssl.string
        // The operand count is variadic because the number of positional
        // args depends on the source-level call. The result is always one
        // String value.
        let sig = CsslOp::StringFormat.signature();
        assert!(
            sig.operands.is_none(),
            "StringFormat operand count is variadic"
        );
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn string_format_op_name_is_canonical() {
        // ‼ The `cssl.string.format` name is part of the MIR public surface :
        // downstream tooling (cssl-staging, body_lower recognizers, future
        // trait-dispatch glue, ABI lowering) keys off this literal. Renaming
        // requires a lock-step update across all consumers.
        assert_eq!(CsslOp::StringFormat.name(), "cssl.string.format");
    }

    #[test]
    fn string_format_op_in_string_category() {
        assert_eq!(CsslOp::StringFormat.category(), OpCategory::String);
    }

    // ── S6-B5 (T11-D76) file-system I/O op coverage ─────────────────────

    #[test]
    fn fs_open_signature_is_2_to_1() {
        // (path-bytes : !cssl.string, flags : i32) → handle : i64
        let sig = CsslOp::FsOpen.signature();
        assert_eq!(sig.operands, Some(2));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn fs_read_signature_is_3_to_1() {
        // (handle : i64, buf-ptr : ptr, buf-len : i64) → bytes-read : i64
        let sig = CsslOp::FsRead.signature();
        assert_eq!(sig.operands, Some(3));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn fs_write_signature_is_3_to_1() {
        // (handle : i64, buf-ptr : ptr, buf-len : i64) → bytes-written : i64
        let sig = CsslOp::FsWrite.signature();
        assert_eq!(sig.operands, Some(3));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn fs_close_signature_is_1_to_1() {
        // (handle : i64) → status : i64    (0 = ok ; -1 = err)
        let sig = CsslOp::FsClose.signature();
        assert_eq!(sig.operands, Some(1));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn fs_op_names_match_cssl_rt_ffi_symbols() {
        // ‼ Naming-match invariant : the MIR op-name suffixes mirror the
        // cssl-rt FFI symbol stems (`open / read / write / close`).
        // Renaming either side requires lock-step changes — see
        // HANDOFF_SESSION_6 landmines + cssl-rt::ffi.
        assert_eq!(CsslOp::FsOpen.name(), "cssl.fs.open");
        assert_eq!(CsslOp::FsRead.name(), "cssl.fs.read");
        assert_eq!(CsslOp::FsWrite.name(), "cssl.fs.write");
        assert_eq!(CsslOp::FsClose.name(), "cssl.fs.close");
    }

    #[test]
    fn fs_ops_in_file_io_category() {
        assert_eq!(CsslOp::FsOpen.category(), OpCategory::FileIo);
        assert_eq!(CsslOp::FsRead.category(), OpCategory::FileIo);
        assert_eq!(CsslOp::FsWrite.category(), OpCategory::FileIo);
        assert_eq!(CsslOp::FsClose.category(), OpCategory::FileIo);
    }

    #[test]
    fn fs_op_display_matches_name() {
        assert_eq!(format!("{}", CsslOp::FsOpen), "cssl.fs.open");
        assert_eq!(format!("{}", CsslOp::FsClose), "cssl.fs.close");
    }

    // ── S7-F4 (T11-D82) network-op coverage ─────────────────────────────

    #[test]
    fn net_socket_signature_is_1_to_1() {
        // (flags : i32) → handle : i64
        let sig = CsslOp::NetSocket.signature();
        assert_eq!(sig.operands, Some(1));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_listen_signature_is_4_to_1() {
        // (sock, addr, port, backlog) → status : i64
        let sig = CsslOp::NetListen.signature();
        assert_eq!(sig.operands, Some(4));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_accept_signature_is_1_to_1() {
        // (sock) → new-sock : i64
        let sig = CsslOp::NetAccept.signature();
        assert_eq!(sig.operands, Some(1));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_connect_signature_is_3_to_1() {
        // (sock, addr, port) → status : i64
        let sig = CsslOp::NetConnect.signature();
        assert_eq!(sig.operands, Some(3));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_send_signature_is_3_to_1() {
        let sig = CsslOp::NetSend.signature();
        assert_eq!(sig.operands, Some(3));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_recv_signature_is_3_to_1() {
        let sig = CsslOp::NetRecv.signature();
        assert_eq!(sig.operands, Some(3));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_sendto_signature_is_5_to_1() {
        // (sock, buf-ptr, buf-len, addr, port) → bytes-sent : i64
        let sig = CsslOp::NetSendTo.signature();
        assert_eq!(sig.operands, Some(5));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_recvfrom_signature_is_5_to_1() {
        // (sock, buf-ptr, buf-len, *addr-out, *port-out) → bytes-recv : i64
        let sig = CsslOp::NetRecvFrom.signature();
        assert_eq!(sig.operands, Some(5));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_close_signature_is_1_to_1() {
        let sig = CsslOp::NetClose.signature();
        assert_eq!(sig.operands, Some(1));
        assert_eq!(sig.results, Some(1));
    }

    #[test]
    fn net_op_names_match_cssl_rt_ffi_symbols() {
        // ‼ Naming-match invariant : the MIR op-name suffixes mirror the
        // cssl-rt FFI symbol stems (`socket / listen / accept / connect /
        // send / recv / sendto / recvfrom / close`). Renaming either
        // side requires lock-step changes — see HANDOFF_SESSION_7
        // landmines + cssl-rt::ffi.
        assert_eq!(CsslOp::NetSocket.name(), "cssl.net.socket");
        assert_eq!(CsslOp::NetListen.name(), "cssl.net.listen");
        assert_eq!(CsslOp::NetAccept.name(), "cssl.net.accept");
        assert_eq!(CsslOp::NetConnect.name(), "cssl.net.connect");
        assert_eq!(CsslOp::NetSend.name(), "cssl.net.send");
        assert_eq!(CsslOp::NetRecv.name(), "cssl.net.recv");
        assert_eq!(CsslOp::NetSendTo.name(), "cssl.net.sendto");
        assert_eq!(CsslOp::NetRecvFrom.name(), "cssl.net.recvfrom");
        assert_eq!(CsslOp::NetClose.name(), "cssl.net.close");
    }

    #[test]
    fn net_ops_in_net_category() {
        assert_eq!(CsslOp::NetSocket.category(), OpCategory::Net);
        assert_eq!(CsslOp::NetListen.category(), OpCategory::Net);
        assert_eq!(CsslOp::NetAccept.category(), OpCategory::Net);
        assert_eq!(CsslOp::NetConnect.category(), OpCategory::Net);
        assert_eq!(CsslOp::NetSend.category(), OpCategory::Net);
        assert_eq!(CsslOp::NetRecv.category(), OpCategory::Net);
        assert_eq!(CsslOp::NetSendTo.category(), OpCategory::Net);
        assert_eq!(CsslOp::NetRecvFrom.category(), OpCategory::Net);
        assert_eq!(CsslOp::NetClose.category(), OpCategory::Net);
    }

    #[test]
    fn net_op_display_matches_name() {
        assert_eq!(format!("{}", CsslOp::NetSocket), "cssl.net.socket");
        assert_eq!(format!("{}", CsslOp::NetClose), "cssl.net.close");
    }
}
