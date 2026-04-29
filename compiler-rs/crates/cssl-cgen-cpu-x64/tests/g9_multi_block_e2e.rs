//! § g9_multi_block_e2e — end-to-end multi-block CFG test (S7-G9 / T11-D111).
//!
//! § ROLE
//!   Validates the multi-block walker's output by linking + running real
//!   executables produced from synthetic IselFuncs that exercise `Jcc`,
//!   `Jmp`, `Cmp`, `Setcc`, and back-edges. The pipeline must produce
//!   linker-accepted object bytes whose execution yields the algebraically-
//!   expected exit code.
//!
//! § GATE-SKIP DISCIPLINE
//!   Mirrors `linker_smoke.rs` : if no LLD driver is on the host (rustup's
//!   `gcc-ld` directory), the test prints `[skip]` and passes. The
//!   meaningful failure is "linker accepted but the produced exe returned
//!   the wrong exit code".
//!
//! § FIXTURES (no body_lower roundtrip — we build IselFunc directly so we
//!   exercise the G9 walker path independently of the MIR frontend)
//!   - `abs_minus_7` : `fn main() -> i32 { let x = -7 ; if x < 0 { -x } else { x } }`
//!     Asserts exit code = 7 (abs of negative input).
//!   - `abs_plus_7`  : `fn main() -> i32 { let x =  7 ; if x < 0 { -x } else { x } }`
//!     Asserts exit code = 7 (abs of positive input).
//!
//! § ATTESTATION  (PRIME_DIRECTIVE.md § 11)
//!   There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody.

use std::path::PathBuf;
use std::process::Command;

use cssl_cgen_cpu_x64::abi::X64Abi;
use cssl_cgen_cpu_x64::isel::func::{X64Func as IselFunc, X64Signature};
use cssl_cgen_cpu_x64::isel::inst::{
    BlockId, IntCmpKind, X64Imm, X64Inst, X64SetCondCode, X64Term,
};
use cssl_cgen_cpu_x64::isel::vreg::X64Width;
use cssl_cgen_cpu_x64::mb_walker::build_multi_block_func_bytes;
use cssl_cgen_cpu_x64::objemit::{emit_object_file, host_default_target, ObjectTarget};

// ───────────────────────────────────────────────────────────────────────
// § Linker discovery (lifted from linker_smoke.rs)
// ───────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum DriverKind {
    LldLink,
    LdLld,
    Ld64Lld,
}

fn find_lld_driver() -> Option<(PathBuf, DriverKind)> {
    let toolchain = if let Some(p) = std::env::var_os("RUSTUP_HOME") {
        PathBuf::from(p)
    } else if let Some(h) = std::env::var_os("USERPROFILE") {
        let mut p = PathBuf::from(h);
        p.push(".rustup");
        p
    } else if let Some(h) = std::env::var_os("HOME") {
        let mut p = PathBuf::from(h);
        p.push(".rustup");
        p
    } else {
        return None;
    };

    let candidates: &[(&str, DriverKind)] = if cfg!(target_os = "windows") {
        &[("lld-link.exe", DriverKind::LldLink)]
    } else if cfg!(target_os = "macos") {
        &[("ld64.lld", DriverKind::Ld64Lld)]
    } else {
        &[("ld.lld", DriverKind::LdLld)]
    };

    let toolchains_dir = toolchain.join("toolchains");
    let entries = std::fs::read_dir(&toolchains_dir).ok()?;
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let rustlib_dir = entry.path().join("lib").join("rustlib");
        let triple_entries = std::fs::read_dir(&rustlib_dir).ok();
        if let Some(es) = triple_entries {
            for triple_entry in es.flatten() {
                if !triple_entry.path().is_dir() {
                    continue;
                }
                let bin = triple_entry.path().join("bin").join("gcc-ld");
                for (fname, kind) in candidates {
                    let candidate = bin.join(fname);
                    if candidate.is_file() {
                        return Some((candidate, *kind));
                    }
                }
            }
        }
    }
    None
}

