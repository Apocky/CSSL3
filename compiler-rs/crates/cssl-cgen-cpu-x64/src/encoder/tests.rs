//! Per-instruction byte-equality tests.
//!
//! § GOLDEN-ENCODING SOURCES
//!   - Intel SDM Vol 2 (per-opcode encoding tables).
//!   - Cross-checked against godbolt.org with `gcc -m64 -O0` / `nasm -f elf64`.
//!   - REX/ModR/M bit-packing verified by hand against §2.1/§2.2.
//!
//! § DISCIPLINE
//!   Each test asserts EXACT byte sequence equality. Hex literals are paired with the
//!   AT&T-syntax form for cross-reference. Comments cite the Intel form where it differs
//!   meaningfully (e.g. operand-order swap MR vs RM forms).
//!
//! § REPORT-BACK SAMPLE
//!   `mov rax, 0x42`        ⇒ 48 C7 C0 42 00 00 00      (REX.W + C7 /0 + imm32)  ✓
//!   `mov rax, 0xDEADBEEFCAFEBABE` ⇒ 48 B8 BE BA FE CA EF BE AD DE  (REX.W + B8+rd + imm64)  ✓

use super::*;

// ─── helpers ─────────────────────────────────────────────────────────

fn enc(i: X64Inst) -> Vec<u8> {
    encode_inst(&i)
}

// ─── REX-prefix unit tests ───────────────────────────────────────────

#[test]
fn rex_prefix_construction_w_only() {
    // REX.W only : 0x48
    assert_eq!(make_rex_optional(true, false, false, false), Some(0x48));
}

#[test]
fn rex_prefix_construction_b_only() {
    // REX.B only (e.g. for [r8] base) : 0x41
    assert_eq!(make_rex_optional(false, false, false, true), Some(0x41));
}

#[test]
fn rex_prefix_construction_all_zero_optional() {
    // No bits set ⇒ no REX needed
    assert_eq!(make_rex_optional(false, false, false, false), None);
}

#[test]
fn rex_prefix_construction_all_set() {
    // All 4 bits : 0x4F
    assert_eq!(make_rex_optional(true, true, true, true), Some(0x4F));
}

// ─── ModR/M packing ──────────────────────────────────────────────────

#[test]
fn modrm_register_direct() {
    // mod=11, reg=000 (rax), r/m=001 (rcx) ⇒ 0xC1
    assert_eq!(make_modrm(0b11, 0, 1), 0xC1);
}

#[test]
fn modrm_memory_no_disp() {
    // mod=00, reg=001 (rcx), r/m=000 (rax) ⇒ 0x08
    assert_eq!(make_modrm(0b00, 1, 0), 0x08);
}

#[test]
fn sib_byte_packing() {
    // scale=00 (×1), index=100 (rsp = no-index), base=100 (rsp) ⇒ 0x24
    assert_eq!(make_sib(0, 0b100, 0b100), 0x24);
    // scale=11 (×8), index=001 (rcx), base=000 (rax) ⇒ 0xC8
    assert_eq!(make_sib(0b11, 0b001, 0b000), 0xC8);
}

// ─── MOV r, imm — the report-back sample ─────────────────────────────

#[test]
fn mov_rax_imm32_short_form() {
    // mov rax, 0x42  ⇒  48 C7 C0 42 00 00 00  (REX.W + C7 /0 + imm32)
    let bytes = enc(X64Inst::MovRI {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        imm: 0x42,
    });
    assert_eq!(bytes, vec![0x48, 0xC7, 0xC0, 0x42, 0x00, 0x00, 0x00]);
}

