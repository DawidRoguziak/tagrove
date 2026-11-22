//! The lightbox (`src/features/lightbox` of the Solid frontend): the selected asset
//! over the gallery, without a GUI.
//!
//! - [`model`]: [`Lightbox`], the port of `createLightbox.ts`: which asset is selected,
//!   where it is in the query result, navigation with wrap, the prefetch of its
//!   neighbours, and the tag, favorite, media group and delete writes, each guarded
//!   by the request it was started as.
//! - [`intents`]: what a tag editor wants saved and how it is merged onto the tags
//!   another writer stored first, and the pending favorite and group writes.
//! - [`presentation`]: `createLightboxPresentation.ts`: the side panel, the fading of
//!   the floating controls after three seconds, the copy button of the group name.
//! - [`delete_confirm`]: the inline delete confirmation with its typed word.
//!
//! [`Lightbox`] lives in the application runtime next to the library and survives
//! every view; the machines of [`presentation`] and [`DeleteConfirm`] belong to the
//! lightbox view and go with it, as the Solid component's owners do. Contract:
//! `docs/subsystems/lightbox.md` ("GPUI port: the lightbox model").

pub mod delete_confirm;
pub mod intents;
pub mod model;
pub mod presentation;

pub use delete_confirm::{DeleteConfirm, DeleteNotice};
pub use intents::{rebased, WriteState};
pub use model::{
    DeletedAsset, DeletedSource, Lightbox, LightboxCommand, LightboxEffect, LightboxEnv,
    LightboxEvent, LightboxInput, LightboxView, NavigationStatus, SelectedAsset,
    LIGHTBOX_TICKET_BASE,
};
pub use presentation::{
    is_narrow, Activity, CopyRequest, GroupClipboard, Sidebar, ACTIVITY_TIMEOUT, COPY_CONFIRMATION,
    NARROW_MAX_WIDTH, SIDEBAR_EXIT,
};

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{GroupClipboard, NavigationStatus};
    use crate::i18n::{Language, Locales};

    /// Every text the lightbox machines name exists in all eleven locale files.
    #[test]
    fn the_locale_keys_of_the_lightbox_exist_in_every_language() {
        let now = Duration::ZERO;
        let mut copied = GroupClipboard::new("group", 1);
        let request = copied.begin_copy().expect("a group name to copy");
        assert!(copied.finish_copy(&request, true, now));
        let mut keys = vec![
            "lightbox.deleteConfirm.confirmWord",
            "lightbox.deleteConfirm.sourceMissing",
            "lightbox.deleteConfirm.cleanupPending",
            "lightbox.deleteConfirm.failedMetadataPreserved",
            copied.title_key(now),
            GroupClipboard::new("group", 1).title_key(now),
            GroupClipboard::new("", 1).title_key(now),
        ];
        keys.extend(
            [
                NavigationStatus::Ready,
                NavigationStatus::Resolving,
                NavigationStatus::Missing,
                NavigationStatus::Failed,
            ]
            .into_iter()
            .filter_map(NavigationStatus::notice_key),
        );
        assert_eq!(keys.len(), 10);
        for language in Language::ALL {
            let catalog = Locales::embedded().catalog(language);
            for key in &keys {
                assert!(catalog.text(key).is_some(), "{language:?} lacks {key}");
            }
        }
    }
}
