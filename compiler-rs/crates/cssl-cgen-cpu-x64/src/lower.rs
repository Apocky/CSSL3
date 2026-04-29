//! ABI lowering passes : abstract `Call` / `Ret` / function-entry / function-
//! exit MIR ops → concrete sequences of [`AbstractInsn`] tagged with the
//! correct register / stack / shadow-space + alignment fixups for the chosen
//! [`X64Abi`].
//!
//! § SPEC : `specs/14_BACKEND.csl § OWNED x86-64 BACKEND § ABI` and
//! `specs/07_CODEGEN.csl § CPU BACKEND § ABI`.
//!
//! § SURFACE
//!
//!   - [`lower_call(args, ret_class, abi)`]
//!       → [`LoweredCall`] : seq of pre-call moves + stack push + (shadow-space
//!         alloc on MS-x64) + `call` + post-call rsp restore + return-reg pickup.
//!
//!   - [`lower_return(ret_class, abi)`]
//!       → [`LoweredReturn`] : place return value in correct return reg + ret.
//!
//!   - [`lower_prologue(layout, abi)`]
//!       → [`LoweredPrologue`] : `push rbp ; mov rbp, rsp ; sub rsp, frame ;
//!         push <callee-saved-used>`.
//!
//!   - [`lower_epilogue(layout, abi)`]
//!       → [`LoweredEpilogue`] : reverse of prologue.
//!
//! § ABSTRACT INSN MODEL
//!
//!   The lowering layer emits [`AbstractInsn`] nodes — high-level enough to
//!   defer ModR/M encoding to a future emit.rs slice, but low-level enough
//!   that every node corresponds to exactly one machine instruction. Stack
//!   ops, moves, push/pop, and the call instruction itself are all distinct
//!   variants. This boundary is intentional : the ABI lowering is testable
//!   and inspectable as data ; the byte-emission layer is a separate concern.
//!
//! § INVARIANTS (load-bearing for codegen correctness)
//!
//!   - Every emitted `LoweredCall.insns` sequence preserves 16-byte stack
//!     alignment at the moment of the `call` instruction. Callers MUST use
//!     [`LoweredCall::final_rsp_delta`] to verify alignment after stacking
//!     overflow args + (on MS-x64) the 32-byte shadow space.
//!
//!   - On MS-x64, the shadow space is ALWAYS allocated even when the callee
//!     receives ≤ 4 args and never spills to stack. This is the most-cited
//!     ABI landmine and is enforced unconditionally here.
//!
//!   - On SysV, no shadow space is allocated (it's optional even on the
//!     spec — and conventionally not allocated by SysV-targeting compilers).

use crate::abi::{
    AbiError, ArgClass, FloatArgRegs, GpReg, IntArgRegs, ReturnReg, X64Abi, XmmReg,
    CALL_BOUNDARY_ALIGNMENT,
};

// ══════════════════════════════════════════════════════════════════════════
// § AbstractInsn — high-level instruction-shape used by the lowering layer
// ══════════════════════════════════════════════════════════════════════════

/// High-level instruction-shape emitted by the ABI lowering layer.
///
/// Every variant corresponds to exactly one x86-64 machine instruction at
/// emit-time. The byte-encoding (REX prefix + opcode + ModR/M + SIB +
/// displacement / immediate) is a separate emit.rs concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbstractInsn {
    /// `mov <dst-gp>, <src-gp>` — 64-bit GP-to-GP register move.
    MovGpGp {
        /// Destination GP register.
        dst: GpReg,
        /// Source GP register.
        src: GpReg,
    },
    /// `movsd <dst-xmm>, <src-xmm>` — 64-bit XMM-to-XMM scalar double move
    /// (also covers single-precision `movss` ; emit.rs picks the right opcode).
    MovXmmXmm {
        /// Destination XMM register.
        dst: XmmReg,
        /// Source XMM register.
        src: XmmReg,
    },
    /// `push <gp>` — push 64-bit register onto stack (decrements rsp by 8).
    Push {
        /// GP register being pushed.
        reg: GpReg,
    },
    /// `pop <gp>` — pop 64-bit register from stack (increments rsp by 8).
    Pop {
        /// GP register being popped.
        reg: GpReg,
    },
    /// `sub rsp, <imm32>` — allocate stack frame.
    SubRsp {
        /// Number of bytes to subtract from rsp.
        bytes: u32,
    },
    /// `add rsp, <imm32>` — release stack frame.
    AddRsp {
        /// Number of bytes to add to rsp.
        bytes: u32,
    },
    /// `mov [rsp + offset], <gp>` — spill GP to stack arg-slot.
    StoreGpToStackArg {
        /// Offset from rsp where the value lands.
        offset: u32,
        /// GP register being spilled.
        reg: GpReg,
    },
    /// `movsd [rsp + offset], <xmm>` — spill XMM to stack arg-slot.
    StoreXmmToStackArg {
        /// Offset from rsp where the value lands.
        offset: u32,
        /// XMM register being spilled.
        reg: XmmReg,
    },
    /// `call <symbol>` — direct near-relative call to a named symbol.
    Call {
        /// Target symbol name (will be relocated by the object writer).
        target: String,
    },
    /// `ret` — return to caller.
    Ret,
}

