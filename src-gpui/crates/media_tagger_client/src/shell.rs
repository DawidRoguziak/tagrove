//! What the shell shows around the views: the gallery header (`AppGalleryView.tsx`), the
//! settings page header (`AppSettingsView.tsx`) and the gallery's empty state
//! (`GalleryEmptyState.tsx`). Texts come from the shared locale files; measures are
//! the CSS pixels of `src/styles.css` and the Tailwind classes of those components.
//!
//! The header has slots that later epics fill: the search field and its toolbar
//! (media kind, favourites, tag list; E12, E13), and the bulk-actions toggle (E11,
//! E14). The shell draws the identity, the settings button, the window controls, the
//! item count and the tile-size slider.

use crate::{
    i18n::{I18n, Params},
    runtime::AppRuntime,
};

/// The product name next to the logo. It is a brand, not a translated text: the Tauri
/// header writes it literally as well.
pub const PRODUCT_NAME: &str = "Tagrove";

/// `.workspace-search`: the first header row.
pub mod search_row {
    pub const MIN_HEIGHT: f32 = 52.0;
    pub const PADDING_Y: f32 = 6.0;
    pub const PADDING_X: f32 = 16.0;
    /// The identity column (`grid-template-columns: 180px …`).
    pub const IDENTITY_WIDTH: f32 = 180.0;
    pub const COLUMN_GAP: f32 = 20.0;
    /// The settings button and the window controls (`gap-3`).
    pub const ACTIONS_GAP: f32 = 12.0;
}

/// `.workspace-identity`: logo and name.
pub mod identity {
    pub const LOGO_SIZE: f32 = 32.0;
    pub const GAP: f32 = 10.0;
    pub const FONT_SIZE: f32 = 18.0;
    pub const FONT_WEIGHT: f32 = 600.0;
    /// `letter-spacing: -0.02em` at 18 px.
    pub const LETTER_SPACING: f32 = -0.36;
}

/// `.workspace-tools`: the second header row.
pub mod tools_row {
    pub const MIN_HEIGHT: f32 = 44.0;
    pub const PADDING_Y: f32 = 5.0;
    pub const PADDING_X: f32 = 16.0;
    pub const GAP: f32 = 8.0;
    /// `.workspace-sizing`: bulk toggle and tile size, pushed to the right.
    pub const SIZING_GAP: f32 = 16.0;
    /// The tile-size group: `gap-3 border-l pl-4`, slider `w-28 py-3`.
    pub const TILE_SIZE_GAP: f32 = 12.0;
    pub const TILE_SIZE_PADDING_LEFT: f32 = 16.0;
    pub const SLIDER_WIDTH: f32 = 112.0;
    pub const SLIDER_PADDING_Y: f32 = 12.0;
}

/// `.workspace-count`: the item count chip.
pub mod count_chip {
    pub const GAP: f32 = 8.0;
    pub const PADDING_Y: f32 = 2.0;
    pub const PADDING_X: f32 = 8.0;
    pub const RADIUS: f32 = 4.0;
    pub const FONT_SIZE: f32 = 12.0;
    pub const DOT_SIZE: f32 = 6.0;
}

/// The settings page header (`px-4 py-3 sm:px-6`, `gap-4`).
pub mod settings_header {
    pub const PADDING_Y: f32 = 12.0;
    /// `sm:px-6`: the window is never narrower than its 1000 px minimum.
    pub const PADDING_X: f32 = 24.0;
    pub const GAP: f32 = 16.0;
    pub const LOGO_SIZE: f32 = 28.0;
    /// The heading block: `border-l pl-4`.
    pub const HEADING_PADDING_LEFT: f32 = 16.0;
    /// `text-2xl leading-tight`.
    pub const HEADING_SIZE: f32 = 24.0;
    pub const HEADING_LINE_HEIGHT: f32 = 1.25;
    /// `mt-0.5 text-xs`.
    pub const DESCRIPTION_MARGIN_TOP: f32 = 2.0;
    /// The back button's arrow (`h-4 w-4`).
    pub const BACK_ICON_SIZE: f32 = 16.0;
    /// The page below the header (`pt-5`).
    pub const BODY_PADDING_TOP: f32 = 20.0;
}

/// The gallery section around the grid and the empty state (`sm:pt-5`).
pub const GALLERY_PADDING_TOP: f32 = 20.0;

/// `GalleryEmptyState`.
pub mod empty_state {
    /// `min-h-[calc(100dvh-180px)]`: the height the state is centred in is the window's
    /// minus this.
    pub const VIEWPORT_INSET: f32 = 180.0;
    pub const MAX_WIDTH: f32 = 540.0;
    pub const PADDING: f32 = 28.0;
    pub const GAP: f32 = 12.0;
    /// The icon tile: `mb-1 h-16 w-16` with a 32 px icon, `bg-primary/10 ring-primary/15`.
    pub const ICON_TILE_SIZE: f32 = 64.0;
    pub const ICON_TILE_MARGIN_BOTTOM: f32 = 4.0;
    pub const ICON_SIZE: f32 = 32.0;
    pub const ICON_TILE_FILL_PERCENT: u8 = 10;
    pub const ICON_TILE_RING_PERCENT: u8 = 15;
    /// `text-2xl leading-tight`.
    pub const TITLE_SIZE: f32 = 24.0;
    pub const TITLE_LINE_HEIGHT: f32 = 1.25;
    /// `max-w-[420px] text-sm leading-[1.45]`. `src/styles.css` sets `--text-sm` to
    /// 13 px, not Tailwind's default 14 px.
    pub const DESCRIPTION_MAX_WIDTH: f32 = 420.0;
    pub const DESCRIPTION_SIZE: f32 = crate::theme::TYPOGRAPHY.sm.size;
    pub const DESCRIPTION_LINE_HEIGHT: f32 = 1.45;
}

