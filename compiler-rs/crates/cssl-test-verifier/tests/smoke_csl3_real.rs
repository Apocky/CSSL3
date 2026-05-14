// § tests/smoke_csl3_real.rs : end-to-end smoke against real CSL3 manifest
// ══════════════════════════════════════════════════════════════════
// covers :
//   ✓ parse Infinity-Engine examples/cssl_game.events.csl (114 lines)
//   ✓ assert sane number of expectations (≈40-50)
//   ✓ assert key headers parsed (timeout, binary, platform)
//   ✓ assert §loop block multiplied counts (~600±10%)
//   ✓ assert require-order + forbid-between captured
// ══════════════════════════════════════════════════════════════════

use cssl_test_verifier::manifest::CountSpec;
use cssl_test_verifier::manifest_csl;

// Try a few candidate paths so the test works whether run from the crate dir
// or a different cwd. Worktree paths can shift across sessions.
const CANDIDATE_PATHS: &[&str] = &[
    "../../../../Infinity Engine/.claude/worktrees/romantic-golick-808345/examples/cssl_game.events.csl",
    "../../../Infinity Engine/.claude/worktrees/romantic-golick-808345/examples/cssl_game.events.csl",
    "C:/Users/Apocky/source/repos/Infinity Engine/.claude/worktrees/romantic-golick-808345/examples/cssl_game.events.csl",
];

fn read_real() -> Option<String> {
    for p in CANDIDATE_PATHS {
        if let Ok(s) = std::fs::read_to_string(p) {
            return Some(s);
        }
    }
    None
}

#[test]
fn parse_real_cssl_game_events_csl() {
    let raw = match read_real() {
        Some(s) => s,
        None => {
            eprintln!(
                "skipping : real cssl_game.events.csl not found at any of {:?} (cwd={:?})",
                CANDIDATE_PATHS,
                std::env::current_dir()
            );
            return;
        }
    };
    let m = manifest_csl::parse(&raw).expect("real manifest parses");
    let total_expects = m.manifest.required.len()
        + m.manifest.forbidden.len()
        + m.manifest.orderings.len()
        + m.manifest.latency_bounds.len()
        + m.manifest.result_predicates.len()
        + m.extras.forbid_between.len();

    eprintln!(
        "parsed cssl_game.events.csl : required={} forbidden={} orderings={} latency_bounds={} result_preds={} forbid_between={} paired_with={} warnings={} total≈{}",
        m.manifest.required.len(),
        m.manifest.forbidden.len(),
        m.manifest.orderings.len(),
        m.manifest.latency_bounds.len(),
        m.manifest.result_predicates.len(),
        m.extras.forbid_between.len(),
        m.extras.paired_with.len(),
        m.extras.warnings.len(),
        total_expects,
    );
    for w in &m.extras.warnings {
        eprintln!("  {}", w);
    }

    // Sanity bounds : the file declares ~46 expectations across all classes.
    assert!(
        total_expects >= 30 && total_expects <= 100,
        "expected ~40-50 total expectations, got {}",
        total_expects
    );

    // Headers sanity
    assert_eq!(m.extras.binary.as_deref(), Some("dist/cssl_game.exe"));
    assert_eq!(m.extras.platform.as_deref(), Some("windows-msvc"));
    assert_eq!(m.extras.timeout_ns, Some(30_000_000_000));

    // Loop-block multiplied counts : at least one Range expectation around 600
    let has_loop_range = m.manifest.required.iter().any(|r| match &r.count {
        CountSpec::Range(lo, hi) => *lo <= 540 && *hi >= 660,
        _ => false,
    });
    assert!(
        has_loop_range,
        "expected at least one §loop-derived count Range covering ~540..660, got requireds={:#?}",
        m.manifest
            .required
            .iter()
            .map(|r| (&r.op, &r.count))
            .collect::<Vec<_>>()
    );

    // require-order captures : at least 9 in §global block
    assert!(
        m.manifest.orderings.len() >= 9,
        "expected ≥9 orderings (require-order + after= sythesized) got {}",
        m.manifest.orderings.len()
    );

    // forbid-between captures : 2 in §global block
    assert!(
        m.extras.forbid_between.len() >= 2,
        "expected ≥2 forbid-between, got {}",
        m.extras.forbid_between.len()
    );

    // expect-not captures : multiple in silent-fallback alarm + setup section
    assert!(
        m.manifest.forbidden.len() >= 5,
        "expected ≥5 forbidden (expect-not), got {}",
        m.manifest.forbidden.len()
    );
}
