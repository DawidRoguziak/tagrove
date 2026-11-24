//! The 22 icons of the Solid frontend (`src/ui/controls/UiIcon.tsx`): their names and
//! where the GPUI adapter keeps each one as an SVG asset.
//!
//! The drawings themselves are files, `media_tagger_gpui/assets/icons/<name>.svg`, one per
//! icon with the stroke style of `UiIcon` (`stroke-width: 1.8`, round caps and joins,
//! `fill: none`). A filled variant (`filled` in `UiIcon`, the active heart) is the same
//! drawing with `fill="currentColor"` on the root, and a heavy variant (the
//! `[stroke-width:2.4]` of the remove and clear crosses) is the same drawing with that
//! stroke width; the adapter derives both when it loads the asset. The adapter's tests
//! compare every file with `UiIcon.tsx`.

/// How an icon is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IconVariant {
    /// The drawing as `UiIcon` strokes it.
    #[default]
    Plain,
    /// `filled` of `UiIcon`.
    Filled,
    /// `[stroke-width:2.4]` in the icon's class.
    Heavy,
}

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
        self.variant_asset_path(if filled {
            IconVariant::Filled
        } else {
            IconVariant::Plain
        })
    }

    /// The asset path of `variant`: `icons/<name>.svg`, `icons/filled/<name>.svg` or
    /// `icons/heavy/<name>.svg`.
    pub fn variant_asset_path(self, variant: IconVariant) -> String {
        let directory = match variant {
            IconVariant::Plain => ASSET_DIR,
            IconVariant::Filled => FILLED_ASSET_DIR,
            IconVariant::Heavy => HEAVY_ASSET_DIR,
        };
        format!("{directory}/{}.svg", self.name())
    }

    /// The icon and variant of an asset path made by [`Self::variant_asset_path`].
    pub fn from_asset_path(path: &str) -> Option<(Self, IconVariant)> {
        let (variant, file) = if let Some(rest) = path.strip_prefix(FILLED_ASSET_DIR) {
            (IconVariant::Filled, rest.strip_prefix('/')?)
        } else if let Some(rest) = path.strip_prefix(HEAVY_ASSET_DIR) {
            (IconVariant::Heavy, rest.strip_prefix('/')?)
        } else {
            (
                IconVariant::Plain,
                path.strip_prefix(ASSET_DIR)?.strip_prefix('/')?,
            )
        };
        Self::from_name(file.strip_suffix(".svg")?).map(|icon| (icon, variant))
    }
}

/// Where the plain drawings live in the adapter's assets.
pub const ASSET_DIR: &str = "icons";
/// Where the filled variants are served from.
pub const FILLED_ASSET_DIR: &str = "icons/filled";
/// Where the heavy variants are served from.
pub const HEAVY_ASSET_DIR: &str = "icons/heavy";

/// `stroke-width` of `UiIcon` and of an icon whose class carries `[stroke-width:2.4]`.
pub const STROKE_WIDTH: &str = "1.8";
pub const HEAVY_STROKE_WIDTH: &str = "2.4";

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

/// The heavy variant of a drawing: the root's stroke width becomes
/// [`HEAVY_STROKE_WIDTH`]. `None` when the root does not carry [`STROKE_WIDTH`].
pub fn heavy_variant(svg: &str) -> Option<String> {
    let root_end = svg.find('>')?;
    let (root, rest) = svg.split_at(root_end);
    let attribute = format!(r#"stroke-width="{STROKE_WIDTH}""#);
    let at = root.find(&attribute)?;
    Some(format!(
        r#"{}stroke-width="{HEAVY_STROKE_WIDTH}"{}{rest}"#,
        &root[..at],
        &root[at + attribute.len()..]
    ))
}

#[cfg(test)]
mod tests {
    use super::{filled_variant, heavy_variant, IconName, IconVariant};

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
            for variant in [IconVariant::Plain, IconVariant::Filled, IconVariant::Heavy] {
                let path = icon.variant_asset_path(variant);
                assert_eq!(
                    IconName::from_asset_path(&path),
                    Some((icon, variant)),
                    "{path}"
                );
            }
        }
        assert_eq!(IconName::Heart.asset_path(false), "icons/heart.svg");
        assert_eq!(IconName::Heart.asset_path(true), "icons/filled/heart.svg");
        assert_eq!(
            IconName::Close.variant_asset_path(IconVariant::Heavy),
            "icons/heavy/close.svg"
        );
        for unknown in [
            "icons/nope.svg",
            "icons/heart.png",
            "heart.svg",
            "icons/filled/",
            "icons/heavy/",
            "icons/heavy/nope.svg",
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

    #[test]
    fn the_heavy_variant_changes_only_the_stroke_width_of_the_root() {
        let svg = r#"<svg fill="none" stroke-width="1.8"><path stroke-width="1.8" d="M0"/></svg>"#;
        assert_eq!(
            heavy_variant(svg).as_deref(),
            Some(r#"<svg fill="none" stroke-width="2.4"><path stroke-width="1.8" d="M0"/></svg>"#)
        );
        assert_eq!(
            heavy_variant("<svg><path stroke-width=\"1.8\"/></svg>"),
            None
        );
    }
}
