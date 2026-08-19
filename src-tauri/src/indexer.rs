use std::{
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
    let _ = visit_supported_files(folder, |path, kind| {
        files.push((path, kind));
        Ok(())
    });
    files
}

pub fn visit_supported_files(
    folder: &Path,
    mut visitor: impl FnMut(PathBuf, String) -> anyhow::Result<()>,
) -> anyhow::Result<DiscoveryReport> {
    let mut discovered = 0usize;
    let mut traversal_errors = 0usize;
    for entry in walkdir::WalkDir::new(folder).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                traversal_errors += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let Some(kind) = detect_kind(&path) else {
            continue;
        };
        visitor(path, kind.to_string())?;
        discovered += 1;
    }
    Ok(DiscoveryReport {
        discovered,
        traversal_errors,
        completeness: if traversal_errors == 0 {
            DiscoveryCompleteness::Complete
        } else {
            DiscoveryCompleteness::Partial
        },
    })
}

pub fn scan_one(path: PathBuf, kind: String, ffmpeg_path: &Path) -> IndexResult {
    let result = read_file_fingerprint(&path)
        .and_then(|fingerprint| build_asset(path, &kind, ffmpeg_path, fingerprint));
    match result {
        Ok(item) => IndexResult {
            item: Some(item),
            failed: 0,
        },
        Err(_) => IndexResult {
            item: None,
            failed: 1,
        },
    }
}

pub fn scan_one_with_fingerprint(
    path: PathBuf,
    kind: String,
    ffmpeg_path: &Path,
    fingerprint: FileFingerprint,
) -> IndexResult {
    match build_asset(path, &kind, ffmpeg_path, fingerprint) {
        Ok(item) => IndexResult {
            item: Some(item),
            failed: 0,
        },
        Err(_) => IndexResult {
            item: None,
            failed: 1,
        },
    }
}

fn build_asset(
    path: PathBuf,
    kind: &str,
    ffmpeg_path: &Path,
    fingerprint: FileFingerprint,
) -> anyhow::Result<IndexedItem> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Skipping media path that is not valid UTF-8"))?
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

    use super::{collect_supported_files, detect_kind, scan_one};

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
    }
}