/// What the gallery shows when it has nothing to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyState {
    /// No scan root yet: "Add first folder" opens the scan settings.
    NoFolders,
    /// Roots exist and the result is empty.
    NoResults,
}

impl EmptyState {
    pub fn title_key(self) -> &'static str {
        match self {
            Self::NoFolders => "gallery.noFoldersTitle",
            Self::NoResults => "gallery.noResultsTitle",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            Self::NoFolders => "gallery.noFoldersDescription",
            Self::NoResults => "gallery.noResultsDescription",
        }
    }

    /// The action button's text, for the state that has one.
    pub fn action_key(self) -> Option<&'static str> {
        match self {
            Self::NoFolders => Some("gallery.addFirstFolder"),
            Self::NoResults => None,
        }
    }

    /// The icon of the tile: a folder without roots, the magnifier otherwise.
    pub fn icon(self) -> crate::icons::IconName {
        match self {
            Self::NoFolders => crate::icons::IconName::Folder,
            Self::NoResults => crate::icons::IconName::Search,
        }
    }
}

/// The empty state the gallery shows, or `None` when it has assets to list (the grid
/// of epic E09). Like the Solid gallery it decides from the result's `total` and the
/// scan roots known so far.
pub fn empty_state(runtime: &AppRuntime) -> Option<EmptyState> {
    if runtime.library().total > 0 {
        return None;
    }
    Some(if runtime.has_scan_roots() {
        EmptyState::NoResults
    } else {
        EmptyState::NoFolders
    })
}

/// `workspace.itemCount` with the count formatted for the language.
pub fn item_count(i18n: &I18n<'_>, total: usize) -> String {
    let count = i64::try_from(total).unwrap_or(i64::MAX);
    i18n.t_with(
        "workspace.itemCount",
        &Params::new()
            .with_count(count)
            .with("formattedCount", i18n.format_count(count)),
    )
}

#[cfg(test)]
mod tests {
    use super::{empty_state, item_count, EmptyState};
    use crate::{
        i18n::{I18n, Language, Params},
        icons::IconName,
        model::Model,
        runtime::{AppRuntime, RuntimeConfig, RuntimeInput},
        theme::ThemeName,
    };
    use media_tagger_core::models::{ScanRoot, StartAssetQueryResult};

    fn runtime() -> AppRuntime {
        AppRuntime::new(RuntimeConfig {
            theme: ThemeName::Dark,
            language: Language::En,
        })
    }

    #[test]
    fn the_count_uses_the_plural_and_the_digit_grouping_of_the_language() {
        let english = I18n::new(Language::En);
        assert_eq!(item_count(&english, 0), "0 items");
        assert_eq!(item_count(&english, 1), "1 item");
        assert_eq!(item_count(&english, 12345), "12,345 items");

        let polish = I18n::new(Language::Pl);
        assert_ne!(item_count(&polish, 2), item_count(&polish, 5));
        assert!(item_count(&polish, 5).starts_with('5'));
    }

    #[test]
    fn the_empty_state_depends_on_roots_and_total() {
        let mut runtime = runtime();
        assert_eq!(empty_state(&runtime), Some(EmptyState::NoFolders));
        assert_eq!(EmptyState::NoFolders.icon(), IconName::Folder);
        assert_eq!(
            EmptyState::NoFolders.action_key(),
            Some("gallery.addFirstFolder")
        );

        let effects = runtime.handle(RuntimeInput::RefreshScanRoots);
        let token = match &effects[..] {
            [crate::runtime::RuntimeEffect::ListScanRoots(token)] => *token,
            other => panic!("unexpected effects {other:?}"),
        };
        runtime.handle(RuntimeInput::ScanRootsLoaded {
            token,
            result: Ok(vec![ScanRoot {
                path: "/media".into(),
                auto_scan_on_startup: false,
            }]),
        });
        assert_eq!(empty_state(&runtime), Some(EmptyState::NoResults));
        assert_eq!(EmptyState::NoResults.action_key(), None);

        runtime.handle(RuntimeInput::RefreshLibrary);
        runtime.handle(RuntimeInput::LibraryLoaded {
            generation: 1,
            result: Ok(StartAssetQueryResult::Ready {
                session_id: 1,
                revision: 1,
                total: 3,
                offset: 0,
                items: Vec::new(),
            }),
        });
        assert_eq!(empty_state(&runtime), None);
    }

    #[test]
    fn every_shell_text_exists_in_english() {
        let english = I18n::new(Language::En);
        for key in [
            "topBar.openSettings",
            "controls.tileSize",
            "common.back",
            "settings.page.heading",
            "settings.page.description",
            "window.controls",
            "window.minimize",
            "window.maximize",
            "window.restore",
            "window.close",
        ]
        .into_iter()
        .chain(
            [EmptyState::NoFolders, EmptyState::NoResults]
                .iter()
                .flat_map(|state| {
                    [
                        Some(state.title_key()),
                        Some(state.description_key()),
                        state.action_key(),
                    ]
                    .into_iter()
                    .flatten()
                }),
        ) {
            assert!(english.resolve(key, &Params::new()).is_some(), "{key}");
        }
    }
}