#[test]
fn mov_rax_imm64_full_form() {
    // mov rax, 0xDEADBEEFCAFEBABE  ⇒  48 B8 BE BA FE CA EF BE AD DE
    #[allow(clippy::cast_possible_wrap)] // bit-pattern transmute, intentional
    let imm = 0xDEAD_BEEF_CAFE_BABE_u64 as i64;
    let bytes = enc(X64Inst::MovRI {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        imm,
    });
    assert_eq!(
        bytes,
        vec![0x48, 0xB8, 0xBE, 0xBA, 0xFE, 0xCA, 0xEF, 0xBE, 0xAD, 0xDE]
    );
}

#[test]
fn mov_eax_imm32() {
    // mov eax, 0x12345678  ⇒  B8 78 56 34 12  (no REX needed)
    let bytes = enc(X64Inst::MovRI {
        size: OperandSize::B32,
        dst: Gpr::Rax,
        imm: 0x1234_5678,
    });
    assert_eq!(bytes, vec![0xB8, 0x78, 0x56, 0x34, 0x12]);
}

#[test]
fn mov_r8_imm32() {
    // mov r8, 0x42  ⇒  49 C7 C0 42 00 00 00  (REX.W + REX.B)
    let bytes = enc(X64Inst::MovRI {
        size: OperandSize::B64,
        dst: Gpr::R8,
        imm: 0x42,
    });
    assert_eq!(bytes, vec![0x49, 0xC7, 0xC0, 0x42, 0x00, 0x00, 0x00]);
}

#[test]
fn mov_r15d_imm32() {
    // mov r15d, 0x1  ⇒  41 BF 01 00 00 00  (REX.B only)
    let bytes = enc(X64Inst::MovRI {
        size: OperandSize::B32,
        dst: Gpr::R15,
        imm: 1,
    });
    assert_eq!(bytes, vec![0x41, 0xBF, 0x01, 0x00, 0x00, 0x00]);
}

// ─── MOV r, r ────────────────────────────────────────────────────────

#[test]
fn mov_rax_rcx() {
    // mov rax, rcx  ⇒  48 89 C8  (REX.W + 89 + ModR/M(11 001 000))
    let bytes = enc(X64Inst::MovRR {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x48, 0x89, 0xC8]);
}

#[test]
fn mov_r8_r9() {
    // mov r8, r9  ⇒  4D 89 C8  (REX.W + REX.R + REX.B + 89 + ModR/M(11 001 000))
    let bytes = enc(X64Inst::MovRR {
        size: OperandSize::B64,
        dst: Gpr::R8,
        src: Gpr::R9,
    });
    assert_eq!(bytes, vec![0x4D, 0x89, 0xC8]);
}

#[test]
fn mov_eax_ecx_no_rex() {
    // mov eax, ecx  ⇒  89 C8  (no REX — 32-bit default)
    let bytes = enc(X64Inst::MovRR {
        size: OperandSize::B32,
        dst: Gpr::Rax,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x89, 0xC8]);
}

// ─── ADD / SUB / CMP r, r ────────────────────────────────────────────

#[test]
fn add_rax_rcx() {
    // add rax, rcx  ⇒  48 01 C8
    let bytes = enc(X64Inst::AddRR {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x48, 0x01, 0xC8]);
}

#[test]
fn sub_rax_rcx() {
    // sub rax, rcx  ⇒  48 29 C8
    let bytes = enc(X64Inst::SubRR {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x48, 0x29, 0xC8]);
}

#[test]
fn cmp_rax_rcx() {
    // cmp rax, rcx  ⇒  48 39 C8
    let bytes = enc(X64Inst::CmpRR {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x48, 0x39, 0xC8]);
}

#[test]
fn add_rax_imm8() {
    // add rax, 0x10  ⇒  48 83 C0 10  (REX.W + 83 /0 + imm8 sign-ext)
    let bytes = enc(X64Inst::AddRI {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        imm: 0x10,
    });
    assert_eq!(bytes, vec![0x48, 0x83, 0xC0, 0x10]);
}

