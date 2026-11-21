//! The engine against the real locale files and against small synthetic ones.

use std::{collections::BTreeSet, path::Path};

use serde_json::json;

use super::{Catalog, I18n, Language, Locales, Params, PluralOperands};

/// The counts every language is pinned with (acceptance criteria of epic E03).
const VECTOR: [i64; 8] = [0, 1, 2, 5, 12, 21, 22, 101];

fn embedded() -> &'static Locales {
    Locales::embedded()
}

#[test]
fn every_embedded_locale_file_parses() {
    let parsed = Locales::parse_embedded().expect("all eleven locale files parse");

    assert_eq!(
        &parsed,
        embedded(),
        "nothing was replaced by an empty catalogue"
    );
}

#[test]
fn the_embedded_files_are_exactly_the_locale_directory() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../src/i18n/locales");
    let files: BTreeSet<String> = std::fs::read_dir(&directory)
        .expect("read src/i18n/locales")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .into_string()
                .unwrap()
        })
        .collect();
    let ours: BTreeSet<String> = Language::ALL
        .iter()
        .map(|language| format!("{}.json", language.code()))
        .collect();

    assert_eq!(files, ours);
}

#[test]
fn every_english_key_resolves_in_every_language_to_its_own_text() {
    let english = embedded().catalog(Language::En).keys();
    assert!(english.len() > 400, "en.json has {} keys", english.len());

    for language in Language::ALL {
        let catalog = embedded().catalog(language);
        let i18n = I18n::new(language);
        for key in &english {
            let own = catalog
                .text(key)
                .unwrap_or_else(|| panic!("{}: {key} is missing", language.code()));
            assert!(
                !own.trim().is_empty(),
                "{}: {key} is blank",
                language.code()
            );
            assert_eq!(
                i18n.resolve(key, &Params::new()).as_deref(),
                Some(own),
                "{}: {key} resolves to the language's own text",
                language.code()
            );
        }
        assert_eq!(
            catalog.keys().len(),
            english.len(),
            "{} has no extra keys",
            language.code()
        );
    }
}

fn synthetic() -> &'static Locales {
    let locales = Locales::from_catalogs(|language| {
        let value = match language {
            Language::En => json!({
                "common": {"only": "English only", "both": "Both (en)", "group": {"a": "A"}},
                "items_one": "{{count}} item (en)",
                "items_other": "{{count}} items (en)",
                "base": "{{count}} base (en)",
                "empty": "",
            }),
            Language::Pl => json!({
                "common": {"both": "Oba (pl)"},
                "items_few": "{{count}} elementy (pl)",
                "base": "{{count}} baza (pl)",
                "zeroed_zero": "zero (pl)",
                "zeroed_many": "{{count}} wiele (pl)",
            }),
            _ => json!({}),
        };
        Catalog::from_json(&value.to_string()).unwrap()
    });
    Box::leak(Box::new(locales))
}

#[test]
fn a_missing_key_falls_back_to_english_then_to_the_key() {
    let polish = I18n::with_locales(Language::Pl, synthetic());

    assert_eq!(polish.t("common.both"), "Oba (pl)");
    assert_eq!(polish.t("common.only"), "English only");
    assert_eq!(polish.t("common.missing"), "common.missing");
    assert_eq!(polish.resolve("common.missing", &Params::new()), None);
    assert_eq!(
        polish.t("common.group"),
        "common.group",
        "a group is not a text"
    );
    assert_eq!(
        polish.t("empty"),
        "",
        "an empty English text is a text, as in i18next (returnEmptyString)"
    );
}

#[test]
fn a_count_tries_each_language_before_falling_back() {
    let polish = I18n::with_locales(Language::Pl, synthetic());
    let german = I18n::with_locales(Language::De, synthetic());

    // pl `few` exists.
    assert_eq!(
        polish.t_with("items", &Params::new().with_count(2)),
        "2 elementy (pl)"
    );
    // pl `one` does not, and Polish has no base `items`: English decides with its
    // own rule.
    assert_eq!(
        polish.t_with("items", &Params::new().with_count(1)),
        "1 item (en)"
    );
    assert_eq!(
        polish.t_with("items", &Params::new().with_count(5)),
        "5 items (en)"
    );
    // The Polish base key wins over an English plural form.
    assert_eq!(
        polish.t_with("base", &Params::new().with_count(5)),
        "5 baza (pl)"
    );
    assert_eq!(
        german.t_with("items", &Params::new().with_count(1)),
        "1 item (en)"
    );
}

#[test]
fn a_count_of_zero_prefers_the_zero_key() {
    let polish = I18n::with_locales(Language::Pl, synthetic());

    assert_eq!(
        polish.t_with("zeroed", &Params::new().with_count(0)),
        "zero (pl)"
    );
    assert_eq!(
        polish.t_with("zeroed", &Params::new().with_count(5)),
        "5 wiele (pl)"
    );
    let half = Params::from_json(&json!({"count": 0.5}));
    assert_eq!(half.count(), PluralOperands::number(0.5));
    assert_eq!(polish.t_with("zeroed", &half), "zeroed", "0.5 is not zero");
}

