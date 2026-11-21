//! The languages of the application and how the starting one is chosen: the
//! counterpart of `src/i18n/languages.ts` and of `normalizeLanguage` and
//! `resolveInitialLanguage` in `src/i18n/core.ts`.

use std::ffi::OsString;

/// A language of `APP_LANGUAGES`, in the order of that list (the order of the
/// language selector).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    En,
    Pl,
    Fr,
    De,
    It,
    Es,
    Ru,
    Zh,
    Ja,
    Ko,
    Cs,
}

/// The language of every key a locale lacks, and of a system without a supported
/// locale.
pub const FALLBACK_LANGUAGE: Language = Language::En;

impl Language {
    /// Every language in the order of `APP_LANGUAGES`.
    pub const ALL: [Self; 11] = [
        Self::En,
        Self::Pl,
        Self::Fr,
        Self::De,
        Self::It,
        Self::Es,
        Self::Ru,
        Self::Zh,
        Self::Ja,
        Self::Ko,
        Self::Cs,
    ];

    /// The code of `APP_LANGUAGES` and the name of its locale file.
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Pl => "pl",
            Self::Fr => "fr",
            Self::De => "de",
            Self::It => "it",
            Self::Es => "es",
            Self::Ru => "ru",
            Self::Zh => "zh",
            Self::Ja => "ja",
            Self::Ko => "ko",
            Self::Cs => "cs",
        }
    }

    /// The label of the language selector (`APP_LANGUAGE_NATIVE_LABELS`), written in
    /// the language itself and therefore the same in every locale.
    pub const fn native_label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Pl => "Polski",
            Self::Fr => "Français",
            Self::De => "Deutsch",
            Self::It => "Italiano",
            Self::Es => "Español",
            Self::Ru => "Русский",
            Self::Zh => "中文（简体）",
            Self::Ja => "日本語",
            Self::Ko => "한국어",
            Self::Cs => "Čeština",
        }
    }

    /// The language whose code is exactly `code`.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == code)
    }

    /// `normalizeLanguage`: the first language, in `APP_LANGUAGES` order, whose code
    /// starts the trimmed and lowercased value. Prefix matching, not BCP 47 parsing:
    /// `fr-FR`, `cs_CZ.UTF-8` and `zh-CN` match, and so does any other text that
    /// starts with a supported code.
    pub fn normalize(value: &str) -> Option<Self> {
        let normalized = value.trim().to_lowercase();
        if normalized.is_empty() {
            return None;
        }
        Self::ALL
            .into_iter()
            .find(|language| normalized.starts_with(language.code()))
    }
}

/// The locale variables of the process that name the language of messages, in POSIX
/// precedence: `LC_ALL` overrides `LC_MESSAGES`, which overrides `LANG`.
///
/// They replace `navigator.language`, which WebKitGTK derives from the same variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocaleEnvironment {
    pub lc_all: Option<OsString>,
    pub lc_messages: Option<OsString>,
    pub lang: Option<OsString>,
}

impl LocaleEnvironment {
    /// Reads the three variables through `env` (`std::env::var_os` in the
    /// application, a table in tests).
    pub fn read(env: &dyn Fn(&str) -> Option<OsString>) -> Self {
        Self {
            lc_all: env("LC_ALL"),
            lc_messages: env("LC_MESSAGES"),
            lang: env("LANG"),
        }
    }

    /// The locale that decides the language of messages: the first variable that is
    /// set and not empty. An empty variable counts as unset (POSIX).
    pub fn messages_locale(&self) -> Option<&OsString> {
        [&self.lc_all, &self.lc_messages, &self.lang]
            .into_iter()
            .flatten()
            .find(|value| !value.is_empty())
    }

    /// The supported language of [`Self::messages_locale`]. An unsupported locale
    /// (`C`, `POSIX`, `sv_SE.UTF-8`) gives `None`; it does not fall through to a
    /// variable of lower precedence.
    pub fn language(&self) -> Option<Language> {
        Language::normalize(&self.messages_locale()?.to_string_lossy())
    }
}