#[test]
fn add_rax_imm32() {
    // add rax, 0x12345678  ⇒  48 05 78 56 34 12 OR 48 81 C0 78 56 34 12
    // We use the 81 /0 form (consistent /ext encoding ; the 05-form is short-AX-imm specific).
    let bytes = enc(X64Inst::AddRI {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        imm: 0x1234_5678,
    });
    assert_eq!(bytes, vec![0x48, 0x81, 0xC0, 0x78, 0x56, 0x34, 0x12]);
}

#[test]
fn sub_rax_imm8() {
    // sub rax, 0x20  ⇒  48 83 E8 20  (REX.W + 83 /5 + imm8)
    let bytes = enc(X64Inst::SubRI {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        imm: 0x20,
    });
    assert_eq!(bytes, vec![0x48, 0x83, 0xE8, 0x20]);
}

#[test]
fn cmp_rax_imm8() {
    // cmp rax, 0x10  ⇒  48 83 F8 10
    let bytes = enc(X64Inst::CmpRI {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        imm: 0x10,
    });
    assert_eq!(bytes, vec![0x48, 0x83, 0xF8, 0x10]);
}

// ─── MUL / IMUL / IDIV ───────────────────────────────────────────────

#[test]
fn mul_rcx() {
    // mul rcx  ⇒  48 F7 E1  (REX.W + F7 /4)
    let bytes = enc(X64Inst::Mul {
        size: OperandSize::B64,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x48, 0xF7, 0xE1]);
}

#[test]
fn imul_rcx_one_operand() {
    // imul rcx  ⇒  48 F7 E9  (REX.W + F7 /5)
    let bytes = enc(X64Inst::ImulR {
        size: OperandSize::B64,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x48, 0xF7, 0xE9]);
}

#[test]
fn imul_rax_rcx_two_operand() {
    // imul rax, rcx  ⇒  48 0F AF C1  (REX.W + 0F AF /r ModR/M(11 000 001))
    let bytes = enc(X64Inst::ImulRR {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x48, 0x0F, 0xAF, 0xC1]);
}

#[test]
fn idiv_rcx() {
    // idiv rcx  ⇒  48 F7 F9  (REX.W + F7 /7)
    let bytes = enc(X64Inst::IDiv {
        size: OperandSize::B64,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x48, 0xF7, 0xF9]);
}

// ─── PUSH / POP / RET / CALL ─────────────────────────────────────────

#[test]
fn push_rax() {
    // push rax  ⇒  50  (1 byte, no REX)
    let bytes = enc(X64Inst::Push { src: Gpr::Rax });
    assert_eq!(bytes, vec![0x50]);
}

#[test]
fn push_r12() {
    // push r12  ⇒  41 54  (REX.B + 50+rb)
    let bytes = enc(X64Inst::Push { src: Gpr::R12 });
    assert_eq!(bytes, vec![0x41, 0x54]);
}

#[test]
fn pop_rbp() {
    // pop rbp  ⇒  5D
    let bytes = enc(X64Inst::Pop { dst: Gpr::Rbp });
    assert_eq!(bytes, vec![0x5D]);
}

#[test]
fn ret_inst() {
    // ret  ⇒  C3
    let bytes = enc(X64Inst::Ret);
    assert_eq!(bytes, vec![0xC3]);
}

#[test]
fn call_rel32() {
    // call +0x100  ⇒  E8 00 01 00 00  (5 bytes always)
    let bytes = enc(X64Inst::CallRel {
        target: BranchTarget::Rel(0x100),
    });
    assert_eq!(bytes, vec![0xE8, 0x00, 0x01, 0x00, 0x00]);
}

// ─── JMP / JCC ───────────────────────────────────────────────────────

#[test]
fn jmp_short_form() {
    // jmp +5  ⇒  EB 05
    let bytes = enc(X64Inst::Jmp {
        target: BranchTarget::Rel(5),
    });
    assert_eq!(bytes, vec![0xEB, 0x05]);
}

