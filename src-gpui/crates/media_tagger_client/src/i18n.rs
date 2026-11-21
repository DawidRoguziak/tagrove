//! User-visible text comes from `src/i18n/locales/*.json`, the files the Solid
//! frontend ships (decision D7 of the GPUI port): all eleven are embedded with
//! `include_str!` and resolved by an engine that follows the part of i18next Tagrove
//! uses (`src/i18n/core.ts`):
//!
//! - nested JSON resources addressed by dotted keys (`common.loading`);
//! - fallback to English for a key the active language lacks, and the key itself
//!   when no language has it;
//! - plural keys `<key>_one|_few|_many|_other` chosen by a `count` with the CLDR rules
//!   of the language ([`plural`]), `<key>_zero` first for a count of 0;
//! - `{{placeholder}}` interpolation without HTML escaping ([`interpolate`]);
//! - `formattedCount` values from [`format_count`] (`Intl.NumberFormat`).
//!
//! Not supported, because no locale file and no caller uses them: nesting (`$t(...)`),
//! contexts, ordinal plurals, namespaces and format functions. A key that names a
//! group instead of a text resolves like a missing key (i18next would return an
//! "object instead of string" notice).

pub mod backend_text;
pub mod interpolate;
pub mod language;
pub mod number;
pub mod plural;

use std::{borrow::Cow, sync::OnceLock};

use serde_json::Value;

pub use backend_text::translate_backend_text;
pub use language::{initial_language, Language, LocaleEnvironment, FALLBACK_LANGUAGE};
pub use number::format_count;
pub use plural::{plural_category, PluralCategory, PluralOperands};

use interpolate::interpolate;

/// The i18next suffix tried first for a count of exactly 0.
const ZERO_SUFFIX: &str = "_zero";

/// One locale file: nested JSON objects with string leaves.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Catalog {
    root: Value,
}

impl Catalog {
    /// The embedded canonical catalogue (`src/i18n/locales/en.json`).
    pub fn english() -> Result<Self, serde_json::Error> {
        Self::from_json(locale_source(Language::En))
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        Ok(Self {
            root: serde_json::from_str(text)?,
        })
    }

    /// The text of a dotted key such as `common.loading`. `None` for an unknown
    /// key and for a key that names a group instead of a text.
    pub fn text(&self, key: &str) -> Option<&str> {
        key.split('.')
            .try_fold(&self.root, |node, segment| node.get(segment))?
            .as_str()
    }

    /// Every text key of the catalogue: the dotted paths of its string leaves.
    pub fn keys(&self) -> Vec<String> {
        fn walk(node: &Value, prefix: &str, keys: &mut Vec<String>) {
            match node {
                Value::Object(children) => {
                    for (name, child) in children {
                        let path = if prefix.is_empty() {
                            name.clone()
                        } else {
                            format!("{prefix}.{name}")
                        };
                        walk(child, &path, keys);
                    }
                }
                Value::String(_) => keys.push(prefix.to_owned()),
                _ => {}
            }
        }
        let mut keys = Vec::new();
        walk(&self.root, "", &mut keys);
        keys
    }
}

/// The embedded source of a locale file.
fn locale_source(language: Language) -> &'static str {
    macro_rules! locale {
        ($code:literal) => {
            include_str!(concat!("../../../../src/i18n/locales/", $code, ".json"))
        };
    }
    match language {
        Language::En => locale!("en"),
        Language::Pl => locale!("pl"),
        Language::Fr => locale!("fr"),
        Language::De => locale!("de"),
        Language::It => locale!("it"),
        Language::Es => locale!("es"),
        Language::Ru => locale!("ru"),
        Language::Zh => locale!("zh"),
        Language::Ja => locale!("ja"),
        Language::Ko => locale!("ko"),
        Language::Cs => locale!("cs"),
    }
}

/// The catalogues of all eleven languages.
#[derive(Debug, Clone, PartialEq)]
pub struct Locales {
    /// Indexed by the position of the language in [`Language::ALL`].
    catalogs: Vec<Catalog>,
}

impl Locales {
    /// Parses the embedded locale files; the error names the first that fails.
    pub fn parse_embedded() -> Result<Self, (Language, serde_json::Error)> {
        let catalogs = Language::ALL
            .into_iter()
            .map(|language| {
                Catalog::from_json(locale_source(language)).map_err(|error| (language, error))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { catalogs })
    }

    /// The embedded locale files, parsed once. A file that does not parse (the tests
    /// rule that out) counts as empty, so its keys fall back to English instead of
    /// stopping the application.
    pub fn embedded() -> &'static Self {
        static EMBEDDED: OnceLock<Locales> = OnceLock::new();
        EMBEDDED.get_or_init(|| {
            Self::from_catalogs(|language| {
                Catalog::from_json(locale_source(language)).unwrap_or_default()
            })
        })
    }

    /// Catalogues built by `catalog` (tests use small synthetic ones).
    pub fn from_catalogs(catalog: impl Fn(Language) -> Catalog) -> Self {
        Self {
            catalogs: Language::ALL.into_iter().map(catalog).collect(),
        }
    }

    pub fn catalog(&self, language: Language) -> &Catalog {
        let index = Language::ALL
            .iter()
            .position(|candidate| *candidate == language)
            .expect("Language::ALL lists every language");
        &self.catalogs[index]
    }
}

/// Interpolation values and the plural `count` of one translation (i18next's
/// options object).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Params {
    count: Option<PluralOperands>,
    values: Vec<(String, String)>,
}

