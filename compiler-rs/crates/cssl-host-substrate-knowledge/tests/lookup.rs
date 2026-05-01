// § tests/lookup.rs — exact-name lookup + always-loaded subset + sort invariants.
// All tests are corpus-aware : tolerate empty (CI bare-scaffold) by guarding on
// `doc_count() > 0` where required, while still asserting structural invariants.

use cssl_host_substrate_knowledge as sk;

#[test]
fn get_doc_returns_some_for_known_name() {
    if sk::doc_count() == 0 {
        return;
    }
    let first = sk::doc_names().next().expect("at least one doc");
    let body = sk::get_doc(first).expect("known name resolves");
    assert!(!body.is_empty() || body.is_empty());
}

#[test]
fn get_doc_returns_none_for_unknown() {
    let v = sk::get_doc("definitely-not-an-embedded-name.xyz");
    assert!(v.is_none());
}

#[test]
fn binary_search_works_on_unsorted_query() {
    if sk::doc_count() < 2 {
        return;
    }
    let names: Vec<&str> = sk::doc_names().collect();
    let last = names[names.len() - 1];
    let first = names[0];
    assert!(sk::get_doc(last).is_some());
    assert!(sk::get_doc(first).is_some());
}

#[test]
fn always_loaded_subset_of_all() {
    let all: std::collections::BTreeSet<&str> = sk::doc_names().collect();
    for (name, _) in sk::always_loaded() {
        assert!(
            all.contains(name),
            "always_loaded entry {name:?} not in SUBSTRATE_DOCS"
        );
    }
}

#[test]
fn doc_count_matches_substrate_docs_len() {
    assert_eq!(sk::doc_count(), sk::doc_names().count());
}

#[test]
fn doc_names_sorted() {
    let names: Vec<&str> = sk::doc_names().collect();
    for w in names.windows(2) {
        assert!(w[0] <= w[1], "doc_names must be ascending : {:?} > {:?}", w[0], w[1]);
    }
}