#[test]
fn jmp_long_form() {
    // jmp +0x1000  ⇒  E9 00 10 00 00
    let bytes = enc(X64Inst::Jmp {
        target: BranchTarget::Rel(0x1000),
    });
    assert_eq!(bytes, vec![0xE9, 0x00, 0x10, 0x00, 0x00]);
}

#[test]
fn jmp_force_long_form() {
    // jmp.32 +5  ⇒  E9 05 00 00 00  (force long form even though +5 fits short)
    let bytes = enc(X64Inst::Jmp {
        target: BranchTarget::Rel32(5),
    });
    assert_eq!(bytes, vec![0xE9, 0x05, 0x00, 0x00, 0x00]);
}

#[test]
fn je_short_form() {
    // je +5  ⇒  74 05  (0x70 | 0x4 = 0x74)
    let bytes = enc(X64Inst::Jcc {
        cond: Cond::E,
        target: BranchTarget::Rel(5),
    });
    assert_eq!(bytes, vec![0x74, 0x05]);
}

#[test]
fn jne_long_form() {
    // jne +0x1000  ⇒  0F 85 00 10 00 00
    let bytes = enc(X64Inst::Jcc {
        cond: Cond::Ne,
        target: BranchTarget::Rel(0x1000),
    });
    assert_eq!(bytes, vec![0x0F, 0x85, 0x00, 0x10, 0x00, 0x00]);
}

#[test]
fn jl_short_form() {
    // jl -5  ⇒  7C FB  (0x70 | 0xC, signed-byte FB = -5)
    let bytes = enc(X64Inst::Jcc {
        cond: Cond::L,
        target: BranchTarget::Rel(-5),
    });
    assert_eq!(bytes, vec![0x7C, 0xFB]);
}

// ─── LOAD / STORE / LEA ──────────────────────────────────────────────

#[test]
fn load_rax_from_rcx() {
    // mov rax, [rcx]  ⇒  48 8B 01  (REX.W + 8B + ModR/M(00 000 001))
    let bytes = enc(X64Inst::Load {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::base(Gpr::Rcx),
    });
    assert_eq!(bytes, vec![0x48, 0x8B, 0x01]);
}

#[test]
fn load_rax_from_rcx_disp8() {
    // mov rax, [rcx + 8]  ⇒  48 8B 41 08
    let bytes = enc(X64Inst::Load {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::base_disp(Gpr::Rcx, 8),
    });
    assert_eq!(bytes, vec![0x48, 0x8B, 0x41, 0x08]);
}

#[test]
fn load_rax_from_rcx_disp32() {
    // mov rax, [rcx + 0x1000]  ⇒  48 8B 81 00 10 00 00
    let bytes = enc(X64Inst::Load {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::base_disp(Gpr::Rcx, 0x1000),
    });
    assert_eq!(bytes, vec![0x48, 0x8B, 0x81, 0x00, 0x10, 0x00, 0x00]);
}

#[test]
fn load_rax_from_rsp_requires_sib() {
    // mov rax, [rsp]  ⇒  48 8B 04 24  (REX.W + 8B + ModR/M(00 000 100) + SIB(00 100 100))
    // r/m=100 forces SIB ; SIB.index=100 means "no index" ; SIB.base=100=rsp.
    let bytes = enc(X64Inst::Load {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::base(Gpr::Rsp),
    });
    assert_eq!(bytes, vec![0x48, 0x8B, 0x04, 0x24]);
}

#[test]
fn load_rax_from_rbp_disp_zero_promotes_to_disp8() {
    // mov rax, [rbp]  ⇒  48 8B 45 00  (mod=01, disp8=0 — avoids RIP-rel slot)
    let bytes = enc(X64Inst::Load {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::base(Gpr::Rbp),
    });
    assert_eq!(bytes, vec![0x48, 0x8B, 0x45, 0x00]);
}

