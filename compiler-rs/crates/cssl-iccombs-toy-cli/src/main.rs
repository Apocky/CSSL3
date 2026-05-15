#![forbid(unsafe_code)]
//! `cssl-iccombs-toy` — TOY interaction-combinator demo binary.
//!
//! ⚠ TOY per `specs/Upgrade/impl/IMPL_06_CORRIGENDUM.csl`. NOT a PD-binding driver.
//! Real driver = `csslc`. Real PD = composition of cssl-caps + cssl-effects + cssl-ifc.
//!
//! Usage :
//!   cssl-iccombs-toy [--lower-iccombs] [--reduce <N>] -- <S-exp>
//!   cssl-iccombs-toy --stdin                          (read source from stdin)

use cssl_iccombs_toy_cli::parse;
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
    let mut source: Option<String> = None;
    let mut from_stdin = false;
    let mut lower_ic = false;
    let mut reduce_steps: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => { print_help(); return Ok(()); }
            "--stdin" => { from_stdin = true; i += 1; }
            "--lower-iccombs" => { lower_ic = true; i += 1; }
            "--reduce" => {
                i += 1;
                let n = args.get(i).ok_or("`--reduce` requires a step-count")?;
                reduce_steps = Some(n.parse::<usize>().map_err(|e| format!("`--reduce` : {e}"))?);
                lower_ic = true;
                i += 1;
            }
            "--" => { i += 1; source = Some(args[i..].join(" ")); break; }
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
    let elab = cssl_iccombs_toy_elab::elaborate(&term).map_err(|e| format!("elab : {e}"))?;

    println!("=== cssl-iccombs-toy report ===");
    println!("hgraph nodes : {}", elab.hgraph.node_count());
    println!("hgraph edges : {}", elab.hgraph.edge_count());
    println!("hgraph cid   : {}", cssl_cas::cid_hex(&elab.cid()));
    println!("effects pure : {}", elab.effects.is_pure());
    println!("effects      : [{}]",
        elab.effects.labels.iter().cloned().collect::<Vec<_>>().join(", "));

    if lower_ic {
        match cssl_lower_iccombs::lower(&term) {
            Ok(mut lowered) => {
                println!("iccombs net  : {} agent(s) initial", lowered.net.agent_count());
                if let Some(max) = reduce_steps {
                    let r = lowered.net.reduce_to_normal_form(max);
                    println!("iccombs reduce: {r:?} ; {} agent(s) post-reduce", lowered.net.agent_count());
                }
            }
            Err(e) => println!("iccombs lower: SKIPPED : {e}"),
        }
    }

    Ok(())
}

fn print_help() {
    println!("cssl-iccombs-toy : CSSLv3 interaction-combinator demo (TOY ; not PD-binding)");
    println!();
    println!("USAGE :");
    println!("  cssl-iccombs-toy [OPTIONS] -- <S-EXP>");
    println!("  cssl-iccombs-toy [OPTIONS] --stdin");
    println!();
    println!("OPTIONS :");
    println!("  --stdin               read source from stdin instead of args");
    println!("  --lower-iccombs       additionally lower term to a cssl-iccombs interaction net (linear fragment only)");
    println!("  --reduce <N>          implies --lower-iccombs ; reduce up to <N> active-pair steps");
    println!("  -h, --help            print this help");
    println!();
    println!("EXAMPLES :");
    println!("  cssl-iccombs-toy -- '(lam x L x)'");
    println!("  cssl-iccombs-toy --lower-iccombs -- '(lam x L x)'");
    println!("  cssl-iccombs-toy --reduce 64 -- '(app (lam x L x) ())'");
}
