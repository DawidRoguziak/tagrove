//! `translateBackendText` (`src/i18n/backendText.ts`): display text from the
//! backend is `i18n:<key>`, a `{"key","params"}` JSON string, a dotted key, or
//! free-form text shown unchanged. Display only: nothing branches on the result
//! (docs/subsystems/settings-operations.md, "backend error text").

use serde_json::Value;

use super::Params;

/// Translates backend display text with `translate`, which returns the key itself
/// for a key it cannot resolve (as i18next's `t` does).
pub fn translate_backend_text(raw: &str, translate: &dyn Fn(&str, &Params) -> String) -> String {
    let trimmed = raw.trim();
    if let Some(key) = trimmed.strip_prefix("i18n:").map(str::trim) {
        if !key.is_empty() {
            return translate(key, &Params::new());
        }
    }
    if trimmed.starts_with('{') {
        if let Ok(Value::Object(object)) = serde_json::from_str::<Value>(trimmed) {
            if let Some(Value::String(key)) = object.get("key") {
                if !key.trim().is_empty() {
                    let params = match object.get("params") {
                        Some(params) => Params::from_json(params),
                        None => Params::new(),
                    };
                    // Untrimmed, like `t(parsed.key, ...)`.
                    return translate(key, &params);
                }
            }
        }
    }
    if is_dotted_token(trimmed) {
        let translated = translate(trimmed, &Params::new());
        if translated != trimmed {
            return translated;
        }
    }
    raw.to_owned()
}

/// `/^[a-z0-9_.-]+$/i` and at least one dot.
fn is_dotted_token(text: &str) -> bool {
    !text.is_empty()
        && text.contains('.')
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::translate_backend_text;
    use crate::i18n::Params;

    /// The translator of `backendText.test.ts`: two messages, the key for any other.
    fn t(key: &str, params: &Params) -> String {
        match key {
            "common.libraryBusy" => "Busy".to_owned(),
            "settings.greeting" => format!("Hello {}", params.value("name").unwrap_or_default()),
            "settings.count" => format!(
                "{:?} {}",
                params.count(),
                params.value("count").unwrap_or_default()
            ),
            other => other.to_owned(),
        }
    }

    #[test]
    fn translates_i18n_keys_json_keys_with_params_and_known_dotted_tokens() {
        assert_eq!(
            translate_backend_text(" i18n:common.libraryBusy ", &t),
            "Busy"
        );
        assert_eq!(
            translate_backend_text(r#"{"key":"settings.greeting","params":{"name":"Ada"}}"#, &t),
            "Hello Ada"
        );
        assert_eq!(translate_backend_text("common.libraryBusy", &t), "Busy");
    }

    #[test]
    fn keeps_free_form_unknown_and_malformed_text_unchanged() {
        assert_eq!(translate_backend_text("disk full", &t), "disk full");
        assert_eq!(translate_backend_text("unknown.key", &t), "unknown.key");
        assert_eq!(translate_backend_text("i18n:", &t), "i18n:");
        assert_eq!(translate_backend_text("{not json", &t), "{not json");
    }

    #[test]
    fn unchanged_text_keeps_its_surrounding_whitespace() {
        assert_eq!(
            translate_backend_text("  disk full \n", &t),
            "  disk full \n"
        );
        assert_eq!(translate_backend_text(" unknown.key ", &t), " unknown.key ");
        assert_eq!(translate_backend_text(" i18n:   ", &t), " i18n:   ");
    }

    #[test]
    fn an_unknown_i18n_key_shows_the_key() {
        assert_eq!(
            translate_backend_text("i18n: missing.key ", &t),
            "missing.key"
        );
    }

    #[test]
    fn json_without_a_usable_key_is_free_form_text() {
        for raw in [
            r#"{"key":"","params":{}}"#,
            r#"{"key":"   "}"#,
            r#"{"key":3}"#,
            r#"{"params":{"name":"Ada"}}"#,
            r#"{"key":"settings.greeting"} trailing"#,
        ] {
            assert_eq!(translate_backend_text(raw, &t), raw, "{raw}");
        }
    }

    #[test]
    fn json_params_are_optional_and_a_numeric_count_selects_the_plural() {
        assert_eq!(
            translate_backend_text(r#"{"key":"settings.greeting"}"#, &t),
            "Hello "
        );
        assert_eq!(
            translate_backend_text(r#"{"key":"settings.greeting","params":null}"#, &t),
            "Hello "
        );
        assert_eq!(
            translate_backend_text(r#"{"key":"settings.count","params":{"count":2}}"#, &t),
            "Some(PluralOperands { i: 2, fraction: false }) 2"
        );
        assert_eq!(
            translate_backend_text(r#"{"key":"settings.count","params":{"count":"2"}}"#, &t),
            "None 2",
            "a string count interpolates but selects no plural, as in i18next"
        );
    }

    #[test]
    fn dotted_tokens_need_a_dot_and_only_token_characters() {
        assert_eq!(translate_backend_text("libraryBusy", &t), "libraryBusy");
        assert_eq!(
            translate_backend_text("common.libraryBusy!", &t),
            "common.libraryBusy!"
        );
        assert_eq!(
            translate_backend_text(" COMMON.libraryBusy", &t),
            " COMMON.libraryBusy"
        );
    }
}
