//! UI preferences: the theme and the language, the only two settings the Solid
//! frontend keeps outside the database (`localStorage` keys `media-tagger.theme` and
//! `media-tagger.language`). The GPUI port keeps them in a file of the profile
//! directory behind the [`PrefsStore`] port (decision D6).
//!
//! Like `localStorage` in the Solid frontend, the store is best effort: reading never
//! fails (an unreadable or damaged file reads as "nothing stored"), and a failed save
//! must not stop the change of theme or language for the running session.

mod file_store;
#[cfg(any(test, feature = "test-support"))]
mod memory;

use std::{
    fmt::{Display, Formatter},
    io,
};

use serde_json::{Map, Value};

pub use file_store::{FilePrefsStore, PREFS_FILE_NAME};
#[cfg(any(test, feature = "test-support"))]
pub use memory::MemoryPrefsStore;

use crate::{
    i18n::{initial_language, Language, LocaleEnvironment},
    theme::ThemeName,
};

/// The stored preferences; `None` is "not stored", which falls back to the default
/// theme or to the system language.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Prefs {
    pub theme: Option<ThemeName>,
    pub language: Option<Language>,
}

/// The member names of the preferences file.
const THEME_KEY: &str = "theme";
const LANGUAGE_KEY: &str = "language";

impl Prefs {
    /// `readInitialTheme`: the stored theme, else dark.
    pub fn theme_or_default(&self) -> ThemeName {
        self.theme.unwrap_or_default()
    }

    /// `resolveInitialLanguage`: the stored language, else the system locale, else
    /// English.
    pub fn language_or_system(&self, environment: &LocaleEnvironment) -> Language {
        initial_language(self.language, environment)
    }

    pub fn with_theme(self, theme: ThemeName) -> Self {
        Self {
            theme: Some(theme),
            ..self
        }
    }

    pub fn with_language(self, language: Language) -> Self {
        Self {
            language: Some(language),
            ..self
        }
    }

    /// The preferences of a file's text. Each member is read on its own, as the two
    /// `localStorage` keys are: a theme other than exactly `light` or `dark` is not
    /// stored, a language is normalized like a stored one in `core.ts`, unknown
    /// members are ignored, and text that is not a JSON object stores nothing.
    pub fn parse(text: &str) -> Self {
        let Ok(Value::Object(members)) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let text_of = |key| members.get(key).and_then(Value::as_str);
        Self {
            theme: text_of(THEME_KEY).and_then(ThemeName::from_css_name),
            language: text_of(LANGUAGE_KEY).and_then(Language::normalize),
        }
    }

    /// The file text of these preferences; only stored values are written.
    pub fn to_json(&self) -> String {
        let mut members = Map::new();
        if let Some(theme) = self.theme {
            members.insert(THEME_KEY.to_owned(), theme.css_name().into());
        }
        if let Some(language) = self.language {
            members.insert(LANGUAGE_KEY.to_owned(), language.code().into());
        }
        let mut text = Value::Object(members).to_string();
        text.push('\n');
        text
    }
}

/// Why preferences could not be saved.
#[derive(Debug)]
pub struct PrefsError {
    operation: &'static str,
    source: io::Error,
}

impl PrefsError {
    pub(crate) fn new(operation: &'static str, source: io::Error) -> Self {
        Self { operation, source }
    }

    /// The I/O error behind the failure.
    pub fn io(&self) -> &io::Error {
        &self.source
    }
}

impl Display for PrefsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cannot {} the UI preferences: {}",
            self.operation, self.source
        )
    }
}

impl std::error::Error for PrefsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Where the UI preferences live. The application uses [`FilePrefsStore`]; tests use
/// [`MemoryPrefsStore`] (feature `test-support`).
///
/// Both methods touch storage, so a view never calls them on the UI thread: the
/// adapter loads once during startup and saves on its executor.
pub trait PrefsStore: Send + Sync {
    /// The stored preferences; nothing stored, unreadable or damaged storage reads as
    /// [`Prefs::default`]. Never fails and never panics.
    fn load(&self) -> Prefs;

    /// Replaces the stored preferences with `prefs`.
    fn save(&self, prefs: &Prefs) -> Result<(), PrefsError>;
}

#[cfg(test)]
mod tests {
    use super::Prefs;
    use crate::{
        i18n::{Language, LocaleEnvironment},
        theme::ThemeName,
    };

    #[test]
    fn preferences_round_trip_through_their_text() {
        for prefs in [
            Prefs::default(),
            Prefs::default().with_theme(ThemeName::Light),
            Prefs::default().with_language(Language::Cs),
            Prefs::default()
                .with_theme(ThemeName::Dark)
                .with_language(Language::Zh),
        ] {
            assert_eq!(Prefs::parse(&prefs.to_json()), prefs, "{prefs:?}");
        }
        // Compared as JSON values, not bytes: when the whole src-gpui workspace is
        // built, gpui turns on serde_json's `preserve_order`, which changes the order
        // in which `Map` writes its members.
        let text = Prefs::default()
            .with_theme(ThemeName::Light)
            .with_language(Language::Pl)
            .to_json();
        assert!(text.ends_with('\n'), "{text:?}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap(),
            serde_json::json!({"language": "pl", "theme": "light"})
        );
        assert_eq!(Prefs::default().to_json(), "{}\n");
    }

    #[test]
    fn damaged_text_stores_nothing() {
        for text in [
            "",
            "{",
            "null",
            "[\"light\"]",
            "\"dark\"",
            "{\"theme\": \"dark\"",
            "\u{0}\u{1}garbage",
        ] {
            assert_eq!(Prefs::parse(text), Prefs::default(), "{text:?}");
        }
    }

    #[test]
    fn each_member_is_read_on_its_own() {
        assert_eq!(
            Prefs::parse(r#"{"theme": "solarized", "language": "pl"}"#),
            Prefs::default().with_language(Language::Pl),
            "an unknown theme does not discard the language"
        );
        assert_eq!(
            Prefs::parse(r#"{"theme": "light", "language": "sv"}"#),
            Prefs::default().with_theme(ThemeName::Light)
        );
        assert_eq!(
            Prefs::parse(r#"{"theme": "Dark", "language": 3}"#),
            Prefs::default(),
            "the theme is matched exactly; a language must be text"
        );
        assert_eq!(
            Prefs::parse(r#"{"language": " fr-FR ", "extra": true}"#),
            Prefs::default().with_language(Language::Fr),
            "a stored language is normalized like the one in localStorage"
        );
    }

    #[test]
    fn defaults_follow_the_solid_frontend() {
        let polish = LocaleEnvironment {
            lang: Some("pl_PL.UTF-8".into()),
            ..Default::default()
        };

        assert_eq!(Prefs::default().theme_or_default(), ThemeName::Dark);
        assert_eq!(Prefs::default().language_or_system(&polish), Language::Pl);
        assert_eq!(
            Prefs::default()
                .with_language(Language::Ja)
                .language_or_system(&polish),
            Language::Ja
        );
        assert_eq!(
            Prefs::default().language_or_system(&LocaleEnvironment::default()),
            Language::En
        );
    }
}
