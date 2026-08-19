pub fn normalize_root_path(path: &str) -> String {
    let trimmed = path.trim();

    #[cfg(windows)]
    {
        let mut normalized = trimmed.replace('/', "\\");
        while normalized.ends_with('\\') && normalized.len() > 3 {
            normalized.pop();
        }
        normalized
    }

    #[cfg(not(windows))]
    {
        let mut normalized = trimmed.to_string();
        while normalized.ends_with('/') && normalized.len() > 1 {
            normalized.pop();
        }
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_root_path;

    #[test]
    #[cfg(windows)]
    fn converts_forward_slashes_and_trims_whitespace_on_windows() {
        let normalized = normalize_root_path("  C:/media/photos  ");
        assert_eq!(normalized, "C:\\media\\photos");
    }

    #[test]
    #[cfg(windows)]
    fn removes_trailing_separator_for_non_drive_root_on_windows() {
        let normalized = normalize_root_path("C:\\media\\");
        assert_eq!(normalized, "C:\\media");
    }

    #[test]
    #[cfg(windows)]
    fn keeps_drive_root_trailing_separator_on_windows() {
        let normalized = normalize_root_path("C:\\");
        assert_eq!(normalized, "C:\\");
    }

    #[test]
    #[cfg(not(windows))]
    fn trims_whitespace_and_trailing_separator_on_unix() {
        let normalized = normalize_root_path("  /var/lib/media/  ");
        assert_eq!(normalized, "/var/lib/media");
    }

    #[test]
    #[cfg(not(windows))]
    fn keeps_unix_root_separator() {
        assert_eq!(normalize_root_path("/"), "/");
    }
}
