// § build.rs — substrate-knowledge corpus embedding.
//
// Scrapes (alphabetically, deterministically) :
//   * `<repo>/specs/grand-vision/*.csl`
//   * `<repo>/specs/30_*.csl`, `31_*.csl`, `32_*.csl`, `33_*.csl`, `41_*.csl`
//   * `<repo>/CLAUDE.md` (skip if absent)
//   * `<repo>/PRIME_DIRECTIVE.md` (skip if absent)
//   * `<repo>/README.md` (skip if absent)
//   * `${HOME-or-USERPROFILE}/.claude/projects/C--Users-Apocky-source-repos-CSSLv3/memory/*.md` (skip if absent)
//
// Emits to OUT_DIR :
//   * `static_substrate_data.rs` — `SUBSTRATE_DOCS: &[(&str,&str)]` + `ALWAYS_LOADED: &[&str]`
//   * `static_hdc_index.rs`     — `DOC_FINGERPRINTS: &[(&str,[u8;32])]` + `DOC_TOKEN_HASHES: &[(&str,&[u32])]`
//
// All slices are name-sorted (binary-search-ready). No spec found ⇒ empty arrays
// emitted ; the crate must still compile + tests still pass.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo"));
    // crate_dir = .../compiler-rs/crates/cssl-host-substrate-knowledge
    // workspace root = ../../.. (compiler-rs is a child of repo-root) — adjust:
    //   .../compiler-rs/crates/cssl-host-substrate-knowledge
    //     -> parent = .../compiler-rs/crates
    //     -> parent = .../compiler-rs
    //     -> parent = repo-root
    let repo_root = crate_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map_or_else(|| crate_dir.clone(), Path::to_path_buf);

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));

    // Deterministic name → contents map.
    let mut docs: BTreeMap<String, String> = BTreeMap::new();

    // ── workspace-root canon docs ─────────────────────────────────────
    for fname in ["CLAUDE.md", "PRIME_DIRECTIVE.md", "README.md"] {
        let p = repo_root.join(fname);
        if let Ok(s) = fs::read_to_string(&p) {
            docs.insert(fname.to_string(), s);
            rerun_if_changed(&p);
        }
    }

    // ── specs/grand-vision/*.csl ──────────────────────────────────────
    let gv_dir = repo_root.join("specs").join("grand-vision");
    collect_dir_alphabetical(&gv_dir, "csl", &mut docs, "specs/grand-vision/");

    // ── specs/{30,31,32,33,41}_*.csl ─────────────────────────────────
    let specs_dir = repo_root.join("specs");
    if let Ok(rd) = fs::read_dir(&specs_dir) {
        let mut hits: Vec<PathBuf> = rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .filter(|p| p.extension() == Some(OsStr::new("csl")))
            .filter(|p| {
                p.file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|n| {
                        n.starts_with("30_")
                            || n.starts_with("31_")
                            || n.starts_with("32_")
                            || n.starts_with("33_")
                            || n.starts_with("41_")
                    })
            })
            .collect();
        hits.sort();
        for p in hits {
            if let Ok(s) = fs::read_to_string(&p) {
                if let Some(n) = p.file_name().and_then(OsStr::to_str) {
                    docs.insert(format!("specs/{n}"), s);
                    rerun_if_changed(&p);
                }
            }
        }
    }

    // ── ${HOME or USERPROFILE}/.claude/projects/<proj>/memory/*.md ────
    let claude_root = env::var("HOME")
        .ok()
        .or_else(|| env::var("USERPROFILE").ok())
        .map(PathBuf::from);
    if let Some(home) = claude_root {
        let mem_dir = home
            .join(".claude")
            .join("projects")
            .join("C--Users-Apocky-source-repos-CSSLv3")
            .join("memory");
        collect_dir_alphabetical(&mem_dir, "md", &mut docs, "memory/");
    }

    // ── emit static_substrate_data.rs ─────────────────────────────────
    let mut data = String::new();
    data.push_str("pub static SUBSTRATE_DOCS: &[(&str, &str)] = &[\n");
    for (name, body) in &docs {
        let _ = writeln!(data, "    ({name:?}, {body:?}),");
    }
    data.push_str("];\n\n");

    data.push_str("pub static ALWAYS_LOADED: &[&str] = &[\n");
    // Only list canon names that were actually embedded (deterministic, sorted).
    let mut canon: Vec<&str> = ["CLAUDE.md", "MEMORY.md", "PRIME_DIRECTIVE.md"]
        .into_iter()
        .filter(|n| docs.contains_key(*n))
        .collect();
    canon.sort_unstable();
    for n in canon {
        let _ = writeln!(data, "    {n:?},");
    }
    data.push_str("];\n");

    let target_data = out_dir.join("static_substrate_data.rs");
    fs::write(&target_data, &data).expect("write static_substrate_data.rs");

    // ── emit static_hdc_index.rs ──────────────────────────────────────
    let mut idx = String::new();
    idx.push_str("pub static DOC_FINGERPRINTS: &[(&str, [u8; 32])] = &[\n");
    for (name, body) in &docs {
        let fp = shingle_fingerprint(body);
        let _ = write!(idx, "    ({name:?}, [");
        for (i, b) in fp.iter().enumerate() {
            if i > 0 {
                idx.push_str(", ");
            }
            let _ = write!(idx, "0x{b:02x}");
        }
        idx.push_str("]),\n");
    }
    idx.push_str("];\n\n");

    idx.push_str("pub static DOC_TOKEN_HASHES: &[(&str, &[u32])] = &[\n");
    for (name, body) in &docs {
        let bag = token_hash_bag(body);
        let _ = write!(idx, "    ({name:?}, &[");
        for (i, h) in bag.iter().enumerate() {
            if i > 0 {
                idx.push_str(", ");
            }
            let _ = write!(idx, "0x{h:08x}");
        }
        idx.push_str("]),\n");
    }
    idx.push_str("];\n");

    let target_idx = out_dir.join("static_hdc_index.rs");
    fs::write(&target_idx, &idx).expect("write static_hdc_index.rs");

    // Re-run if build.rs itself changes (cargo handles by default but explicit ¬ harmful).
    println!("cargo:rerun-if-changed=build.rs");
}