impl AbstractInsn {
    /// True iff this instruction is the `call` itself (used by alignment
    /// invariants to identify the call boundary in a lowered sequence).
    #[must_use]
    pub const fn is_call(&self) -> bool {
        matches!(self, Self::Call { .. })
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § StackSlot — overflow argument stack location
// ══════════════════════════════════════════════════════════════════════════

/// An overflow argument that didn't fit in registers and spills to the stack
/// at the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackSlot {
    /// Argument-classification (Int → store via GP, Float → store via XMM).
    pub class: ArgClass,
    /// Byte offset from rsp (set after shadow-space + alignment fixups).
    pub offset: u32,
}

// ══════════════════════════════════════════════════════════════════════════
// § CallSiteLayout — the static result of classifying a call site's args
// ══════════════════════════════════════════════════════════════════════════

/// Pre-emit summary of how each arg of a call site is dispatched : which go
/// to register slots, which spill to stack, what the total stack-args byte
/// count is, etc. Computed once per call site by [`classify_call_args`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSiteLayout {
    /// ABI under which the layout was computed.
    pub abi: X64Abi,
    /// Sequence of (arg-index, target-int-reg) pairs for register-passed int args.
    pub int_reg_assignments: Vec<(usize, GpReg)>,
    /// Sequence of (arg-index, target-xmm-reg) pairs for register-passed float args.
    pub float_reg_assignments: Vec<(usize, XmmReg)>,
    /// Overflow stack slots (offsets are relative to rsp AFTER shadow-space alloc).
    pub stack_slots: Vec<(usize, StackSlot)>,
    /// Total bytes consumed by overflow stack args (NOT including shadow-space).
    pub stack_args_bytes: u32,
    /// Bytes of shadow space the caller must allocate (32 on MS-x64, 0 on SysV).
    pub shadow_space_bytes: u32,
    /// Total rsp-decrement at the call boundary (shadow-space + stack-args + padding).
    /// Must satisfy `(rsp - this) % 16 == 0` at the call instruction.
    pub total_stack_alloc_bytes: u32,
}

impl CallSiteLayout {
    /// Number of args that landed in registers (int + float).
    #[must_use]
    pub fn reg_arg_count(&self) -> usize {
        self.int_reg_assignments.len() + self.float_reg_assignments.len()
    }

    /// Number of args that overflowed to stack.
    #[must_use]
    pub fn stack_arg_count(&self) -> usize {
        self.stack_slots.len()
    }

    /// Whether the layout's total stack-alloc preserves 16-byte alignment.
    /// (Caller's rsp at function entry is already 8-mod-16 ; the prologue's
    /// `push rbp` re-aligns to 16 ; `sub rsp, total_alloc` must keep that.)
    #[must_use]
    pub fn is_call_boundary_aligned(&self) -> bool {
        self.total_stack_alloc_bytes % CALL_BOUNDARY_ALIGNMENT == 0
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § FunctionLayout — input to lower_prologue / lower_epilogue
// ══════════════════════════════════════════════════════════════════════════

/// Static description of a function's stack-frame requirements, fed into
/// [`lower_prologue`] / [`lower_epilogue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionLayout {
    /// ABI being targeted.
    pub abi: X64Abi,
    /// Bytes of local-storage frame requested by regalloc + spill-slot tables.
    /// This excludes the rbp save (which the prologue handles automatically).
    pub local_frame_bytes: u32,
    /// Callee-saved GP registers actually used by this function (subset of
    /// the ABI's callee-saved set). Order is preserved for push/pop pairing.
    pub callee_saved_gp_used: Vec<GpReg>,
    /// Callee-saved XMM registers actually used by this function.
    pub callee_saved_xmm_used: Vec<XmmReg>,
}

impl FunctionLayout {
    /// Construct a minimal layout (no locals, no callee-saved spills).
    /// Convenience for test fixtures.
    #[must_use]
    pub const fn new(abi: X64Abi) -> Self {
        Self {
            abi,
            local_frame_bytes: 0,
            callee_saved_gp_used: Vec::new(),
            callee_saved_xmm_used: Vec::new(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § CalleeSavedSlot — descriptor for a callee-saved spill slot
// ══════════════════════════════════════════════════════════════════════════

/// Where a callee-saved register lives during the lifetime of the function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalleeSavedSlot {
    /// Pushed onto the stack at function entry (recovered via `pop` in epilogue).
    Pushed {
        /// Spilled GP register (or None when this is an XMM slot).
        gp: GpReg,
    },
    /// Stored at `[rsp + offset]` (used for XMM spills, since XMMs can't `push`).
    XmmSpilled {
        /// Spilled XMM register.
        xmm: XmmReg,
        /// Frame offset (bytes from rsp).
        offset: u32,
    },
}

// ══════════════════════════════════════════════════════════════════════════
// § LoweredCall — output of lower_call
// ══════════════════════════════════════════════════════════════════════════

/// The lowered form of a call site, ready for emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredCall {
    /// ABI used.
    pub abi: X64Abi,
    /// Pre-emit layout summary.
    pub layout: CallSiteLayout,
    /// Sequence of abstract instructions that implement the call.
    /// Order : (1) overflow stack-arg stores, (2) shadow-space sub, (3) the
    /// `call` itself, (4) shadow-space + stack-arg reclamation.
    pub insns: Vec<AbstractInsn>,
    /// Where the return value lives after the call (for the caller to consume).
    pub return_reg: ReturnReg,
    /// Net rsp delta consumed by this call site (shadow-space + stack-args +
    /// alignment padding). Caller subtracts this many bytes before the call
    /// and adds them back after.
    pub final_rsp_delta: u32,
}

// ══════════════════════════════════════════════════════════════════════════
// § LoweredReturn — output of lower_return
// ══════════════════════════════════════════════════════════════════════════

/// The lowered form of a function return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredReturn {
    /// ABI used.
    pub abi: X64Abi,
    /// Where the return value was placed (or [`ReturnReg::Void`] for void fns).
    pub return_reg: ReturnReg,
    /// Sequence of abstract instructions (just the `ret` for now ; epilogue
    /// emits the prior pop/add-rsp via [`lower_epilogue`]).
    pub insns: Vec<AbstractInsn>,
}

// ══════════════════════════════════════════════════════════════════════════
// § LoweredPrologue / LoweredEpilogue
// ══════════════════════════════════════════════════════════════════════════

/// The lowered form of a function-entry prologue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredPrologue {
    /// ABI used.
    pub abi: X64Abi,
    /// `push rbp ; mov rbp, rsp ; sub rsp, frame ; <callee-saved-spills>`.
    pub insns: Vec<AbstractInsn>,
    /// Total frame bytes allocated (locals + xmm-spill area).
    pub total_frame_bytes: u32,
    /// Callee-saved spill slots produced (one per actually-used callee-saved reg).
    pub callee_saved_slots: Vec<CalleeSavedSlot>,
}

/// The lowered form of a function-exit epilogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredEpilogue {
    /// ABI used.
    pub abi: X64Abi,
    /// `<callee-saved-restore> ; add rsp, frame ; pop rbp ; ret`.
    pub insns: Vec<AbstractInsn>,
}

