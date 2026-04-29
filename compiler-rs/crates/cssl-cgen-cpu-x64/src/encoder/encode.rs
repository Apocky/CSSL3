//! Per-instruction byte-emission : `X64Inst` → `Vec<u8>`.
//!
//! § SPEC : Intel SDM Vol 2 (per-opcode tables), `specs/14_BACKEND.csl`.
//!
//! § STRATEGY
//!   For each `X64Inst` variant we follow the canonical Intel-SDM emission order :
//!     `[legacy-prefix(es)]  [REX]  [opcode]  [ModR/M]  [SIB]  [disp]  [immediate]`
//!   Helper builders compose the prefix + opcode part separately from the operand part
//!   so that we can share encoding paths across r-r and r-m variants where applicable.
//!
//! § COVERAGE
//!   See `inst.rs` for the canonical X64Inst surface. This module emits each variant.
//!
//! § BRANCH-ENCODING ‼
//!   Branches accept a [`BranchTarget::Rel`] (auto-pick short/long) or [`BranchTarget::Rel32`]
//!   (force long). When the encoded relative offset fits ±127 we emit the 2-byte short form
//!   (1 opcode + 1 disp8) ; otherwise the 6-byte long form (1 opcode + 1 secondary + 4 disp32).
//!   Note : the offset is measured from END-of-instruction. For a placeholder pattern that
//!   the linker patches, callers should use `Rel32(0)` and emit a relocation.

use crate::encoder::inst::{BranchTarget, Cond, X64Inst};
use crate::encoder::mem::MemOperand;
use crate::encoder::modrm::{
    emit_disp, lower_mem_operand, make_modrm, make_rex_optional, MemEmission,
};
use crate::encoder::reg::{Gpr, OperandSize, Xmm};

// ─── public surface ──────────────────────────────────────────────────

/// Emit the byte sequence for a single [`X64Inst`].
#[must_use]
pub fn encode_inst(inst: &X64Inst) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16);
    encode_into(&mut buf, inst);
    buf
}

