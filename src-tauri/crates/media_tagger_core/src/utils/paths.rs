pub fn normalize_root_path(path: &str) -> String {
    let trimmed = path.trim();

    let mut normalized = trimmed.to_string();
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::normalize_root_path;

    #[test]
    fn trims_whitespace_and_trailing_separator_on_unix() {
        let normalized = normalize_root_path("  /var/lib/media/  ");
        assert_eq!(normalized, "/var/lib/media");
    }

    #[test]
    fn keeps_unix_root_separator() {
        assert_eq!(normalize_root_path("/"), "/");
    }
}
