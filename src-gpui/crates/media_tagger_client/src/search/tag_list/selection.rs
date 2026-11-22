//! `tagListSelectionService.ts`: the vocabulary fallback of the tag list, the merge of its
//! pages, and the include/exclude choices that become a search.
//!
//! Tags are compared by their normalized name ([`normalize_tag_name`]: trimmed with
//! JavaScript's whitespace and lowercased); the first spelling seen is kept.

use std::{cmp::Ordering, collections::BTreeMap};

use crate::search::{text::js_trim, tokens::serialize_tag_token};

/// How a tag takes part in the search the tag list builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagSelectionMode {
    Include,
    Exclude,
}

/// One page of the list: the tags shown and how many match in total.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagPage {
    pub items: Vec<String>,
    pub total: usize,
}

/// `normalizeTagName`: trimmed and lowercased.
pub fn normalize_tag_name(tag: &str) -> String {
    js_trim(tag).to_lowercase()
}

/// The base letter of a Latin letter with a diacritic, for the ordering of
/// [`compare_tags`]; other characters are returned as they are.
fn fold_diacritic(c: char) -> char {
    const FOLDS: &[(&str, char)] = &[
        ("àáâãäåāăą", 'a'),
        ("çćĉċč", 'c'),
        ("ďđ", 'd'),
        ("èéêëēĕėęě", 'e'),
        ("ĝğġģ", 'g'),
        ("ĥħ", 'h'),
        ("ìíîïĩīĭįı", 'i'),
        ("ĵ", 'j'),
        ("ķ", 'k'),
        ("ĺļľŀł", 'l'),
        ("ñńņňŉ", 'n'),
        ("òóôõöøōŏő", 'o'),
        ("ŕŗř", 'r'),
        ("śŝşšſ", 's'),
        ("ţťŧ", 't'),
        ("ùúûüũūŭůűų", 'u'),
        ("ŵ", 'w'),
        ("ýÿŷ", 'y'),
        ("źżž", 'z'),
    ];
    if c.is_ascii() {
        return c;
    }
    FOLDS
        .iter()
        .find(|(letters, _)| letters.contains(c))
        .map_or(c, |(_, base)| *base)
}

/// The primary weight of one character: punctuation and symbols first, then digits,
/// then letters (and everything else), each group in code-point order of the
/// lowercased letter without its diacritic.
fn primary_key(c: char) -> (u8, char) {
    let lower = c.to_lowercase().next().unwrap_or(c);
    let base = fold_diacritic(lower);
    let group = if base.is_alphabetic() {
        2
    } else if base.is_numeric() {
        1
    } else {
        0
    };
    (group, base)
}

/// `new Intl.Collator(undefined, { sensitivity: "base" }).compare`, approximated: case
/// and the diacritics of Latin letters are ignored, punctuation sorts before digits and
/// digits before letters. Strings equal under this order compare equal, so a stable
/// sort keeps their order.
pub fn compare_tags(left: &str, right: &str) -> Ordering {
    left.chars()
        .flat_map(char::to_lowercase)
        .map(primary_key)
        .cmp(right.chars().flat_map(char::to_lowercase).map(primary_key))
}

/// `sortTagsCaseInsensitive`.
pub fn sort_tags_case_insensitive(tags: &mut [String]) {
    tags.sort_by(|left, right| compare_tags(left, right));
}

/// `dedupeKnownTags`: trimmed, without blanks and duplicates (first spelling wins),
/// sorted.
pub fn dedupe_known_tags(tags: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for tag in tags {
        let trimmed = js_trim(tag);
        let normalized = trimmed.to_lowercase();
        if normalized.is_empty() || !seen.insert(normalized) {
            continue;
        }
        unique.push(trimmed.to_owned());
    }
    sort_tags_case_insensitive(&mut unique);
    unique
}

/// `createFallbackPage`: the known tags containing `query` (normalized), from
/// `offset`, at most `limit`.
pub fn create_fallback_page(
    known_tags: &[String],
    query: &str,
    offset: usize,
    limit: usize,
) -> TagPage {
    let query = normalize_tag_name(query);
    let filtered: Vec<&String> = known_tags
        .iter()
        .filter(|tag| query.is_empty() || normalize_tag_name(tag).contains(&query))
        .collect();
    TagPage {
        items: filtered
            .iter()
            .skip(offset)
            .take(limit)
            .map(|tag| (*tag).clone())
            .collect(),
        total: filtered.len(),
    }
}