#[test]
fn load_rax_from_r13_disp_zero_promotes_to_disp8() {
    // mov rax, [r13]  ⇒  49 8B 45 00  (REX.B + mod=01 disp8=0)
    let bytes = enc(X64Inst::Load {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::base(Gpr::R13),
    });
    assert_eq!(bytes, vec![0x49, 0x8B, 0x45, 0x00]);
}

#[test]
fn load_rax_rip_relative() {
    // mov rax, [rip + 0]  ⇒  48 8B 05 00 00 00 00
    let bytes = enc(X64Inst::Load {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::rip_rel(0),
    });
    assert_eq!(bytes, vec![0x48, 0x8B, 0x05, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn store_to_rcx() {
    // mov [rcx], rax  ⇒  48 89 01
    let bytes = enc(X64Inst::Store {
        size: OperandSize::B64,
        dst: MemOperand::base(Gpr::Rcx),
        src: Gpr::Rax,
    });
    assert_eq!(bytes, vec![0x48, 0x89, 0x01]);
}

#[test]
fn lea_rax_from_rcx_plus_8() {
    // lea rax, [rcx + 8]  ⇒  48 8D 41 08
    let bytes = enc(X64Inst::Lea {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::base_disp(Gpr::Rcx, 8),
    });
    assert_eq!(bytes, vec![0x48, 0x8D, 0x41, 0x08]);
}

#[test]
fn lea_rax_from_rax_plus_rcx_times_4_disp8() {
    // lea rax, [rax + rcx*4 + 8]  ⇒  48 8D 44 88 08
    // SIB scale=10(×4) index=001(rcx) base=000(rax) ⇒ 0x88
    let bytes = enc(X64Inst::Lea {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::BaseIndex {
            base: Gpr::Rax,
            index: Gpr::Rcx,
            scale: Scale::S4,
            disp: 8,
        },
    });
    assert_eq!(bytes, vec![0x48, 0x8D, 0x44, 0x88, 0x08]);
}

// ─── SSE2 scalar FP ──────────────────────────────────────────────────

#[test]
fn movsd_xmm0_xmm1() {
    // movsd xmm0, xmm1  ⇒  F2 0F 10 C1
    let bytes = enc(X64Inst::MovsdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x10, 0xC1]);
}

#[test]
fn addsd_xmm0_xmm1() {
    // addsd xmm0, xmm1  ⇒  F2 0F 58 C1
    let bytes = enc(X64Inst::AddsdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x58, 0xC1]);
}

#[test]
fn addss_xmm0_xmm1() {
    // addss xmm0, xmm1  ⇒  F3 0F 58 C1
    let bytes = enc(X64Inst::AddssRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x58, 0xC1]);
}

#[test]
fn subsd_xmm0_xmm1() {
    let bytes = enc(X64Inst::SubsdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    // F2 0F 5C C1
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x5C, 0xC1]);
}

#[test]
fn mulsd_xmm0_xmm1() {
    let bytes = enc(X64Inst::MulsdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    // F2 0F 59 C1
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x59, 0xC1]);
}

#[test]
fn divsd_xmm0_xmm1() {
    let bytes = enc(X64Inst::DivsdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    // F2 0F 5E C1
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x5E, 0xC1]);
}

#[test]
fn ucomisd_xmm0_xmm1() {
    // ucomisd xmm0, xmm1  ⇒  66 0F 2E C1
    let bytes = enc(X64Inst::UComisdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0x66, 0x0F, 0x2E, 0xC1]);
}

#[test]
fn cvtsi2sd_xmm0_eax() {
    // cvtsi2sd xmm0, eax  ⇒  F2 0F 2A C0
    let bytes = enc(X64Inst::CvtSi2sdRR {
        size: OperandSize::B32,
        dst: Xmm::Xmm0,
        src: Gpr::Rax,
    });
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x2A, 0xC0]);
}