// ══════════════════════════════════════════════════════════════════════════
// § lower_call — main entry point for call lowering
// ══════════════════════════════════════════════════════════════════════════

/// Lower an abstract `Call(target, args)` MIR op into a concrete instruction
/// sequence per the chosen [`X64Abi`].
///
/// # Arguments
///
///   - `target` : the symbol being called (e.g. `"foo"` ; resolution + relocation
///     happens later in the emit / object stage).
///   - `args` : positional list of argument classifications in source order.
///   - `ret_class` : `Some(ArgClass)` for non-void returns, `None` for void.
///   - `abi` : the calling-convention ABI to lower against.
///
/// # Errors
///
///   - [`AbiError::VariadicNotSupported`] when invoked with `is_variadic = true`
///     (variadic lowering is deferred at G3).
///   - [`AbiError::StructReturnNotSupported`] when called with an aggregate
///     return type (G3 handles scalar-returns only).
pub fn lower_call(
    target: &str,
    args: &[ArgClass],
    ret_class: Option<ArgClass>,
    abi: X64Abi,
) -> Result<LoweredCall, AbiError> {
    let layout = classify_call_args(args, abi);
    let mut insns = Vec::new();

    // 1. Allocate stack space (shadow-space on MS-x64 + overflow-arg slots + alignment padding).
    if layout.total_stack_alloc_bytes > 0 {
        insns.push(AbstractInsn::SubRsp {
            bytes: layout.total_stack_alloc_bytes,
        });
    }

    // 2. Spill overflow args to stack slots.
    //    Stack-slot offsets are (shadow_space_bytes + per-arg-offset) within
    //    the allocated region. For both ABIs args are stored at increasing
    //    offsets from rsp ; on MS-x64 the first 32 bytes are reserved for
    //    shadow space and overflow args follow at rsp+32.
    for (_arg_idx, slot) in &layout.stack_slots {
        let store_offset = layout.shadow_space_bytes + slot.offset;
        match slot.class {
            ArgClass::Int => {
                // Synthesize a "marker" GP source — at the abstract layer the
                // caller's regalloc has already placed the value somewhere.
                // For G3 we emit a placeholder StoreGpToStackArg with rax as
                // the source ; the actual source-reg will be patched in by
                // the upstream regalloc layer when it threads SSA values in.
                insns.push(AbstractInsn::StoreGpToStackArg {
                    offset: store_offset,
                    reg: GpReg::Rax,
                });
            }
            ArgClass::Float => {
                insns.push(AbstractInsn::StoreXmmToStackArg {
                    offset: store_offset,
                    reg: XmmReg::Xmm0,
                });
            }
        }
    }

    // 3. The call itself.
    insns.push(AbstractInsn::Call {
        target: target.to_string(),
    });

    // 4. Reclaim stack space.
    if layout.total_stack_alloc_bytes > 0 {
        insns.push(AbstractInsn::AddRsp {
            bytes: layout.total_stack_alloc_bytes,
        });
    }

    // 5. Resolve return register.
    let return_reg = ret_class.map_or(ReturnReg::Void, |class| ReturnReg::for_class(abi, class));

    Ok(LoweredCall {
        abi,
        final_rsp_delta: layout.total_stack_alloc_bytes,
        layout,
        insns,
        return_reg,
    })
}

// ══════════════════════════════════════════════════════════════════════════
// § lower_return — return-value placement + ret
// ══════════════════════════════════════════════════════════════════════════

