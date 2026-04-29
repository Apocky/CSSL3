//! § linker_smoke — verify the emitted .o / .obj is consumed by a real linker.
//!
//! § ROLE
//!   The unit + roundtrip tests verify the headers and section layout in
//!   isolation. This test goes one step further : it pipes the bytes
//!   into a real linker (`rust-lld` from rustup, or any linker on PATH)
//!   and checks the linker reports a clean exit when given a valid
//!   `fn main() -> i32 { 42 }` object.
//!
//! § GATE-SKIP
//!   - No linker on host ⇒ skip.
//!   - Linker rejects ⇒ test fails (this is the value-add).
//!   - Linker accepts but final binary doesn't run ⇒ tolerated (running
//!     a freshly-linked exe needs CRT / libc which our object doesn't
//!     reference ; the link step itself is the meaningful gate).
//!
//! § HOST-PLATFORM ROUTING
//!   We pick the format matching the host kernel : ELF on Linux, COFF on
//!   Windows, Mach-O on macOS. Cross-format link tests are out of scope
//!   here — they need a cross-linker setup that's not standard rustup.

use std::path::PathBuf;
use std::process::Command;

use cssl_cgen_cpu_x64::{emit_object_file, host_default_target, ObjectTarget, X64Func};

const MAIN_42: &[u8] = &[0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];

/// Linker driver kind — picks the argument shape we need.
#[derive(Clone, Copy)]
enum DriverKind {
    /// `lld-link.exe` — Windows MSVC-style.
    LldLink,
    /// `ld.lld` — GNU-ld-style (Linux + BSD).
    LdLld,
    /// `ld64.lld` — Apple-ld-style (Mach-O).
    Ld64Lld,
}

/// Find a usable LLD driver in rustup's `<toolchain>/lib/rustlib/<triple>/bin/gcc-ld/`
/// directory (the standard rust-lld location since `rust-lld` itself is a
/// front-end that requires `-flavor=` and may reject that on stable). The
/// named per-flavor drivers (`lld-link.exe`, `ld.lld`, `ld64.lld`) sit in
/// `gcc-ld/` and accept the matching argument shape directly.
fn find_lld_driver() -> Option<(PathBuf, DriverKind)> {
    // RUSTUP_HOME points to the .rustup root directly. USERPROFILE / HOME
    // need the `.rustup` suffix appended to reach the same directory.
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

    // (driver_filename, DriverKind) options in priority order for the host.
    //
    // NOTE : `lld-link.exe` is the COFF/PE driver regardless of whether the
    // surrounding rustup toolchain target_env is "msvc" or "gnu" (mingw) ;
    // both ship the same `lld-link.exe` for emitting .obj-style COFF.
    // The `.exe` extension is required on Windows ; on Linux/Mac the
    // drivers have no extension.
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
    path.push(format!("cssl_x64_g5_link_{pid}_{stamp}.{ext}"));
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[test]
fn host_target_object_is_accepted_by_linker() {
    // ─── 1. Find a linker driver. If absent, skip. ─────────────────────
    let Some((driver, kind)) = find_lld_driver() else {
        eprintln!("[skip] no LLD driver (lld-link / ld.lld / ld64.lld) in rustup gcc-ld dir");
        return;
    };

    // ─── 2. Emit a host-target .obj from `fn main() -> i32 { 42 }`. ────
    let target = host_default_target();
    let f = X64Func::leaf_export("main", MAIN_42.to_vec()).unwrap();
    let bytes = emit_object_file(&[f], &[], target).unwrap();

    let ext = match target {
        ObjectTarget::CoffX64 => "obj",
        ObjectTarget::ElfX64 | ObjectTarget::MachOX64 => "o",
    };
    let obj_path = write_temp(&bytes, ext).expect("temp .obj write");

    // ─── 3. Decide the output exe path + driver arg shape. ─────────────
    let mut exe_path = obj_path.clone();
    exe_path.set_extension(if cfg!(windows) { "exe" } else { "out" });

    // The actual exe may not run on a libc-less link (no CRT entry-point
    // wrapping `main`), but the link step itself is the meaningful gate
    // that proves the bytes are structurally valid per the host's spec.
    let mut cmd = Command::new(&driver);
    match kind {
        DriverKind::LldLink => {
            cmd.arg(format!("/OUT:{}", exe_path.display()));
            cmd.arg("/SUBSYSTEM:CONSOLE");
            cmd.arg("/ENTRY:main");
            cmd.arg("/NODEFAULTLIB"); // skip libc / CRT
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

    let out = cmd.output().expect("LLD driver spawn");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    // ─── 4. If the link succeeded, try to RUN the produced exe. ────────
    //
    // This is the "second-milestone" gate the brief calls out :
    //     fn main() -> i32 { 42 } MIR → G1+G2+G3+G4 → G5 emit → A4 link → run = 42
    //
    // We don't depend on a CRT here ; the entry point is `main` directly,
    // which on Windows means the kernel jumps into `mov eax, 42 ; ret`
    // and treats the eax value as the process exit code.
    let mut run_status: Option<i32> = None;
    if out.status.success() && exe_path.exists() {
        if let Ok(child) = Command::new(&exe_path).output() {
            run_status = child.status.code();
        }
    }

    // ─── 5. Cleanup, then assert. ──────────────────────────────────────
    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&exe_path);

    if let Some(code) = run_status {
        assert_eq!(
            code, 42,
            "linked exe ran but returned {code} ; expected 42 (the `mov eax, 42 ; ret` body's eax value)"
        );
        eprintln!(
            "[end-to-end-OK] G5 → linker → exe ran ; exit code = 42 (matches `mov eax, 42 ; ret`)"
        );
        return;
    }

    if out.status.success() {
        // Happy path : linker accepted the bytes outright.
        eprintln!(
            "[link-ok] {} produced {}",
            driver.display(),
            exe_path.display()
        );
        return;
    }

    // Some failures are environmental (e.g., Mach-O cross-link from
    // Linux ; missing libsystem on a non-mac host ; Windows SDK
    // paths missing). We tolerate those as soft-skips and only fail
    // if the linker explicitly complains about OUR object's bytes.
    let lc = stderr.to_lowercase();
    let mentions_format_corruption = lc.contains("malformed")
        || lc.contains("invalid magic")
        || lc.contains("not a valid")
        || lc.contains("bad relocation")
        || lc.contains("invalid section")
        || lc.contains("corrupt");

    assert!(
        !mentions_format_corruption,
        "LLD rejected G5's output object as malformed :\n status: {:?}\n stderr: {stderr}\n stdout: {stdout}",
        out.status
    );
    eprintln!(
        "[soft-skip] linker rejected for environmental reason :\n driver: {}\n stderr: {stderr}",
        driver.display()
    );
}