impl Params {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the value of `{{name}}`.
    pub fn with(mut self, name: impl Into<String>, value: impl ToString) -> Self {
        self.set(name.into(), value.to_string());
        self
    }

    /// Sets `count`: it selects the plural key and is the value of `{{count}}`.
    pub fn with_count(mut self, count: i64) -> Self {
        self.count = Some(PluralOperands::integer(count));
        self.set("count".to_owned(), count.to_string());
        self
    }

    /// The plural operands of `count`, when it is a number.
    pub fn count(&self) -> Option<PluralOperands> {
        self.count
    }

    /// The value of `{{name}}`.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn set(&mut self, name: String, value: String) {
        match self.values.iter_mut().find(|(key, _)| *key == name) {
            Some(entry) => entry.1 = value,
            None => self.values.push((name, value)),
        }
    }

    /// The options i18next gets from `{...params}` of a backend JSON text: the
    /// members of an object (or the indices of an array) as text, the way JavaScript
    /// turns them into strings, and a numeric `count` as the plural count. Any other
    /// value gives no parameters.
    pub fn from_json(params: &Value) -> Self {
        let mut result = Self::new();
        let members: Vec<(String, &Value)> = match params {
            Value::Object(object) => object.iter().map(|(k, v)| (k.clone(), v)).collect(),
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(index, v)| (index.to_string(), v))
                .collect(),
            _ => Vec::new(),
        };
        for (name, value) in members {
            if name == "count" {
                if let Value::Number(number) = value {
                    result.count = number
                        .as_i64()
                        .map(PluralOperands::integer)
                        .or_else(|| number.as_f64().and_then(PluralOperands::number));
                }
            }
            result.set(name, js_string(value));
        }
        result
    }
}

/// `'' + value` for a JSON value.
fn js_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => match (number.as_i64(), number.as_u64(), number.as_f64()) {
            (Some(integer), _, _) => integer.to_string(),
            (None, Some(integer), _) => integer.to_string(),
            (None, None, Some(float)) => js_number(float),
            (None, None, None) => number.to_string(),
        },
        Value::String(text) => text.clone(),
        // `Array.prototype.toString`: elements joined by commas, null as nothing.
        Value::Array(items) => items.iter().map(js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

/// `String(number)` for the numbers a backend sends: whole numbers without a
/// fraction, others in Rust's shortest round-trip form (JavaScript switches to
/// exponent notation from 1e21 and below 1e-6; Rust does not).
fn js_number(value: f64) -> String {
    if value == 0.0 {
        // Also -0, which JavaScript prints as 0.
        "0".to_owned()
    } else if value.fract() == 0.0 && value.abs() < 1e21 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

/// The translator of one language over a set of catalogues: `i18n.t` of the Solid
/// frontend with the active language fixed.
#[derive(Clone, Copy)]
pub struct I18n<'a> {
    language: Language,
    locales: &'a Locales,
}

impl std::fmt::Debug for I18n<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18n")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

impl I18n<'static> {
    /// The translator of `language` over the embedded locale files.
    pub fn new(language: Language) -> Self {
        Self::with_locales(language, Locales::embedded())
    }
}

impl<'a> I18n<'a> {
    pub fn with_locales(language: Language, locales: &'a Locales) -> Self {
        Self { language, locales }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// The text of `key`; see [`Self::t_with`].
    pub fn t(&self, key: &str) -> String {
        self.t_with(key, &Params::new())
    }

    /// The text of `key` with `params`, or the key itself when no language has it
    /// (i18next's default), so a missing text is never blank.
    pub fn t_with(&self, key: &str, params: &Params) -> String {
        self.resolve(key, params).unwrap_or_else(|| key.to_owned())
    }

    /// The interpolated text of `key` in the active language, else in English;
    /// `None` when neither has it.
    ///
    /// With a count, each language is searched for `<key>_zero` (count 0 only), then
    /// `<key><plural suffix of that language>`, then `<key>`, before the next
    /// language is tried, as in i18next's `Translator.resolve`.
    pub fn resolve(&self, key: &str, params: &Params) -> Option<String> {
        let template = self.template(key, params)?;
        let text = interpolate(template, &|name| params.value(name).map(Cow::Borrowed));
        Some(text.into_owned())
    }

    fn template(&self, key: &str, params: &Params) -> Option<&'a str> {
        let mut languages = vec![self.language];
        if self.language != FALLBACK_LANGUAGE {
            languages.push(FALLBACK_LANGUAGE);
        }
        languages.into_iter().find_map(|language| {
            let catalog = self.locales.catalog(language);
            let Some(count) = params.count() else {
                return catalog.text(key);
            };
            let zero = (count.i == 0 && !count.fraction)
                .then(|| catalog.text(&format!("{key}{ZERO_SUFFIX}")))
                .flatten();
            zero.or_else(|| {
                let suffix = plural_category(language, count).suffix();
                catalog.text(&format!("{key}{suffix}"))
            })
            .or_else(|| catalog.text(key))
        })
    }

    /// `new Intl.NumberFormat(language).format(count)`, the value of
    /// `formattedCount`.
    pub fn format_count(&self, count: i64) -> String {
        format_count(self.language, count)
    }

    /// Backend display text in this language ([`translate_backend_text`]).
    pub fn backend_text(&self, raw: &str) -> String {
        translate_backend_text(raw, &|key, params| self.t_with(key, params))
    }
}

#[cfg(test)]
mod tests;
