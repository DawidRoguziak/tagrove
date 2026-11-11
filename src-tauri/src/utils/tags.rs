use std::collections::HashSet;

use anyhow::ensure;

use super::text::canonical_key;

/// Whitespace as the frontend's `\s`/`trim()` see it: Unicode White_Space plus U+FEFF (BOM / zero
/// width no-break space), which `char::is_whitespace` rejects but JavaScript treats as a space.
fn is_tag_space(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{FEFF}'
}

fn is_invalid_tag_character(ch: char) -> bool {
    is_tag_space(ch) || ch.is_control() || matches!(ch, ',' | ';')
}

pub fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for tag in tags {
        let normalized = canonical_key(tag.trim_matches(is_tag_space));
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
}

pub fn normalize_and_validate_tags(tags: Vec<String>) -> anyhow::Result<Vec<String>> {
    let normalized = normalize_tags(tags);
    for tag in &normalized {
        ensure!(
            !tag.chars().any(is_invalid_tag_character),
            "Invalid tag '{tag}': tags cannot contain whitespace, commas, semicolons, or control characters"
        );
    }
    Ok(normalized)
}

/// Stored tags with U+FEFF split into separate tags (then normalized). Earlier versions could store
/// such a tag through CSV import; reads that feed validation use this so the legacy row neither
/// aborts a bulk merge, a CSV import nor a CSV export. Other invalid characters keep failing.
pub fn split_legacy_feff(stored: &[String]) -> Vec<String> {
    normalize_tags(
        stored
            .iter()
            .flat_map(|tag| tag.split('\u{FEFF}'))
            .map(str::to_string)
            .collect(),
    )
}

pub fn merge_tags(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut merged = split_legacy_feff(existing);
    let mut seen: HashSet<String> = merged.iter().cloned().collect();

    for tag in incoming {
        if seen.insert(tag.clone()) {
            merged.push(tag.clone());
        }
    }

    merged
}

pub fn parse_csv_tags(raw: &str) -> Vec<String> {
    raw.split(|ch: char| ch == ',' || ch == ';' || is_tag_space(ch))
        .filter_map(|tag| {
            let trimmed = tag.trim_matches(is_tag_space);
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

pub fn parse_legacy_tags(raw: &str) -> Vec<String> {
    raw.split(is_invalid_tag_character)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        merge_tags, normalize_and_validate_tags, normalize_tags, parse_csv_tags, parse_legacy_tags,
    };

    #[test]
    fn normalize_tags_lowercases_and_dedupes() {
        let tags = vec![" Cat ".to_string(), "cat".to_string(), "DOG".to_string()];
        assert_eq!(
            normalize_tags(tags),
            vec!["cat".to_string(), "dog".to_string()]
        );
    }

    #[test]
    fn merge_tags_preserves_existing_order() {
        let existing = vec!["cat".to_string()];
        let incoming = vec!["cat".to_string(), "dog".to_string()];
        assert_eq!(
            merge_tags(&existing, &incoming),
            vec!["cat".to_string(), "dog".to_string()]
        );
    }

    #[test]
    fn parse_csv_tags_splits_on_common_delimiters() {
        let parsed = parse_csv_tags("cat, dog;bird fish");
        assert_eq!(parsed, vec!["cat", "dog", "bird", "fish"]);
    }

    /// Shared with `src/utils/__tests__/media.test.ts`: (input, accepted tags). `None` means the
    /// tag is invalid (Rust rejects it, the frontend drops it).
    const FEFF_VECTORS: [(&str, Option<&[&str]>); 5] = [
        ("\u{FEFF}cat", Some(&["cat"])),
        ("ca\u{FEFF}t", None),
        ("cat\u{FEFF}", Some(&["cat"])),
        ("\u{FEFF}", Some(&[])),
        ("\u{FEFF}Cat\u{FEFF}", Some(&["cat"])),
    ];

    #[test]
    fn feff_is_trimmed_at_the_edges_and_invalid_inside_like_the_frontend() {
        for (input, expected) in FEFF_VECTORS {
            let result = normalize_and_validate_tags(vec![input.to_string()]);
            match expected {
                Some(tags) => assert_eq!(result.unwrap(), tags, "{input:?}"),
                None => assert!(result.is_err(), "{input:?}"),
            }
        }
        assert_eq!(parse_csv_tags("cat\u{FEFF}dog"), vec!["cat", "dog"]);
        assert_eq!(parse_csv_tags("\u{FEFF}cat, \u{FEFF}"), vec!["cat"]);
        assert_eq!(parse_legacy_tags("cat\u{FEFF}dog"), vec!["cat", "dog"]);
    }

    #[test]
    fn parse_legacy_tags_removes_every_now_invalid_separator() {
        let parsed = parse_legacy_tags("cat\0dog\u{7}bird,new york");
        assert_eq!(parsed, vec!["cat", "dog", "bird", "new", "york"]);
    }

    #[test]
    fn tag_validation_rejects_values_that_cannot_round_trip_through_csv() {
        for invalid in ["new york", "cat,dog", "cat;dog", "cat\ndog"] {
            assert!(normalize_and_validate_tags(vec![invalid.to_string()]).is_err());
        }
        assert_eq!(
            normalize_and_validate_tags(vec![" ŻÓŁW ".to_string()]).unwrap(),
            vec!["żółw"]
        );
    }
}
