//! User-visible text comes from `src/i18n/locales/*.json`, the files the Solid
//! frontend ships (decision D7 of the GPUI port). This first step embeds the canonical
//! English catalogue and resolves plain keys; the other ten languages, plural forms
//! and `{{placeholder}}` interpolation arrive with epic E03.

use serde_json::Value;

const EN_JSON: &str = include_str!("../../../../src/i18n/locales/en.json");

/// One locale file: nested JSON objects with string leaves.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    root: Value,
}

impl Catalog {
    /// The embedded canonical catalogue (`src/i18n/locales/en.json`).
    pub fn english() -> Result<Self, serde_json::Error> {
        Self::from_json(EN_JSON)
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
}

#[cfg(test)]
mod tests {
    use super::Catalog;

    #[test]
    fn english_catalogue_resolves_dotted_keys() {
        let catalog = Catalog::english().expect("en.json parses");

        assert_eq!(catalog.text("common.loading"), Some("Loading..."));
        assert_eq!(catalog.text("window.close"), Some("Close window"));
    }

    #[test]
    fn unknown_keys_and_groups_have_no_text() {
        let catalog =
            Catalog::from_json(r#"{"common": {"loading": "Loading...", "count": 3}}"#).unwrap();

        assert_eq!(catalog.text("common.loading"), Some("Loading..."));
        assert_eq!(catalog.text("common"), None, "a group is not a text");
        assert_eq!(catalog.text("common.count"), None, "only strings are texts");
        assert_eq!(catalog.text("common.missing"), None);
        assert_eq!(catalog.text("common.loading.deeper"), None);
        assert_eq!(catalog.text(""), None);
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(Catalog::from_json("{").is_err());
    }
}
