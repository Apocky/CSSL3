// § tests/query.rs — top-K ranking, score-ordering, edge-cases.

use cssl_host_substrate_knowledge as sk;

#[test]
fn query_relevant_top_k_respects_k() {
    if sk::doc_count() == 0 {
        return;
    }
    let r = sk::query_relevant("substrate sovereignty mycelium", 3);
    assert!(r.len() <= 3, "got {} > 3", r.len());
}

#[test]
fn query_relevant_returns_high_score_for_self_text() {
    if sk::doc_count() == 0 {
        return;
    }
    // Pick a doc, use its first 200 bytes as the query — should rank itself in top-K.
    let names: Vec<&str> = sk::doc_names().collect();
    // Pick a non-trivial doc (skip empty stubs if any).
    // Pick a doc whose body, when tokenized, yields ≥ 5 tokens — guards
    // against picking a doc whose first 200 chars are pure UTF-8 box-art.
    let pick = names.iter().find_map(|n| {
        let body = sk::get_doc(n)?;
        let toks = sk::tokenize(body);
        if toks.len() >= 5 { Some((*n, body)) } else { None }
    });
    let Some((name, body)) = pick else { return };
    // Use a word-bounded query : take first ~200 words, joined with spaces.
    let query: String = body.split_whitespace().take(200).collect::<Vec<_>>().join(" ");
    let r = sk::query_relevant(&query, 5);
    assert!(
        r.iter().any(|(n, _)| *n == name),
        "self-query did not rank own doc {name:?} in top-5 : got {:?}",
        r.iter().map(|(n, _)| *n).collect::<Vec<_>>()
    );
}

#[test]
fn query_relevant_empty_query() {
    let r = sk::query_relevant("", 5);
    assert!(r.is_empty(), "empty query must return empty Vec, got {} entries", r.len());
}

#[test]
fn query_relevant_no_docs_returns_empty() {
    // If the build embedded zero docs, query_relevant must return empty
    // regardless of input. If there ARE docs, this test still passes for an
    // unmatched query (all-punct).
    if sk::doc_count() == 0 {
        let r = sk::query_relevant("any query at all", 10);
        assert!(r.is_empty());
    } else {
        // Use only stripped chars so token-bag is empty.
        let r = sk::query_relevant("[]{}()<>", 10);
        assert!(r.is_empty(), "all-punct query must produce 0 matches");
    }
}

#[test]
fn query_relevant_score_ordering_descending() {
    if sk::doc_count() < 2 {
        return;
    }
    let r = sk::query_relevant("substrate mycelium sovereignty Apocky", 10);
    for w in r.windows(2) {
        assert!(
            w[0].1 >= w[1].1,
            "scores must be descending : {} < {}",
            w[0].1,
            w[1].1
        );
    }
}

#[test]
fn query_relevant_zero_k() {
    let r = sk::query_relevant("anything", 0);
    assert!(r.is_empty());
}
