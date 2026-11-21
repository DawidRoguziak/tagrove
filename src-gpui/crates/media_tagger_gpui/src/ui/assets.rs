//! The assets of the adapter: the 22 icons of `UiIcon.tsx` as SVG files in
//! `assets/icons/`, built into the binary. `media_tagger_client::icons` names them; the
//! filled variant of an icon is derived from its file when it is loaded.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};
use media_tagger_client::icons::{filled_variant, IconName};

/// Every icon file, by the name of `UiIconName`.
const ICON_FILES: [(&str, &[u8]); 22] = [
    ("search", include_bytes!("../../assets/icons/search.svg")),
    (
        "settings",
        include_bytes!("../../assets/icons/settings.svg"),
    ),
    ("palette", include_bytes!("../../assets/icons/palette.svg")),
    (
        "languages",
        include_bytes!("../../assets/icons/languages.svg"),
    ),
    (
        "arrow-left",
        include_bytes!("../../assets/icons/arrow-left.svg"),
    ),
    ("tag", include_bytes!("../../assets/icons/tag.svg")),
    ("group", include_bytes!("../../assets/icons/group.svg")),
    ("info", include_bytes!("../../assets/icons/info.svg")),
    ("copy", include_bytes!("../../assets/icons/copy.svg")),
    ("reset", include_bytes!("../../assets/icons/reset.svg")),
    (
        "window-minimize",
        include_bytes!("../../assets/icons/window-minimize.svg"),
    ),
    (
        "window-maximize",
        include_bytes!("../../assets/icons/window-maximize.svg"),
    ),
    (
        "window-restore",
        include_bytes!("../../assets/icons/window-restore.svg"),
    ),
    (
        "fullscreen",
        include_bytes!("../../assets/icons/fullscreen.svg"),
    ),
    ("close", include_bytes!("../../assets/icons/close.svg")),
    ("heart", include_bytes!("../../assets/icons/heart.svg")),
    (
        "check-circle",
        include_bytes!("../../assets/icons/check-circle.svg"),
    ),
    (
        "check-square",
        include_bytes!("../../assets/icons/check-square.svg"),
    ),
    (
        "bulk-actions",
        include_bytes!("../../assets/icons/bulk-actions.svg"),
    ),
    (
        "grip-vertical",
        include_bytes!("../../assets/icons/grip-vertical.svg"),
    ),
    ("folder", include_bytes!("../../assets/icons/folder.svg")),
    ("trash", include_bytes!("../../assets/icons/trash.svg")),
];

/// The SVG source of an icon.
pub fn icon_svg(icon: IconName) -> &'static str {
    let (_, bytes) = ICON_FILES
        .iter()
        .find(|(name, _)| *name == icon.name())
        .expect("every icon has a file (tested)");
    std::str::from_utf8(bytes).expect("the icon files are UTF-8 (tested)")
}