/// Emit into an existing buffer (avoids allocation in hot paths).
pub fn encode_into(buf: &mut Vec<u8>, inst: &X64Inst) {
    match *inst {
        // ─── integer moves ─────────────────────────────────────────
        X64Inst::MovRR { size, dst, src } => emit_mov_rr(buf, size, dst, src),
        X64Inst::MovRI { size, dst, imm } => emit_mov_ri(buf, size, dst, imm),
        X64Inst::Load { size, dst, src } => emit_load(buf, size, dst, src),
        X64Inst::Store { size, dst, src } => emit_store(buf, size, dst, src),
        X64Inst::Lea { size, dst, src } => emit_lea(buf, size, dst, src),
        // ─── integer arithmetic ────────────────────────────────────
        X64Inst::AddRR { size, dst, src } => emit_alu_rr(buf, size, dst, src, /*op=*/ 0x01),
        X64Inst::AddRI { size, dst, imm } => emit_alu_ri(buf, size, dst, imm, /*op_ext=*/ 0),
        X64Inst::SubRR { size, dst, src } => emit_alu_rr(buf, size, dst, src, /*op=*/ 0x29),
        X64Inst::SubRI { size, dst, imm } => emit_alu_ri(buf, size, dst, imm, /*op_ext=*/ 5),
        X64Inst::Mul { size, src } => emit_unary_grp3(buf, size, src, /*op_ext=*/ 4),
        X64Inst::ImulR { size, src } => emit_unary_grp3(buf, size, src, /*op_ext=*/ 5),
        X64Inst::ImulRR { size, dst, src } => emit_imul_rr(buf, size, dst, src),
        X64Inst::IDiv { size, src } => emit_unary_grp3(buf, size, src, /*op_ext=*/ 7),
        X64Inst::CmpRR { size, dst, src } => emit_alu_rr(buf, size, dst, src, /*op=*/ 0x39),
        X64Inst::CmpRI { size, dst, imm } => emit_alu_ri(buf, size, dst, imm, /*op_ext=*/ 7),
        // ─── stack / control ───────────────────────────────────────
        X64Inst::Push { src } => emit_push(buf, src),
        X64Inst::Pop { dst } => emit_pop(buf, dst),
        X64Inst::Ret => buf.push(0xC3),
        X64Inst::CallRel { target } => emit_call_rel(buf, target),
        X64Inst::Jmp { target } => emit_jmp(buf, target),
        X64Inst::Jcc { cond, target } => emit_jcc(buf, cond, target),
        // ─── SSE2 scalar FP ────────────────────────────────────────
        X64Inst::MovssRR { dst, src } => emit_sse_rr(buf, 0xF3, 0x10, dst, src),
        X64Inst::MovsdRR { dst, src } => emit_sse_rr(buf, 0xF2, 0x10, dst, src),
        X64Inst::AddssRR { dst, src } => emit_sse_rr(buf, 0xF3, 0x58, dst, src),
        X64Inst::AddsdRR { dst, src } => emit_sse_rr(buf, 0xF2, 0x58, dst, src),
        X64Inst::SubssRR { dst, src } => emit_sse_rr(buf, 0xF3, 0x5C, dst, src),
        X64Inst::SubsdRR { dst, src } => emit_sse_rr(buf, 0xF2, 0x5C, dst, src),
        X64Inst::MulssRR { dst, src } => emit_sse_rr(buf, 0xF3, 0x59, dst, src),
        X64Inst::MulsdRR { dst, src } => emit_sse_rr(buf, 0xF2, 0x59, dst, src),
        X64Inst::DivssRR { dst, src } => emit_sse_rr(buf, 0xF3, 0x5E, dst, src),
        X64Inst::DivsdRR { dst, src } => emit_sse_rr(buf, 0xF2, 0x5E, dst, src),
        X64Inst::UComisdRR { dst, src } => emit_ucomisd_rr(buf, dst, src),
        X64Inst::CvtSi2sdRR { size, dst, src } => emit_cvtsi2sd(buf, size, dst, src),
        X64Inst::CvtSd2siRR { size, dst, src } => emit_cvtsd2si(buf, size, dst, src),
        X64Inst::XorpsRR { dst, src } => emit_xorps_rr(buf, dst, src),
    }
}

// ─── integer moves ───────────────────────────────────────────────────

fn emit_mov_rr(buf: &mut Vec<u8>, size: OperandSize, dst: Gpr, src: Gpr) {
    // Use 0x89 /r (MOV r/m, r) form : reg-field = src, r/m-field = dst.
    if size.needs_op_size_prefix() {
        buf.push(0x66);
    }
    let rex = make_rex_optional(
        size.rex_w(),
        /*R=*/ src.rex_bit(),
        /*X=*/ false,
        /*B=*/ dst.rex_bit(),
    );
    if let Some(r) = rex {
        buf.push(r);
    }
    let opcode = if matches!(size, OperandSize::B8) {
        0x88
    } else {
        0x89
    };
    buf.push(opcode);
    buf.push(make_modrm(0b11, src.rm_bits(), dst.rm_bits()));
}

fn emit_mov_ri(buf: &mut Vec<u8>, size: OperandSize, dst: Gpr, imm: i64) {
    match size {
        OperandSize::B8 => {
            // B0+rb ib (no REX.W) ; need REX.B if dst ≥ r8.
            let rex = make_rex_optional(false, false, false, dst.rex_bit());
            if let Some(r) = rex {
                buf.push(r);
            }
            buf.push(0xB0 | dst.rm_bits());
            buf.push(imm as u8);
        }
        OperandSize::B16 => {
            buf.push(0x66);
            let rex = make_rex_optional(false, false, false, dst.rex_bit());
            if let Some(r) = rex {
                buf.push(r);
            }
            buf.push(0xB8 | dst.rm_bits());
            let v = imm as i16;
            buf.extend_from_slice(&v.to_le_bytes());
        }
        OperandSize::B32 => {
            // B8+rd id
            let rex = make_rex_optional(false, false, false, dst.rex_bit());
            if let Some(r) = rex {
                buf.push(r);
            }
            buf.push(0xB8 | dst.rm_bits());
            let v = imm as i32;
            buf.extend_from_slice(&v.to_le_bytes());
        }
        OperandSize::B64 => {
            // Shortest form selection :
            //   if imm fits sign-extended-32 ⇒ MOV r/m64, imm32 (REX.W + C7 /0 id) — 7 bytes
            //   else                          ⇒ MOV r64, imm64 (REX.W + B8+rd io) — 10 bytes
            let rex =
                make_rex_optional(/*W=*/ true, false, false, dst.rex_bit()).expect("REX.W set");
            buf.push(rex);
            let imm32 = imm as i32;
            if i64::from(imm32) == imm {
                buf.push(0xC7);
                buf.push(make_modrm(0b11, 0, dst.rm_bits()));
                buf.extend_from_slice(&imm32.to_le_bytes());
            } else {
                buf.push(0xB8 | dst.rm_bits());
                buf.extend_from_slice(&imm.to_le_bytes());
            }
        }
    }
}

