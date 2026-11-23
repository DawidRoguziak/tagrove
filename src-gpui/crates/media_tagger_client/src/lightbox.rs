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
//! - [`pan_zoom`], [`image_controls`]: the image viewer: fit, zoom from 1 to 12,
//!   the pan and its bounds, the drag, and fullscreen through the window.
//! - [`media`]: what the media stage shows, what the adapter may decode, and the
//!   first frame of a GIF; [`info`]: the texts made of the asset's metadata.
//! - [`session`]: one opening of the lightbox: the machines above together, with the
//!   rules for Escape, the backdrop, the viewer's keys and the focus.
//! - [`layout`]: the measures of the dialog, the side panel and the action rail.
//!
//! [`Lightbox`] lives in the application runtime next to the library and survives
//! every view; the machines of [`presentation`] and [`DeleteConfirm`] belong to the
//! lightbox view and go with it, as the Solid component's owners do; the view keeps
//! them in a [`Session`]. Contract: `docs/subsystems/lightbox.md` ("GPUI port: the
//! lightbox model" and "GPUI port: the lightbox view").

pub mod delete_confirm;
pub mod image_controls;
pub mod info;
pub mod intents;
pub mod layout;
pub mod media;
pub mod model;
pub mod pan_zoom;
pub mod presentation;
pub mod session;

pub use delete_confirm::{DeleteConfirm, DeleteNotice};
pub use image_controls::{
    Fullscreen, ImageControls, ImageRect, Viewport, WheelMode, WindowFullscreen,
    WINDOW_ANSWER_TIMEOUT,
};
pub use info::{
    format_bytes, format_duration, info_rows, media_summary, video_placeholder_lines, InfoRow,
};
pub use intents::{rebased, WriteState};
pub use media::{
    decode_refusal, gif_first_frame, probe_dimensions, sniff_format, source_refusal,
    stored_dimensions, webp_frame_count, DecodeRefusal, LoadState, MediaSource, PictureFormat,
    StageContent, StageKind, MAX_SOURCE_BYTES,
};
pub use model::{
    DeletedAsset, DeletedSource, Lightbox, LightboxCommand, LightboxEffect, LightboxEnv,
    LightboxEvent, LightboxInput, LightboxView, NavigationStatus, SelectedAsset,
    LIGHTBOX_TICKET_BASE,
};
pub use pan_zoom::{
    calculate_fit_scale, calculate_fitted_media_size, clamp_pan, clamp_zoom_factor,
    double_click_zoom_factor, next_zoom_factor, FittedMedia, MediaDimensions, Pan, ZoomDirection,
    MAX_ZOOM_FACTOR, MIN_ZOOM_FACTOR, ZOOM_MULTIPLIER,
};
pub use presentation::{
    is_narrow, Activity, CopyRequest, GroupClipboard, Sidebar, ACTIVITY_TIMEOUT, COPY_CONFIRMATION,
    NARROW_MAX_WIDTH, SIDEBAR_EXIT,
};
pub use session::{AddTag, Dismissal, FocusTarget, KeyOutcome, Session, Shown, ViewerKey};

/// The locale keys of the placeholders that stand in for what the GPUI lightbox
/// does not play yet (plan decision D8): the note under the video placeholder and
/// the description of the GIF chip.
pub const VIDEO_PLACEHOLDER_KEY: &str = "lightbox.placeholder.videoUnavailable";
pub const GIF_PLACEHOLDER_KEY: &str = "lightbox.placeholder.gifStill";

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        GroupClipboard, NavigationStatus, StageKind, GIF_PLACEHOLDER_KEY, VIDEO_PLACEHOLDER_KEY,
    };
    use crate::i18n::{Language, Locales};

    /// The keys the lightbox view passes to the translator itself.
    const VIEW_KEYS: &[&str] = &[
        "common.apply",
        "common.cancel",
        "common.close",
        "common.confirm",
        "gallery.gifChip",
        "gallery.retry",
        "bulk.groupModal.generateUuid",
        "bulk.panel.saveFailedTitle",
        "bulk.panel.tagSaveFailed",
        "search.errors.tagInvalidCharacters",
        "lightbox.favorite.add",
        "lightbox.favorite.remove",
        "lightbox.resetZoom",
        "lightbox.toggleFullscreen",
        "lightbox.deleteMedia",
        "lightbox.closePreview",
        "lightbox.openPanel",
        "lightbox.closePanel",
        "lightbox.previewDialog",
        "lightbox.sidebarAria",
        "lightbox.taggingHeading",
        "lightbox.addTag",
        "lightbox.tagInputPlaceholder",
        "lightbox.mediaGroup",
        "lightbox.removeTagAria",
        "lightbox.infoHeading",
        "lightbox.type",
        "lightbox.size",
        "lightbox.dimensions",
        "lightbox.duration",
        "lightbox.path",
        "lightbox.mediaGroupKeyPlaceholder",
        "lightbox.mediaGroupKeyAria",
        "lightbox.generateGroupAria",
        "lightbox.mediaGroupOrderPlaceholder",
        "lightbox.mediaGroupOrderAria",
        "lightbox.deleteConfirm.heading",
        "lightbox.deleteConfirm.description",
        "lightbox.deleteConfirm.typeYesLabel",
        "lightbox.deleteConfirm.typeYesPlaceholder",
        "lightbox.deleteConfirm.confirm",
        "lightbox.mediaError.image",
        "lightbox.tagSaving",
        "lightbox.retryTagSave",
        "lightbox.tagDetailsLoading",
        "lightbox.tagDetailsLoadFailedTitle",
        "lightbox.tagDetailsLoadFailed",
        "lightbox.retryTagDetails",
        "lightbox.assetDetailsLoadFailed",
        "lightbox.retryAssetDetails",
        "lightbox.saveFailedTitle",
        "lightbox.favoriteSaveFailed",
        "lightbox.mediaGroupSaveFailed",
    ];

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
        keys.extend([VIDEO_PLACEHOLDER_KEY, GIF_PLACEHOLDER_KEY]);
        keys.extend([StageKind::Image, StageKind::Gif, StageKind::Video].map(StageKind::label_key));
        // What the view draws by name.
        keys.extend(VIEW_KEYS.iter().copied());
        for language in Language::ALL {
            let catalog = Locales::embedded().catalog(language);
            for key in &keys {
                assert!(catalog.text(key).is_some(), "{language:?} lacks {key}");
            }
        }
    }
}