/// `gallery.resultCount` for the vector counts: the plural form of each language's
/// own file, with `formattedCount` from `Intl.NumberFormat`.
#[test]
fn plural_counts_resolve_to_the_languages_own_plural_keys() {
    for language in Language::ALL {
        let i18n = I18n::new(language);
        let catalog = embedded().catalog(language);
        for count in VECTOR.into_iter().chain([1_000_000, 12_345]) {
            let suffix = super::plural_category(language, PluralOperands::integer(count)).suffix();
            let template = catalog
                .text(&format!("gallery.resultCount{suffix}"))
                .unwrap_or_else(|| panic!("{}: resultCount{suffix}", language.code()));
            let formatted = i18n.format_count(count);
            let params = Params::new()
                .with_count(count)
                .with("formattedCount", &formatted);

            let text = i18n.t_with("gallery.resultCount", &params);

            assert_eq!(
                text,
                template.replace("{{formattedCount}}", &formatted),
                "{} {count}",
                language.code()
            );
            assert!(!text.contains("{{"), "{} {count}: {text}", language.code());
        }
    }
}

#[test]
fn polish_and_english_counts_read_naturally() {
    let polish = I18n::new(Language::Pl);
    let english = I18n::new(Language::En);
    let count = |i18n: &I18n, n: i64| {
        i18n.t_with(
            "gallery.resultCount",
            &Params::new()
                .with_count(n)
                .with("formattedCount", i18n.format_count(n)),
        )
    };

    assert_eq!(count(&english, 1), "1 item");
    assert_eq!(count(&english, 0), "0 items");
    assert_eq!(count(&english, 12345), "12,345 items");
    assert_eq!(count(&polish, 1), "1 element");
    assert_eq!(count(&polish, 2), "2 elementy");
    assert_eq!(count(&polish, 5), "5 elementów");
    assert_eq!(count(&polish, 22), "22 elementy");
    assert_eq!(count(&polish, 12345), "12\u{a0}345 elementów");
}

#[test]
fn placeholders_take_their_values_unescaped() {
    let english = I18n::new(Language::En);
    let params = Params::new()
        .with("groups", "<b>2 groups</b>")
        .with("assets", "5 & more");

    assert_eq!(
        english.t_with("settings.duplicates.lastResult", &params),
        "Last result: <b>2 groups</b> / 5 & more"
    );
    assert_eq!(
        english.t("settings.duplicates.lastResult"),
        "Last result: {{groups}} / {{assets}}",
        "missing values keep their placeholders"
    );
}

#[test]
fn backend_text_is_translated_in_the_active_language() {
    let polish = I18n::new(Language::Pl);
    let busy = embedded()
        .catalog(Language::Pl)
        .text("common.libraryBusy")
        .unwrap();

    assert_eq!(polish.backend_text("i18n:common.libraryBusy"), busy);
    assert_eq!(polish.backend_text("common.libraryBusy"), busy);
    assert_eq!(
        polish.backend_text(r#"{"key":"common.libraryBusy","params":{}}"#),
        busy
    );
    assert_eq!(polish.backend_text("disk full"), "disk full");
}

#[test]
fn params_from_json_stringify_like_javascript() {
    let params = Params::from_json(&json!({
        "text": "a",
        "int": -3,
        "big": 18_446_744_073_709_551_615_u64,
        "float": 1.5,
        "whole": 2.0,
        "flag": true,
        "none": null,
        "list": [1, null, "x", [2, 3]],
        "object": {"a": 1},
        "count": 3,
    }));

    for (name, expected) in [
        ("text", "a"),
        ("int", "-3"),
        ("big", "18446744073709551615"),
        ("float", "1.5"),
        ("whole", "2"),
        ("flag", "true"),
        ("none", ""),
        ("list", "1,,x,2,3"),
        ("object", "[object Object]"),
        ("count", "3"),
    ] {
        assert_eq!(params.value(name), Some(expected), "{name}");
    }
    assert_eq!(params.count(), Some(PluralOperands::integer(3)));

    let indexed = Params::from_json(&json!(["zero", 1]));
    assert_eq!(indexed.value("0"), Some("zero"));
    assert_eq!(indexed.value("1"), Some("1"));
    for scalar in [json!("text"), json!(3), json!(null), json!(true)] {
        assert_eq!(Params::from_json(&scalar), Params::new(), "{scalar}");
    }
    assert_eq!(
        Params::from_json(&json!({"count": "4"})).count(),
        None,
        "a string count selects no plural"
    );
}

#[test]
fn later_values_replace_earlier_ones() {
    let params = Params::new()
        .with("name", "a")
        .with("name", "b")
        .with_count(2);

    assert_eq!(params.value("name"), Some("b"));
    assert_eq!(params.value("count"), Some("2"));
    assert_eq!(params.with_count(7).value("count"), Some("7"));
}
