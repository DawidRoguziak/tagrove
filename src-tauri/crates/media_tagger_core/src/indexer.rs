use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::models::NewAsset;

#[derive(Debug)]
pub struct IndexedItem {
    pub asset: NewAsset,
    pub fingerprint_mtime_ns: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct FileFingerprint {
    pub size_bytes: i64,
    pub modified_at: i64,
    pub modified_at_ns: i64,
}

#[derive(Debug)]
pub struct IndexResult {
    pub item: Option<IndexedItem>,
    pub failed: usize,
    /// Why the file could not be indexed, for the scan warning log.
    pub warning: Option<String>,
}

/// At most this many warnings are written to stderr for one scan root; the
/// rest are only counted, then summarized once when the root finishes.
pub const MAX_LOGGED_SCAN_WARNINGS: usize = 20;

/// Non-blocking per-file scan warnings on stderr, capped per root so a folder
/// with many bad entries cannot flood the log. Logging never decides whether a
/// root is complete; the callers' counters do. Counters are cells so discovery
/// and its visitor can share one log for the same root.
#[derive(Debug, Default)]
pub struct ScanWarningLog {
    logged: Cell<usize>,
    suppressed: Cell<usize>,
}

impl ScanWarningLog {
    pub fn warn(&self, message: impl FnOnce() -> String) {
        if self.logged.get() < MAX_LOGGED_SCAN_WARNINGS {
            self.logged.set(self.logged.get() + 1);
            eprintln!("[scan] warning: {}", message());
        } else {
            self.suppressed.set(self.suppressed.get() + 1);
        }
    }

    /// Summarizes the warnings past the cap and resets the log for the next root.
    pub fn finish_root(&self, root: &str) {
        let suppressed = self.suppressed.replace(0);
        if suppressed > 0 {
            eprintln!("[scan] warning: {suppressed} more warning(s) under {root} were not shown");
        }
        self.logged.set(0);
    }

    #[cfg(test)]
    pub fn counts(&self) -> (usize, usize) {
        (self.logged.get(), self.suppressed.get())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryCompleteness {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Copy)]
pub struct DiscoveryReport {
    pub discovered: usize,
    pub traversal_errors: usize,
    /// Supported files skipped because their path is not valid UTF-8. Asset
    /// paths are stored as UTF-8 text, so no row can exist for such a file and
    /// skipping it does not make the root partial.
    pub non_utf8_skipped: usize,
    pub completeness: DiscoveryCompleteness,
}

impl DiscoveryReport {
    pub fn is_complete(self) -> bool {
        self.completeness == DiscoveryCompleteness::Complete
    }
}

#[cfg(test)]
pub fn collect_supported_files(folder: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    let _ = visit_supported_files(folder, &ScanWarningLog::default(), |path, kind| {
        files.push((path, kind));
        Ok(())
    });
    files
}

pub fn visit_supported_files(
    folder: &Path,
    warnings: &ScanWarningLog,
    mut visitor: impl FnMut(PathBuf, String) -> anyhow::Result<()>,
) -> anyhow::Result<DiscoveryReport> {
    let mut discovered = 0usize;
    let mut traversal_errors = 0usize;
    let mut non_utf8_skipped = 0usize;
    for entry in walkdir::WalkDir::new(folder).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                traversal_errors += 1;
                warnings.warn(|| format!("cannot read a folder entry: {error}"));
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(kind) = detect_kind(entry.path()) else {
            continue;
        };
        if entry.path().to_str().is_none() {
            non_utf8_skipped += 1;
            warnings.warn(|| {
                format!(
                    "skipped a media file whose path is not valid UTF-8: {}",
                    entry.path().display()
                )
            });
            continue;
        }
        visitor(entry.into_path(), kind.to_string())?;
        discovered += 1;
    }
    Ok(DiscoveryReport {
        discovered,
        traversal_errors,
        non_utf8_skipped,
        completeness: if traversal_errors == 0 {
            DiscoveryCompleteness::Complete
        } else {
            DiscoveryCompleteness::Partial
        },
    })
}

pub fn scan_one(path: PathBuf, kind: String, ffmpeg_path: &Path) -> IndexResult {
    let result = read_file_fingerprint(&path)
        .and_then(|fingerprint| build_asset(&path, &kind, ffmpeg_path, fingerprint));
    index_result(&path, result)
}

pub fn scan_one_with_fingerprint(
    path: PathBuf,
    kind: String,
    ffmpeg_path: &Path,
    fingerprint: FileFingerprint,
) -> IndexResult {
    let result = build_asset(&path, &kind, ffmpeg_path, fingerprint);
    index_result(&path, result)
}

fn index_result(path: &Path, result: anyhow::Result<IndexedItem>) -> IndexResult {
    match result {
        Ok(item) => IndexResult {
            item: Some(item),
            failed: 0,
            warning: None,
        },
        Err(error) => IndexResult {
            item: None,
            failed: 1,
            warning: Some(format!("cannot index {}: {error}", path.display())),
        },
    }
}

fn build_asset(
    path: &Path,
    kind: &str,
    ffmpeg_path: &Path,
    fingerprint: FileFingerprint,
) -> anyhow::Result<IndexedItem> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("the path is not valid UTF-8"))?
        .to_string();
    let filesystem_path = Path::new(&path);
    let mut width = None;
    let mut height = None;
    let mut duration_ms = None;

    if kind == "image" || kind == "gif" {
        if let Ok((w, h)) = image::image_dimensions(filesystem_path) {
            width = Some(w as i64);
            height = Some(h as i64);
        }
    } else if kind == "video" {
        duration_ms = crate::thumbs::probe_video_duration_ms(ffmpeg_path, filesystem_path);
    }

    Ok(IndexedItem {
        asset: NewAsset {
            path,
            kind: kind.to_string(),
            size_bytes: fingerprint.size_bytes,
            modified_at: fingerprint.modified_at,
            width,
            height,
            duration_ms,
            thumb_path: None,
        },
        fingerprint_mtime_ns: fingerprint.modified_at_ns,
    })
}

