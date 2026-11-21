//! The stored theme and language through the `PrefsStore` port (epic E03, decision
//! D6): the counterpart of `src/ui/dom/__tests__/theme.test.ts` and of the stored
//! language of `src/i18n/core.ts`, on a fresh temporary profile directory.

use std::{ffi::OsString, fs};

use media_tagger_client::{
    i18n::{Language, LocaleEnvironment},
    prefs::{FilePrefsStore, MemoryPrefsStore, Prefs, PrefsStore, PREFS_FILE_NAME},
    theme::ThemeName,
};
use tempfile::tempdir;

fn locale(lang: &str) -> LocaleEnvironment {
    LocaleEnvironment {
        lang: Some(OsString::from(lang)),
        ..LocaleEnvironment::default()
    }
}

/// theme.test.ts "uses dark theme by default".
#[test]
fn uses_dark_theme_by_default() {
    let profile = tempdir().unwrap();

    assert_eq!(
        FilePrefsStore::new(profile.path())
            .load()
            .theme_or_default(),
        ThemeName::Dark
    );
    assert_eq!(
        MemoryPrefsStore::default().load().theme_or_default(),
        ThemeName::Dark
    );
}

/// theme.test.ts "reads stored light or dark theme".
#[test]
fn reads_stored_light_or_dark_theme() {
    let profile = tempdir().unwrap();
    let store = FilePrefsStore::new(profile.path());

    fs::write(store.path(), r#"{"theme":"light"}"#).unwrap();
    assert_eq!(store.load().theme_or_default(), ThemeName::Light);

    fs::write(store.path(), r#"{"theme":"dark"}"#).unwrap();
    assert_eq!(store.load().theme_or_default(), ThemeName::Dark);
}

/// theme.test.ts "ignores invalid stored theme".
#[test]
fn ignores_invalid_stored_theme() {
    let profile = tempdir().unwrap();
    let store = FilePrefsStore::new(profile.path());

    for stored in [
        r#"{"theme":"solarized"}"#,
        r#"{"theme":"LIGHT"}"#,
        r#"{"theme":" light"}"#,
        r#"{"theme":1}"#,
    ] {
        fs::write(store.path(), stored).unwrap();
        assert_eq!(store.load().theme, None, "{stored}");
        assert_eq!(store.load().theme_or_default(), ThemeName::Dark, "{stored}");
    }
}

/// theme.test.ts "applies theme to document and local storage": the storage half.
/// Each change is stored and read back, and changing the theme keeps the language.
#[test]
fn applying_a_theme_stores_it_and_keeps_the_language() {
    let profile = tempdir().unwrap();
    let store = FilePrefsStore::new(profile.path());
    store
        .save(&Prefs::default().with_language(Language::Pl))
        .unwrap();

    for theme in [ThemeName::Light, ThemeName::Dark] {
        store.save(&store.load().with_theme(theme)).unwrap();

        let reopened = FilePrefsStore::new(profile.path()).load();
        assert_eq!(reopened.theme_or_default(), theme);
        assert_eq!(reopened.language, Some(Language::Pl));
        let text = fs::read_to_string(profile.path().join(PREFS_FILE_NAME)).unwrap();
        assert!(
            text.contains(&format!("\"{}\"", theme.css_name())),
            "{text}"
        );
    }
}

/// theme.test.ts "still synchronizes the native theme when persistence fails": the
/// storage half. A failed save reports an error, leaves the stored value as it was,
/// and the next save after the failure stores again.
#[test]
fn a_failed_theme_save_reports_and_keeps_the_stored_value() {
    let store = MemoryPrefsStore::new(Prefs::default().with_theme(ThemeName::Dark));
    store.fail_saves(true);

    let session = store.load().with_theme(ThemeName::Light);
    assert!(store.save(&session).is_err());

    assert_eq!(session.theme_or_default(), ThemeName::Light);
    assert_eq!(store.load().theme_or_default(), ThemeName::Dark);

    store.fail_saves(false);
    store.save(&session).unwrap();
    assert_eq!(store.load().theme_or_default(), ThemeName::Light);
}

/// core.ts `resolveInitialLanguage`: a stored language wins over the system locale.
#[test]
fn a_stored_language_wins_over_the_system_locale() {
    let profile = tempdir().unwrap();
    let store = FilePrefsStore::new(profile.path());
    fs::write(store.path(), r#"{"language":"de"}"#).unwrap();

    assert_eq!(
        store.load().language_or_system(&locale("pl_PL.UTF-8")),
        Language::De
    );
}

/// core.ts: a stored value is normalized like `normalizeLanguage`, and one that names
/// no supported language falls through to the system locale, then to English.
#[test]
fn a_stored_language_is_normalized_and_an_unknown_one_falls_through() {
    let profile = tempdir().unwrap();
    let store = FilePrefsStore::new(profile.path());

    fs::write(store.path(), r#"{"language":" DE-at "}"#).unwrap();
    assert_eq!(
        store.load().language_or_system(&locale("pl_PL.UTF-8")),
        Language::De
    );

    fs::write(store.path(), r#"{"language":"sv"}"#).unwrap();
    assert_eq!(
        store.load().language_or_system(&locale("pl_PL.UTF-8")),
        Language::Pl
    );
    assert_eq!(
        store.load().language_or_system(&locale("C.UTF-8")),
        Language::En
    );
    assert_eq!(
        store
            .load()
            .language_or_system(&LocaleEnvironment::default()),
        Language::En
    );
}

/// core.ts `changeAppLanguage` persists the language; the theme is kept.
#[test]
fn changing_the_language_stores_it_and_keeps_the_theme() {
    let profile = tempdir().unwrap();
    let store = FilePrefsStore::new(profile.path());
    store
        .save(&Prefs::default().with_theme(ThemeName::Light))
        .unwrap();

    for language in Language::ALL {
        store.save(&store.load().with_language(language)).unwrap();

        let reopened = FilePrefsStore::new(profile.path()).load();
        assert_eq!(reopened.language_or_system(&locale("pl_PL")), language);
        assert_eq!(reopened.theme, Some(ThemeName::Light));
    }
}

/// Acceptance "damaged file -> defaults without a panic", for every way a file can be
/// damaged short of being unreadable, and the defaults it yields.
#[test]
fn a_damaged_file_gives_the_default_theme_and_the_system_language() {
    let profile = tempdir().unwrap();
    let store = FilePrefsStore::new(profile.path());
    for damaged in [
        &b"{\"theme\":\"light\",\"language\":"[..],
        b"\xef\xbb\xbf{\"theme\":\"light\"}",
        b"\x00\x00\x00",
        b"true",
        b"{\"theme\":{\"nested\":\"light\"}}",
    ] {
        fs::write(store.path(), damaged).unwrap();

        let prefs = store.load();
        assert_eq!(prefs.theme_or_default(), ThemeName::Dark, "{damaged:?}");
        assert_eq!(
            prefs.language_or_system(&locale("cs_CZ.UTF-8")),
            Language::Cs,
            "{damaged:?}"
        );
    }
}