#[test]
fn cvtsi2sd_xmm0_rax_64bit() {
    // cvtsi2sd xmm0, rax  ⇒  F2 48 0F 2A C0
    let bytes = enc(X64Inst::CvtSi2sdRR {
        size: OperandSize::B64,
        dst: Xmm::Xmm0,
        src: Gpr::Rax,
    });
    assert_eq!(bytes, vec![0xF2, 0x48, 0x0F, 0x2A, 0xC0]);
}

#[test]
fn cvtsd2si_eax_xmm0() {
    // cvtsd2si eax, xmm0  ⇒  F2 0F 2D C0
    let bytes = enc(X64Inst::CvtSd2siRR {
        size: OperandSize::B32,
        dst: Gpr::Rax,
        src: Xmm::Xmm0,
    });
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x2D, 0xC0]);
}

#[test]
fn cvtsd2si_rax_xmm0_64bit() {
    // cvtsd2si rax, xmm0  ⇒  F2 48 0F 2D C0
    let bytes = enc(X64Inst::CvtSd2siRR {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: Xmm::Xmm0,
    });
    assert_eq!(bytes, vec![0xF2, 0x48, 0x0F, 0x2D, 0xC0]);
}

// ─── extended-reg SSE2 ───────────────────────────────────────────────

#[test]
fn addsd_xmm8_xmm9_extended_regs() {
    // addsd xmm8, xmm9  ⇒  F2 45 0F 58 C1  (REX.R + REX.B + 0F 58 + ModR/M(11 000 001))
    let bytes = enc(X64Inst::AddsdRR {
        dst: Xmm::Xmm8,
        src: Xmm::Xmm9,
    });
    assert_eq!(bytes, vec![0xF2, 0x45, 0x0F, 0x58, 0xC1]);
}

// ─── 32-bit forms verification ───────────────────────────────────────

#[test]
fn add_eax_ecx_no_rex() {
    // add eax, ecx  ⇒  01 C8  (no REX, 32-bit default)
    let bytes = enc(X64Inst::AddRR {
        size: OperandSize::B32,
        dst: Gpr::Rax,
        src: Gpr::Rcx,
    });
    assert_eq!(bytes, vec![0x01, 0xC8]);
}

#[test]
fn lea_with_rsp_base_forces_sib() {
    // lea rax, [rsp + 0x10]  ⇒  48 8D 44 24 10
    let bytes = enc(X64Inst::Lea {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: MemOperand::base_disp(Gpr::Rsp, 0x10),
    });
    assert_eq!(bytes, vec![0x48, 0x8D, 0x44, 0x24, 0x10]);
}

#[test]
fn store_with_extended_regs() {
    // mov [r10 + 4], r11  ⇒  4D 89 5A 04
    // REX.R(r11) + REX.B(r10) + REX.W ⇒ 4D ; opcode 89 ; ModR/M(01 011 010) ; disp8=04
    let bytes = enc(X64Inst::Store {
        size: OperandSize::B64,
        dst: MemOperand::base_disp(Gpr::R10, 4),
        src: Gpr::R11,
    });
    assert_eq!(bytes, vec![0x4D, 0x89, 0x5A, 0x04]);
}

// ─── group-3 op-ext sanity ───────────────────────────────────────────

#[test]
fn grp3_op_ext_validity() {
    assert!(encode::is_grp3_op(4)); // MUL
    assert!(encode::is_grp3_op(5)); // IMUL
    assert!(encode::is_grp3_op(6)); // DIV
    assert!(encode::is_grp3_op(7)); // IDIV
    assert!(!encode::is_grp3_op(0));
    assert!(!encode::is_grp3_op(8));
}

// ─── encode_into round-trip ──────────────────────────────────────────

#[test]
fn encode_into_appends_to_existing_buffer() {
    let mut buf = vec![0xDE, 0xAD];
    encode_into(&mut buf, &X64Inst::Ret);
    assert_eq!(buf, vec![0xDE, 0xAD, 0xC3]);
}

