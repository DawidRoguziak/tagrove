//! The texts of the `kit` board (epic E05): every shared control in both themes, for
//! review against `src/styles.css`. The board shows labels of the real screens, taken
//! from the shared locale files like every text of the interface; nothing on it is
//! written in code.

use crate::i18n::{I18n, Params};

/// Every text of the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitTexts {
    pub dark: String,
    pub light: String,
    pub primary: String,
    pub secondary: String,
    pub ghost: String,
    pub danger: String,
    pub disabled: String,
    pub settings: String,
    pub favorite: String,
    pub bulk: String,
    pub delete: String,
    pub close: String,
    pub chip_default: String,
    pub chip_video: String,
    pub chip_gif: String,
    pub info_title: String,
    pub info_message: String,
    pub warning_title: String,
    pub warning_message: String,
    pub error_title: String,
    pub error_message: String,
    pub progress: String,
    pub loading: String,
    pub slider: String,
    pub media_kind: String,
    pub kinds: [String; 4],
    pub switch: String,
    pub switch_label: String,
    pub pending: String,
    pub placeholder: String,
    pub confirm_word: String,
    pub dialog_heading: String,
    pub dialog_description: String,
    pub dialog_cancel: String,
    pub dialog_confirm: String,
}

/// The value of the determinate progress bar on the board, of 100.
pub const PROGRESS_VALUE: i64 = 25;
/// The slider's value, of 0..=100.
pub const SLIDER_VALUE: f64 = 40.0;
/// The values of the media-kind segments, as the search toolbar has them.
pub const KINDS: [&str; 4] = ["all", "image", "gif", "video"];

/// Every locale key of the board.
pub const KEYS: [&str; 34] = [
    "settings.appearance.theme.dark",
    "settings.appearance.theme.light",
    "common.apply",
    "common.cancel",
    "common.back",
    "common.remove",
    "common.confirm",
    "topBar.openSettings",
    "lightbox.favorite.remove",
    "controls.disableBulkActions",
    "lightbox.deleteMedia",
    "common.close",
    "search.mediaKinds.image",
    "gallery.videoChip",
    "gallery.gifChip",
    "settings.scan.heading",
    "settings.scan.description",
    "bulk.groupModal.overwriteWarningTitle",
    "bulk.groupModal.overwriteWarningDescription",
    "bulk.panel.saveFailedTitle",
    "bulk.panel.tagSaveFailed",
    "settings.progress.scanning",
    "common.loading",
    "controls.tileSize",
    "search.mediaKind",
    "search.mediaKinds.all",
    "search.mediaKinds.gif",
    "search.mediaKinds.video",
    "settings.scan.autoScan",
    "settings.scan.startupPending",
    "search.placeholder",
    "lightbox.deleteConfirm.confirmWord",
    "settings.dialogs.removeScanRoot.heading",
    "settings.dialogs.removeScanRoot.description",
];

impl KitTexts {
    pub fn new(i18n: &I18n) -> Self {
        let t = |key: &str| i18n.t(key);
        Self {
            dark: t("settings.appearance.theme.dark"),
            light: t("settings.appearance.theme.light"),
            primary: t("common.apply"),
            secondary: t("common.cancel"),
            ghost: t("common.back"),
            danger: t("common.remove"),
            disabled: t("common.confirm"),
            settings: t("topBar.openSettings"),
            favorite: t("lightbox.favorite.remove"),
            bulk: t("controls.disableBulkActions"),
            delete: t("lightbox.deleteMedia"),
            close: t("common.close"),
            chip_default: t("search.mediaKinds.image"),
            chip_video: t("gallery.videoChip"),
            chip_gif: t("gallery.gifChip"),
            info_title: t("settings.scan.heading"),
            info_message: t("settings.scan.description"),
            warning_title: t("bulk.groupModal.overwriteWarningTitle"),
            warning_message: t("bulk.groupModal.overwriteWarningDescription"),
            error_title: t("bulk.panel.saveFailedTitle"),
            error_message: t("bulk.panel.tagSaveFailed"),
            progress: i18n.t_with(
                "settings.progress.scanning",
                &Params::new()
                    .with("processed", PROGRESS_VALUE)
                    .with("total", 100),
            ),
            loading: t("common.loading"),
            slider: t("controls.tileSize"),
            media_kind: t("search.mediaKind"),
            kinds: KINDS.map(|kind| t(&format!("search.mediaKinds.{kind}"))),
            switch: t("settings.scan.autoScan"),
            switch_label: t("settings.scan.autoScan"),
            pending: t("settings.scan.startupPending"),
            placeholder: t("search.placeholder"),
            confirm_word: t("lightbox.deleteConfirm.confirmWord"),
            dialog_heading: t("settings.dialogs.removeScanRoot.heading"),
            dialog_description: t("settings.dialogs.removeScanRoot.description"),
            dialog_cancel: t("common.cancel"),
            dialog_confirm: t("common.remove"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{KitTexts, KEYS};
    use crate::i18n::{I18n, Language, Params};

    #[test]
    fn every_text_comes_from_the_locale_files_of_every_language() {
        for language in Language::ALL {
            let i18n = I18n::new(language);
            for key in KEYS {
                let text = i18n.resolve(key, &Params::new());
                assert!(
                    text.is_some_and(|text| !text.is_empty()),
                    "{language:?}: {key}"
                );
            }
            let texts = KitTexts::new(&i18n);
            assert!(
                !texts.progress.contains("{{"),
                "{language:?}: {}",
                texts.progress
            );
        }
        let english = KitTexts::new(&I18n::new(Language::En));
        assert_eq!(english.progress, "Scanning 25/100");
        assert_eq!(
            english.kinds,
            ["All", "Images", "GIF", "Video"].map(String::from)
        );
    }
}