fn rerun_if_changed(p: &Path) {
    if let Some(s) = p.to_str() {
        println!("cargo:rerun-if-changed={s}");
    }
}

fn collect_dir_alphabetical(
    dir: &Path,
    ext: &str,
    docs: &mut BTreeMap<String, String>,
    name_prefix: &str,
) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut hits: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| p.extension() == Some(OsStr::new(ext)))
        .collect();
    hits.sort();
    for p in hits {
        if let Ok(s) = fs::read_to_string(&p) {
            if let Some(n) = p.file_name().and_then(OsStr::to_str) {
                docs.insert(format!("{name_prefix}{n}"), s);
                rerun_if_changed(&p);
            }
        }
    }
}

// ── tokenizer (mirror of src/tokenize.rs) ────────────────────────────
// Lower-case ; strip [],{}()<>"'` ; split-on-whitespace ; len ≥ 3 ; dedupe ;
// take first 256.
fn tokenize_for_build(s: &str) -> Vec<String> {
    const STRIPS: &[char] = &['[', ']', '{', '}', '(', ')', '<', '>', '"', '\'', '`', ',', '.', ';', ':', '!', '?'];
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for raw in s.split_whitespace() {
        let cleaned: String = raw
            .chars()
            .filter(|c| !STRIPS.contains(c))
            .collect::<String>()
            .to_lowercase();
        if cleaned.chars().count() < 3 {
            continue;
        }
        if seen.insert(cleaned.clone()) {
            out.push(cleaned);
            if out.len() >= 256 {
                break;
            }
        }
    }
    out
}

fn shingle_fingerprint(body: &str) -> [u8; 32] {
    let toks = tokenize_for_build(body);
    let joined = toks.join(" ");
    let h = blake3::hash(joined.as_bytes());
    *h.as_bytes()
}

fn token_hash_bag(body: &str) -> Vec<u32> {
    let toks = tokenize_for_build(body);
    let mut out = Vec::with_capacity(toks.len());
    for t in toks {
        let h = blake3::hash(t.as_bytes());
        let bytes = h.as_bytes();
        let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        out.push(v);
    }
    // Deterministic order, deduplicated.
    out.sort_unstable();
    out.dedup();
    out
}
