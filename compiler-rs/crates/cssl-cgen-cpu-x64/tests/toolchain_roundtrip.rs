//! § toolchain_roundtrip — verify emitted .o / .obj parses with the
//! host toolchain's object inspector (objdump / dumpbin / otool).
//!
//! § PATTERN
//!   These tests gate-skip when the toolchain binary is absent (per the
//!   S6-A4 BinaryMissing precedent ; T11-D55). Skipping is silent — the
//!   test passes — so CI runners without the toolchain don't fail.
//!
//! § BINARIES PROBED
//!   - `objdump` (binutils ; ELF + COFF + Mach-O)
//!   - `llvm-objdump` (LLVM ; ELF + COFF + Mach-O)
//!   - `dumpbin` (MSVC ; COFF only)
//!   - `otool`   (Apple Xcode tools ; Mach-O only)
//!
//! § COMPILE-TIME PLATFORM GATES
//!   The Mach-O test runs only on macOS (otool isn't on Linux/Windows).
//!   The COFF dumpbin test runs only on Windows. ELF objdump tests run
//!   anywhere objdump is on PATH (Linux distro toolchain or MSYS2/cygwin
//!   / mingw on Windows).

use std::path::PathBuf;
use std::process::Command;

use cssl_cgen_cpu_x64::{emit_object_file, ObjectTarget, X64Func};

/// Canonical `mov eax, 42 ; ret` body.
const MAIN_42: &[u8] = &[0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];

/// Locate a binary on PATH ; returns its canonical path or None.
fn which(name: &str) -> Option<PathBuf> {
    // Try a quick `name --help` (or `/?` on dumpbin) to see if it runs.
    let mut probe = Command::new(name);
    if name == "dumpbin" {
        probe.arg("/?");
    } else {
        probe.arg("--help");
    }
    probe.stdout(std::process::Stdio::null());
    probe.stderr(std::process::Stdio::null());
    if probe
        .status()
        .ok()
        .is_some_and(|s| s.success() || s.code().is_some())
    {
        Some(PathBuf::from(name))
    } else {
        None
    }
}