/// `mergeTagPages`: `previous` followed by the tags of `next` not in it yet, by
/// normalized name; blank ones are dropped.
pub fn merge_tag_pages(previous: &[String], next: &[String]) -> Vec<String> {
    let mut merged = previous.to_vec();
    let mut known: std::collections::HashSet<String> = previous
        .iter()
        .map(|item| normalize_tag_name(item))
        .collect();
    for item in next {
        let normalized = normalize_tag_name(item);
        if normalized.is_empty() || !known.insert(normalized) {
            continue;
        }
        merged.push(item.clone());
    }
    merged
}

/// `TagSelections`: the included and excluded tags, by normalized name, with the
/// spelling that was chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagSelections {
    included: BTreeMap<String, String>,
    excluded: BTreeMap<String, String>,
}

impl TagSelections {
    pub fn new() -> Self {
        Self::default()
    }

    /// The included tags as chosen, in no particular order.
    pub fn included(&self) -> impl Iterator<Item = &str> {
        self.included.values().map(String::as_str)
    }

    pub fn excluded(&self) -> impl Iterator<Item = &str> {
        self.excluded.values().map(String::as_str)
    }

    pub fn included_count(&self) -> usize {
        self.included.len()
    }

    pub fn excluded_count(&self) -> usize {
        self.excluded.len()
    }

    pub fn is_empty(&self) -> bool {
        self.included.is_empty() && self.excluded.is_empty()
    }

    /// `getTagMode`.
    pub fn mode(&self, tag: &str) -> Option<TagSelectionMode> {
        let normalized = normalize_tag_name(tag);
        if normalized.is_empty() {
            None
        } else if self.included.contains_key(&normalized) {
            Some(TagSelectionMode::Include)
        } else if self.excluded.contains_key(&normalized) {
            Some(TagSelectionMode::Exclude)
        } else {
            None
        }
    }

    /// `removeTagSelection`.
    pub fn remove(&self, tag: &str) -> Self {
        let normalized = normalize_tag_name(tag);
        let mut next = self.clone();
        next.included.remove(&normalized);
        next.excluded.remove(&normalized);
        next
    }

    /// `putTagInMode`: a blank tag changes nothing.
    pub fn put(&self, tag: &str, mode: TagSelectionMode) -> Self {
        let trimmed = js_trim(tag);
        let normalized = trimmed.to_lowercase();
        if normalized.is_empty() {
            return self.clone();
        }
        let mut next = self.remove(tag);
        let target = match mode {
            TagSelectionMode::Include => &mut next.included,
            TagSelectionMode::Exclude => &mut next.excluded,
        };
        target.insert(normalized, trimmed.to_owned());
        next
    }

    fn chosen_spelling<'a>(&'a self, tag: &'a str) -> &'a str {
        let normalized = normalize_tag_name(tag);
        self.included
            .get(&normalized)
            .or_else(|| self.excluded.get(&normalized))
            .map_or(tag, String::as_str)
    }

    /// `applySingleSelectionAction`: inactive → included; selected → inactive.
    pub fn apply_single(&self, tag: &str) -> Self {
        match self.mode(tag) {
            None => self.put(tag, TagSelectionMode::Include),
            Some(_) => self.remove(tag),
        }
    }

    /// `applyDoubleSelectionAction`: inactive or included → excluded; excluded →
    /// included. The chosen spelling is kept.
    pub fn apply_double(&self, tag: &str) -> Self {
        let spelling = self.chosen_spelling(tag).to_owned();
        match self.mode(tag) {
            Some(TagSelectionMode::Exclude) => self.put(&spelling, TagSelectionMode::Include),
            _ => self.put(&spelling, TagSelectionMode::Exclude),
        }
    }

