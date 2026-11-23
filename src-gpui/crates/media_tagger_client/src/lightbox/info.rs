//! The texts the lightbox makes of the selected asset's metadata: the summary of the
//! action rail (`LightboxActionRail.tsx`), the rows of the Information panel
//! (`LightboxInfoPanel.tsx`) and the lines of the video placeholder; with the two
//! formatters of `src/utils/media.ts` they use.

use super::{media::StageKind, model::SelectedAsset};
use crate::i18n::I18n;

/// `formatBytes`: bytes below 1024, then KB, MB, GB and TB with one decimal below ten
/// and none from ten on.
pub fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    value /= 1024.0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    // `toFixed` rounds a half up; Rust's formatting rounds it to even. The value is a
    // whole number of bytes over a power of two, so its tenths are exact.
    if value >= 10.0 {
        format!("{} {}", value.round(), UNITS[unit])
    } else {
        format!("{:.1} {}", (value * 10.0).round() / 10.0, UNITS[unit])
    }
}

/// `formatDuration`: minutes and two-digit seconds; `-` without a positive duration.
pub fn format_duration(duration_ms: Option<i64>) -> String {
    let Some(duration_ms) = duration_ms.filter(|duration| *duration > 0) else {
        return "-".to_owned();
    };
    let total_seconds = duration_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

/// The summary between the buttons of the action rail: the dimensions when both are
/// known, the size once the details are read, and the kind in capitals. The first
/// entry is drawn in the primary text colour.
pub fn media_summary(selected: &SelectedAsset, i18n: &I18n<'_>) -> Vec<String> {
    let summary = &selected.summary;
    let mut parts = Vec::with_capacity(3);
    if let (Some(width), Some(height)) = (summary.width, summary.height) {
        parts.push(format!("{width}\u{d7}{height}"));
    }
    if let Some(bytes) = selected.size_bytes {
        parts.push(format_bytes(bytes));
    }
    parts.push(
        i18n.t(StageKind::of(&summary.kind).label_key())
            .to_uppercase(),
    );
    parts
}

/// One row of the Information panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoRow {
    /// The label without its colon.
    pub label: String,
    pub value: String,
    /// The path row: its label is not set in bold and its value may break anywhere.
    pub path: bool,
}

/// The rows of the Information panel: the type, the size once known, the dimensions
/// when both are known, the duration when the asset has one, and the path (the file
/// name until the details are read).
pub fn info_rows(selected: &SelectedAsset, i18n: &I18n<'_>) -> Vec<InfoRow> {
    let summary = &selected.summary;
    let row = |label: &str, value: String| InfoRow {
        label: i18n.t(label),
        value,
        path: false,
    };
    let mut rows = vec![row(
        "lightbox.type",
        i18n.t(StageKind::of(&summary.kind).label_key()),
    )];
    if let Some(bytes) = selected.size_bytes {
        rows.push(row("lightbox.size", format_bytes(bytes)));
    }
    if let (Some(width), Some(height)) = (summary.width, summary.height) {
        rows.push(row("lightbox.dimensions", format!("{width} x {height}")));
    }
    if summary.duration_ms.is_some() {
        rows.push(row(
            "lightbox.duration",
            format_duration(summary.duration_ms),
        ));
    }
    rows.push(InfoRow {
        label: i18n.t("lightbox.path"),
        value: selected
            .path
            .clone()
            .unwrap_or_else(|| summary.file_name.clone()),
        path: true,
    });
    rows
}

/// The metadata lines of the video placeholder: dimensions, duration and size, each
/// only when known.
pub fn video_placeholder_lines(selected: &SelectedAsset, i18n: &I18n<'_>) -> Vec<String> {
    let summary = &selected.summary;
    let line = |label: &str, value: String| format!("{}: {value}", i18n.t(label));
    let mut lines = Vec::with_capacity(3);
    if let (Some(width), Some(height)) = (summary.width, summary.height) {
        lines.push(line("lightbox.dimensions", format!("{width} x {height}")));
    }
    if summary.duration_ms.is_some_and(|duration| duration > 0) {
        lines.push(line(
            "lightbox.duration",
            format_duration(summary.duration_ms),
        ));
    }
    if let Some(bytes) = selected.size_bytes {
        lines.push(line("lightbox.size", format_bytes(bytes)));
    }
    lines
}