// ─── G11 (T11-D102) — SSE2 scalar float path extensions ──────────────

#[test]
fn ucomiss_xmm0_xmm1() {
    // ucomiss xmm0, xmm1  ⇒  0F 2E C1  (no scalar prefix ; one of the
    // few SSE-1 era ops that share the legacy zero-prefix encoding).
    let bytes = enc(X64Inst::UComissRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0x0F, 0x2E, 0xC1]);
}

#[test]
fn comiss_xmm0_xmm1() {
    // comiss xmm0, xmm1  ⇒  0F 2F C1
    let bytes = enc(X64Inst::ComissRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0x0F, 0x2F, 0xC1]);
}

#[test]
fn comisd_xmm0_xmm1() {
    // comisd xmm0, xmm1  ⇒  66 0F 2F C1
    let bytes = enc(X64Inst::ComisdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0x66, 0x0F, 0x2F, 0xC1]);
}

#[test]
fn sqrtss_xmm0_xmm1() {
    // sqrtss xmm0, xmm1  ⇒  F3 0F 51 C1
    let bytes = enc(X64Inst::SqrtssRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x51, 0xC1]);
}

#[test]
fn sqrtsd_xmm0_xmm1() {
    // sqrtsd xmm0, xmm1  ⇒  F2 0F 51 C1
    let bytes = enc(X64Inst::SqrtsdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x51, 0xC1]);
}

#[test]
fn cvtsi2ss_xmm0_eax() {
    // cvtsi2ss xmm0, eax  ⇒  F3 0F 2A C0
    let bytes = enc(X64Inst::CvtSi2ssRR {
        size: OperandSize::B32,
        dst: Xmm::Xmm0,
        src: Gpr::Rax,
    });
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x2A, 0xC0]);
}

#[test]
fn cvtsi2ss_xmm0_rax_64bit() {
    // cvtsi2ss xmm0, rax  ⇒  F3 48 0F 2A C0
    let bytes = enc(X64Inst::CvtSi2ssRR {
        size: OperandSize::B64,
        dst: Xmm::Xmm0,
        src: Gpr::Rax,
    });
    assert_eq!(bytes, vec![0xF3, 0x48, 0x0F, 0x2A, 0xC0]);
}

#[test]
fn cvtss2si_eax_xmm0() {
    // cvtss2si eax, xmm0  ⇒  F3 0F 2D C0
    let bytes = enc(X64Inst::CvtSs2siRR {
        size: OperandSize::B32,
        dst: Gpr::Rax,
        src: Xmm::Xmm0,
    });
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x2D, 0xC0]);
}

#[test]
fn cvtss2si_rax_xmm0_64bit() {
    // cvtss2si rax, xmm0  ⇒  F3 48 0F 2D C0
    let bytes = enc(X64Inst::CvtSs2siRR {
        size: OperandSize::B64,
        dst: Gpr::Rax,
        src: Xmm::Xmm0,
    });
    assert_eq!(bytes, vec![0xF3, 0x48, 0x0F, 0x2D, 0xC0]);
}

#[test]
fn xorps_xmm0_xmm1() {
    // xorps xmm0, xmm1  ⇒  0F 57 C1
    let bytes = enc(X64Inst::XorpsRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0x0F, 0x57, 0xC1]);
}

#[test]
fn xorpd_xmm0_xmm1() {
    // xorpd xmm0, xmm1  ⇒  66 0F 57 C1
    let bytes = enc(X64Inst::XorpdRR {
        dst: Xmm::Xmm0,
        src: Xmm::Xmm1,
    });
    assert_eq!(bytes, vec![0x66, 0x0F, 0x57, 0xC1]);
}

