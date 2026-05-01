//! § cssl-host-substrate-knowledge — build-time-embedded substrate corpus.
//!
//! § purpose
//!   Mycelium-Desktop (spec/grand-vision/23) bakes ALL specs/grand-vision/*.csl
//!   + select root specs (`30..33` + `41`) + workspace canon docs (CLAUDE.md /
//!   PRIME_DIRECTIVE.md / README.md) + per-project memory/*.md *into* the
//!   binary at build-time, so the desktop agent can answer "what does the
//!   substrate say about X?" with ¬ filesystem ¬ network ¬ external dep.
//!
//! § runtime API
//!   * [`get_doc`]              — O(log N) exact name-lookup
//!   * [`always_loaded`]        — canon docs ⇒ load-at-session-start (~5K tok)
//!   * [`query_relevant`]       — top-K jaccard-similarity ranking over query
//!   * [`doc_count`]            — total embedded doc count
//!   * [`doc_names`]            — iterate sorted doc names
//!   * [`estimated_tokens`]     — coarse byte/4 token-count heuristic
//!
//! § indexing model
//!   build.rs splits each doc into ≤ 256 unique len-≥-3 lowercased tokens
//!   (punct-stripped), hashes each via BLAKE3 → first-4-bytes ⇒ `u32`, sorts
//!   + dedupes ⇒ `&[u32]` per doc. Runtime tokenizes the query identically
//!   then ranks docs by jaccard `|A∩B| / |A∪B|`.
//!
//! § no-fs no-net guarantee
//!   The corpus is fully embedded at compile time. Empty corpus (e.g. CI
//!   bare-scaffold) compiles + tests pass.

#![forbid(unsafe_code)]

mod tokenize;

pub use tokenize::tokenize;

include!(concat!(env!("OUT_DIR"), "/static_substrate_data.rs"));
include!(concat!(env!("OUT_DIR"), "/static_hdc_index.rs"));

/// O(log N) lookup by exact embedded doc-name.
///
/// Names are the keys printed in `SUBSTRATE_DOCS` ; e.g. `"CLAUDE.md"`,
/// `"PRIME_DIRECTIVE.md"`, `"specs/grand-vision/23_MYCELIUM_DESKTOP.csl"`,
/// `"memory/MEMORY.md"`. Returns `None` if the name is unknown.
pub fn get_doc(name: &str) -> Option<&'static str> {
    SUBSTRATE_DOCS
        .binary_search_by_key(&name, |(n, _)| *n)
        .ok()
        .map(|i| SUBSTRATE_DOCS[i].1)
}

/// Returns canon docs that should be loaded into context at session-start.
///
/// Stage-0 set : `CLAUDE.md`, `MEMORY.md`, `PRIME_DIRECTIVE.md`. Only docs
/// actually present in the build are returned ; the desktop bootstrap can
/// `flat_map` this directly.
pub fn always_loaded() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::with_capacity(ALWAYS_LOADED.len());
    for n in ALWAYS_LOADED {
        if let Some(body) = get_doc(n) {
            out.push((*n, body));
        }
    }
    out
}

/// Top-K docs ranked by jaccard-similarity of token-hash bags vs query.
///
/// `top_k = 0` ⇒ empty `Vec`. Empty corpus ⇒ empty `Vec`. Score ties resolve
/// by name (ascending) for deterministic output. Returned scores are in
/// `[0.0, 1.0]` ; `0.0`-score entries are filtered out.
pub fn query_relevant(query: &str, top_k: usize) -> Vec<(&'static str, f32)> {
    if top_k == 0 || DOC_TOKEN_HASHES.is_empty() {
        return Vec::new();
    }
    let q_tokens = tokenize(query);
    if q_tokens.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(&'static str, f32)> = DOC_TOKEN_HASHES
        .iter()
        .filter_map(|(name, bag)| {
            let s = jaccard(&q_tokens, bag);
            if s > 0.0 {
                Some((*name, s))
            } else {
                None
            }
        })
        .collect();

    // descending score, ascending name on tie
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });

    scored.truncate(top_k);
    scored
}

/// Total number of embedded docs.
pub fn doc_count() -> usize {
    SUBSTRATE_DOCS.len()
}

/// Iterator over all embedded doc names (sorted ascending).
pub fn doc_names() -> impl Iterator<Item = &'static str> {
    SUBSTRATE_DOCS.iter().map(|(n, _)| *n)
}

/// Coarse total-token-count heuristic (`bytes / 4`) summed across all docs.
pub fn estimated_tokens() -> usize {
    SUBSTRATE_DOCS.iter().map(|(_, b)| b.len() / 4).sum()
}

// ── jaccard over sorted u32 bags ─────────────────────────────────────
fn jaccard(a: &[u32], b: &[u32]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    // Both are sorted+dedup'd by build.rs / tokenize.
    let (mut i, mut j) = (0usize, 0usize);
    let mut inter = 0u32;
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
        }
    }
    let union = (a.len() + b.len()) as u32 - inter;
    if union == 0 {
        0.0
    } else {
        f32::from(u16::try_from(inter).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(union).unwrap_or(u16::MAX))
    }
}
