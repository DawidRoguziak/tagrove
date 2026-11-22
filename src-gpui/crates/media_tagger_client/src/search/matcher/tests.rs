// Expected highlights are lists of byte ranges, one range being a common answer.
#![allow(clippy::single_range_in_vec_init)]

use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::Instant,
};

use super::{
    build_tag_suggestions, build_tag_suggestions_with, is_meta_filter_query, Cancel, Cancelled,
    TagIndex,
};
use crate::search::active_token::{read_active_token, ActiveToken};

fn index(tags: &[&str]) -> TagIndex {
    TagIndex::new(Arc::new(tags.iter().map(|tag| (*tag).to_owned()).collect()))
}

fn token(query: &str) -> ActiveToken {
    ActiveToken {
        start: 0,
        end: query.len(),
        query: query.into(),
        negative: false,
        literal: false,
    }
}

fn values(tags: &[&str], token: Option<&ActiveToken>, used: &[&str], limit: usize) -> Vec<String> {
    let used: BTreeSet<String> = used.iter().map(|tag| (*tag).to_owned()).collect();
    build_tag_suggestions_with(
        token,
        &used,
        &HashSet::new(),
        &index(tags),
        20,
        limit,
        &Cancel::new(),
    )
    .expect("not cancelled")
    .into_iter()
    .map(|suggestion| suggestion.value)
    .collect()
}

#[test]
fn no_token_no_suggestions() {
    assert!(values(&["cat", "car"], None, &[], 8).is_empty());
}

#[test]
fn duplicates_and_used_tags_are_left_out() {
    let found = values(
        &["Cat", "cat", "car", "camera"],
        Some(&token("ca")),
        &["car"],
        10,
    );
    let lower: Vec<String> = found.iter().map(|value| value.to_lowercase()).collect();
    assert!(lower.contains(&"cat".to_owned()));
    assert!(!lower.contains(&"car".to_owned()));
    let unique: HashSet<_> = lower.iter().collect();
    assert_eq!(unique.len(), lower.len());
}

#[test]
fn the_exact_query_stays_even_when_used() {
    assert_eq!(values(&["cat"], Some(&token("cat")), &["cat"], 8), ["cat"]);
}

#[test]
fn the_suggestion_limit_holds() {
    assert_eq!(
        values(
            &["cat", "car", "camera", "cabin"],
            Some(&token("ca")),
            &[],
            2
        )
        .len(),
        2
    );
}

#[test]
fn operator_queries_get_no_suggestions() {
    assert!(values(&["tags", "tagstone", "cat"], Some(&token("tags")), &[], 8).is_empty());
    assert!(values(
        &["gn:trip", "trip-2026", "cat"],
        Some(&token("gN:trip-2026")),
        &[],
        8
    )
    .is_empty());
    for query in ["gn", "GN:x", "tags:", "Tags:3"] {
        assert!(is_meta_filter_query(query), "{query}");
    }
    for query in ["gnu", "tagsx", "-tags"] {
        assert!(!is_meta_filter_query(query), "{query}");
    }
}

#[test]
fn quoted_operators_get_suggestions_in_vocabulary_order() {
    let token = read_active_token("-\"tags\"", 5).expect("a token");
    assert!(token.literal && token.negative);
    assert_eq!(
        values(&["tags", "tags:3"], Some(&token), &[], 8),
        ["tags", "tags:3"]
    );
}

#[test]
fn prefixes_rank_before_later_substrings_and_fuzzy_matches() {
    let found = values(
        &["scatter", "tomcat", "cat", "cart", "category", "dog"],
        Some(&token("cat")),
        &[],
        8,
    );
    // Exact and prefix matches score 0 (vocabulary order between them), a match one
    // character late 0.01, a fuzzy one a third, a later substring by its distance.
    assert_eq!(found, ["cat", "category", "scatter", "tomcat", "cart"]);
}

#[test]
fn a_two_letter_query_needs_an_exact_substring() {
    assert_eq!(
        values(&["cat", "car", "dog", "cb"], Some(&token("ca")), &[], 8),
        ["cat", "car"]
    );
    assert_eq!(
        values(&["trip-2026", "cat"], Some(&token("2026")), &[], 8),
        ["trip-2026"]
    );
}

#[test]
fn hard_exclusions_do_not_count_against_the_search_limit() {
    let tags: Vec<String> = (0..25).map(|i| format!("cat{i:02}")).collect();
    let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
    let excluded: HashSet<String> = tags[..5].iter().map(|tag| (*tag).to_owned()).collect();
    let found = build_tag_suggestions_with(
        Some(&token("cat")),
        &BTreeSet::new(),
        &excluded,
        &index(&tags),
        20,
        30,
        &Cancel::new(),
    )
    .expect("not cancelled");
    assert_eq!(found.len(), 20);
    assert_eq!(found[0].value, "cat05");
}

#[test]
fn highlights_are_the_matched_characters_as_byte_ranges() {
    let index = index(&["scatter", "żółw-cat", "cart"]);
    assert_eq!(index.highlight(0, "cat"), [1..4]);
    assert_eq!(index.highlight(1, "cat"), [8..11]);
    // c-a-r-t against "cat": c and a match, t after a skipped r.
    assert_eq!(index.highlight(2, "cat"), [0..2, 3..4]);
    let found = build_tag_suggestions(
        Some(&token("żół")),
        &BTreeSet::new(),
        &HashSet::new(),
        &index,
        &Cancel::new(),
    )
    .expect("not cancelled");
    assert_eq!(found[0].value, "żółw-cat");
    assert_eq!(found[0].ranges, [0..6]);
}

#[test]
fn a_cancelled_search_stops() {
    let cancel = Cancel::new();
    cancel.cancel();
    assert_eq!(index(&["cat"]).search("cat", 8, &cancel), Err(Cancelled));
}

/// Acceptance criterion of E12: typing does not block the interface with 100,000 tags.
/// The search runs on the executor; this measures one search and the index build.
#[test]
fn a_hundred_thousand_tags_are_searched_in_the_background_budget() {
    let tags: Vec<String> = (0..100_000)
        .map(|i| format!("tag-{:05}-{}", i, ["cat", "dog", "landscape", "x"][i % 4]))
        .collect();
    let built = Instant::now();
    let index = TagIndex::new(Arc::new(tags));
    let build = built.elapsed();
    let searched = Instant::now();
    let found = build_tag_suggestions(
        Some(&token("lands")),
        &BTreeSet::new(),
        &HashSet::new(),
        &index,
        &Cancel::new(),
    )
    .expect("not cancelled");
    let search = searched.elapsed();
    eprintln!("100k tags: index {build:?}, search {search:?}");
    assert_eq!(found.len(), 8);
    assert!(search.as_secs_f64() < 5.0, "{search:?}");
}
