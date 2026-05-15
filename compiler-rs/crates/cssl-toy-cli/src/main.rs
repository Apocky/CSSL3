#![forbid(unsafe_code)]
//! `cssl-toy` — end-to-end demo binary.
//!
//! Usage :
//!   cssl-toy [--grant <label>:<consent>]... [--consent <level>] -- <S-exp>
//!   cssl-toy --stdin                                          (read source from stdin)
//!
//! Examples :
//!   cssl-toy -- "(lam x L x)"
//!   cssl-toy --grant io:Implicit --consent Explicit -- "(op io)"

use cssl_consent::Consent;
use cssl_ocap::{CapType, Grantor};
use cssl_pd_check::{check, Grant, PolicySet};
use cssl_toy_cli::parse;
use std::io::Read;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let mut grants: Vec<(String, Consent)> = Vec::new();
    let mut consent = Consent::Explicit;
    let mut source: Option<String> = None;
    let mut from_stdin = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "--stdin" => { from_stdin = true; i += 1; }
            "--consent" => {
                i += 1;
                let lvl = args.get(i).ok_or("`--consent` requires a value")?;
                consent = parse_consent(lvl)?;
                i += 1;
            }
            "--grant" => {
                i += 1;
                let g = args.get(i).ok_or("`--grant` requires `<label>:<consent>`")?;
                let (label, lvl) = g.split_once(':').ok_or_else(|| {
                    format!("`--grant` value `{g}` must be `<label>:<consent>`")
                })?;
                grants.push((label.into(), parse_consent(lvl)?));
                i += 1;
            }
            "--" => {
                i += 1;
                source = Some(args[i..].join(" "));
                break;
            }
            other => return Err(format!("unknown arg `{other}` (try --help)")),
        }
    }

    let src = if from_stdin {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
        s
    } else {
        source.ok_or("no source : pass `-- <S-exp>` or `--stdin`")?
    };

    let term = parse(&src).map_err(|e| e.to_string())?;
    let elab = cssl_elab::elaborate(&term).map_err(|e| format!("elab : {e}"))?;

    println!("=== cssl-toy report ===");
    println!("hgraph nodes : {}", elab.hgraph.node_count());
    println!("hgraph edges : {}", elab.hgraph.edge_count());
    println!("hgraph cid   : {}", cssl_cas::cid_hex(&elab.cid()));
    println!("effects pure : {}", elab.effects.is_pure());
    println!("effects      : [{}]",
        elab.effects.labels.iter().cloned().collect::<Vec<_>>().join(", "));

    // Build a policy from --grant flags.
    let mut key = [0u8; 32];
    key[0] = 1; key[1] = 2; key[2] = 3;
    let grantor = Grantor::new(key);
    let mut policy = PolicySet::new(grantor.clone(), consent);
    let mut rng = rand::thread_rng();
    for (label, lvl) in grants {
        let token = grantor.mint(CapType(cssl_cas::cid_of_bytes(label.as_bytes())), &mut rng);
        policy.grant(label, Grant { token, required_consent: lvl });
    }

    match check(&elab, &policy) {
        Ok(report) => {
            println!("pd-check     : OK ({} effect(s) discharged)", report.effects_checked);
            Ok(())
        }
        Err(v) => {
            println!("pd-check     : VIOLATION : {v}");
            Err(format!("PD-binding violation : {v}"))
        }
    }
}

fn parse_consent(s: &str) -> Result<Consent, String> {
    match s {
        "Denied"   => Ok(Consent::Denied),
        "Implicit" => Ok(Consent::Implicit),
        "Explicit" => Ok(Consent::Explicit),
        "Revoked"  => Ok(Consent::Revoked),
        other      => Err(format!("unknown consent level `{other}` (use Denied|Implicit|Explicit|Revoked)")),
    }
}

fn print_help() {
    println!("cssl-toy : CSSLv3 foundation-crate end-to-end demo");
    println!();
    println!("USAGE :");
    println!("  cssl-toy [OPTIONS] -- <S-EXP>");
    println!("  cssl-toy [OPTIONS] --stdin");
    println!();
    println!("OPTIONS :");
    println!("  --consent <LEVEL>     held consent : Denied | Implicit | Explicit | Revoked  [default: Explicit]");
    println!("  --grant <L>:<LEVEL>   grant a capability for effect <L> requiring consent <LEVEL>  (repeatable)");
    println!("  --stdin               read source from stdin instead of args");
    println!("  -h, --help            print this help");
    println!();
    println!("EXAMPLES :");
    println!("  cssl-toy -- '(lam x L x)'");
    println!("  cssl-toy --grant io:Implicit -- '(op io)'");
    println!("  cssl-toy --grant io:Explicit --consent Implicit -- '(op io)'   # fails (insufficient consent)");
}
