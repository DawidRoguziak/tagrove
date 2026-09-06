/// Debug binaries may open only the two explicitly isolated application profiles.
/// A configurable publisher identifier must receive the same release protection
/// as the local release identifier.
pub fn is_debug_profile(identifier: &str) -> bool {
    matches!(
        identifier,
        "com.example.mediatagger.dev" | "com.example.mediatagger.e2e"
    )
}

#[cfg(test)]
mod tests {
    use super::is_debug_profile;

    #[test]
    fn permits_only_the_known_isolated_profiles() {
        assert!(is_debug_profile("com.example.mediatagger.dev"));
        assert!(is_debug_profile("com.example.mediatagger.e2e"));
        assert!(!is_debug_profile("com.example.mediatagger"));
        assert!(!is_debug_profile("io.github.publisher.Tagrove"));
        assert!(!is_debug_profile("io.github.publisher.Tagrove.dev"));
    }
}
