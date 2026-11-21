//! Tag normalization (`normalizeTags` and `isValidTag` of `src/utils/media.ts`).
//!
//! The rule itself is the core's (`media_tagger_core::utils::tags`, reached through
//! the backend adapter): a tag is trimmed and lowercased, and it cannot hold
//! whitespace (U+FEFF counts, ADR 0033), a comma, a semicolon or a control character.
//! This module only adds what the Solid frontend does with that rule on lists it shows
//! and edits: a tag that breaks the rule is dropped instead of failing the list, so a
//! legacy stored tag with an inner U+FEFF never reaches an editor.
//!
//! One difference to `src/utils/media.ts` remains and is the core's to decide: the
//! core trims every Unicode `White_Space` character, U+0085 (next line) included,
//! while JavaScript's `trim()` leaves U+0085 in place and `isValidTag` then drops the
//! tag as one with a control character. Inside a tag U+0085 is invalid on both sides.
//!
//! The search grammar of the same TypeScript file (`parseSearchFilter`, quoted
//! tokens) belongs to the search model.

use crate::backend::core_backend::tag_rules;

/// `normalizeTags` for a list: every tag trimmed and lowercased, tags the rule
/// forbids and blank tags dropped, duplicates removed in first-seen order.
pub fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    // The usual list is valid as a whole; only a list with a forbidden tag is
    // checked tag by tag.
    if let Some(valid) = tag_rules::normalize_valid(tags.clone()) {
        return valid;
    }
    tag_rules::normalize(tags)
        .into_iter()
        .filter(|tag| is_valid_tag(tag))
        .collect()
}

