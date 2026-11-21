//! Behaviour of the i18n engine over the shared locale files (epic E03), through the
//! public API only: the acceptance criteria "plural vectors for all 11 languages" and
//! "every key of en.json resolves in every language; a missing key falls back to
//! English, never to blank text", plus every placeholder of the real files.

use media_tagger_client::i18n::{
    plural_category, Catalog, I18n, Language, Locales, Params, PluralOperands,
};

/// The plural vector of the acceptance criteria, plus millions (`many` in fr, it, es).
const VECTOR: [i64; 10] = [0, 1, 2, 5, 12, 21, 22, 101, 1_000_000, 2_000_001];

const PLURAL_SUFFIXES: [&str; 5] = ["_zero", "_one", "_few", "_many", "_other"];

fn english() -> &'static Catalog {
    Locales::embedded().catalog(Language::En)
}

/// The base keys of every plural group of en.json (`gallery.resultCount` for
/// `gallery.resultCount_one`, ...), sorted and unique.
fn plural_bases() -> Vec<String> {
    let mut bases: Vec<String> = english()
        .keys()
        .into_iter()
        .filter_map(|key| {
            PLURAL_SUFFIXES
                .iter()
                .find_map(|suffix| key.strip_suffix(suffix).map(str::to_owned))
        })
        .collect();
    bases.sort();
    bases.dedup();
    bases
}

/// The names of the `{{name}}` placeholders of a text, as the locale contract reads
/// them (`scripts/locale-contract.mjs`: `{{\s*[\w.]+\s*}}`).
fn placeholder_names(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else {
            break;
        };
        let name = after[..close].trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        {
            names.push(name.to_owned());
        }
        rest = &after[close + 2..];
    }
    names
}

#[test]
fn en_json_has_plural_groups_to_pin() {
    let bases = plural_bases();
    assert!(
        bases.contains(&"gallery.resultCount".to_owned()),
        "{bases:?}"
    );
    assert!(bases.len() >= 5, "{bases:?}");
}

/// Every plural key of the real files, for every vector count in every language,
/// renders the form its own file has for the CLDR category of the count, with the
/// count interpolated: no language falls back to English or to the key.
#[test]
fn every_plural_key_renders_the_languages_own_form_for_the_vector() {
    for language in Language::ALL {
        let i18n = I18n::new(language);
        let catalog = Locales::embedded().catalog(language);
        for base in plural_bases() {
            for count in VECTOR {
                let suffix = plural_category(language, PluralOperands::integer(count)).suffix();
                let own = catalog
                    .text(&format!("{base}{suffix}"))
                    .unwrap_or_else(|| panic!("{}: {base}{suffix} is missing", language.code()));
                let formatted = i18n.format_count(count);
                let params = Params::new()
                    .with_count(count)
                    .with("formattedCount", &formatted);
                let mut expected = own.replace("{{formattedCount}}", &formatted);
                expected = expected.replace("{{count}}", &count.to_string());

                let text = i18n.t_with(&base, &params);

                assert_eq!(text, expected, "{} {base} {count}", language.code());
                assert!(
                    !text.trim().is_empty(),
                    "{} {base} {count}",
                    language.code()
                );
            }
        }
    }
}

/// A language that lacks every key (an empty file) shows English for each key of
/// en.json, never the key and never blank text; plural keys use the English form
/// chosen by the English rule.
#[test]
fn a_language_without_a_key_shows_english_never_blank_text() {
    let locales = Locales::from_catalogs(|language| match language {
        Language::En => english().clone(),
        _ => Catalog::from_json("{}").unwrap(),
    });
    let keys = english().keys();
    for language in Language::ALL {
        let i18n = I18n::with_locales(language, &locales);
        for key in &keys {
            let english_text = english().text(key).unwrap();
            let text = i18n.t(key);
            assert_eq!(text, english_text, "{}: {key}", language.code());
            assert!(!text.trim().is_empty(), "{}: {key}", language.code());
        }
        for base in plural_bases() {
            for count in VECTOR {
                let suffix = plural_category(Language::En, PluralOperands::integer(count)).suffix();
                let english_form = english().text(&format!("{base}{suffix}")).unwrap();
                let text = i18n.t_with(
                    &base,
                    &Params::new()
                        .with_count(count)
                        .with("formattedCount", count),
                );
                assert_eq!(
                    text,
                    english_form
                        .replace("{{formattedCount}}", &count.to_string())
                        .replace("{{count}}", &count.to_string()),
                    "{}: {base} {count}",
                    language.code()
                );
            }
        }
    }
}

/// A language missing only some keys keeps its own text for the rest.
#[test]
fn only_the_missing_keys_fall_back_to_english() {
    let polish_source: serde_json::Value =
        serde_json::from_str(include_str!("../../../../src/i18n/locales/pl.json")).unwrap();
    let mut trimmed = polish_source.clone();
    trimmed["common"]
        .as_object_mut()
        .expect("pl.json has a common group")
        .remove("libraryBusy");
    let locales = Locales::from_catalogs(|language| match language {
        Language::Pl => Catalog::from_json(&trimmed.to_string()).unwrap(),
        other => Locales::embedded().catalog(other).clone(),
    });
    let polish = I18n::with_locales(Language::Pl, &locales);

    assert_eq!(
        polish.t("common.libraryBusy"),
        english().text("common.libraryBusy").unwrap()
    );
    assert_eq!(
        polish.t("common.loading"),
        Locales::embedded()
            .catalog(Language::Pl)
            .text("common.loading")
            .unwrap()
    );
}

/// Every placeholder of every text in every language is replaced when its value is
/// given: the engine reads each placeholder form the real files use.
#[test]
fn every_placeholder_of_the_real_files_is_interpolated() {
    let mut checked = 0;
    for language in Language::ALL {
        let i18n = I18n::new(language);
        let catalog = Locales::embedded().catalog(language);
        for key in catalog.keys() {
            let template = catalog.text(&key).unwrap();
            let names = placeholder_names(template);
            if names.is_empty() {
                continue;
            }
            let mut params = Params::new();
            for name in &names {
                params = if name == "count" {
                    params.with_count(7)
                } else {
                    params.with(name.clone(), format!("<{name}>"))
                };
            }
            // Plural forms are rendered through their base key with the count.
            let text = match PLURAL_SUFFIXES
                .iter()
                .find_map(|suffix| key.strip_suffix(suffix))
            {
                Some(_) => i18n
                    .resolve(&key, &params)
                    .unwrap_or_else(|| panic!("{}: {key}", language.code())),
                None => i18n.t_with(&key, &params),
            };

            assert!(
                !text.contains("{{"),
                "{}: {key} keeps a placeholder: {text}",
                language.code()
            );
            for name in names.iter().filter(|name| *name != "count") {
                assert!(
                    text.contains(&format!("<{name}>")),
                    "{}: {key} lacks {name}: {text}",
                    language.code()
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 100, "only {checked} texts with placeholders");
}

#[test]
fn placeholder_names_follow_the_locale_contract() {
    assert_eq!(
        placeholder_names("{{ a }} {{b.c}} {{}} {{- d}} {{e, number}}"),
        ["a", "b.c"]
    );
}