pub fn read_file_fingerprint(path: &Path) -> anyhow::Result<FileFingerprint> {
    let metadata = fs::metadata(path)?;
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    Ok(FileFingerprint {
        size_bytes: metadata.len() as i64,
        modified_at: modified.as_secs() as i64,
        modified_at_ns: modified.as_nanos().min(i64::MAX as u128) as i64,
    })
}

pub fn detect_kind(path: &Path) -> Option<&'static str> {
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())?;

    match extension.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" => Some("image"),
        "gif" => Some("gif"),
        "mp4" | "mkv" | "webm" | "mov" | "avi" => Some("video"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use super::{
        collect_supported_files, detect_kind, scan_one, scan_one_with_fingerprint,
        visit_supported_files, DiscoveryCompleteness, FileFingerprint, ScanWarningLog,
        MAX_LOGGED_SCAN_WARNINGS,
    };

    #[test]
    fn detect_kind_matches_supported_extensions_case_insensitively() {
        assert_eq!(detect_kind(Path::new("a.JPEG")), Some("image"));
        assert_eq!(detect_kind(Path::new("a.GiF")), Some("gif"));
        assert_eq!(detect_kind(Path::new("a.MP4")), Some("video"));
        assert_eq!(detect_kind(Path::new("a.txt")), None);
    }

    #[test]
    fn collect_supported_files_returns_only_supported_media_extensions() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested");

        fs::write(root.join("a.jpg"), b"a").expect("write a.jpg");
        fs::write(root.join("b.GIF"), b"b").expect("write b.GIF");
        fs::write(root.join("c.mp4"), b"c").expect("write c.mp4");
        fs::write(root.join("d.txt"), b"d").expect("write d.txt");
        fs::write(nested.join("e.webp"), b"e").expect("write e.webp");

        let mut discovered = collect_supported_files(root)
            .into_iter()
            .map(|(path, kind)| {
                (
                    path.file_name()
                        .expect("file name")
                        .to_string_lossy()
                        .to_string(),
                    kind,
                )
            })
            .collect::<Vec<_>>();
        discovered.sort_by(|left, right| left.0.cmp(&right.0));

        assert_eq!(
            discovered,
            vec![
                ("a.jpg".to_string(), "image".to_string()),
                ("b.GIF".to_string(), "gif".to_string()),
                ("c.mp4".to_string(), "video".to_string()),
                ("e.webp".to_string(), "image".to_string())
            ]
        );
    }

    #[test]
    fn scan_one_returns_item_for_existing_file_and_failure_for_missing_file() {
        let tmp = tempdir().expect("tempdir");
        let existing = tmp.path().join("photo.jpg");
        fs::write(&existing, b"not-an-image").expect("write existing file");

        let success = scan_one(existing.clone(), "image".to_string(), Path::new("ffmpeg"));
        assert_eq!(success.failed, 0);
        let item = success.item.expect("indexed item");
        assert_eq!(item.asset.path, existing.to_string_lossy().to_string());
        assert_eq!(item.asset.kind, "image");
        assert!(item.asset.size_bytes > 0);

        let missing = tmp.path().join("missing.jpg");
        let failure = scan_one(missing, "image".to_string(), Path::new("ffmpeg"));
        assert_eq!(failure.failed, 1);
        assert!(failure.item.is_none());
        let warning = failure.warning.expect("failure warning");
        assert!(warning.contains("missing.jpg"), "{warning}");
        assert!(success.warning.is_none());
    }

    #[cfg(unix)]
    fn non_utf8_name(prefix: &[u8], suffix: &[u8]) -> std::ffi::OsString {
        use std::os::unix::ffi::OsStringExt;
        // 0xE9 is Latin-1 "e acute", an invalid UTF-8 byte on its own.
        let mut bytes = prefix.to_vec();
        bytes.push(0xE9);
        bytes.extend_from_slice(suffix);
        std::ffi::OsString::from_vec(bytes)
    }

    #[cfg(unix)]
    #[test]
    fn discovery_skips_non_utf8_media_paths_without_making_the_root_partial() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        fs::write(root.join("kept.jpg"), b"a").expect("write kept.jpg");
        fs::write(root.join(non_utf8_name(b"caf", b".jpg")), b"b").expect("write non-UTF-8 file");
        // Every file below a non-UTF-8 folder has a non-UTF-8 path too.
        let folder = root.join(non_utf8_name(b"alb", b"um"));
        fs::create_dir_all(&folder).expect("create non-UTF-8 folder");
        fs::write(folder.join("inside.png"), b"c").expect("write file in non-UTF-8 folder");
        // Unsupported files are invisible, not skipped.
        fs::write(root.join(non_utf8_name(b"notes", b".txt")), b"d").expect("write text file");

        let warnings = ScanWarningLog::default();
        let mut visited = Vec::new();
        let report = visit_supported_files(root, &warnings, |path, kind| {
            visited.push((path, kind));
            Ok(())
        })
        .expect("discovery");

        assert_eq!(visited, vec![(root.join("kept.jpg"), "image".to_string())]);
        assert_eq!(report.discovered, 1);
        assert_eq!(report.non_utf8_skipped, 2);
        assert_eq!(report.traversal_errors, 0);
        assert_eq!(report.completeness, DiscoveryCompleteness::Complete);
        assert_eq!(warnings.counts(), (2, 0));
    }

    #[cfg(unix)]
    #[test]
    fn indexing_a_non_utf8_path_fails_with_a_warning() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join(non_utf8_name(b"caf", b".jpg"));
        fs::write(&path, b"a").expect("write non-UTF-8 file");
        let result = scan_one_with_fingerprint(
            path,
            "image".to_string(),
            Path::new("ffmpeg"),
            FileFingerprint {
                size_bytes: 1,
                modified_at: 1,
                modified_at_ns: 1,
            },
        );
        assert_eq!(result.failed, 1);
        assert!(result.item.is_none());
        let warning = result.warning.expect("warning");
        assert!(warning.contains("not valid UTF-8"), "{warning}");
    }

    #[test]
    fn scan_warning_log_caps_lines_per_root_and_resets_after_the_root() {
        let warnings = ScanWarningLog::default();
        let mut built = 0;
        for _ in 0..MAX_LOGGED_SCAN_WARNINGS + 5 {
            warnings.warn(|| {
                built += 1;
                "synthetic warning".to_string()
            });
        }
        // Suppressed warnings do not even format their message.
        assert_eq!(built, MAX_LOGGED_SCAN_WARNINGS);
        assert_eq!(warnings.counts(), (MAX_LOGGED_SCAN_WARNINGS, 5));
        warnings.finish_root("/synthetic/root");
        assert_eq!(warnings.counts(), (0, 0));
        warnings.warn(|| "next root".to_string());
        assert_eq!(warnings.counts(), (1, 0));
    }
}