/// `isValidTag` for a tag in its stored spelling: not blank, already trimmed, and
/// without whitespace, comma, semicolon or control character. Letter case does not
/// matter.
pub fn is_valid_tag(tag: &str) -> bool {
    match tag_rules::normalize_valid(vec![tag.to_owned()]) {
        // A tag that had to be trimmed held whitespace at an edge.
        Some(normalized) => normalized
            .first()
            .is_some_and(|normalized| *normalized == tag.to_lowercase()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_valid_tag, normalize_tags};

    fn list(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| (*tag).to_owned()).collect()
    }

    /// media.test.ts "normalizes tags to lowercase and deduplicates" (list form).
    #[test]
    fn lists_are_lowercased_deduplicated_and_lose_invalid_tags() {
        assert_eq!(
            normalize_tags(list(&[" Cat ", "DOG", "cat", ""])),
            ["cat", "dog"]
        );
        assert_eq!(
            normalize_tags(list(&["cat", "new york", "dog;bird"])),
            ["cat"]
        );
        assert_eq!(normalize_tags(list(&[" ŻÓŁW "])), ["żółw"]);
        assert!(normalize_tags(Vec::new()).is_empty());
    }

    /// The vectors shared by `src/utils/__tests__/media.test.ts` and `FEFF_VECTORS`
    /// of the core's `utils/tags.rs`: U+FEFF is trimmed at the edges and invalid
    /// inside (`None`: the tag is dropped).
    #[test]
    fn feff_is_trimmed_at_the_edges_and_invalid_inside_like_the_frontend() {
        let vectors: [(&str, Option<&[&str]>); 5] = [
            ("\u{FEFF}cat", Some(&["cat"])),
            ("ca\u{FEFF}t", None),
            ("cat\u{FEFF}", Some(&["cat"])),
            ("\u{FEFF}", Some(&[])),
            ("\u{FEFF}Cat\u{FEFF}", Some(&["cat"])),
        ];
        for (input, expected) in vectors {
            assert_eq!(
                normalize_tags(list(&[input])),
                list(expected.unwrap_or(&[])),
                "{input:?}"
            );
        }
        assert!(!is_valid_tag("ca\u{FEFF}t"));
    }

    /// `isValidTag`: `Boolean(tag) && !/[,;\s\p{Cc}]/u.test(tag)`.
    #[test]
    fn a_valid_tag_has_no_separator_whitespace_or_control_character() {
        for valid in ["cat", "Cat", "żółć|%_:'新", "-", "tags:3", "a.b"] {
            assert!(is_valid_tag(valid), "{valid:?}");
        }
        for invalid in [
            "",
            " ",
            "new york",
            " cat",
            "cat ",
            "cat,dog",
            "cat;dog",
            "cat\ndog",
            "cat\u{0}",
            "cat\u{7}",
            "\u{FEFF}cat",
            "cat\u{A0}dog",
            "cat\u{85}dog",
        ] {
            assert!(!is_valid_tag(invalid), "{invalid:?}");
        }
    }

    /// The frontend's side of the rule, read from its source: when it changes, this
    /// module and the core's rule have to be looked at again.
    const MEDIA_TS: &str = include_str!("../../../../../src/utils/media.ts");
    const MEDIA_TEST_TS: &str = include_str!("../../../../../src/utils/__tests__/media.test.ts");

    #[test]
    fn the_frontend_rule_is_the_one_this_module_was_matched_against() {
        for line in [
            r"return Boolean(tag) && !/[,;\s\p{Cc}]/u.test(tag);",
            ".map((item) => item.trim().toLowerCase())",
            ".filter(isValidTag)",
        ] {
            assert!(
                MEDIA_TS.contains(line),
                "src/utils/media.ts changed: {line}"
            );
        }
    }

    /// Every U+FEFF vector of `media.test.ts`, as that file lists them, gives the same
    /// list here.
    #[test]
    fn the_feff_vectors_of_the_frontend_tests_give_the_same_lists() {
        let table = MEDIA_TEST_TS
            .split_once("const vectors: Array<[string, string[] | null]> = [")
            .and_then(|(_, rest)| rest.split_once("];"))
            .map(|(table, _)| table)
            .expect("the vector table of media.test.ts");
        let mut checked = 0;
        for line in table.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let (input, expected): (String, Option<Vec<String>>) =
                serde_json::from_str(line.trim_end_matches(',')).expect("a vector as JSON");
            assert_eq!(
                normalize_tags(vec![input.clone()]),
                expected.unwrap_or_default(),
                "{input:?}"
            );
            checked += 1;
        }
        assert_eq!(checked, 5, "the table of media.test.ts changed");
    }

    /// What JavaScript's `\s` and `trim()` treat as whitespace is whitespace for a tag:
    /// trimmed at its edges, invalid inside. U+0085 is the one character the core
    /// trims that JavaScript does not; inside a tag it is invalid on both sides.
    #[test]
    fn javascript_whitespace_is_trimmed_at_the_edges_and_invalid_inside() {
        let mut spaces: Vec<char> = vec![
            '\t', '\n', '\u{B}', '\u{C}', '\r', ' ', '\u{A0}', '\u{1680}', '\u{2028}', '\u{2029}',
            '\u{202F}', '\u{205F}', '\u{3000}', '\u{FEFF}',
        ];
        spaces.extend('\u{2000}'..='\u{200A}');
        for space in spaces {
            assert_eq!(
                normalize_tags(vec![format!("{space}Cat{space}")]),
                ["cat"],
                "{space:?} at the edges"
            );
            assert!(
                normalize_tags(vec![format!("ca{space}t")]).is_empty(),
                "{space:?} inside"
            );
            assert!(!is_valid_tag(&format!("ca{space}t")), "{space:?}");
        }
        assert_eq!(normalize_tags(vec!["\u{85}cat\u{85}".into()]), ["cat"]);
        assert!(normalize_tags(vec!["ca\u{85}t".into()]).is_empty());
    }

    #[test]
    fn one_invalid_tag_does_not_cost_the_others_their_order() {
        assert_eq!(
            normalize_tags(list(&["b", "a,x", "A", "c\u{FEFF}d", "b", "C"])),
            ["b", "a", "c"]
        );
    }
}