/// The asset source of the application (`Application::with_assets`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let Some((icon, filled)) = IconName::from_asset_path(path) else {
            return Ok(None);
        };
        let svg = icon_svg(icon);
        Ok(Some(if filled {
            // Every file has `fill="none"` on its root (tested).
            Cow::Owned(
                filled_variant(svg)
                    .unwrap_or_else(|| svg.to_owned())
                    .into_bytes(),
            )
        } else {
            Cow::Borrowed(svg.as_bytes())
        }))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let path = path.trim_end_matches('/');
        Ok(IconName::ALL
            .into_iter()
            .flat_map(|icon| [icon.asset_path(false), icon.asset_path(true)])
            .filter(|asset| {
                path.is_empty()
                    || asset
                        .strip_prefix(path)
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .map(SharedString::from)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use gpui::AssetSource as _;
    use media_tagger_client::icons::{IconName, ASSET_DIR, FILLED_ASSET_DIR};

    use super::{icon_svg, Assets, ICON_FILES};

    /// The drawing elements of each icon in `UiIcon.tsx`, whitespace collapsed.
    fn solid_icons() -> Vec<(String, Vec<String>)> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../src/ui/controls/UiIcon.tsx");
        let source = std::fs::read_to_string(path).expect("read UiIcon.tsx");
        let start = source.find("const ICONS").expect("the ICONS table");
        let end = source[start..].find("} satisfies").expect("its end") + start;
        let table = &source[start..end];
        let mut icons: Vec<(String, Vec<String>)> = Vec::new();
        for line in table.lines() {
            let trimmed = line.trim_end();
            if let Some(key) = trimmed
                .strip_prefix("  ")
                .filter(|rest| !rest.starts_with(' '))
                .and_then(|rest| rest.strip_suffix(": {"))
            {
                icons.push((key.trim_matches('"').to_owned(), Vec::new()));
            }
        }
        // Element by element, in the order of each block.
        let mut cursor = 0;
        let mut current = 0;
        let blocks: Vec<usize> = icons
            .iter()
            .map(|(name, _)| {
                let quoted = format!("  \"{name}\": {{");
                let plain = format!("  {name}: {{");
                table
                    .find(&quoted)
                    .or_else(|| table.find(&plain))
                    .expect("block")
            })
            .collect();
        while let Some(offset) = table[cursor..].find('<') {
            let at = cursor + offset;
            while current + 1 < blocks.len() && blocks[current + 1] < at {
                current += 1;
            }
            let rest = &table[at + 1..];
            let tag: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            if ["path", "circle", "line", "rect"].contains(&tag.as_str()) {
                let close = rest.find("/>").expect("self-closing element");
                let element = collapse(&format!("<{}/>", rest[..close].trim_end()));
                icons[current].1.push(element);
                cursor = at + 1 + close;
            } else {
                cursor = at + 1;
            }
        }
        icons
    }

    fn collapse(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn file_elements(svg: &str) -> Vec<String> {
        svg.lines()
            .map(str::trim)
            .filter(|line| {
                !line.starts_with("<svg") && !line.starts_with("</svg") && !line.is_empty()
            })
            .map(collapse)
            .collect()
    }

    #[test]
    fn every_icon_file_draws_what_ui_icon_draws() {
        let solid = solid_icons();
        assert_eq!(solid.len(), 22);
        let names: Vec<_> = solid.iter().map(|(name, _)| name.as_str()).collect();
        let ours: Vec<_> = IconName::ALL.iter().map(|icon| icon.name()).collect();
        assert_eq!(names, ours, "the same icons in the same order");
        assert_eq!(ICON_FILES.len(), 22);

        for (name, elements) in &solid {
            let icon = IconName::from_name(name).expect("known icon");
            let svg = icon_svg(icon);
            assert!(!elements.is_empty(), "{name}");
            assert_eq!(&file_elements(svg), elements, "{name}");
            assert!(
                svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">"#),
                "{name}: the stroke style of UiIcon"
            );
        }
    }

    #[test]
    fn the_asset_source_serves_both_variants_and_nothing_else() {
        let heart = Assets.load("icons/heart.svg").unwrap().expect("heart");
        assert!(std::str::from_utf8(&heart)
            .unwrap()
            .contains(r#"fill="none""#));
        let filled = Assets
            .load("icons/filled/heart.svg")
            .unwrap()
            .expect("filled heart");
        let filled = std::str::from_utf8(&filled).unwrap();
        assert!(filled.starts_with(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor""#
        ));
        assert!(Assets.load("icons/unknown.svg").unwrap().is_none());
        assert!(Assets.load("fonts/Inter.ttf").unwrap().is_none());

        assert_eq!(Assets.list(ASSET_DIR).unwrap().len(), 44);
        assert_eq!(Assets.list(FILLED_ASSET_DIR).unwrap().len(), 22);
        assert!(Assets.list("nothing").unwrap().is_empty());
    }
}