#[test]
fn movsd_load_xmm0_from_rcx() {
    // movsd xmm0, [rcx]  ⇒  F2 0F 10 01
    let bytes = enc(X64Inst::MovsdLoad {
        dst: Xmm::Xmm0,
        src: MemOperand::base(Gpr::Rcx),
    });
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x10, 0x01]);
}

#[test]
fn movsd_store_xmm0_to_rcx() {
    // movsd [rcx], xmm0  ⇒  F2 0F 11 01
    let bytes = enc(X64Inst::MovsdStore {
        dst: MemOperand::base(Gpr::Rcx),
        src: Xmm::Xmm0,
    });
    assert_eq!(bytes, vec![0xF2, 0x0F, 0x11, 0x01]);
}

#[test]
fn movss_load_xmm0_from_rcx() {
    // movss xmm0, [rcx]  ⇒  F3 0F 10 01
    let bytes = enc(X64Inst::MovssLoad {
        dst: Xmm::Xmm0,
        src: MemOperand::base(Gpr::Rcx),
    });
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x10, 0x01]);
}

#[test]
fn movss_store_xmm0_to_rcx() {
    // movss [rcx], xmm0  ⇒  F3 0F 11 01
    let bytes = enc(X64Inst::MovssStore {
        dst: MemOperand::base(Gpr::Rcx),
        src: Xmm::Xmm0,
    });
    assert_eq!(bytes, vec![0xF3, 0x0F, 0x11, 0x01]);
}

#[test]
fn movd_xmm_from_eax() {
    // movd xmm0, eax  ⇒  66 0F 6E C0
    let bytes = enc(X64Inst::MovdXmmFromGp {
        dst: Xmm::Xmm0,
        src: Gpr::Rax,
    });
    assert_eq!(bytes, vec![0x66, 0x0F, 0x6E, 0xC0]);
}

#[test]
fn movd_eax_from_xmm() {
    // movd eax, xmm0  ⇒  66 0F 7E C0
    let bytes = enc(X64Inst::MovdGpFromXmm {
        dst: Gpr::Rax,
        src: Xmm::Xmm0,
    });
    assert_eq!(bytes, vec![0x66, 0x0F, 0x7E, 0xC0]);
}

#[test]
fn movq_xmm_from_rax() {
    // movq xmm0, rax  ⇒  66 48 0F 6E C0
    let bytes = enc(X64Inst::MovqXmmFromGp {
        dst: Xmm::Xmm0,
        src: Gpr::Rax,
    });
    assert_eq!(bytes, vec![0x66, 0x48, 0x0F, 0x6E, 0xC0]);
}

#[test]
fn movq_rax_from_xmm() {
    // movq rax, xmm0  ⇒  66 48 0F 7E C0
    let bytes = enc(X64Inst::MovqGpFromXmm {
        dst: Gpr::Rax,
        src: Xmm::Xmm0,
    });
    assert_eq!(bytes, vec![0x66, 0x48, 0x0F, 0x7E, 0xC0]);
}

#[test]
fn sqrtsd_extended_xmm() {
    // sqrtsd xmm8, xmm9  ⇒  F2 45 0F 51 C1  (REX.R + REX.B)
    let bytes = enc(X64Inst::SqrtsdRR {
        dst: Xmm::Xmm8,
        src: Xmm::Xmm9,
    });
    assert_eq!(bytes, vec![0xF2, 0x45, 0x0F, 0x51, 0xC1]);
}

#[test]
fn movq_extended_xmm_from_extended_gpr() {
    // movq xmm8, r9  ⇒  66 4D 0F 6E C1
    let bytes = enc(X64Inst::MovqXmmFromGp {
        dst: Xmm::Xmm8,
        src: Gpr::R9,
    });
    assert_eq!(bytes, vec![0x66, 0x4D, 0x0F, 0x6E, 0xC1]);
}

// ─── version sentinel ────────────────────────────────────────────────

#[test]
fn x64_encoder_version_present() {
    assert!(!X64_ENCODER_VERSION.is_empty());
}