/// `resolveInitialLanguage`: the stored preference, then the system locale, then
/// English.
pub fn initial_language(stored: Option<Language>, environment: &LocaleEnvironment) -> Language {
    stored
        .or_else(|| environment.language())
        .unwrap_or(FALLBACK_LANGUAGE)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, ffi::OsString};

    use super::{initial_language, Language, LocaleEnvironment, FALLBACK_LANGUAGE};

    const LANGUAGES_TS: &str = include_str!("../../../../../src/i18n/languages.ts");
    const CORE_TS: &str = include_str!("../../../../../src/i18n/core.ts");

    /// The quoted strings between `start` and the next `end` of `source`.
    fn quoted_between<'a>(source: &'a str, start: &str, end: &str) -> Vec<&'a str> {
        let from = source.find(start).expect("start marker") + start.len();
        let body = &source[from..from + source[from..].find(end).expect("end marker")];
        body.split('"').skip(1).step_by(2).collect()
    }

    #[test]
    fn languages_follow_app_languages_in_order() {
        let codes = quoted_between(LANGUAGES_TS, "APP_LANGUAGES = [", "]");
        let ours: Vec<&str> = Language::ALL
            .iter()
            .map(|language| language.code())
            .collect();

        assert_eq!(ours, codes);
        assert_eq!(Language::ALL[0], FALLBACK_LANGUAGE, "English comes first");
        assert_eq!(Language::default(), FALLBACK_LANGUAGE);
    }

    #[test]
    fn native_labels_match_languages_ts() {
        // `Record<AppLanguage, string> = { en: "English", ... }`: each quoted value
        // keyed by the bare identifier before its colon.
        let from = LANGUAGES_TS
            .find("APP_LANGUAGE_NATIVE_LABELS")
            .expect("labels");
        let body = &LANGUAGES_TS[from..];
        let body = &body[body.find('{').unwrap() + 1..body.find("};").unwrap()];
        let labels: HashMap<&str, &str> = body
            .split(',')
            .filter_map(|entry| {
                let (key, value) = entry.split_once(':')?;
                Some((key.trim(), value.trim().trim_matches('"')))
            })
            .collect();

        assert_eq!(labels.len(), Language::ALL.len());
        for language in Language::ALL {
            assert_eq!(
                labels.get(language.code()).copied(),
                Some(language.native_label()),
                "{}",
                language.code()
            );
        }
    }

    #[test]
    fn every_language_is_a_resource_of_core_ts() {
        for language in Language::ALL {
            let code = language.code();
            assert!(
                CORE_TS.contains(&format!("import {code} from \"./locales/{code}.json\";")),
                "{code} is imported"
            );
            assert!(
                CORE_TS.contains(&format!("{code}: {{ translation: {code} }}")),
                "{code} is a resource"
            );
        }
    }

    #[test]
    fn codes_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
        assert_eq!(Language::from_code("EN"), None);
        assert_eq!(Language::from_code("pl-PL"), None);
        assert_eq!(Language::from_code(""), None);
    }

    #[test]
    fn normalize_matches_the_prefix_of_a_trimmed_lowercase_value() {
        for (value, expected) in [
            ("pl", Some(Language::Pl)),
            ("  PL ", Some(Language::Pl)),
            ("fr-FR", Some(Language::Fr)),
            ("cs_CZ.UTF-8", Some(Language::Cs)),
            ("zh-CN", Some(Language::Zh)),
            ("zh_TW", Some(Language::Zh)),
            ("de@euro", Some(Language::De)),
            ("english", Some(Language::En)),
            ("sv_SE.UTF-8", None),
            ("C", None),
            ("C.UTF-8", None),
            ("POSIX", None),
            ("", None),
            ("   ", None),
            ("e", None),
        ] {
            assert_eq!(Language::normalize(value), expected, "{value:?}");
        }
    }

    fn environment(pairs: &[(&str, &str)]) -> LocaleEnvironment {
        let table: HashMap<String, OsString> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
            .collect();
        LocaleEnvironment::read(&|name| table.get(name).cloned())
    }

    #[test]
    fn the_stored_preference_wins_over_the_system_locale() {
        let polish = environment(&[("LANG", "pl_PL.UTF-8")]);

        assert_eq!(initial_language(Some(Language::De), &polish), Language::De);
        assert_eq!(initial_language(None, &polish), Language::Pl);
    }

    #[test]
    fn the_system_locale_follows_posix_precedence() {
        for (pairs, expected) in [
            (vec![("LANG", "fr_FR.UTF-8")], Language::Fr),
            (
                vec![("LANG", "fr_FR.UTF-8"), ("LC_MESSAGES", "ja_JP.UTF-8")],
                Language::Ja,
            ),
            (
                vec![
                    ("LANG", "fr_FR.UTF-8"),
                    ("LC_MESSAGES", "ja_JP.UTF-8"),
                    ("LC_ALL", "ko_KR.UTF-8"),
                ],
                Language::Ko,
            ),
            (
                vec![("LANG", "fr_FR.UTF-8"), ("LC_ALL", ""), ("LC_MESSAGES", "")],
                Language::Fr,
            ),
        ] {
            assert_eq!(
                initial_language(None, &environment(&pairs)),
                expected,
                "{pairs:?}"
            );
        }
    }

    #[test]
    fn an_unsupported_or_missing_locale_falls_back_to_english() {
        for pairs in [
            vec![],
            vec![("LANG", "C.UTF-8")],
            vec![("LANG", "sv_SE.UTF-8")],
            // The effective locale decides; a supported LANG under it does not.
            vec![("LANG", "pl_PL.UTF-8"), ("LC_MESSAGES", "POSIX")],
        ] {
            assert_eq!(
                initial_language(None, &environment(&pairs)),
                Language::En,
                "{pairs:?}"
            );
        }
    }
}