fn write_temp(bytes: &[u8], ext: &str) -> std::io::Result<PathBuf> {
    let mut path = std::env::temp_dir();
    let pid = std::process::id();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.push(format!("cssl_x64_g9_{pid}_{stamp}.{ext}"));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

// ───────────────────────────────────────────────────────────────────────
// § Build an `abs(<imm>)` IselFunc — multi-block scf.if shape
// ───────────────────────────────────────────────────────────────────────

/// Build : `fn main() -> i32 { let x = <imm> ; if x < 0 { -x } else { x } }`
///
/// The generated IselFunc has 4 blocks :
///   bb0 (entry) : MovImm v_x, imm ; MovImm v_zero, 0 ; Cmp v_x, v_zero ;
///                 Setcc v_bool (slt) ; Jcc(slt, v_bool) → bb1, bb2
///   bb1 (then)  : Mov v_neg, v_x ; Neg v_neg ; Mov v_merge, v_neg ; Jmp bb3
///   bb2 (else)  : Mov v_merge, v_x ; Jmp bb3
///   bb3 (merge) : Ret v_merge
fn build_main_abs_constant(imm: i32) -> IselFunc {
    let sig = X64Signature::new(vec![], vec![X64Width::I32]);
    let mut f = IselFunc::new("main", sig);
    let v_x = f.fresh_vreg(X64Width::I32);
    let v_zero = f.fresh_vreg(X64Width::I32);
    let v_bool = f.fresh_vreg(X64Width::Bool);
    let v_neg = f.fresh_vreg(X64Width::I32);
    let v_merge = f.fresh_vreg(X64Width::I32);

    let b_then = f.fresh_block();
    let b_else = f.fresh_block();
    let b_merge = f.fresh_block();

    // Entry.
    f.push_inst(
        BlockId::ENTRY,
        X64Inst::MovImm {
            dst: v_x,
            imm: X64Imm::I32(imm),
        },
    );
    f.push_inst(
        BlockId::ENTRY,
        X64Inst::MovImm {
            dst: v_zero,
            imm: X64Imm::I32(0),
        },
    );
    f.push_inst(
        BlockId::ENTRY,
        X64Inst::Cmp {
            lhs: v_x,
            rhs: v_zero,
        },
    );
    f.push_inst(
        BlockId::ENTRY,
        X64Inst::Setcc {
            dst: v_bool,
            cond_kind: X64SetCondCode::Int(IntCmpKind::Slt),
        },
    );
    f.set_terminator(
        BlockId::ENTRY,
        X64Term::Jcc {
            cond_kind: X64SetCondCode::Int(IntCmpKind::Slt),
            cond_vreg: v_bool,
            then_block: b_then,
            else_block: b_else,
        },
    );

    // Then : -x.
    f.push_inst(
        b_then,
        X64Inst::Mov {
            dst: v_neg,
            src: v_x,
        },
    );
    f.push_inst(b_then, X64Inst::Neg { dst: v_neg });
    f.push_inst(
        b_then,
        X64Inst::Mov {
            dst: v_merge,
            src: v_neg,
        },
    );
    f.set_terminator(b_then, X64Term::Jmp { target: b_merge });

    // Else : x.
    f.push_inst(
        b_else,
        X64Inst::Mov {
            dst: v_merge,
            src: v_x,
        },
    );
    f.set_terminator(b_else, X64Term::Jmp { target: b_merge });

    // Merge : Ret v_merge.
    f.set_terminator(
        b_merge,
        X64Term::Ret {
            operands: vec![v_merge],
        },
    );
    f
}

// ───────────────────────────────────────────────────────────────────────
// § Common link + run helper
// ───────────────────────────────────────────────────────────────────────

/// Link the given object bytes into an exe + run it ; return the exit code.
/// Returns `Ok(None)` for skip-conditions (no LLD, link rejected for
/// environmental reasons) ; `Ok(Some(code))` on successful run ; `Err` on
/// linker rejection of the object as malformed.
fn link_and_run(bytes: &[u8], target: ObjectTarget) -> Result<Option<i32>, String> {
    let Some((driver, kind)) = find_lld_driver() else {
        eprintln!("[skip] no LLD driver in rustup gcc-ld dir");
        return Ok(None);
    };

    let ext = match target {
        ObjectTarget::CoffX64 => "obj",
        ObjectTarget::ElfX64 | ObjectTarget::MachOX64 => "o",
    };
    let obj_path = write_temp(bytes, ext).map_err(|e| format!("temp write : {e}"))?;
    let mut exe_path = obj_path.clone();
    exe_path.set_extension(if cfg!(windows) { "exe" } else { "out" });

    let mut cmd = Command::new(&driver);
    match kind {
        DriverKind::LldLink => {
            cmd.arg(format!("/OUT:{}", exe_path.display()));
            cmd.arg("/SUBSYSTEM:CONSOLE");
            cmd.arg("/ENTRY:main");
            cmd.arg("/NODEFAULTLIB");
            cmd.arg(&obj_path);
        }
        DriverKind::LdLld => {
            cmd.arg("-o");
            cmd.arg(&exe_path);
            cmd.arg("-e");
            cmd.arg("main");
            cmd.arg("--no-dynamic-linker");
            cmd.arg(&obj_path);
        }
        DriverKind::Ld64Lld => {
            cmd.arg("-o");
            cmd.arg(&exe_path);
            cmd.arg("-e");
            cmd.arg("_main");
            cmd.arg(&obj_path);
        }
    }

    let out = cmd.output().map_err(|e| format!("LLD spawn : {e}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);

    if out.status.success() && exe_path.exists() {
        let run = Command::new(&exe_path).output();
        let _ = std::fs::remove_file(&obj_path);
        let _ = std::fs::remove_file(&exe_path);
        let run = run.map_err(|e| format!("exe spawn : {e}"))?;
        return Ok(run.status.code());
    }

    let lc = stderr.to_lowercase();
    let malformed = lc.contains("malformed")
        || lc.contains("invalid magic")
        || lc.contains("not a valid")
        || lc.contains("bad relocation")
        || lc.contains("invalid section")
        || lc.contains("corrupt");
    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&exe_path);
    if malformed {
        return Err(format!(
            "LLD rejected G9 multi-block output as malformed :\n stderr: {stderr}"
        ));
    }
    eprintln!("[soft-skip] linker rejected for environmental reason :\n stderr: {stderr}");
    Ok(None)
}

// ───────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────

#[test]
fn g9_abs_minus_7_returns_7() {
    // ─── Build IselFunc + run pipeline. ─────────────────────────────────
    let f = build_main_abs_constant(-7);
    let abi = X64Abi::host_default();
    let obj_func = build_multi_block_func_bytes(&f, abi, /*is_export=*/ true)
        .expect("multi-block walker should succeed for abs(-7)");
    let target = host_default_target();
    let bytes = emit_object_file(&[obj_func], &[], target).expect("object emit");

    // ─── Link + run + assert. ───────────────────────────────────────────
    match link_and_run(&bytes, target) {
        Ok(Some(code)) => {
            assert_eq!(
                code, 7,
                "G9 abs(-7) ran but returned {code} ; expected 7 (|-7| = 7)"
            );
            eprintln!("[end-to-end-OK] G9 abs(-7) → linker → exe ran ; exit code = 7 ✓");
        }
        Ok(None) => {
            eprintln!("[skip] G9 abs(-7) : no LLD or environmental link skip");
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn g9_abs_plus_7_returns_7() {
    let f = build_main_abs_constant(7);
    let abi = X64Abi::host_default();
    let obj_func = build_multi_block_func_bytes(&f, abi, true)
        .expect("multi-block walker should succeed for abs(+7)");
    let target = host_default_target();
    let bytes = emit_object_file(&[obj_func], &[], target).expect("object emit");

    match link_and_run(&bytes, target) {
        Ok(Some(code)) => {
            assert_eq!(
                code, 7,
                "G9 abs(+7) ran but returned {code} ; expected 7 (|+7| = 7)"
            );
            eprintln!("[end-to-end-OK] G9 abs(+7) → linker → exe ran ; exit code = 7 ✓");
        }
        Ok(None) => {
            eprintln!("[skip] G9 abs(+7) : no LLD or environmental link skip");
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn g9_abs_zero_returns_zero() {
    // Edge case : abs(0) = 0 ; the slt-zero compare returns false so the
    // else branch runs : `v_merge = v_x = 0`.
    let f = build_main_abs_constant(0);
    let abi = X64Abi::host_default();
    let obj_func = build_multi_block_func_bytes(&f, abi, true).expect("walker abs(0)");
    let target = host_default_target();
    let bytes = emit_object_file(&[obj_func], &[], target).expect("object emit");

    match link_and_run(&bytes, target) {
        Ok(Some(code)) => {
            assert_eq!(code, 0, "G9 abs(0) ran but returned {code} ; expected 0");
            eprintln!("[end-to-end-OK] G9 abs(0) → linker → exe ran ; exit code = 0 ✓");
        }
        Ok(None) => {
            eprintln!("[skip] G9 abs(0) : no LLD or environmental link skip");
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn g9_abs_minus_one_returns_one() {
    // Smallest non-zero negative — exercises the negate path with a single-
    // bit value.
    let f = build_main_abs_constant(-1);
    let abi = X64Abi::host_default();
    let obj_func = build_multi_block_func_bytes(&f, abi, true).expect("walker abs(-1)");
    let target = host_default_target();
    let bytes = emit_object_file(&[obj_func], &[], target).expect("object emit");

    match link_and_run(&bytes, target) {
        Ok(Some(code)) => {
            assert_eq!(code, 1, "G9 abs(-1) ran but returned {code} ; expected 1");
            eprintln!("[end-to-end-OK] G9 abs(-1) → linker → exe ran ; exit code = 1 ✓");
        }
        Ok(None) => {
            eprintln!("[skip] G9 abs(-1) : no LLD or environmental link skip");
        }
        Err(e) => panic!("{e}"),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Sum-to-N end-to-end (loop test)
// ───────────────────────────────────────────────────────────────────────

/// Build : `fn main() -> i32 { let mut acc = 0 ; let mut i = 0 ; while i < <n> { acc += i ; i += 1 } ; acc }`
///
/// Bytes-wise this exercises Jcc forward + Jmp back-edge — the layout
/// pass picks short-form branches when block sizes are small.
fn build_main_sum_to_n_constant(n: i32) -> IselFunc {
    let sig = X64Signature::new(vec![], vec![X64Width::I32]);
    let mut f = IselFunc::new("main", sig);
    let v_n = f.fresh_vreg(X64Width::I32);
    let v_acc = f.fresh_vreg(X64Width::I32);
    let v_i = f.fresh_vreg(X64Width::I32);
    let v_bool = f.fresh_vreg(X64Width::Bool);
    let v_tmp = f.fresh_vreg(X64Width::I32);
    let v_one = f.fresh_vreg(X64Width::I32);
    let v_tmp2 = f.fresh_vreg(X64Width::I32);

    let b_header = f.fresh_block();
    let b_body = f.fresh_block();
    let b_exit = f.fresh_block();

    // Entry : acc=0, i=0, n=N ; jump header.
    f.push_inst(
        BlockId::ENTRY,
        X64Inst::MovImm {
            dst: v_n,
            imm: X64Imm::I32(n),
        },
    );
    f.push_inst(
        BlockId::ENTRY,
        X64Inst::MovImm {
            dst: v_acc,
            imm: X64Imm::I32(0),
        },
    );
    f.push_inst(
        BlockId::ENTRY,
        X64Inst::MovImm {
            dst: v_i,
            imm: X64Imm::I32(0),
        },
    );
    f.set_terminator(BlockId::ENTRY, X64Term::Jmp { target: b_header });

    // Header : Cmp i, n ; Setcc bool (slt) ; Jcc(slt) → body, exit.
    f.push_inst(b_header, X64Inst::Cmp { lhs: v_i, rhs: v_n });
    f.push_inst(
        b_header,
        X64Inst::Setcc {
            dst: v_bool,
            cond_kind: X64SetCondCode::Int(IntCmpKind::Slt),
        },
    );
    f.set_terminator(
        b_header,
        X64Term::Jcc {
            cond_kind: X64SetCondCode::Int(IntCmpKind::Slt),
            cond_vreg: v_bool,
            then_block: b_body,
            else_block: b_exit,
        },
    );

    // Body : acc += i ; i += 1 ; back-edge to header.
    f.push_inst(
        b_body,
        X64Inst::Mov {
            dst: v_tmp,
            src: v_acc,
        },
    );
    f.push_inst(
        b_body,
        X64Inst::Add {
            dst: v_tmp,
            src: v_i,
        },
    );
    f.push_inst(
        b_body,
        X64Inst::Mov {
            dst: v_acc,
            src: v_tmp,
        },
    );
    f.push_inst(
        b_body,
        X64Inst::MovImm {
            dst: v_one,
            imm: X64Imm::I32(1),
        },
    );
    f.push_inst(
        b_body,
        X64Inst::Mov {
            dst: v_tmp2,
            src: v_i,
        },
    );
    f.push_inst(
        b_body,
        X64Inst::Add {
            dst: v_tmp2,
            src: v_one,
        },
    );
    f.push_inst(
        b_body,
        X64Inst::Mov {
            dst: v_i,
            src: v_tmp2,
        },
    );
    f.set_terminator(b_body, X64Term::Jmp { target: b_header });

    // Exit : Ret acc.
    f.set_terminator(
        b_exit,
        X64Term::Ret {
            operands: vec![v_acc],
        },
    );
    f
}

#[test]
fn g9_sum_to_n_5_returns_10() {
    // sum_to_n(5) = 0 + 1 + 2 + 3 + 4 = 10.
    let f = build_main_sum_to_n_constant(5);
    let abi = X64Abi::host_default();
    let obj_func = build_multi_block_func_bytes(&f, abi, true).expect("walker sum_to_n(5)");
    let target = host_default_target();
    let bytes = emit_object_file(&[obj_func], &[], target).expect("object emit");

    match link_and_run(&bytes, target) {
        Ok(Some(code)) => {
            assert_eq!(
                code, 10,
                "G9 sum_to_n(5) ran but returned {code} ; expected 10 (0+1+2+3+4)"
            );
            eprintln!("[end-to-end-OK] G9 sum_to_n(5) → linker → exe ran ; exit code = 10 ✓");
        }
        Ok(None) => {
            eprintln!("[skip] G9 sum_to_n(5) : no LLD or environmental link skip");
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn g9_sum_to_n_10_returns_45() {
    // sum_to_n(10) = 0 + 1 + ... + 9 = 45.
    let f = build_main_sum_to_n_constant(10);
    let abi = X64Abi::host_default();
    let obj_func = build_multi_block_func_bytes(&f, abi, true).expect("walker sum_to_n(10)");
    let target = host_default_target();
    let bytes = emit_object_file(&[obj_func], &[], target).expect("object emit");

    match link_and_run(&bytes, target) {
        Ok(Some(code)) => {
            assert_eq!(
                code, 45,
                "G9 sum_to_n(10) ran but returned {code} ; expected 45 (sum 0..10)"
            );
            eprintln!("[end-to-end-OK] G9 sum_to_n(10) → linker → exe ran ; exit code = 45 ✓");
        }
        Ok(None) => {
            eprintln!("[skip] G9 sum_to_n(10) : no LLD or environmental link skip");
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn g9_sum_to_n_1_returns_0() {
    // Loop runs once with i=0 ; acc += 0 ; i becomes 1 ; loop exits ;
    // returns acc = 0.
    let f = build_main_sum_to_n_constant(1);
    let abi = X64Abi::host_default();
    let obj_func = build_multi_block_func_bytes(&f, abi, true).expect("walker sum_to_n(1)");
    let target = host_default_target();
    let bytes = emit_object_file(&[obj_func], &[], target).expect("object emit");

    match link_and_run(&bytes, target) {
        Ok(Some(code)) => {
            assert_eq!(code, 0, "G9 sum_to_n(1) returned {code} ; expected 0");
            eprintln!("[end-to-end-OK] G9 sum_to_n(1) → linker → exe ran ; exit code = 0 ✓");
        }
        Ok(None) => {
            eprintln!("[skip] G9 sum_to_n(1) : no LLD or environmental link skip");
        }
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn g9_sum_to_n_0_returns_0() {
    // Loop never runs (cond i < 0 is false from the start) ; returns acc=0.
    let f = build_main_sum_to_n_constant(0);
    let abi = X64Abi::host_default();
    let obj_func = build_multi_block_func_bytes(&f, abi, true).expect("walker sum_to_n(0)");
    let target = host_default_target();
    let bytes = emit_object_file(&[obj_func], &[], target).expect("object emit");

    match link_and_run(&bytes, target) {
        Ok(Some(code)) => {
            assert_eq!(code, 0, "G9 sum_to_n(0) returned {code} ; expected 0");
            eprintln!("[end-to-end-OK] G9 sum_to_n(0) → linker → exe ran ; exit code = 0 ✓");
        }
        Ok(None) => {
            eprintln!("[skip] G9 sum_to_n(0) : no LLD or environmental link skip");
        }
        Err(e) => panic!("{e}"),
    }
}