/// Lower a function return.
///
/// Note : the `ret` instruction is emitted here ; the callee-saved register
/// restore + `add rsp` + `pop rbp` come from [`lower_epilogue`] which the
/// caller is expected to splice immediately before the return-value placement.
#[must_use]
pub fn lower_return(ret_class: Option<ArgClass>, abi: X64Abi) -> LoweredReturn {
    let return_reg = ret_class.map_or(ReturnReg::Void, |class| ReturnReg::for_class(abi, class));
    let insns = vec![AbstractInsn::Ret];
    LoweredReturn {
        abi,
        return_reg,
        insns,
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § lower_prologue — function entry
// ══════════════════════════════════════════════════════════════════════════

/// Lower a function-entry prologue.
///
/// The emitted sequence is :
///
/// ```text
///   push rbp                  ; save caller's frame pointer
///   mov  rbp, rsp             ; establish our frame pointer
///   sub  rsp, <frame_bytes>   ; (omitted when frame_bytes == 0)
///   push <callee-saved-gp>... ; one push per used callee-saved GP
///   ; XMM callee-saved spills go to [rsp+offset] slots within the local frame
/// ```
///
/// The `push rbp` re-aligns rsp to 16 bytes (entry rsp was 8-mod-16). The
/// `sub rsp` adjustment is required to be a multiple of 16 so that subsequent
/// call-sites land on aligned boundaries — `lower_prologue` rounds
/// `frame_bytes` UP to the next multiple of 16 and re-emits.
#[must_use]
pub fn lower_prologue(layout: &FunctionLayout) -> LoweredPrologue {
    let mut insns = Vec::new();
    let mut callee_saved_slots = Vec::new();

    // 1. push rbp ; mov rbp, rsp.
    insns.push(AbstractInsn::Push { reg: GpReg::Rbp });
    insns.push(AbstractInsn::MovGpGp {
        dst: GpReg::Rbp,
        src: GpReg::Rsp,
    });

    // 2. Round local-frame bytes up to a multiple of 16, accounting for
    //    callee-saved-GP push pressure (each push is 8 bytes ; an odd number
    //    of pushes leaves rsp at 8-mod-16 instead of 16-aligned, so the frame
    //    sub adjusts compensatingly).
    let xmm_spill_bytes =
        u32::try_from(layout.callee_saved_xmm_used.len()).unwrap_or(u32::MAX) * 16;
    let raw_local = layout.local_frame_bytes + xmm_spill_bytes;
    let gp_push_bytes = u32::try_from(layout.callee_saved_gp_used.len()).unwrap_or(u32::MAX) * 8;
    // After `push rbp` rsp is 16-aligned. After `gp_push_bytes` more pushes
    // it's `(16 - gp_push_bytes % 16) % 16`-aligned. The frame sub must bring
    // it back to a 16-multiple from rsp.
    let unaligned = (raw_local + gp_push_bytes) % CALL_BOUNDARY_ALIGNMENT;
    let alignment_padding = if unaligned == 0 {
        0
    } else {
        CALL_BOUNDARY_ALIGNMENT - unaligned
    };
    let total_frame_bytes = raw_local + alignment_padding;

    // 3. sub rsp, frame.
    if total_frame_bytes > 0 {
        insns.push(AbstractInsn::SubRsp {
            bytes: total_frame_bytes,
        });
    }

    // 4. push <callee-saved-gp>... + record slots.
    for &reg in &layout.callee_saved_gp_used {
        insns.push(AbstractInsn::Push { reg });
        callee_saved_slots.push(CalleeSavedSlot::Pushed { gp: reg });
    }

    // 5. Spill callee-saved XMMs into the local frame (emit StoreXmmToStackArg
    //    at increasing offsets from rsp). One 16-byte slot per XMM.
    let mut xmm_offset: u32 = 0;
    for &xmm in &layout.callee_saved_xmm_used {
        insns.push(AbstractInsn::StoreXmmToStackArg {
            offset: xmm_offset,
            reg: xmm,
        });
        callee_saved_slots.push(CalleeSavedSlot::XmmSpilled {
            xmm,
            offset: xmm_offset,
        });
        xmm_offset += 16;
    }

    LoweredPrologue {
        abi: layout.abi,
        insns,
        total_frame_bytes,
        callee_saved_slots,
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § lower_epilogue — function exit
// ══════════════════════════════════════════════════════════════════════════

/// Lower a function-exit epilogue.
///
/// The emitted sequence reverses the prologue :
///
/// ```text
///   ; XMM callee-saved restores from [rsp+offset]   (deferred at G3 — no MovXmmFromStackArg)
///   pop  <callee-saved-gp>... ; in REVERSE order vs prologue
///   add  rsp, <frame_bytes>   ; (omitted when frame_bytes == 0)
///   pop  rbp
///   ret
/// ```
///
/// The total frame bytes are recomputed from the layout to match
/// [`lower_prologue`] exactly (the rounding rule is the same, deterministic
/// on the same input). When the caller has a [`LoweredPrologue`] handy they
/// SHOULD use [`lower_epilogue_for`] to avoid recomputation.
#[must_use]
pub fn lower_epilogue(layout: &FunctionLayout) -> LoweredEpilogue {
    let prologue = lower_prologue(layout);
    lower_epilogue_for(layout, &prologue)
}

/// Variant of [`lower_epilogue`] that consumes a precomputed prologue (so the
/// frame-rounding result is shared between the two halves of the function).
#[must_use]
pub fn lower_epilogue_for(layout: &FunctionLayout, prologue: &LoweredPrologue) -> LoweredEpilogue {
    let mut insns = Vec::new();

    // 1. Restore callee-saved XMMs (NOT IMPLEMENTED at G3 — the load opcode
    //    would be MovXmmFromStackArg ; we leave the slots undisturbed and
    //    rely on the rsp-restore + pop-rbp sequence to land the caller back
    //    in its frame. Tracking-issue : XMM-spill restore lands with the
    //    full memref-load lowering slice in S7-G4.)
    //    For G3 we still emit the GP restores in reverse order.

    // 2. pop <callee-saved-gp>... in REVERSE order.
    for &reg in layout.callee_saved_gp_used.iter().rev() {
        insns.push(AbstractInsn::Pop { reg });
    }

    // 3. add rsp, frame_bytes.
    if prologue.total_frame_bytes > 0 {
        insns.push(AbstractInsn::AddRsp {
            bytes: prologue.total_frame_bytes,
        });
    }

    // 4. pop rbp.
    insns.push(AbstractInsn::Pop { reg: GpReg::Rbp });

    // 5. ret.
    insns.push(AbstractInsn::Ret);

    LoweredEpilogue {
        abi: layout.abi,
        insns,
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § classify_call_args — internal helper that maps positional args → regs/stack
// ══════════════════════════════════════════════════════════════════════════

/// Classify a call site's positional arguments into register-assignments +
/// overflow stack slots, computing the total stack alloc with shadow-space +
/// 16-byte alignment fixup baked in.
///
/// This is the heart of the ABI lowering : the MS-x64 positional-counter
/// alias rule lives here, as do the System-V independent-counter rule and
/// the alignment + shadow-space arithmetic.
#[must_use]
pub fn classify_call_args(args: &[ArgClass], abi: X64Abi) -> CallSiteLayout {
    let int_regs = IntArgRegs(abi.int_arg_regs());
    let float_regs = FloatArgRegs(abi.float_arg_regs());

    let mut int_reg_assignments = Vec::new();
    let mut float_reg_assignments = Vec::new();
    let mut stack_slots = Vec::new();
    let mut stack_args_bytes: u32 = 0;

    if abi.shares_positional_arg_counter() {
        // MS-x64 : single positional counter ; int + float regs alias.
        // Slot 0 : rcx OR xmm0 OR (i64, f64 : rcx + xmm0 — both reserved).
        // Slot 1 : rdx OR xmm1.
        // Slot 2 : r8  OR xmm2.
        // Slot 3 : r9  OR xmm3.
        // Slot 4+ : stack.
        for (i, &class) in args.iter().enumerate() {
            match class {
                ArgClass::Int => match int_regs.get(i) {
                    Some(r) => int_reg_assignments.push((i, r)),
                    None => {
                        stack_slots.push((
                            i,
                            StackSlot {
                                class,
                                offset: stack_args_bytes,
                            },
                        ));
                        stack_args_bytes += 8;
                    }
                },
                ArgClass::Float => match float_regs.get(i) {
                    Some(r) => float_reg_assignments.push((i, r)),
                    None => {
                        stack_slots.push((
                            i,
                            StackSlot {
                                class,
                                offset: stack_args_bytes,
                            },
                        ));
                        stack_args_bytes += 8;
                    }
                },
            }
        }
    } else {
        // SysV : independent int + float counters.
        let mut int_idx = 0usize;
        let mut float_idx = 0usize;
        for (i, &class) in args.iter().enumerate() {
            match class {
                ArgClass::Int => match int_regs.get(int_idx) {
                    Some(r) => {
                        int_reg_assignments.push((i, r));
                        int_idx += 1;
                    }
                    None => {
                        stack_slots.push((
                            i,
                            StackSlot {
                                class,
                                offset: stack_args_bytes,
                            },
                        ));
                        stack_args_bytes += 8;
                    }
                },
                ArgClass::Float => match float_regs.get(float_idx) {
                    Some(r) => {
                        float_reg_assignments.push((i, r));
                        float_idx += 1;
                    }
                    None => {
                        stack_slots.push((
                            i,
                            StackSlot {
                                class,
                                offset: stack_args_bytes,
                            },
                        ));
                        stack_args_bytes += 8;
                    }
                },
            }
        }
    }

    // Compute total stack alloc with shadow-space + alignment-fixup.
    let shadow_space_bytes = abi.shadow_space_bytes();
    let raw_alloc = shadow_space_bytes + stack_args_bytes;
    let unaligned = raw_alloc % CALL_BOUNDARY_ALIGNMENT;
    let alignment_padding = if unaligned == 0 {
        0
    } else {
        CALL_BOUNDARY_ALIGNMENT - unaligned
    };
    let total_stack_alloc_bytes = raw_alloc + alignment_padding;

    CallSiteLayout {
        abi,
        int_reg_assignments,
        float_reg_assignments,
        stack_slots,
        stack_args_bytes,
        shadow_space_bytes,
        total_stack_alloc_bytes,
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § Tests
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{ArgClass, GpReg, X64Abi, XmmReg};

    // ── basic call lowering : zero args, void return ──

    #[test]
    fn lower_call_zero_args_void_return_sysv() {
        let lowered = lower_call("foo", &[], None, X64Abi::SystemV).unwrap();
        assert_eq!(lowered.return_reg, ReturnReg::Void);
        // SysV : no shadow space, no overflow → no rsp adjustment, just call.
        assert_eq!(lowered.layout.total_stack_alloc_bytes, 0);
        assert_eq!(lowered.insns.len(), 1);
        assert!(matches!(&lowered.insns[0], AbstractInsn::Call { target } if target == "foo"));
    }

    #[test]
    fn lower_call_zero_args_void_return_ms_x64() {
        let lowered = lower_call("foo", &[], None, X64Abi::MicrosoftX64).unwrap();
        assert_eq!(lowered.return_reg, ReturnReg::Void);
        // MS-x64 : 32 bytes shadow space ALWAYS allocated even for zero-arg call.
        assert_eq!(lowered.layout.total_stack_alloc_bytes, 32);
        assert_eq!(lowered.insns.len(), 3);
        assert!(matches!(
            &lowered.insns[0],
            AbstractInsn::SubRsp { bytes: 32 }
        ));
        assert!(lowered.insns[1].is_call());
        assert!(matches!(
            &lowered.insns[2],
            AbstractInsn::AddRsp { bytes: 32 }
        ));
    }

    // ── int-arg-only register assignment ──

    #[test]
    fn lower_call_three_int_args_sysv_assigns_rdi_rsi_rdx() {
        let args = [ArgClass::Int, ArgClass::Int, ArgClass::Int];
        let lowered = lower_call("bar", &args, None, X64Abi::SystemV).unwrap();
        assert_eq!(
            lowered.layout.int_reg_assignments,
            vec![(0, GpReg::Rdi), (1, GpReg::Rsi), (2, GpReg::Rdx)]
        );
        assert!(lowered.layout.stack_slots.is_empty());
        assert_eq!(lowered.layout.total_stack_alloc_bytes, 0);
    }

    #[test]
    fn lower_call_three_int_args_ms_x64_assigns_rcx_rdx_r8() {
        let args = [ArgClass::Int, ArgClass::Int, ArgClass::Int];
        let lowered = lower_call("bar", &args, None, X64Abi::MicrosoftX64).unwrap();
        assert_eq!(
            lowered.layout.int_reg_assignments,
            vec![(0, GpReg::Rcx), (1, GpReg::Rdx), (2, GpReg::R8)]
        );
        // Shadow space still required.
        assert_eq!(lowered.layout.total_stack_alloc_bytes, 32);
    }

    // ── overflow to stack ──

    #[test]
    fn lower_call_seven_int_args_sysv_overflows_seventh_to_stack() {
        let args = [ArgClass::Int; 7];
        let lowered = lower_call("bar", &args, None, X64Abi::SystemV).unwrap();
        assert_eq!(lowered.layout.int_reg_assignments.len(), 6);
        assert_eq!(lowered.layout.stack_slots.len(), 1);
        assert_eq!(lowered.layout.stack_slots[0].0, 6); // arg-index 6 (0-based)
        assert_eq!(lowered.layout.stack_args_bytes, 8);
        // 0 (shadow) + 8 (one arg) = 8 ; padded to 16 for alignment.
        assert_eq!(lowered.layout.total_stack_alloc_bytes, 16);
    }

    #[test]
    fn lower_call_six_int_args_ms_x64_overflows_two_to_stack() {
        let args = [ArgClass::Int; 6];
        let lowered = lower_call("bar", &args, None, X64Abi::MicrosoftX64).unwrap();
        assert_eq!(lowered.layout.int_reg_assignments.len(), 4);
        assert_eq!(lowered.layout.stack_slots.len(), 2);
        // 32 (shadow) + 16 (two args) = 48 (already 16-aligned).
        assert_eq!(lowered.layout.total_stack_alloc_bytes, 48);
    }

    // ── float-only assignment ──

    #[test]
    fn lower_call_three_float_args_sysv_assigns_xmm0_xmm1_xmm2() {
        let args = [ArgClass::Float, ArgClass::Float, ArgClass::Float];
        let lowered = lower_call("bar", &args, None, X64Abi::SystemV).unwrap();
        assert_eq!(
            lowered.layout.float_reg_assignments,
            vec![(0, XmmReg::Xmm0), (1, XmmReg::Xmm1), (2, XmmReg::Xmm2)]
        );
        assert!(lowered.layout.int_reg_assignments.is_empty());
    }

    #[test]
    fn lower_call_nine_float_args_sysv_overflows_ninth() {
        let args = [ArgClass::Float; 9];
        let lowered = lower_call("bar", &args, None, X64Abi::SystemV).unwrap();
        assert_eq!(lowered.layout.float_reg_assignments.len(), 8);
        assert_eq!(lowered.layout.stack_slots.len(), 1);
    }

    // ── mixed int + float assignment (the SysV independent-counter rule) ──

    #[test]
    fn lower_call_int_float_int_sysv_uses_rdi_xmm0_rsi() {
        let args = [ArgClass::Int, ArgClass::Float, ArgClass::Int];
        let lowered = lower_call("bar", &args, None, X64Abi::SystemV).unwrap();
        assert_eq!(
            lowered.layout.int_reg_assignments,
            vec![(0, GpReg::Rdi), (2, GpReg::Rsi)]
        );
        assert_eq!(
            lowered.layout.float_reg_assignments,
            vec![(1, XmmReg::Xmm0)]
        );
    }

    // ── mixed int + float assignment (the MS-x64 positional-alias rule) ──

    #[test]
    fn lower_call_int_float_int_ms_x64_uses_rcx_xmm1_r8() {
        // MS-x64 : positional. Slot 0 = rcx (int), slot 1 = xmm1 (float ; NOT xmm0 !),
        // slot 2 = r8 (int).
        let args = [ArgClass::Int, ArgClass::Float, ArgClass::Int];
        let lowered = lower_call("bar", &args, None, X64Abi::MicrosoftX64).unwrap();
        assert_eq!(
            lowered.layout.int_reg_assignments,
            vec![(0, GpReg::Rcx), (2, GpReg::R8)]
        );
        assert_eq!(
            lowered.layout.float_reg_assignments,
            vec![(1, XmmReg::Xmm1)]
        );
    }

    #[test]
    fn lower_call_float_int_float_ms_x64_uses_xmm0_rdx_xmm2() {
        // Slot 0 = xmm0 (float), slot 1 = rdx (int), slot 2 = xmm2 (float).
        let args = [ArgClass::Float, ArgClass::Int, ArgClass::Float];
        let lowered = lower_call("bar", &args, None, X64Abi::MicrosoftX64).unwrap();
        assert_eq!(
            lowered.layout.float_reg_assignments,
            vec![(0, XmmReg::Xmm0), (2, XmmReg::Xmm2)]
        );
        assert_eq!(lowered.layout.int_reg_assignments, vec![(1, GpReg::Rdx)]);
    }

    // ── alignment invariant ──

    #[test]
    fn call_boundary_alignment_holds_after_classify() {
        for arity in 0..12 {
            for &abi in &[X64Abi::SystemV, X64Abi::MicrosoftX64] {
                let args = vec![ArgClass::Int; arity];
                let layout = classify_call_args(&args, abi);
                assert!(
                    layout.is_call_boundary_aligned(),
                    "alignment failed @ abi={abi}, arity={arity}, total={}",
                    layout.total_stack_alloc_bytes
                );
            }
        }
    }

    // ── return-value placement ──

    #[test]
    fn lower_call_int_return_lands_in_rax() {
        let lowered = lower_call("ret_int", &[], Some(ArgClass::Int), X64Abi::SystemV).unwrap();
        assert_eq!(lowered.return_reg, ReturnReg::Int(GpReg::Rax));
        let lowered =
            lower_call("ret_int", &[], Some(ArgClass::Int), X64Abi::MicrosoftX64).unwrap();
        assert_eq!(lowered.return_reg, ReturnReg::Int(GpReg::Rax));
    }

    #[test]
    fn lower_call_float_return_lands_in_xmm0() {
        let lowered = lower_call("ret_f", &[], Some(ArgClass::Float), X64Abi::SystemV).unwrap();
        assert_eq!(lowered.return_reg, ReturnReg::Float(XmmReg::Xmm0));
    }

    // ── lower_return ──

    #[test]
    fn lower_return_int_class_yields_rax_return() {
        let r = lower_return(Some(ArgClass::Int), X64Abi::SystemV);
        assert_eq!(r.return_reg, ReturnReg::Int(GpReg::Rax));
        assert_eq!(r.insns, vec![AbstractInsn::Ret]);
    }

    #[test]
    fn lower_return_void_yields_no_return_reg() {
        let r = lower_return(None, X64Abi::MicrosoftX64);
        assert_eq!(r.return_reg, ReturnReg::Void);
        assert_eq!(r.insns.len(), 1);
        assert!(matches!(r.insns[0], AbstractInsn::Ret));
    }

    // ── prologue / epilogue ──

    #[test]
    fn prologue_minimal_layout_is_push_rbp_mov_rbp_rsp() {
        let layout = FunctionLayout::new(X64Abi::MicrosoftX64);
        let p = lower_prologue(&layout);
        // No frame, no callee-saved → push rbp ; mov rbp, rsp.
        assert_eq!(p.insns.len(), 2);
        assert!(matches!(
            &p.insns[0],
            AbstractInsn::Push { reg: GpReg::Rbp }
        ));
        assert!(matches!(
            &p.insns[1],
            AbstractInsn::MovGpGp {
                dst: GpReg::Rbp,
                src: GpReg::Rsp,
            }
        ));
        assert_eq!(p.total_frame_bytes, 0);
        assert!(p.callee_saved_slots.is_empty());
    }

    #[test]
    fn prologue_with_local_frame_emits_sub_rsp() {
        let layout = FunctionLayout {
            abi: X64Abi::SystemV,
            local_frame_bytes: 16,
            callee_saved_gp_used: vec![],
            callee_saved_xmm_used: vec![],
        };
        let p = lower_prologue(&layout);
        assert_eq!(p.total_frame_bytes, 16);
        // push rbp ; mov rbp, rsp ; sub rsp, 16.
        assert_eq!(p.insns.len(), 3);
        assert!(matches!(p.insns[2], AbstractInsn::SubRsp { bytes: 16 }));
    }

    #[test]
    fn prologue_rounds_unaligned_local_frame_up_to_sixteen() {
        let layout = FunctionLayout {
            abi: X64Abi::SystemV,
            local_frame_bytes: 4, // not 16-aligned → rounded up
            callee_saved_gp_used: vec![],
            callee_saved_xmm_used: vec![],
        };
        let p = lower_prologue(&layout);
        // 4 bytes raw + 12 bytes pad → 16.
        assert_eq!(p.total_frame_bytes, 16);
    }

    #[test]
    fn prologue_with_callee_saved_pushes_them_in_order() {
        let layout = FunctionLayout {
            abi: X64Abi::SystemV,
            local_frame_bytes: 0,
            callee_saved_gp_used: vec![GpReg::Rbx, GpReg::R12, GpReg::R13],
            callee_saved_xmm_used: vec![],
        };
        let p = lower_prologue(&layout);
        // push rbp, mov rbp/rsp, [no sub rsp since unaligned local==0 + 24-byte pushes
        //  are 8-mod-16 → 8 bytes pad needed], push rbx, push r12, push r13.
        // After 3 GP pushes = 24 bytes ; 24 % 16 = 8 ; padding = 8 ;
        // total_frame_bytes = 0 + 8 = 8.
        assert_eq!(p.total_frame_bytes, 8);
        assert!(
            p.insns
                .iter()
                .filter(|i| matches!(i, AbstractInsn::Push { reg: GpReg::Rbx }))
                .count()
                == 1
        );
        assert!(
            p.insns
                .iter()
                .filter(|i| matches!(i, AbstractInsn::Push { reg: GpReg::R12 }))
                .count()
                == 1
        );
        assert!(
            p.insns
                .iter()
                .filter(|i| matches!(i, AbstractInsn::Push { reg: GpReg::R13 }))
                .count()
                == 1
        );
        assert_eq!(p.callee_saved_slots.len(), 3);
    }

    #[test]
    fn prologue_two_callee_saved_keeps_alignment_without_padding() {
        let layout = FunctionLayout {
            abi: X64Abi::MicrosoftX64,
            local_frame_bytes: 0,
            // 2 GP pushes = 16 bytes → already 16-aligned ; no extra padding needed.
            callee_saved_gp_used: vec![GpReg::Rbx, GpReg::Rdi],
            callee_saved_xmm_used: vec![],
        };
        let p = lower_prologue(&layout);
        assert_eq!(p.total_frame_bytes, 0);
    }

    #[test]
    fn epilogue_pops_callee_saved_in_reverse_order() {
        let layout = FunctionLayout {
            abi: X64Abi::SystemV,
            local_frame_bytes: 16,
            callee_saved_gp_used: vec![GpReg::Rbx, GpReg::R12, GpReg::R13],
            callee_saved_xmm_used: vec![],
        };
        let e = lower_epilogue(&layout);
        // Expect : pop r13, pop r12, pop rbx, add rsp, frame, pop rbp, ret.
        let pop_indices: Vec<_> = e
            .insns
            .iter()
            .enumerate()
            .filter_map(|(i, insn)| match insn {
                AbstractInsn::Pop { reg }
                    if matches!(reg, GpReg::R13 | GpReg::R12 | GpReg::Rbx) =>
                {
                    Some((i, *reg))
                }
                _ => None,
            })
            .collect();
        assert_eq!(pop_indices.len(), 3);
        assert_eq!(pop_indices[0].1, GpReg::R13);
        assert_eq!(pop_indices[1].1, GpReg::R12);
        assert_eq!(pop_indices[2].1, GpReg::Rbx);
        // Last two insns should be `pop rbp ; ret`.
        let last_two = &e.insns[e.insns.len() - 2..];
        assert!(matches!(last_two[0], AbstractInsn::Pop { reg: GpReg::Rbp }));
        assert!(matches!(last_two[1], AbstractInsn::Ret));
    }

    #[test]
    fn epilogue_for_uses_prologue_frame_bytes_directly() {
        let layout = FunctionLayout {
            abi: X64Abi::MicrosoftX64,
            local_frame_bytes: 12,
            callee_saved_gp_used: vec![],
            callee_saved_xmm_used: vec![],
        };
        let p = lower_prologue(&layout);
        let e = lower_epilogue_for(&layout, &p);
        // total_frame_bytes was rounded to 16. Epilogue's add_rsp must match.
        let add_rsps: Vec<_> = e
            .insns
            .iter()
            .filter_map(|i| match i {
                AbstractInsn::AddRsp { bytes } => Some(*bytes),
                _ => None,
            })
            .collect();
        assert_eq!(add_rsps, vec![16]);
    }

    // ── shadow-space invariant on MS-x64 ──

    #[test]
    fn ms_x64_zero_arg_call_still_allocates_thirty_two_byte_shadow() {
        let lowered = lower_call("noargs", &[], None, X64Abi::MicrosoftX64).unwrap();
        assert_eq!(lowered.final_rsp_delta, 32);
    }

    #[test]
    fn ms_x64_four_arg_call_in_regs_still_allocates_thirty_two_byte_shadow() {
        let args = [ArgClass::Int; 4];
        let lowered = lower_call("foo", &args, None, X64Abi::MicrosoftX64).unwrap();
        // All 4 args in regs, but shadow space still required.
        assert_eq!(lowered.layout.shadow_space_bytes, 32);
        assert_eq!(lowered.final_rsp_delta, 32);
    }

    // ── classify_call_args edge cases ──

    #[test]
    fn classify_empty_call_returns_empty_layout() {
        let layout = classify_call_args(&[], X64Abi::SystemV);
        assert!(layout.int_reg_assignments.is_empty());
        assert!(layout.float_reg_assignments.is_empty());
        assert!(layout.stack_slots.is_empty());
        assert_eq!(layout.stack_args_bytes, 0);
        assert_eq!(layout.shadow_space_bytes, 0);
        assert_eq!(layout.total_stack_alloc_bytes, 0);
    }

    #[test]
    fn classify_reports_correct_reg_count() {
        let args = [
            ArgClass::Int,
            ArgClass::Float,
            ArgClass::Int,
            ArgClass::Float,
        ];
        let layout = classify_call_args(&args, X64Abi::SystemV);
        assert_eq!(layout.reg_arg_count(), 4);
        assert_eq!(layout.stack_arg_count(), 0);
    }

    // ── return reg consistency ──

    #[test]
    fn return_reg_resolution_is_abi_invariant_for_scalar_return() {
        for &abi in &[X64Abi::SystemV, X64Abi::MicrosoftX64] {
            assert_eq!(
                ReturnReg::for_class(abi, ArgClass::Int),
                ReturnReg::Int(GpReg::Rax)
            );
            assert_eq!(
                ReturnReg::for_class(abi, ArgClass::Float),
                ReturnReg::Float(XmmReg::Xmm0)
            );
        }
    }

    // ── final_rsp_delta matches layout.total_stack_alloc_bytes ──

    #[test]
    fn lower_call_final_rsp_delta_matches_layout_alloc() {
        for arity in [0, 1, 4, 6, 8, 10] {
            for &abi in &[X64Abi::SystemV, X64Abi::MicrosoftX64] {
                let args = vec![ArgClass::Int; arity];
                let l = lower_call("foo", &args, None, abi).unwrap();
                assert_eq!(l.final_rsp_delta, l.layout.total_stack_alloc_bytes);
            }
        }
    }

    // ── ABI-shape signature snapshot tests (the readable ABI tables) ──

    #[test]
    fn sysv_int_arg_shape_snapshot() {
        let args = [ArgClass::Int; 8];
        let layout = classify_call_args(&args, X64Abi::SystemV);
        let assigned: Vec<GpReg> = layout.int_reg_assignments.iter().map(|(_, r)| *r).collect();
        assert_eq!(
            assigned,
            vec![
                GpReg::Rdi,
                GpReg::Rsi,
                GpReg::Rdx,
                GpReg::Rcx,
                GpReg::R8,
                GpReg::R9,
            ]
        );
        assert_eq!(layout.stack_slots.len(), 2);
    }

    #[test]
    fn ms_x64_int_arg_shape_snapshot() {
        let args = [ArgClass::Int; 6];
        let layout = classify_call_args(&args, X64Abi::MicrosoftX64);
        let assigned: Vec<GpReg> = layout.int_reg_assignments.iter().map(|(_, r)| *r).collect();
        assert_eq!(assigned, vec![GpReg::Rcx, GpReg::Rdx, GpReg::R8, GpReg::R9]);
        assert_eq!(layout.stack_slots.len(), 2);
    }
}