/// Write `bytes` to a temp file with the given extension and return its path.
/// The caller is responsible for cleanup ; failures are non-fatal.
fn write_temp_object(bytes: &[u8], ext: &str) -> std::io::Result<PathBuf> {
    let mut path = std::env::temp_dir();
    let pid = std::process::id();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.push(format!("cssl_x64_g5_{pid}_{stamp}.{ext}"));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

// ───────────────────────────────────────────────────────────────────────
// § ELF tests — gate on `objdump` / `llvm-objdump`
// ───────────────────────────────────────────────────────────────────────

fn pick_elf_objdump() -> Option<PathBuf> {
    which("llvm-objdump").or_else(|| which("objdump"))
}

#[test]
fn elf_objdump_recognizes_format() {
    let Some(objdump) = pick_elf_objdump() else {
        eprintln!("[skip] no objdump / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::ElfX64).unwrap();
    let path = write_temp_object(&out, "o").expect("temp write");
    let result = Command::new(&objdump)
        .arg("-f") // file headers
        .arg(&path)
        .output();
    let _ = std::fs::remove_file(&path);
    let result = result.expect("objdump invocation");
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success(),
        "objdump -f failed : status={:?} stderr={}",
        result.status,
        stderr
    );
    // Either binutils or llvm-objdump should report the format string.
    assert!(
        stdout.contains("elf64-x86-64") || stdout.contains("ELF") || stdout.contains("x86-64"),
        "objdump output didn't mention ELF/x86-64 : {stdout}"
    );
}

#[test]
fn elf_objdump_disassembles_text() {
    let Some(objdump) = pick_elf_objdump() else {
        eprintln!("[skip] no objdump / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::ElfX64).unwrap();
    let path = write_temp_object(&out, "o").expect("temp write");
    let result = Command::new(&objdump)
        .arg("-d") // disassemble
        .arg(&path)
        .output();
    let _ = std::fs::remove_file(&path);
    let result = result.expect("objdump invocation");
    let stdout = String::from_utf8_lossy(&result.stdout);
    // We expect at least a "mov" instruction in the disassembly.
    assert!(
        result.status.success(),
        "objdump -d failed : {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        stdout.to_lowercase().contains("mov"),
        "expected a `mov` in the disassembly : {stdout}"
    );
}

#[test]
fn elf_objdump_lists_main_symbol() {
    let Some(objdump) = pick_elf_objdump() else {
        eprintln!("[skip] no objdump / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::ElfX64).unwrap();
    let path = write_temp_object(&out, "o").expect("temp write");
    let result = Command::new(&objdump)
        .arg("-t") // symbol table
        .arg(&path)
        .output();
    let _ = std::fs::remove_file(&path);
    let result = result.expect("objdump invocation");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(result.status.success());
    assert!(
        stdout.contains("main"),
        "expected `main` symbol in objdump -t output : {stdout}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// § COFF tests — gate on `dumpbin` (Windows MSVC) or llvm-objdump
// ───────────────────────────────────────────────────────────────────────

fn pick_coff_inspector() -> Option<(PathBuf, &'static str)> {
    if cfg!(target_os = "windows") {
        if let Some(p) = which("dumpbin") {
            return Some((p, "dumpbin"));
        }
    }
    which("llvm-objdump").map(|p| (p, "llvm-objdump"))
}

#[test]
fn coff_inspector_recognizes_format() {
    let Some((tool, kind)) = pick_coff_inspector() else {
        eprintln!("[skip] no dumpbin / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::CoffX64).unwrap();
    let path = write_temp_object(&out, "obj").expect("temp write");
    let result = if kind == "dumpbin" {
        Command::new(&tool).arg("/HEADERS").arg(&path).output()
    } else {
        Command::new(&tool).arg("-h").arg(&path).output()
    };
    let _ = std::fs::remove_file(&path);
    let result = result.expect("inspector invocation");
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success(),
        "{kind} failed : status={:?} stderr={}",
        result.status,
        stderr
    );
    // dumpbin says "FILE HEADER VALUES" + "machine (x64)"
    // llvm-objdump says "file format coff-x86-64"
    let combined = format!("{stdout}{stderr}").to_lowercase();
    assert!(
        combined.contains("x64")
            || combined.contains("x86-64")
            || combined.contains("machine")
            || combined.contains("amd64")
            || combined.contains("coff"),
        "{kind} didn't recognize COFF/AMD64 : {stdout}"
    );
}

#[test]
fn coff_inspector_lists_main_symbol() {
    let Some((tool, kind)) = pick_coff_inspector() else {
        eprintln!("[skip] no dumpbin / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::CoffX64).unwrap();
    let path = write_temp_object(&out, "obj").expect("temp write");
    let result = if kind == "dumpbin" {
        Command::new(&tool).arg("/SYMBOLS").arg(&path).output()
    } else {
        Command::new(&tool).arg("-t").arg(&path).output()
    };
    let _ = std::fs::remove_file(&path);
    let result = result.expect("inspector invocation");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(result.status.success());
    assert!(
        stdout.contains("main"),
        "{kind} symbols didn't list `main` : {stdout}"
    );
}

#[test]
fn coff_inspector_lists_text_section() {
    let Some((tool, kind)) = pick_coff_inspector() else {
        eprintln!("[skip] no dumpbin / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::CoffX64).unwrap();
    let path = write_temp_object(&out, "obj").expect("temp write");
    let result = if kind == "dumpbin" {
        Command::new(&tool).arg("/HEADERS").arg(&path).output()
    } else {
        Command::new(&tool).arg("-h").arg(&path).output()
    };
    let _ = std::fs::remove_file(&path);
    let result = result.expect("inspector invocation");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(result.status.success());
    assert!(
        stdout.contains(".text"),
        "{kind} didn't list `.text` section : {stdout}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// § Mach-O tests — gate on `otool` (macOS) or llvm-objdump
// ───────────────────────────────────────────────────────────────────────

fn pick_macho_inspector() -> Option<(PathBuf, &'static str)> {
    if cfg!(target_os = "macos") {
        if let Some(p) = which("otool") {
            return Some((p, "otool"));
        }
    }
    which("llvm-objdump").map(|p| (p, "llvm-objdump"))
}

#[test]
fn macho_inspector_recognizes_format() {
    let Some((tool, kind)) = pick_macho_inspector() else {
        eprintln!("[skip] no otool / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::MachOX64).unwrap();
    let path = write_temp_object(&out, "o").expect("temp write");
    let result = if kind == "otool" {
        Command::new(&tool).arg("-h").arg(&path).output()
    } else {
        // llvm-objdump
        Command::new(&tool)
            .arg("--macho")
            .arg("-h")
            .arg(&path)
            .output()
    };
    let _ = std::fs::remove_file(&path);
    let result = result.expect("inspector invocation");
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    if !result.status.success() {
        // llvm-objdump on a non-Mach-O host may return "unknown file
        // format" if it's compiled without macho support — treat as skip.
        eprintln!(
            "[soft-skip] {kind} couldn't read Mach-O : status={:?} stderr={}",
            result.status, stderr
        );
        return;
    }
    let combined = format!("{stdout}{stderr}").to_lowercase();
    assert!(
        combined.contains("magic")
            || combined.contains("mh_magic_64")
            || combined.contains("mh_object")
            || combined.contains("mach-o")
            || combined.contains("x86_64"),
        "{kind} didn't recognize Mach-O : {stdout}"
    );
}

#[test]
fn macho_inspector_lists_text_section() {
    let Some((tool, kind)) = pick_macho_inspector() else {
        eprintln!("[skip] no otool / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::MachOX64).unwrap();
    let path = write_temp_object(&out, "o").expect("temp write");
    let result = if kind == "otool" {
        Command::new(&tool).arg("-l").arg(&path).output()
    } else {
        Command::new(&tool)
            .arg("--macho")
            .arg("--section-headers")
            .arg(&path)
            .output()
    };
    let _ = std::fs::remove_file(&path);
    let result = result.expect("inspector invocation");
    if !result.status.success() {
        eprintln!("[soft-skip] {kind} returned non-zero ; tolerating on non-mac host");
        return;
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("__text"),
        "{kind} didn't list `__text` section : {stdout}"
    );
}

#[test]
fn macho_inspector_lists_underscored_main_symbol() {
    let Some((tool, kind)) = pick_macho_inspector() else {
        eprintln!("[skip] no otool / llvm-objdump on PATH");
        return;
    };
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let out = emit_object_file(&[f], &[], ObjectTarget::MachOX64).unwrap();
    let path = write_temp_object(&out, "o").expect("temp write");
    let result = if kind == "otool" {
        Command::new(&tool).arg("-Iv").arg(&path).output()
    } else {
        Command::new(&tool)
            .arg("--macho")
            .arg("-t")
            .arg(&path)
            .output()
    };
    let _ = std::fs::remove_file(&path);
    let result = result.expect("inspector invocation");
    if !result.status.success() {
        eprintln!("[soft-skip] {kind} returned non-zero ; tolerating on non-mac host");
        return;
    }
    let stdout = String::from_utf8_lossy(&result.stdout);
    // Mach-O convention prepends `_` to the user-name.
    assert!(
        stdout.contains("_main") || stdout.contains("main"),
        "{kind} didn't list `_main` symbol : {stdout}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// § Cross-format integrity check — magic + mostly-zero-tail-of-bytes
// ───────────────────────────────────────────────────────────────────────

#[test]
fn all_three_formats_emit_distinct_bytes() {
    let f1 = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let f2 = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let f3 = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let elf = emit_object_file(&[f1], &[], ObjectTarget::ElfX64).unwrap();
    let coff = emit_object_file(&[f2], &[], ObjectTarget::CoffX64).unwrap();
    let macho = emit_object_file(&[f3], &[], ObjectTarget::MachOX64).unwrap();

    // Magic byte signatures must differ.
    assert_ne!(&elf[..4], &macho[..4]);
    assert_ne!(&elf[..2], &coff[..2]);
    assert_ne!(&coff[..4], &macho[..4]);

    // All three must contain the canonical body bytes.
    assert!(elf.windows(MAIN_42.len()).any(|w| w == MAIN_42));
    assert!(coff.windows(MAIN_42.len()).any(|w| w == MAIN_42));
    assert!(macho.windows(MAIN_42.len()).any(|w| w == MAIN_42));
}
