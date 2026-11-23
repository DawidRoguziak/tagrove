//! The About section (`AboutSection.tsx`): the build metadata, the two links and the
//! legal texts, all read from the files the Tauri page reads them from, so the two
//! interfaces cannot drift apart.

use std::sync::OnceLock;

const PACKAGE_JSON: &str = include_str!("../../../../../package.json");
const PUBLISHER_JSON: &str = include_str!("../../../../../packaging/flatpak/publisher.json");
/// The full GPL text (`LICENSE`), shown in English.
pub const LICENSE_TEXT: &str = include_str!("../../../../../LICENSE");
/// The privacy notice (`PRIVACY.md`), shown in English.
pub const PRIVACY_TEXT: &str = include_str!("../../../../../PRIVACY.md");

/// What the About section lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutInfo {
    /// `version` of `package.json`.
    pub version: String,
    /// `license` of `package.json`.
    pub license: String,
    /// `developerName` of `packaging/flatpak/publisher.json`.
    pub publisher: String,
    /// `repository` of `packaging/flatpak/publisher.json`.
    pub repository: String,
}

fn field(json: &serde_json::Value, name: &str) -> String {
    json[name].as_str().unwrap_or_default().to_owned()
}

/// The metadata of this build.
pub fn info() -> &'static AboutInfo {
    static INFO: OnceLock<AboutInfo> = OnceLock::new();
    INFO.get_or_init(|| {
        let package: serde_json::Value = serde_json::from_str(PACKAGE_JSON).unwrap_or_default();
        let publisher: serde_json::Value = serde_json::from_str(PUBLISHER_JSON).unwrap_or_default();
        AboutInfo {
            version: field(&package, "version"),
            license: field(&package, "license"),
            publisher: field(&publisher, "developerName"),
            repository: field(&publisher, "repository"),
        }
    })
}

/// A link button of the section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AboutLink {
    Source,
    Contact,
}

impl AboutLink {
    pub const ALL: [Self; 2] = [Self::Source, Self::Contact];

    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Source => "settings.about.source",
            Self::Contact => "settings.about.contact",
        }
    }

    /// The repository, or its issue tracker.
    pub fn url(self) -> String {
        let repository = &info().repository;
        match self {
            Self::Source => repository.clone(),
            Self::Contact => format!("{repository}/issues"),
        }
    }
}

/// The two folding legal texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalText {
    License,
    Privacy,
}

impl LegalText {
    pub const ALL: [Self; 2] = [Self::License, Self::Privacy];

    pub const fn summary_key(self) -> &'static str {
        match self {
            Self::License => "settings.about.fullLicense",
            Self::Privacy => "settings.about.privacy",
        }
    }

    pub const fn text(self) -> &'static str {
        match self {
            Self::License => LICENSE_TEXT,
            Self::Privacy => PRIVACY_TEXT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_metadata_comes_from_the_shared_files() {
        let info = info();
        assert!(!info.version.is_empty());
        assert_eq!(info.license, "GPL-3.0-or-later");
        assert!(!info.publisher.is_empty());
        assert!(info.repository.starts_with("https://"));
        assert_eq!(
            AboutLink::Contact.url(),
            format!("{}/issues", info.repository)
        );
        assert!(LICENSE_TEXT.contains("GNU GENERAL PUBLIC LICENSE"));
        assert!(!PRIVACY_TEXT.is_empty());
    }
}
