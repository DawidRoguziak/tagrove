use std::collections::HashSet;

use anyhow::ensure;

use super::text::canonical_key;

fn is_invalid_tag_character(ch: char) -> bool {
    ch.is_whitespace() || ch.is_control() || matches!(ch, ',' | ';')
}

pub fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for tag in tags {
        let normalized = canonical_key(tag.trim());
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

pub fn merge_tags(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut merged = normalize_tags(existing.to_vec());
    let mut seen: HashSet<String> = merged.iter().cloned().collect();

    for tag in incoming {
        if seen.insert(tag.clone()) {
            merged.push(tag.clone());
        }
    }

    merged
}

pub fn parse_csv_tags(raw: &str) -> Vec<String> {
    raw.split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .filter_map(|tag| {
            let trimmed = tag.trim();
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
