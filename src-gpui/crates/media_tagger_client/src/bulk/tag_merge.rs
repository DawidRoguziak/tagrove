//! `bulkTagMergeService.ts`: the tag a bulk draft stands for, and tag lists merged
//! without duplicates.

use crate::library::normalize_tags;

/// `normalizeBulkTag`: the draft trimmed and lowercased, or the empty string when it
/// is no valid tag (blank, or with whitespace, a comma, a semicolon or a control
/// character inside).
pub fn normalize_bulk_tag(raw: &str) -> String {
    normalize_tags(vec![raw.to_owned()])
        .pop()
        .unwrap_or_default()
}

/// `normalizeBulkTagList`: every valid tag once, normalized, in first-seen order.
pub fn normalize_bulk_tag_list(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(tags.len());
    for tag in tags {
        let normalized = normalize_bulk_tag(tag);
        if !normalized.is_empty() && !out.contains(&normalized) {
            out.push(normalized);
        }
    }
    out
}

/// `mergeTagLists`: the existing tags, then the incoming ones they do not hold yet.
pub fn merge_tag_lists(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut merged = normalize_bulk_tag_list(existing);
    for tag in normalize_bulk_tag_list(incoming) {
        if !merged.contains(&tag) {
            merged.push(tag);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::{merge_tag_lists, normalize_bulk_tag, normalize_bulk_tag_list};

    fn list(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| (*tag).to_owned()).collect()
    }

    #[test]
    fn a_draft_is_trimmed_and_lowercased_or_refused() {
        assert_eq!(normalize_bulk_tag("  Cat "), "cat");
        assert_eq!(normalize_bulk_tag("ŻÓŁW"), "żółw");
        for invalid in ["", "   ", "new york", "a,b", "a;b", "a\tb"] {
            assert_eq!(normalize_bulk_tag(invalid), "", "{invalid:?}");
        }
    }

    #[test]
    fn lists_keep_the_first_spelling_of_each_tag() {
        assert_eq!(
            normalize_bulk_tag_list(&list(&[" Dog", "cat", "DOG", "", "a b"])),
            ["dog", "cat"]
        );
    }

    #[test]
    fn merging_appends_only_new_tags() {
        assert_eq!(
            merge_tag_lists(&list(&["cat", "dog"]), &list(&["Dog", "bird", "bird"])),
            ["cat", "dog", "bird"]
        );
        assert_eq!(
            merge_tag_lists(&list(&["Cat", "cat"]), &list(&[" "])),
            ["cat"]
        );
    }
}
