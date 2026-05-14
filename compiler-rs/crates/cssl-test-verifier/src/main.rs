// § main.rs : cssl-test-verify CLI entry-point
// ══════════════════════════════════════════════════════════════════
// usage : cssl-test-verify --manifest <path.csl> --events <path.jsonl>
//                          [--profile <name>] [--json] [--strict]
//
// exit-codes :
//   0 = pass
//   1 = fail (any failures or unallowed silent-fallbacks)
//   2 = invocation error (missing args / bad files)
//   3 = strict-mode unexpected-events trip
// ══════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use cssl_test_verifier::events::load_jsonl;
use cssl_test_verifier::manifest::{load_manifest, Manifest};
use cssl_test_verifier::manifest_csl;
use cssl_test_verifier::output::{render_human, render_json};
use cssl_test_verifier::verify::verify;

#[derive(Debug, Default)]
struct Args {
    manifest: Option<PathBuf>,
    events: Option<PathBuf>,
    profile: Option<String>,
    json: bool,
    strict: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--manifest" => args.manifest = Some(it.next().ok_or("missing --manifest value")?.into()),
            "--events" => args.events = Some(it.next().ok_or("missing --events value")?.into()),
            "--profile" => args.profile = Some(it.next().ok_or("missing --profile value")?),
            "--json" => args.json = true,
            "--strict" => args.strict = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {}", other)),
        }
    }
    if args.manifest.is_none() {
        return Err("--manifest <path> required".into());
    }
    if args.events.is_none() {
        return Err("--events <path> required".into());
    }
    Ok(args)
}

fn print_help() {
    eprintln!(
        "cssl-test-verify · CSSLv3 stage0 event-trace verifier
USAGE :
  cssl-test-verify --manifest <path.csl> --events <path.jsonl>
                   [--profile <name>] [--json] [--strict]

FLAGS :
  --manifest PATH   Manifest file (line-oriented stage0 format).
  --events   PATH   JSONL trace from cssl-rt (canonical schema).
  --profile  NAME   Activate one profile section ; ignore others.
  --json            Emit machine-readable JSON report on stdout.
  --strict          Treat unexpected-events as fail (exit 3).

EXIT :
  0 pass · 1 fail · 2 invocation-error · 3 strict-unexpected
"
    );
}

// § auto-detect manifest format ←
//   .csl / .events.csl → CSL3-glyph-dense parser (manifest_csl)
//   .manifest          → line-oriented stage0 (manifest)
//   else               → try CSL3 first ; on missing-header error fall-back to line-oriented
fn load_manifest_auto(
    path: &PathBuf,
    profile: Option<&str>,
) -> Result<Manifest, String> {
    let pstr = path.display().to_string();
    let lower = pstr.to_lowercase();
    let is_csl = lower.ends_with(".csl") || lower.ends_with(".events.csl");
    let is_manifest_ext = lower.ends_with(".manifest");

    if is_manifest_ext {
        return load_manifest(path, profile).map_err(|e| e.to_string());
    }
    if is_csl {
        let csl = manifest_csl::load(path).map_err(|e| e.to_string())?;
        for w in &csl.extras.warnings {
            eprintln!("{}", w);
        }
        return Ok(csl.manifest);
    }
    // Unknown extension : try CSL3 first
    match manifest_csl::load(path) {
        Ok(csl) => {
            for w in &csl.extras.warnings {
                eprintln!("{}", w);
            }
            Ok(csl.manifest)
        }
        Err(_) => load_manifest(path, profile).map_err(|e| e.to_string()),
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("error: {}", msg);
            print_help();
            return ExitCode::from(2);
        }
    };

    let manifest_path = args.manifest.as_ref().unwrap();
    let manifest: Manifest = match load_manifest_auto(manifest_path, args.profile.as_deref()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: manifest load: {}", e);
            return ExitCode::from(2);
        }
    };

    let events = match load_jsonl(args.events.as_ref().unwrap()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: events load: {}", e);
            return ExitCode::from(2);
        }
    };

    let report = verify(&manifest, &events);

    if args.json {
        println!("{}", render_json(&report));
    } else {
        print!("{}", render_human(&report));
    }

    if !report.passed {
        return ExitCode::from(1);
    }
    if args.strict && !report.unexpected_events.is_empty() {
        return ExitCode::from(3);
    }
    ExitCode::from(0)
}
