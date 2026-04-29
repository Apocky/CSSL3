//! ModR/M + SIB + REX prefix synthesis.
//!
//! § SPEC : Intel SDM Vol 2 §2.1 (REX), §2.2 (ModR/M + SIB).
//!
//! § BIT-LAYOUT
//! ```text
//!   REX byte : 0100 W R X B
//!     W : 1 ⇒ 64-bit operand size (forces operand size to 64 even when default is 32).
//!     R : extends ModR/M.reg by 1 bit (4-bit reg index).
//!     X : extends SIB.index by 1 bit.
//!     B : extends ModR/M.r/m OR SIB.base OR opcode-embedded reg by 1 bit.
//!   ModR/M byte : mod[7:6] | reg[5:3] | r/m[2:0]
//!     mod = 00 : [r/m] (or rip-rel if r/m == 101 ; or [disp32 + sib] if r/m == 100 + sib.base==101).
//!     mod = 01 : [r/m + disp8].
//!     mod = 10 : [r/m + disp32].
//!     mod = 11 : r/m is a register (register-direct).
//!   SIB byte : scale[7:6] | index[5:3] | base[2:0]
//!     index == 100 ⇒ no index (rsp-as-index = "no index" by encoding).
//!     base == 101 with mod==00 ⇒ no base (use disp32 only).
//! ```
//!
//! § EXPORT-SURFACE
//!   - `make_rex_optional` / `make_rex_forced` — 0x40-prefix synthesis.
//!   - `make_modrm` / `make_sib` — bit-packers.
//!   - `emit_mem_operand` — full ModR/M + (optional) SIB + (optional) disp emission for any
//!     [`MemOperand`], parameterised by the 3-bit reg-field (the OTHER operand).
//!
//! § INVARIANTS
//!   - rsp/r12 as base ⇒ SIB byte mandatory.
//!   - rbp/r13 with disp=0 ⇒ promote to mod=01 + disp8=0 (avoid RIP-rel collision).
//!   - SIB.base==101 with mod=00 ⇒ disp32 takes the place of base ; for `[rbp/r13 + index*scale]`
//!     with disp=0 we promote to mod=01 + disp8=0.

use crate::encoder::mem::{MemOperand, Scale};
use crate::encoder::reg::Gpr;

/// Pack a REX prefix byte. Returns `Some(byte)` if any of W/R/X/B set OR `force=true`.
///
/// Spec : Intel SDM Vol 2 §2.1.
#[must_use]
#[allow(clippy::fn_params_excessive_bools)] // 4 bools = REX bit-field W/R/X/B by spec
pub const fn make_rex_optional(w: bool, r: bool, x: bool, b: bool) -> Option<u8> {
    if w || r || x || b {
        Some(make_rex_forced(w, r, x, b))
    } else {
        None
    }
}

/// Always pack a REX prefix byte (used for 8-bit ops on spl/bpl/sil/dil where REX must
/// be present even with W=R=X=B=0 to disambiguate from ah/ch/dh/bh).
#[must_use]
#[allow(clippy::fn_params_excessive_bools)] // 4 bools = REX bit-field W/R/X/B by spec
pub const fn make_rex_forced(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | ((w as u8) << 3) | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

/// Pack a ModR/M byte from (mod, reg, r/m) fields.
///
/// `mode` ∈ {0,1,2,3} ; `reg` and `rm` are 3-bit fields (caller must mask by 0b111).
#[must_use]
pub const fn make_modrm(mode: u8, reg: u8, rm: u8) -> u8 {
    debug_assert!(mode < 4);
    ((mode & 0b11) << 6) | ((reg & 0b111) << 3) | (rm & 0b111)
}

/// Pack a SIB byte from (scale, index, base) fields.
#[must_use]
pub const fn make_sib(scale: u8, index: u8, base: u8) -> u8 {
    debug_assert!(scale < 4);
    ((scale & 0b11) << 6) | ((index & 0b111) << 3) | (base & 0b111)
}

/// Result of emitting a memory operand : the bytes (ModR/M + optional SIB + optional disp)
/// plus the REX.X / REX.B extension bits the caller must merge into the prefix.
#[derive(Debug, Clone, Copy)]
pub struct MemEmission {
    /// 3-bit r/m field (already masked) when `mod != 11`. Caller bakes into ModR/M.
    pub rm: u8,
    /// 2-bit mod field.
    pub mode: u8,
    /// REX.X (set if SIB.index ≥ 8).
    pub rex_x: bool,
    /// REX.B (set if SIB.base ≥ 8 OR ModR/M.r/m ≥ 8).
    pub rex_b: bool,
    /// Optional SIB byte to emit AFTER ModR/M.
    pub sib: Option<u8>,
    /// Number of displacement bytes to emit AFTER (ModR/M[+SIB]) : 0, 1, or 4.
    pub disp_kind: DispKind,
    /// Raw displacement value (sign-extended into the `disp_kind`-many emission bytes).
    pub disp: i32,
}

/// How many displacement bytes to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispKind {
    /// No displacement bytes.
    None,
    /// 1-byte signed displacement (mod=01).
    D8,
    /// 4-byte signed displacement (mod=00 + r/m=101 ; mod=00 + sib.base=101 ; mod=10 + any).
    D32,
}