    /// `buildFilterInput`: the included tags sorted, then the excluded ones sorted with
    /// a `-`, each serialized as a search token.
    pub fn filter_input(&self) -> String {
        let mut included: Vec<String> = self.included.values().cloned().collect();
        let mut excluded: Vec<String> = self.excluded.values().cloned().collect();
        sort_tags_case_insensitive(&mut included);
        sort_tags_case_insensitive(&mut excluded);
        included
            .iter()
            .map(|tag| serialize_tag_token(tag, false))
            .chain(excluded.iter().map(|tag| serialize_tag_token(tag, true)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn normalizes_by_trim_and_lowercase() {
        assert_eq!(normalize_tag_name("  Cat\u{feff}"), "cat");
        assert_eq!(normalize_tag_name("ŻÓŁW"), "żółw");
        assert_eq!(normalize_tag_name("   "), "");
    }

    #[test]
    fn sorts_without_case_and_latin_diacritics() {
        let mut tags = strings(&["beta", "Alpha", "żaba", "zebra", "Ćma", "cel", "_x", "9"]);
        sort_tags_case_insensitive(&mut tags);
        assert_eq!(
            tags,
            ["_x", "9", "Alpha", "beta", "cel", "Ćma", "żaba", "zebra"]
        );
        assert_eq!(compare_tags("Apple", "apple"), Ordering::Equal);
        assert_eq!(compare_tags("é", "e"), Ordering::Equal);
    }

    #[test]
    fn dedupes_known_tags_keeping_the_first_spelling() {
        assert_eq!(
            dedupe_known_tags(&strings(&[" Zebra ", "alpha", "Alpha", "beta", "  ", ""])),
            ["alpha", "beta", "Zebra"]
        );
    }

    #[test]
    fn the_fallback_page_filters_and_slices_the_known_tags() {
        let tags = strings(&["alpha", "alpine", "beta"]);
        assert_eq!(
            create_fallback_page(&tags, " ALP ", 0, 100),
            TagPage {
                items: strings(&["alpha", "alpine"]),
                total: 2
            }
        );
        assert_eq!(
            create_fallback_page(&tags, "", 1, 1),
            TagPage {
                items: strings(&["alpine"]),
                total: 3
            }
        );
        assert_eq!(
            create_fallback_page(&tags, "", 5, 10).items,
            Vec::<String>::new()
        );
    }

    #[test]
    fn merges_pages_without_duplicates_or_blanks() {
        assert_eq!(
            merge_tag_pages(
                &strings(&["Alpha", "Beta"]),
                &strings(&["beta", "Gamma", " ", "gamma"])
            ),
            ["Alpha", "Beta", "Gamma"]
        );
    }

    #[test]
    fn single_and_double_actions_cycle_the_modes() {
        let empty = TagSelections::new();
        let included = empty.apply_single("Cat");
        assert_eq!(included.mode("cat"), Some(TagSelectionMode::Include));
        assert_eq!(included.apply_single("CAT"), empty);
        let excluded = included.apply_double("cat");
        assert_eq!(excluded.mode("Cat"), Some(TagSelectionMode::Exclude));
        assert_eq!(
            excluded.filter_input(),
            "-Cat",
            "the chosen spelling is kept"
        );
        let back = excluded.apply_double("cat");
        assert_eq!(back.mode("cat"), Some(TagSelectionMode::Include));
        assert_eq!(back.filter_input(), "Cat");
        assert_eq!(
            empty.apply_double("dog").mode("dog"),
            Some(TagSelectionMode::Exclude)
        );
        assert_eq!(excluded.apply_single("cat"), empty);
        assert_eq!(empty.put("  ", TagSelectionMode::Include), empty);
        assert_eq!(empty.mode(""), None);
    }

    #[test]
    fn the_filter_input_sorts_and_serializes() {
        let selections = TagSelections::new()
            .put("banana", TagSelectionMode::Include)
            .put("Apple", TagSelectionMode::Include)
            .put("cherry", TagSelectionMode::Exclude)
            .put("-dash", TagSelectionMode::Exclude);
        assert_eq!(
            selections.filter_input(),
            r#"Apple banana -"-dash" -cherry"#
        );
        assert_eq!(
            (selections.included_count(), selections.excluded_count()),
            (2, 2)
        );
    }

    #[test]
    fn literal_names_need_no_special_handling() {
        for name in ["constructor", "__proto__"] {
            let selections = TagSelections::new().apply_single(name);
            assert_eq!(selections.filter_input(), name);
            assert_eq!(
                TagSelections::new().apply_double(name).filter_input(),
                format!("-{name}")
            );
        }
    }
}
