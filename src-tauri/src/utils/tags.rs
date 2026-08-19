use std::collections::HashSet;

pub fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for tag in tags {
        let normalized = tag.trim().to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::{merge_tags, normalize_tags, parse_csv_tags};

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
}
