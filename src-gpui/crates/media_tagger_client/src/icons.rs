//! The 22 icons of the Solid frontend (`src/ui/controls/UiIcon.tsx`): their names and
//! where the GPUI adapter keeps each one as an SVG asset.
//!
//! The drawings themselves are files, `media_tagger_gpui/assets/icons/<name>.svg`, one per
//! icon with the stroke style of `UiIcon` (`stroke-width: 1.8`, round caps and joins,
//! `fill: none`). A filled variant (`filled` in `UiIcon`, the active heart) is the same
//! drawing with `fill="currentColor"` on the root; the adapter derives it when it loads
//! the asset. The adapter's tests compare every file with `UiIcon.tsx`.

/// An icon of `UiIconName`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IconName {
    Search,
    Settings,
    Palette,
    Languages,
    ArrowLeft,
    Tag,
    Group,
    Info,
    Copy,
    Reset,
    WindowMinimize,
    WindowMaximize,
    WindowRestore,
    Fullscreen,
    Close,
    Heart,
    CheckCircle,
    CheckSquare,
    BulkActions,
    GripVertical,
    Folder,
    Trash,
}

impl IconName {
    /// Every icon, in the order of `UiIcon.tsx`.
    pub const ALL: [Self; 22] = [
        Self::Search,
        Self::Settings,
        Self::Palette,
        Self::Languages,
        Self::ArrowLeft,
        Self::Tag,
        Self::Group,
        Self::Info,
        Self::Copy,
        Self::Reset,
        Self::WindowMinimize,
        Self::WindowMaximize,
        Self::WindowRestore,
        Self::Fullscreen,
        Self::Close,
        Self::Heart,
        Self::CheckCircle,
        Self::CheckSquare,
        Self::BulkActions,
        Self::GripVertical,
        Self::Folder,
        Self::Trash,
    ];

    /// The name of `UiIconName`, which is also the file name of the asset.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Settings => "settings",
            Self::Palette => "palette",
            Self::Languages => "languages",
            Self::ArrowLeft => "arrow-left",
            Self::Tag => "tag",
            Self::Group => "group",
            Self::Info => "info",
            Self::Copy => "copy",
            Self::Reset => "reset",
            Self::WindowMinimize => "window-minimize",
            Self::WindowMaximize => "window-maximize",
            Self::WindowRestore => "window-restore",
            Self::Fullscreen => "fullscreen",
            Self::Close => "close",
            Self::Heart => "heart",
            Self::CheckCircle => "check-circle",
            Self::CheckSquare => "check-square",
            Self::BulkActions => "bulk-actions",
            Self::GripVertical => "grip-vertical",
            Self::Folder => "folder",
            Self::Trash => "trash",
        }
    }

    /// The icon called `name` in `UiIconName`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|icon| icon.name() == name)
    }

    /// The asset path the adapter's asset source answers: `icons/<name>.svg`, or
    /// `icons/filled/<name>.svg` for the filled variant.
    pub fn asset_path(self, filled: bool) -> String {
        if filled {
            format!("{FILLED_ASSET_DIR}/{}.svg", self.name())
        } else {
            format!("{ASSET_DIR}/{}.svg", self.name())
        }
    }

    /// The icon and variant of an asset path made by [`Self::asset_path`].
    pub fn from_asset_path(path: &str) -> Option<(Self, bool)> {
        let (filled, file) = match path.strip_prefix(FILLED_ASSET_DIR) {
            Some(rest) => (true, rest.strip_prefix('/')?),
            None => (false, path.strip_prefix(ASSET_DIR)?.strip_prefix('/')?),
        };
        Self::from_name(file.strip_suffix(".svg")?).map(|icon| (icon, filled))
    }
}

/// Where the plain drawings live in the adapter's assets.
pub const ASSET_DIR: &str = "icons";
/// Where the filled variants are served from.
pub const FILLED_ASSET_DIR: &str = "icons/filled";

/// The `filled` variant of a drawing: `fill="none"` on the root becomes
/// `fill="currentColor"` (`fill-current` on the `svg` of `UiIcon`); children keep their
/// own `fill`. `None` when the root has no `fill="none"`.
pub fn filled_variant(svg: &str) -> Option<String> {
    let root_end = svg.find('>')?;
    let (root, rest) = svg.split_at(root_end);
    let at = root.find(r#"fill="none""#)?;
    Some(format!(
        r#"{}fill="currentColor"{}{rest}"#,
        &root[..at],
        &root[at + r#"fill="none""#.len()..]
    ))
}

#[cfg(test)]
mod tests {
    use super::{filled_variant, IconName};

    #[test]
    fn there_are_the_22_icons_of_ui_icon_with_unique_names() {
        let mut names: Vec<_> = IconName::ALL.iter().map(|icon| icon.name()).collect();
        assert_eq!(names.len(), 22);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 22);
        for icon in IconName::ALL {
            assert_eq!(IconName::from_name(icon.name()), Some(icon));
        }
        assert_eq!(IconName::from_name("Search"), None);
    }

    #[test]
    fn asset_paths_round_trip_for_both_variants() {
        for icon in IconName::ALL {
            for filled in [false, true] {
                let path = icon.asset_path(filled);
                assert_eq!(
                    IconName::from_asset_path(&path),
                    Some((icon, filled)),
                    "{path}"
                );
            }
        }
        assert_eq!(IconName::Heart.asset_path(false), "icons/heart.svg");
        assert_eq!(IconName::Heart.asset_path(true), "icons/filled/heart.svg");
        for unknown in [
            "icons/nope.svg",
            "icons/heart.png",
            "heart.svg",
            "icons/filled/",
            "",
        ] {
            assert_eq!(IconName::from_asset_path(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn the_filled_variant_fills_only_the_root() {
        let svg = r#"<svg fill="none" stroke="currentColor"><circle fill="currentColor" stroke="none"/><path d="M0"/></svg>"#;
        assert_eq!(
            filled_variant(svg).as_deref(),
            Some(
                r#"<svg fill="currentColor" stroke="currentColor"><circle fill="currentColor" stroke="none"/><path d="M0"/></svg>"#
            )
        );
        assert_eq!(filled_variant("<svg><path fill=\"none\"/></svg>"), None);
    }
}
