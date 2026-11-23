//! The sections of the settings page and its navigation (`SettingsNavigation.tsx`).
//!
//! The navigation marks the section the reader is in: the last section whose top has
//! passed a line 120 px below the top of the scrolled page, or the last section once
//! the page is scrolled to its end. A click scrolls its section to the top of the
//! page, below the header (`scroll-margin-top: 96px` from the page's top), and marks
//! it at once.

use crate::icons::IconName;

/// A section of the settings page, in page order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SettingsSection {
    #[default]
    Scan,
    Appearance,
    Language,
    ImportExport,
    Duplicates,
    Danger,
    About,
}

impl SettingsSection {
    pub const ALL: [Self; 7] = [
        Self::Scan,
        Self::Appearance,
        Self::Language,
        Self::ImportExport,
        Self::Duplicates,
        Self::Danger,
        Self::About,
    ];

    /// The id of the section's anchor (`settings-<id>`).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Appearance => "appearance",
            Self::Language => "language",
            Self::ImportExport => "import-export",
            Self::Duplicates => "duplicates",
            Self::Danger => "danger",
            Self::About => "about",
        }
    }

    /// The navigation's label, the heading of the section.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Scan => "settings.scan.heading",
            Self::Appearance => "settings.appearance.heading",
            Self::Language => "settings.language.heading",
            Self::ImportExport => "settings.importExport.heading",
            Self::Duplicates => "settings.duplicates.heading",
            Self::Danger => "settings.danger.heading",
            Self::About => "settings.about.heading",
        }
    }

    pub const fn icon(self) -> IconName {
        match self {
            Self::Scan => IconName::Folder,
            Self::Appearance => IconName::Palette,
            Self::Language => IconName::Languages,
            Self::ImportExport => IconName::Copy,
            Self::Duplicates => IconName::Group,
            Self::Danger => IconName::Trash,
            Self::About => IconName::Info,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Scan => 0,
            Self::Appearance => 1,
            Self::Language => 2,
            Self::ImportExport => 3,
            Self::Duplicates => 4,
            Self::Danger => 5,
            Self::About => 6,
        }
    }

    /// The section with this anchor id.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.id() == id)
    }
}

/// How far below the top of the page a section's top must have passed to be the
/// current one.
pub const ACTIVE_LINE: f32 = 120.0;
/// `scroll-margin-top` of `.settings-section`: where a section scrolled to by the
/// navigation starts, from the top of the page.
pub const SCROLL_MARGIN_TOP: f32 = 96.0;

/// Where the page is scrolled: every length in pixels, measured from the top of the
/// page (the window's top, where the Tauri page's scroll port begins).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollMetrics {
    /// How far the content is scrolled.
    pub offset: f32,
    /// The furthest it can be scrolled; 0 when everything fits.
    pub max_offset: f32,
}

impl ScrollMetrics {
    /// Scrolled to the end of a page that scrolls (`scrollTop + clientHeight >=
    /// scrollHeight - 1`).
    pub fn at_end(&self) -> bool {
        self.max_offset > 0.0 && self.offset >= self.max_offset - 1.0
    }
}

/// The section the navigation marks. `section_tops` are the tops of the sections in
/// page order, measured from the top of the page as they are drawn now (so they move
/// up as the page scrolls).
pub fn active_section(section_tops: &[f32], metrics: ScrollMetrics) -> SettingsSection {
    if metrics.at_end() {
        return SettingsSection::About;
    }
    section_tops
        .iter()
        .zip(SettingsSection::ALL)
        .rev()
        .find(|(top, _)| **top <= ACTIVE_LINE)
        .map(|(_, section)| section)
        .unwrap_or_default()
}

/// The scroll offset that brings a section whose top is `section_top` (from the top
/// of the page, as drawn at scroll offset `metrics.offset`) to the scroll margin,
/// within the page's range. `viewport_top` is where the scrolled area begins, below
/// a header that does not scroll.
pub fn scroll_target(section_top: f32, viewport_top: f32, metrics: ScrollMetrics) -> f32 {
    let margin = SCROLL_MARGIN_TOP.max(viewport_top);
    (metrics.offset + section_top - margin).clamp(0.0, metrics.max_offset.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn metrics(offset: f32, max_offset: f32) -> ScrollMetrics {
        ScrollMetrics { offset, max_offset }
    }

    #[test]
    fn sections_keep_the_order_and_ids_of_the_page() {
        let ids: Vec<_> = SettingsSection::ALL.iter().map(|s| s.id()).collect();
        assert_eq!(
            ids,
            [
                "scan",
                "appearance",
                "language",
                "import-export",
                "duplicates",
                "danger",
                "about"
            ]
        );
        for section in SettingsSection::ALL {
            assert_eq!(SettingsSection::from_id(section.id()), Some(section));
            assert_eq!(SettingsSection::ALL[section.index()], section);
        }
    }

    #[test]
    fn the_last_section_past_the_line_is_active() {
        let tops = [100.0, 400.0, 700.0, 900.0, 1100.0, 1300.0, 1500.0];
        assert_eq!(
            active_section(&tops, metrics(0.0, 1000.0)),
            SettingsSection::Scan
        );
        let scrolled = tops.map(|top| top - 290.0);
        assert_eq!(
            active_section(&scrolled, metrics(290.0, 1000.0)),
            SettingsSection::Appearance
        );
        // Nothing has passed the line: the first section.
        let low = tops.map(|top| top + 100.0);
        assert_eq!(
            active_section(&low, metrics(0.0, 0.0)),
            SettingsSection::Scan
        );
    }

    #[test]
    fn the_end_of_a_scrolling_page_marks_the_last_section() {
        let tops = [100.0, 400.0, 700.0, 900.0, 1100.0, 1300.0, 1500.0];
        assert_eq!(
            active_section(&tops, metrics(999.5, 1000.0)),
            SettingsSection::About
        );
        // A page that does not scroll is never at its end.
        assert_eq!(
            active_section(&tops, metrics(0.0, 0.0)),
            SettingsSection::Scan
        );
    }

    #[test]
    fn a_click_scrolls_the_section_to_the_margin_within_the_range() {
        assert_eq!(scroll_target(500.0, 80.0, metrics(0.0, 1000.0)), 404.0);
        assert_eq!(scroll_target(50.0, 80.0, metrics(0.0, 1000.0)), 0.0);
        assert_eq!(scroll_target(2000.0, 80.0, metrics(100.0, 1000.0)), 1000.0);
        // A header taller than the margin keeps the section below it.
        assert_eq!(scroll_target(500.0, 120.0, metrics(0.0, 1000.0)), 380.0);
    }
}
