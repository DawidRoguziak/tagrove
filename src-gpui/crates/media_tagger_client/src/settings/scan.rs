//! The scan section's pure parts (`createScanSettings.ts`, `ScanSettingsSection.tsx`):
//! the folder picker's selection, the texts of added folders, and the roots the list
//! shows before "Show more".

use media_tagger_core::models::ScanRoot;

use crate::i18n::{I18n, Params};

/// How many roots the list shows until it is expanded.
pub const COLLAPSED_ROOTS: usize = 3;

/// `normalizeFolderSelection`: the chosen folders without repeats, in the order they
/// were chosen; `None` for a cancelled or empty choice.
pub fn normalize_folder_selection(selection: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut unique: Vec<String> = Vec::new();
    for path in selection? {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    (!unique.is_empty()).then_some(unique)
}

/// The roots of `next` that `previous` did not have, in `next`'s order.
pub fn added_paths(previous: &[String], next: &[ScanRoot]) -> Vec<String> {
    next.iter()
        .filter(|root| !previous.contains(&root.path))
        .map(|root| root.path.clone())
        .collect()
}

fn count(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// `formatFolderAddMessage`.
pub fn folder_add_message(
    i18n: &I18n<'_>,
    selected: &[String],
    added: &[String],
    skipped: usize,
) -> String {
    match (added.len(), skipped) {
        (1, 0) => {
            let path = if selected.len() == 1 {
                &selected[0]
            } else {
                &added[0]
            };
            i18n.t_with(
                "settings.actions.summary.folderAdded",
                &Params::new().with("path", path),
            )
        }
        (1, skipped) => i18n.t_with(
            "settings.actions.summary.folderAddedWithSkipped",
            &Params::new()
                .with("path", &added[0])
                .with("skipped", skipped),
        ),
        (n, 0) if n > 1 => i18n.t_with(
            "settings.actions.summary.foldersAdded",
            &Params::new().with_count(count(n)),
        ),
        (n, skipped) if n > 1 => i18n.t_with(
            "settings.actions.summary.foldersAddedWithSkipped",
            &Params::new().with_count(count(n)).with("skipped", skipped),
        ),
        (_, skipped) if skipped > 0 => i18n.t_with(
            "settings.actions.summary.noNewFoldersAddedWithSkipped",
            &Params::new().with("skipped", skipped),
        ),
        _ => i18n.t("settings.actions.summary.noNewFoldersAdded"),
    }
}

/// The roots the list shows: all of them when expanded, else the first three.
pub fn visible_roots(roots: &[ScanRoot], expanded: bool) -> &[ScanRoot] {
    if expanded {
        roots
    } else {
        &roots[..roots.len().min(COLLAPSED_ROOTS)]
    }
}

/// Whether the list has a "Show more"/"Show less" toggle.
pub fn has_expand_toggle(roots: &[ScanRoot]) -> bool {
    roots.len() > COLLAPSED_ROOTS
}

/// The toggle's text.
pub fn expand_toggle_key(expanded: bool) -> &'static str {
    if expanded {
        "settings.scan.showLess"
    } else {
        "settings.scan.showMore"
    }
}

/// The video tool line (`settings.scan.videoToolAvailable` or `…Unavailable`).
pub fn video_tool_line(i18n: &I18n<'_>, tool: &str, available: bool) -> String {
    i18n.t_with(
        if available {
            "settings.scan.videoToolAvailable"
        } else {
            "settings.scan.videoToolUnavailable"
        },
        &Params::new().with("tool", tool),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn roots(paths: &[&str]) -> Vec<ScanRoot> {
        paths
            .iter()
            .map(|path| ScanRoot {
                path: (*path).to_owned(),
                auto_scan_on_startup: true,
            })
            .collect()
    }

    #[test]
    fn a_selection_loses_repeats_and_an_empty_one_is_a_cancel() {
        assert_eq!(normalize_folder_selection(None), None);
        assert_eq!(normalize_folder_selection(Some(Vec::new())), None);
        assert_eq!(
            normalize_folder_selection(Some(strings(&["/b", "/a", "/b"]))),
            Some(strings(&["/b", "/a"]))
        );
    }

    #[test]
    fn the_add_message_names_one_folder_or_counts_several() {
        let i18n = I18n::new(Language::En);
        assert_eq!(
            folder_add_message(&i18n, &strings(&["/a"]), &strings(&["/a"]), 0),
            "Folder added to scan list: /a"
        );
        assert_eq!(
            folder_add_message(&i18n, &strings(&["/a", "/b"]), &strings(&["/b"]), 1),
            "Folder added to scan list: /b. Skipped existing: 1."
        );
        assert_eq!(
            folder_add_message(&i18n, &strings(&["/a", "/b"]), &strings(&["/a", "/b"]), 0),
            "2 folders added to scan list."
        );
        assert_eq!(
            folder_add_message(&i18n, &strings(&["/a"]), &[], 1),
            "No new folders were added to the scan list. Already attached: 1."
        );
        assert_eq!(
            folder_add_message(&i18n, &[], &[], 0),
            "No new folders were added to the scan list."
        );
    }

    #[test]
    fn the_list_shows_three_roots_until_it_is_expanded() {
        let four = roots(&["/a", "/b", "/c", "/d"]);
        assert_eq!(visible_roots(&four, false).len(), 3);
        assert_eq!(visible_roots(&four, true).len(), 4);
        assert!(has_expand_toggle(&four));
        assert!(!has_expand_toggle(&four[..3]));
        assert_eq!(
            added_paths(&strings(&["/a"]), &four),
            strings(&["/b", "/c", "/d"])
        );
    }
}