/// Compute ModR/M + SIB + disp encoding for a [`MemOperand`].
///
/// The `reg`-field (the OTHER operand of the ModR/M byte) is NOT included here —
/// callers add it via [`make_modrm`] using the returned `mode` + `rm`.
#[must_use]
pub fn lower_mem_operand(mem: MemOperand) -> MemEmission {
    match mem {
        MemOperand::Base { base, disp } => lower_base(base, disp),
        MemOperand::BaseIndex {
            base,
            index,
            scale,
            disp,
        } => lower_base_index(base, index, scale, disp),
        MemOperand::IndexOnly { index, scale, disp } => lower_index_only(index, scale, disp),
        MemOperand::RipRel { disp } => MemEmission {
            rm: 0b101,
            mode: 0b00,
            rex_x: false,
            rex_b: false,
            sib: None,
            disp_kind: DispKind::D32,
            disp,
        },
    }
}

fn lower_base(base: Gpr, disp: i32) -> MemEmission {
    // rsp/r12 always require SIB byte (r/m=100 means "SIB-follows" at mod ∈ {00,01,10}).
    if base.forces_sib_as_base() {
        // SIB encodes : scale=00 (×1), index=100 ("no index"), base=base.rm_bits()
        let sib = make_sib(Scale::S1.bits(), 0b100, base.rm_bits());
        let (mode, disp_kind) = pick_disp_mode(disp, /*rbp_collision=*/ false);
        return MemEmission {
            rm: 0b100,
            mode,
            rex_x: false,
            rex_b: base.rex_bit(),
            sib: Some(sib),
            disp_kind,
            disp,
        };
    }

    // rbp/r13 with disp=0 collide with RIP-rel slot at mod=00 r/m=101 ; we promote
    // those to mod=01 + disp8=0 to disambiguate.
    let (mode, disp_kind) = pick_disp_mode(disp, base.collides_with_riprel());
    MemEmission {
        rm: base.rm_bits(),
        mode,
        rex_x: false,
        rex_b: base.rex_bit(),
        sib: None,
        disp_kind,
        disp,
    }
}

fn lower_base_index(base: Gpr, index: Gpr, scale: Scale, disp: i32) -> MemEmission {
    // index == rsp is invalid for SIB.index (encoding means "no index").
    debug_assert!(
        !matches!(index, Gpr::Rsp),
        "SIB.index = rsp is encoded as 'no index' ; use IndexOnly or rewrite"
    );

    let sib = make_sib(scale.bits(), index.rm_bits(), base.rm_bits());
    // If base is rbp/r13 with disp=0, promote to mod=01 disp8=0 (SIB.base=101 + mod=00
    // means "no base", which we don't want for the [rbp/r13 + index*scale] case).
    let (mode, disp_kind) = pick_disp_mode(disp, base.collides_with_riprel());
    MemEmission {
        rm: 0b100,
        mode,
        rex_x: index.rex_bit(),
        rex_b: base.rex_bit(),
        sib: Some(sib),
        disp_kind,
        disp,
    }
}

fn lower_index_only(index: Gpr, scale: Scale, disp: i32) -> MemEmission {
    // Mod=00 SIB.base=101 ⇒ "no base" + always disp32.
    let sib = make_sib(scale.bits(), index.rm_bits(), 0b101);
    MemEmission {
        rm: 0b100,
        mode: 0b00,
        rex_x: index.rex_bit(),
        rex_b: false,
        sib: Some(sib),
        disp_kind: DispKind::D32,
        disp,
    }
}

/// Pick `mod` field + displacement-kind for a non-rsp/r12 base.
fn pick_disp_mode(disp: i32, rbp_collision: bool) -> (u8, DispKind) {
    if disp == 0 && !rbp_collision {
        (0b00, DispKind::None)
    } else if (-128..=127).contains(&disp) {
        (0b01, DispKind::D8)
    } else {
        (0b10, DispKind::D32)
    }
}

/// Append displacement bytes per [`DispKind`].
pub fn emit_disp(buf: &mut Vec<u8>, kind: DispKind, disp: i32) {
    match kind {
        DispKind::None => {}
        DispKind::D8 => buf.push((disp as i8) as u8),
        DispKind::D32 => buf.extend_from_slice(&disp.to_le_bytes()),
    }
}