#[cfg(test)]
mod tests {
    use media_tagger_core::models::AssetSummary;

    use super::*;
    use crate::i18n::Language;

    fn selected(kind: &str) -> SelectedAsset {
        SelectedAsset {
            summary: AssetSummary {
                id: 1,
                file_name: "clip.mp4".into(),
                preview_path: None,
                kind: kind.into(),
                modified_at: 0,
                width: Some(160),
                height: Some(90),
                duration_ms: Some(8_000),
                thumb_version: "0123456789abcdef".into(),
                is_favorite: false,
                media_group_key: None,
                media_group_order: None,
            },
            path: None,
            size_bytes: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn bytes_are_formatted_as_the_solid_helper_does() {
        for (bytes, text) in [
            (0, "0 B"),
            (1023, "1023 B"),
            (1024, "1.0 KB"),
            (1536, "1.5 KB"),
            // An exact half at the one decimal goes up, as `toFixed(1)` takes it.
            (1280, "1.3 KB"),
            (2304, "2.3 KB"),
            (1_310_720, "1.3 MB"),
            // Just under ten keeps its decimal, also when it rounds to ten.
            (10_188, "9.9 KB"),
            (10_189, "10.0 KB"),
            (10 * 1024, "10 KB"),
            (10 * 1024 + 512, "11 KB"),
            (5 * 1024 * 1024, "5.0 MB"),
            (3 * 1024 * 1024 * 1024, "3.0 GB"),
            (2048 * 1024 * 1024 * 1024 * 1024, "2048 TB"),
        ] {
            assert_eq!(format_bytes(bytes), text, "{bytes}");
        }
    }

    #[test]
    fn durations_are_minutes_and_two_digit_seconds() {
        assert_eq!(format_duration(None), "-");
        assert_eq!(format_duration(Some(0)), "-");
        assert_eq!(format_duration(Some(-5)), "-");
        assert_eq!(format_duration(Some(999)), "0:00");
        assert_eq!(format_duration(Some(8_000)), "0:08");
        assert_eq!(format_duration(Some(125_900)), "2:05");
        assert_eq!(format_duration(Some(3_600_000)), "60:00");
    }

    #[test]
    fn the_rail_summary_and_the_information_rows_follow_what_is_known() {
        let i18n = I18n::new(Language::En);
        let mut asset = selected("video");
        assert_eq!(media_summary(&asset, &i18n), ["160\u{d7}90", "VIDEO"]);
        let rows = info_rows(&asset, &i18n);
        let texts: Vec<_> = rows
            .iter()
            .map(|row| format!("{}: {}", row.label, row.value))
            .collect();
        assert_eq!(
            texts,
            [
                "Type: Video",
                "Dimensions: 160 x 90",
                "Duration: 0:08",
                "Path: clip.mp4"
            ]
        );
        assert!(rows.last().is_some_and(|row| row.path));
        assert_eq!(
            video_placeholder_lines(&asset, &i18n),
            ["Dimensions: 160 x 90", "Duration: 0:08"]
        );

        asset.path = Some("/media/clip.mp4".into());
        asset.size_bytes = Some(2048);
        asset.summary.width = None;
        asset.summary.duration_ms = None;
        assert_eq!(media_summary(&asset, &i18n), ["2.0 KB", "VIDEO"]);
        let texts: Vec<_> = info_rows(&asset, &i18n)
            .into_iter()
            .map(|row| format!("{}: {}", row.label, row.value))
            .collect();
        assert_eq!(
            texts,
            ["Type: Video", "Size: 2.0 KB", "Path: /media/clip.mp4"]
        );
        assert_eq!(video_placeholder_lines(&asset, &i18n), ["Size: 2.0 KB"]);
    }
}