fn emit_load(buf: &mut Vec<u8>, size: OperandSize, dst: Gpr, src: MemOperand) {
    // 0x8B /r — MOV r, r/m. reg-field = dst, r/m-field = mem.
    if size.needs_op_size_prefix() {
        buf.push(0x66);
    }
    let mem = lower_mem_operand(src);
    emit_rex_for_reg_mem(buf, size.rex_w(), dst.rex_bit(), &mem);
    let opcode = if matches!(size, OperandSize::B8) {
        0x8A
    } else {
        0x8B
    };
    buf.push(opcode);
    emit_modrm_sib_disp(buf, dst.rm_bits(), &mem);
}

fn emit_store(buf: &mut Vec<u8>, size: OperandSize, dst: MemOperand, src: Gpr) {
    // 0x89 /r — MOV r/m, r. reg-field = src, r/m-field = mem.
    if size.needs_op_size_prefix() {
        buf.push(0x66);
    }
    let mem = lower_mem_operand(dst);
    emit_rex_for_reg_mem(buf, size.rex_w(), src.rex_bit(), &mem);
    let opcode = if matches!(size, OperandSize::B8) {
        0x88
    } else {
        0x89
    };
    buf.push(opcode);
    emit_modrm_sib_disp(buf, src.rm_bits(), &mem);
}

fn emit_lea(buf: &mut Vec<u8>, size: OperandSize, dst: Gpr, src: MemOperand) {
    // 0x8D /r — LEA. Always REX.W for 64-bit lea (32-bit form is 0x8D no W).
    let mem = lower_mem_operand(src);
    emit_rex_for_reg_mem(buf, size.rex_w(), dst.rex_bit(), &mem);
    buf.push(0x8D);
    emit_modrm_sib_disp(buf, dst.rm_bits(), &mem);
}

// ─── integer arithmetic ──────────────────────────────────────────────

/// Generic ALU r/r form : `op /r` with reg=src r/m=dst (MR encoding).
fn emit_alu_rr(buf: &mut Vec<u8>, size: OperandSize, dst: Gpr, src: Gpr, opcode_64: u8) {
    if size.needs_op_size_prefix() {
        buf.push(0x66);
    }
    let rex = make_rex_optional(size.rex_w(), src.rex_bit(), false, dst.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    let opcode = if matches!(size, OperandSize::B8) {
        // 8-bit forms : ADD=0x00, SUB=0x28, CMP=0x38 (one less than the 32/64 form)
        opcode_64 - 1
    } else {
        opcode_64
    };
    buf.push(opcode);
    buf.push(make_modrm(0b11, src.rm_bits(), dst.rm_bits()));
}

/// Generic ALU r/imm form : 0x81 /op_ext id  OR  0x83 /op_ext ib (sign-extended).
fn emit_alu_ri(buf: &mut Vec<u8>, size: OperandSize, dst: Gpr, imm: i32, op_ext: u8) {
    if size.needs_op_size_prefix() {
        buf.push(0x66);
    }
    let rex = make_rex_optional(size.rex_w(), false, false, dst.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    if matches!(size, OperandSize::B8) {
        // 80 /ext ib
        buf.push(0x80);
        buf.push(make_modrm(0b11, op_ext, dst.rm_bits()));
        buf.push(imm as u8);
    } else if (-128..=127).contains(&imm) {
        // 83 /ext ib (sign-extended)
        buf.push(0x83);
        buf.push(make_modrm(0b11, op_ext, dst.rm_bits()));
        buf.push(imm as u8);
    } else if matches!(size, OperandSize::B16) {
        buf.push(0x81);
        buf.push(make_modrm(0b11, op_ext, dst.rm_bits()));
        buf.extend_from_slice(&(imm as i16).to_le_bytes());
    } else {
        buf.push(0x81);
        buf.push(make_modrm(0b11, op_ext, dst.rm_bits()));
        buf.extend_from_slice(&imm.to_le_bytes());
    }
}

/// Group-3 unary (MUL=4, IMUL=5, IDIV=7) : F7 /ext (or F6 /ext for 8-bit).
fn emit_unary_grp3(buf: &mut Vec<u8>, size: OperandSize, src: Gpr, op_ext: u8) {
    if size.needs_op_size_prefix() {
        buf.push(0x66);
    }
    let rex = make_rex_optional(size.rex_w(), false, false, src.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    let opcode = if matches!(size, OperandSize::B8) {
        0xF6
    } else {
        0xF7
    };
    buf.push(opcode);
    buf.push(make_modrm(0b11, op_ext, src.rm_bits()));
}

/// IMUL r/r 2-operand form : 0F AF /r (dst = dst * src).
fn emit_imul_rr(buf: &mut Vec<u8>, size: OperandSize, dst: Gpr, src: Gpr) {
    if size.needs_op_size_prefix() {
        buf.push(0x66);
    }
    let rex = make_rex_optional(size.rex_w(), dst.rex_bit(), false, src.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    buf.push(0x0F);
    buf.push(0xAF);
    buf.push(make_modrm(0b11, dst.rm_bits(), src.rm_bits()));
}

// ─── stack / control ─────────────────────────────────────────────────

fn emit_push(buf: &mut Vec<u8>, src: Gpr) {
    // 50+rd — short form. REX.B if r8..r15.
    if src.rex_bit() {
        buf.push(make_rex_optional(false, false, false, true).expect("REX.B"));
    }
    buf.push(0x50 | src.rm_bits());
}

fn emit_pop(buf: &mut Vec<u8>, dst: Gpr) {
    if dst.rex_bit() {
        buf.push(make_rex_optional(false, false, false, true).expect("REX.B"));
    }
    buf.push(0x58 | dst.rm_bits());
}

fn emit_call_rel(buf: &mut Vec<u8>, target: BranchTarget) {
    // E8 cd — relative 32-bit, always 5 bytes (no short call form).
    let off = match target {
        BranchTarget::Rel(o) | BranchTarget::Rel32(o) => o,
    };
    buf.push(0xE8);
    buf.extend_from_slice(&off.to_le_bytes());
}

fn emit_jmp(buf: &mut Vec<u8>, target: BranchTarget) {
    match target {
        BranchTarget::Rel(off) if (-128..=127).contains(&off) => {
            // EB cb — short jump, 2 bytes.
            buf.push(0xEB);
            buf.push(off as u8);
        }
        BranchTarget::Rel(off) | BranchTarget::Rel32(off) => {
            // E9 cd — near jump, 5 bytes.
            buf.push(0xE9);
            buf.extend_from_slice(&off.to_le_bytes());
        }
    }
}

fn emit_jcc(buf: &mut Vec<u8>, cond: Cond, target: BranchTarget) {
    match target {
        BranchTarget::Rel(off) if (-128..=127).contains(&off) => {
            // 70+cc cb — short conditional, 2 bytes.
            buf.push(0x70 | cond.code());
            buf.push(off as u8);
        }
        BranchTarget::Rel(off) | BranchTarget::Rel32(off) => {
            // 0F 80+cc cd — near conditional, 6 bytes.
            buf.push(0x0F);
            buf.push(0x80 | cond.code());
            buf.extend_from_slice(&off.to_le_bytes());
        }
    }
}

// ─── SSE2 scalar FP ──────────────────────────────────────────────────

/// Generic SSE2 r/r emission : `[scalar-prefix] [REX] 0F [opcode] [ModR/M r=dst,r/m=src]`.
fn emit_sse_rr(buf: &mut Vec<u8>, scalar_prefix: u8, opcode: u8, dst: Xmm, src: Xmm) {
    buf.push(scalar_prefix);
    let rex = make_rex_optional(false, dst.rex_bit(), false, src.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    buf.push(0x0F);
    buf.push(opcode);
    buf.push(make_modrm(0b11, dst.rm_bits(), src.rm_bits()));
}

fn emit_ucomisd_rr(buf: &mut Vec<u8>, dst: Xmm, src: Xmm) {
    // 66 0F 2E /r
    buf.push(0x66);
    let rex = make_rex_optional(false, dst.rex_bit(), false, src.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    buf.push(0x0F);
    buf.push(0x2E);
    buf.push(make_modrm(0b11, dst.rm_bits(), src.rm_bits()));
}

fn emit_cvtsi2sd(buf: &mut Vec<u8>, size: OperandSize, dst: Xmm, src: Gpr) {
    // F2 [REX.W?] 0F 2A /r
    debug_assert!(
        matches!(size, OperandSize::B32 | OperandSize::B64),
        "cvtsi2sd only takes 32/64-bit GPR src"
    );
    buf.push(0xF2);
    let rex = make_rex_optional(size.rex_w(), dst.rex_bit(), false, src.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    buf.push(0x0F);
    buf.push(0x2A);
    buf.push(make_modrm(0b11, dst.rm_bits(), src.rm_bits()));
}

fn emit_cvtsd2si(buf: &mut Vec<u8>, size: OperandSize, dst: Gpr, src: Xmm) {
    // F2 [REX.W?] 0F 2D /r
    debug_assert!(
        matches!(size, OperandSize::B32 | OperandSize::B64),
        "cvtsd2si only takes 32/64-bit GPR dst"
    );
    buf.push(0xF2);
    let rex = make_rex_optional(size.rex_w(), dst.rex_bit(), false, src.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    buf.push(0x0F);
    buf.push(0x2D);
    buf.push(make_modrm(0b11, dst.rm_bits(), src.rm_bits()));
}

fn emit_xorps_rr(buf: &mut Vec<u8>, dst: Xmm, src: Xmm) {
    // 0F 57 /r — XORPS xmm, xmm. No prefix byte (unlike scalar SSE2 ops
    // which use F2/F3) ; the SSE1 packed-single XOR has bare 0F 57.
    let rex = make_rex_optional(false, dst.rex_bit(), false, src.rex_bit());
    if let Some(r) = rex {
        buf.push(r);
    }
    buf.push(0x0F);
    buf.push(0x57);
    buf.push(make_modrm(0b11, dst.rm_bits(), src.rm_bits()));
}

// ─── shared mem-operand emission helpers ─────────────────────────────

/// Emit REX prefix when needed for a [reg, mem] form. `reg_rex` is the bit-3 of the
/// non-memory operand (the ModR/M.reg field).
fn emit_rex_for_reg_mem(buf: &mut Vec<u8>, w: bool, reg_rex: bool, mem: &MemEmission) {
    if let Some(r) = make_rex_optional(w, reg_rex, mem.rex_x, mem.rex_b) {
        buf.push(r);
    }
}

/// Emit ModR/M [+ SIB] [+ disp] given the reg-field and a lowered memory operand.
fn emit_modrm_sib_disp(buf: &mut Vec<u8>, reg_field: u8, mem: &MemEmission) {
    buf.push(make_modrm(mem.mode, reg_field, mem.rm));
    if let Some(sib) = mem.sib {
        buf.push(sib);
    }
    emit_disp(buf, mem.disp_kind, mem.disp);
}

// ─── helper exported for external testing convenience ────────────────

/// Returns true if the op-ext value is a valid Group-3 unary extension.
#[doc(hidden)]
#[must_use]
pub const fn is_grp3_op(op_ext: u8) -> bool {
    matches!(op_ext, 4..=7)
}
