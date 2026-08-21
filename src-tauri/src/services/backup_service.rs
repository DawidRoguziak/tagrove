use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use tauri::State;

use crate::{
    app::state::AppState,
    db,
    error::AppResult,
    models::{
        DbBundleExportSummary, DbBundleImportSummary, DbBundleInspection, DbRootMapping,
    },
    services::{asset_query_service, db_pool},
};

const BUNDLE_APPLICATION_ID: &str = "io.github.mediatagger.bundle";
const BUNDLE_FORMAT_VERSION: u32 = 2;
const MAX_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 250_000;
const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 32 * 1024 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 1_000;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BundleManifest {
    format_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    application_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    schema_version: Option<u32>,
    source_platform: String,
    roots: Vec<String>,
    source_thumbs_dir: String,
}

#[derive(Debug, Clone)]
struct ValidatedArchiveEntry {
    index: usize,
    path: PathBuf,
    size: u64,
}

struct ValidatedArchive {
    entries: Vec<ValidatedArchiveEntry>,
    manifest: Option<BundleManifest>,
}

pub fn export_db_bundle(path: String, state: &State<AppState>) -> AppResult<DbBundleExportSummary> {
    let target_file = PathBuf::from(path.trim());
    if target_file.as_os_str().is_empty() {
        return Err("DB export archive path is empty".into());
    }

    if let Some(parent) = target_file.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let conn = db::open_connection(&state.db_path)?;
    db::ensure_no_pending_file_operations(&conn)?;
    let manifest = BundleManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        application_id: Some(BUNDLE_APPLICATION_ID.to_string()),
        schema_version: Some(db::SCHEMA_VERSION as u32),
        source_platform: std::env::consts::OS.to_string(),
        roots: db::list_scan_roots(&conn)?,
        source_thumbs_dir: state.thumbs_dir.to_string_lossy().to_string(),
    };
    let file = fs::File::create(&target_file)?;
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file(
        "manifest.json",
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644),
    )?;
    serde_json::to_writer(&mut zip, &manifest).map_err(|error| error.to_string())?;
    add_file_to_zip(&mut zip, &state.db_path, "media.db")?;
    let mut copied_files = 1usize;

    let wal_path = sqlite_sidecar_path(&state.db_path, "-wal");
    if wal_path.exists() {
        add_file_to_zip(&mut zip, &wal_path, "media.db-wal")?;
        copied_files += 1;
    }

    let shm_path = sqlite_sidecar_path(&state.db_path, "-shm");
    if shm_path.exists() {
        add_file_to_zip(&mut zip, &shm_path, "media.db-shm")?;
        copied_files += 1;
    }

    let mut copied_thumbnails = 0usize;
    if state.thumbs_dir.exists() {
        for entry in walkdir::WalkDir::new(&state.thumbs_dir) {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().is_file() {
                continue;
            }

            let source_path = entry.path();
            let relative = source_path
                .strip_prefix(&state.thumbs_dir)
                .map_err(|e| e.to_string())?;
            let relative_name = relative.to_string_lossy().replace('\\', "/");
            let zip_name = format!("thumbs/{relative_name}");
            add_file_to_zip(&mut zip, source_path, &zip_name)?;
            copied_thumbnails += 1;
        }
    }

    zip.finish()?;

    Ok(DbBundleExportSummary {
        copied_files,
        copied_thumbnails,
    })
}

pub fn inspect_db_bundle(path: String, state: &State<AppState>) -> AppResult<DbBundleInspection> {
    let source_file = validate_bundle_source(&path)?;
    let file = fs::File::open(&source_file)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let validated = validate_archive(&mut archive)?;
    let inspect_root = state
        .db_path
        .parent()
        .ok_or("Cannot resolve app data directory")?
        .join(format!("restore-inspect-{}-{}", std::process::id(), rand::random::<u64>()));
    let _ = fs::remove_dir_all(&inspect_root);
    fs::create_dir_all(&inspect_root)?;
    let inspect_db = inspect_root.join("media.db");
    let result = (|| -> AppResult<DbBundleInspection> {
        extract_database_entries(&mut archive, &validated.entries, &inspect_db)?;
        validate_sqlite_header(&inspect_db)?;
        let allow_legacy = validated.manifest.as_ref().map_or(true, |manifest| {
            manifest.format_version == 1
        });
        let conn = db::open_connection_read_only(&inspect_db)?;
        db::validate_backup_database(&conn, allow_legacy)?;
        let roots = db::list_scan_roots(&conn)?;
        if let Some(manifest) = &validated.manifest {
            validate_manifest_roots(manifest, &roots)?;
        }
        let requires_mapping = cfg!(not(windows))
            && roots.iter().any(|root| is_windows_absolute(root));
        Ok(DbBundleInspection {
            format_version: validated.manifest.as_ref().map(|manifest| manifest.format_version),
            source_platform: validated
                .manifest
                .as_ref()
                .map(|manifest| manifest.source_platform.clone()),
            source_thumbs_dir: validated
                .manifest
                .as_ref()
                .map(|manifest| manifest.source_thumbs_dir.clone()),
            roots,
            requires_mapping,
        })
    })();
    let _ = fs::remove_dir_all(inspect_root);
    result
}

pub fn import_db_bundle(
    path: String,
    root_mappings: Vec<DbRootMapping>,
    state: &State<AppState>,
) -> AppResult<DbBundleImportSummary> {
    let source_file = validate_bundle_source(&path)?;

    let file = fs::File::open(&source_file)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let validated = validate_archive(&mut archive)?;

    let app_data_dir = state
        .db_path
        .parent()
        .ok_or_else(|| "Cannot resolve app data directory".to_string())?;
    let staging_root = app_data_dir.join("restore-staging");
    let staging_db = staging_root.join("media.db");
    let staging_thumbs = staging_root.join("thumbs");
    if staging_root.exists() {
        fs::remove_dir_all(&staging_root)?;
    }
    fs::create_dir_all(&staging_thumbs)?;

    let staged_result = (|| -> AppResult<(usize, usize)> {
        let (restored_files, restored_thumbnails) = extract_archive_entries(
            &mut archive,
            &validated.entries,
            &staging_db,
            &staging_thumbs,
        )?;
        validate_sqlite_header(&staging_db)?;
        let allow_legacy = validated.manifest.as_ref().map_or(true, |manifest| {
            manifest.format_version == 1
        });
        {
            let conn = db::open_connection_read_only(&staging_db)?;
            db::validate_backup_database(&conn, allow_legacy)?;
            if let Some(manifest) = &validated.manifest {
                validate_manifest_roots(manifest, &db::list_scan_roots(&conn)?)?;
            }
        }
        {
            let conn = db::open_connection(&staging_db)?;
            db::init_schema(&conn)?;
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        }
        rewrite_staged_paths(
            &staging_db,
            &staging_thumbs,
            &state.thumbs_dir,
            &root_mappings,
            validated.manifest.as_ref(),
        )?;
        {
            let conn = db::open_connection_read_only(&staging_db)?;
            db::validate_backup_database(&conn, false)?;
        }
        Ok((restored_files, restored_thumbnails))
    })();
    let (restored_files, restored_thumbnails) = match staged_result {
        Ok(summary) => summary,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(error);
        }
    };

    let previous_db = app_data_dir.join("media.db.restore-previous");
    let previous_thumbs = app_data_dir.join("thumbs.restore-previous");
    let _ = fs::remove_file(&previous_db);
    if previous_thumbs.exists() {
        fs::remove_dir_all(&previous_thumbs)?;
    }
    let current_wal = sqlite_sidecar_path(&state.db_path, "-wal");
    let current_shm = sqlite_sidecar_path(&state.db_path, "-shm");
    db_pool::invalidate(&state.db_path);
    asset_query_service::manager().clear();
    if state.db_path.exists() {
        let current = db::open_connection(&state.db_path)?;
        current.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    let _ = fs::remove_file(&current_wal);
    let _ = fs::remove_file(&current_shm);

    if state.db_path.exists() {
        fs::rename(&state.db_path, &previous_db)?;
    }
    if state.thumbs_dir.exists() {
        fs::rename(&state.thumbs_dir, &previous_thumbs)?;
    }

    let install_result = (|| -> AppResult<()> {
        fs::rename(&staging_db, &state.db_path)?;
        fs::rename(&staging_thumbs, &state.thumbs_dir)?;
        Ok(())
    })();
    if let Err(error) = install_result {
        let _ = fs::remove_file(&state.db_path);
        let _ = fs::remove_dir_all(&state.thumbs_dir);
        if previous_db.exists() {
            let _ = fs::rename(&previous_db, &state.db_path);
        }
        if previous_thumbs.exists() {
            let _ = fs::rename(&previous_thumbs, &state.thumbs_dir);
        }
        return Err(error);
    }

    let _ = fs::remove_file(previous_db);
    let _ = fs::remove_dir_all(previous_thumbs);
    let _ = fs::remove_dir_all(staging_root);
    let conn = db::open_connection(&state.db_path)?;
    db::init_schema(&conn)?;
    db::bump_library_revision(&conn)?;
    db_pool::invalidate(&state.db_path);

    Ok(DbBundleImportSummary {
        restored_files,
        restored_thumbnails,
    })
}

fn validate_bundle_source(path: &str) -> AppResult<PathBuf> {
    let source_file = PathBuf::from(path.trim());
    if !source_file.exists() || !source_file.is_file() {
        return Err("DB import archive path does not exist or is not a file".into());
    }
    if source_file.metadata()?.len() > MAX_ARCHIVE_BYTES {
        return Err("Backup archive exceeds the compressed size limit".into());
    }
    Ok(source_file)
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    (bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
        || path.starts_with("\\\\")
}

fn mapped_path(path: &str, mappings: &[DbRootMapping]) -> Option<String> {
    mappings.iter().find_map(|mapping| {
        if path.eq_ignore_ascii_case(&mapping.source_root) {
            return Some(mapping.target_root.clone());
        }
        let source = mapping.source_root.trim_end_matches(['\\', '/']);
        let suffix = path.get(source.len()..)?;
        if path[..source.len()].eq_ignore_ascii_case(source)
            && suffix.starts_with(['\\', '/'])
        {
            let suffix = suffix.trim_start_matches(['\\', '/']).replace('\\', "/");
            return Some(Path::new(&mapping.target_root).join(suffix).to_string_lossy().to_string());
        }
        None
    })
}

fn rewrite_staged_paths(
    staging_db: &Path,
    staging_thumbs: &Path,
    live_thumbs: &Path,
    root_mappings: &[DbRootMapping],
    manifest: Option<&BundleManifest>,
) -> AppResult<()> {
    let conn = db::open_connection(staging_db)?;
    let roots = db::list_scan_roots(&conn)?;
    for root in &roots {
        validate_stored_path(root)?;
        let root_path = Path::new(root);
        if root_path.exists() && !root_path.is_dir() {
            return Err(format!("Backup scan root is not a directory: {root}").into());
        }
    }
    let mut mappings = root_mappings.to_vec();
    mappings.sort_by_key(|mapping| std::cmp::Reverse(mapping.source_root.len()));
    validate_root_mappings(&roots, &mappings)?;

    let assets = db::list_backup_asset_paths(&conn)?;
    let asset_root_mappings = db::list_backup_asset_root_mappings(&conn)?;
    let mut thumb_candidates: HashMap<String, PathBuf> = HashMap::new();
    let mut legacy_thumb_candidates: HashMap<String, Vec<PathBuf>> = HashMap::new();
    if staging_thumbs.exists() {
        for entry in walkdir::WalkDir::new(staging_thumbs) {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.file_type().is_file() {
                let relative = entry.path().strip_prefix(staging_thumbs).map_err(|error| error.to_string())?;
                let key = normalized_relative_path(relative)?;
                if thumb_candidates.insert(key, entry.path().to_path_buf()).is_some() {
                    return Err("Backup contains duplicate thumbnail destinations".into());
                }
                if let Some(name) = entry.file_name().to_str() {
                    legacy_thumb_candidates
                        .entry(name.to_string())
                        .or_default()
                        .push(entry.path().to_path_buf());
                }
            }
        }
    }
    let mut targets = HashSet::new();
    let mut canonical_targets = HashSet::new();
    let mut rewritten = Vec::new();
    for asset in assets {
        validate_stored_path(&asset.path)?;
        let source_root = roots
            .iter()
            .filter(|root| {
                path_is_within_root(&asset.path, root)
                    && asset_root_mappings
                        .iter()
                        .any(|(asset_id, mapped_root)| {
                            *asset_id == asset.id && mapped_root == *root
                        })
            })
            .max_by_key(|root| root.len())
            .ok_or_else(|| {
                format!(
                    "Asset path is outside its declared root mappings: {}",
                    asset.path
                )
            })?;
        let next_path = if mappings.is_empty() {
            asset.path.clone()
        } else {
            mapped_path(&asset.path, &mappings)
                .ok_or_else(|| format!("Asset path has no root mapping: {}", asset.path))?
        };
        validate_stored_path(&next_path)?;
        let target_root = if mappings.is_empty() {
            source_root.as_str()
        } else {
            mappings
                .iter()
                .find(|mapping| mapping.source_root.eq_ignore_ascii_case(source_root))
                .map(|mapping| mapping.target_root.as_str())
                .ok_or_else(|| format!("Missing target mapping for scan root: {source_root}"))?
        };
        if !path_is_within_root(&next_path, target_root) {
            return Err(format!("Mapped asset path escapes scan root: {next_path}").into());
        }
        validate_existing_path_within_root(&next_path, target_root)?;
        let target_key = if cfg!(any(windows, target_os = "macos")) {
            next_path.to_lowercase()
        } else {
            next_path.clone()
        };
        if !targets.insert(target_key) {
            return Err(format!("Path mapping collision: {next_path}").into());
        }
        if Path::new(&next_path).exists()
            && !canonical_targets.insert(Path::new(&next_path).canonicalize()?)
        {
            return Err(format!("Canonical path mapping collision: {next_path}").into());
        }
        let file_name = next_path
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_string();
        let next_thumb = match asset.thumb_path.as_deref() {
            Some(value) => resolve_imported_thumbnail(
                value,
                manifest,
                staging_thumbs,
                live_thumbs,
                &thumb_candidates,
                &legacy_thumb_candidates,
            )?,
            None => None,
        };
        rewritten.push((asset.id, next_path, file_name, next_thumb));
    }

    let tx = conn.unchecked_transaction()?;
    if !mappings.is_empty() {
        tx.execute("DELETE FROM asset_scan_roots", [])?;
        tx.execute("DELETE FROM scan_roots", [])?;
        for mapping in &mappings {
            tx.execute(
                "INSERT INTO scan_roots(path) VALUES (?1)",
                rusqlite::params![mapping.target_root],
            )?;
        }
    }
    for (id, path, file_name, thumb_path) in rewritten {
        tx.execute(
            "UPDATE assets SET path=?1, file_name=?2, file_name_key=lower(?2), thumb_path=?3 WHERE id=?4",
            rusqlite::params![path, file_name, thumb_path, id],
        )?;
    }
    if !mappings.is_empty() {
        for mapping in &mappings {
            let escaped = mapping
                .target_root
                .trim_end_matches(['/', '\\'])
                .replace('^', "^^")
                .replace('%', "^%")
                .replace('_', "^_");
            let pattern = format!("{escaped}/%");
            tx.execute(
                "INSERT INTO asset_scan_roots(asset_id, root_path, last_seen_generation) SELECT id, ?1, 0 FROM assets WHERE path=?1 OR path LIKE ?2 ESCAPE '^'",
                rusqlite::params![mapping.target_root, pattern],
            )?;
        }
    }
    tx.execute("DELETE FROM thumbnail_failures", [])?;
    tx.commit()?;
    Ok(())
}

fn validate_root_mappings(roots: &[String], mappings: &[DbRootMapping]) -> AppResult<()> {
    if mappings.is_empty() {
        if cfg!(not(windows)) && roots.iter().any(|root| is_windows_absolute(root)) {
            return Err("Windows backup requires a target mapping for every scan root".into());
        }
        return Ok(());
    }

    let known_roots = roots
        .iter()
        .map(|root| root.to_lowercase())
        .collect::<HashSet<_>>();
    let mut sources = HashSet::new();
    let mut targets = HashSet::new();
    for mapping in mappings {
        let source_key = mapping.source_root.to_lowercase();
        if !known_roots.contains(&source_key) || !sources.insert(source_key) {
            return Err(format!("Invalid or duplicate source root mapping: {}", mapping.source_root).into());
        }
        let target = Path::new(&mapping.target_root);
        if !target.is_absolute() || !target.is_dir() {
            return Err(format!(
                "Mapped scan root is not an existing absolute directory: {}",
                mapping.target_root
            )
            .into());
        }
        let canonical_target = target.canonicalize()?;
        if !targets.insert(canonical_target) {
            return Err("Multiple scan roots map to the same target directory".into());
        }
    }
    if sources.len() != roots.len() {
        return Err("A target mapping is required for every scan root".into());
    }
    Ok(())
}

fn validate_stored_path(path: &str) -> AppResult<()> {
    if !(Path::new(path).is_absolute() || is_windows_absolute(path)) {
        return Err(format!("Backup contains a non-absolute media path: {path}").into());
    }
    let normalized = path.replace('\\', "/");
    if normalized
        .split('/')
        .any(|component| matches!(component, "." | ".."))
    {
        return Err(format!("Backup contains an unsafe media path: {path}").into());
    }
    Ok(())
}

fn path_is_within_root(path: &str, root: &str) -> bool {
    let path = path.replace('\\', "/");
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let case_insensitive = is_windows_absolute(&path) || is_windows_absolute(root);
    let (path, root) = if case_insensitive {
        (path.to_lowercase(), root.to_lowercase())
    } else {
        (path, root.to_string())
    };
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn validate_existing_path_within_root(path: &str, root: &str) -> AppResult<()> {
    let candidate = Path::new(path);
    let root = Path::new(root);
    if !root.exists() {
        return Ok(());
    }
    if candidate.exists() && !candidate.is_file() {
        return Err(format!("Media path is not a regular file: {path}").into());
    }
    let canonical_root = root.canonicalize()?;
    let relative = if let Ok(relative) = candidate.strip_prefix(root) {
        relative.to_path_buf()
    } else {
        stored_relative_path(path, &root.to_string_lossy())
            .map(PathBuf::from)
            .ok_or_else(|| format!("Media path cannot be resolved below its scan root: {path}"))?
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let resolved = current.canonicalize().map_err(|_| {
                    format!("Media path contains an unresolved symbolic link: {path}")
                })?;
                if !resolved.starts_with(&canonical_root) {
                    return Err(
                        format!("Media path escapes its scan root through a symlink: {path}")
                            .into(),
                    );
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(error.into()),
        }
    }
    if candidate.exists() && !candidate.canonicalize()?.starts_with(&canonical_root) {
        return Err(format!("Media path escapes its scan root through a symlink: {path}").into());
    }
    Ok(())
}

fn normalized_relative_path(path: &Path) -> AppResult<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_lowercase()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("Backup contains an unsafe relative thumbnail path".into())
            }
        }
    }
    if parts.is_empty() {
        return Err("Backup contains an empty thumbnail path".into());
    }
    Ok(parts.join("/"))
}

fn stored_relative_path(path: &str, root: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let root = root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    let suffix = if is_windows_absolute(&path) || is_windows_absolute(root) {
        let lower_path = path.to_lowercase();
        let lower_root = root.to_lowercase();
        if lower_path == lower_root {
            ""
        } else {
            path.get(root.len()..).filter(|_| lower_path.starts_with(&lower_root))?
        }
    } else {
        path.strip_prefix(root)?
    };
    let suffix = suffix.strip_prefix('/')?;
    normalized_relative_path(Path::new(suffix)).ok()
}

fn resolve_imported_thumbnail(
    old_thumb: &str,
    manifest: Option<&BundleManifest>,
    staging_thumbs: &Path,
    live_thumbs: &Path,
    candidates: &HashMap<String, PathBuf>,
    legacy_candidates: &HashMap<String, Vec<PathBuf>>,
) -> AppResult<Option<String>> {
    let candidate = if let Some(manifest) = manifest.filter(|value| value.format_version >= 2) {
        let relative = stored_relative_path(old_thumb, &manifest.source_thumbs_dir)
            .ok_or("Format-v2 thumbnail path is outside the declared source thumbnail directory")?;
        candidates
            .get(&relative)
            .ok_or("Format-v2 thumbnail reference has no matching archive entry")?
    } else {
        let Some(name) = old_thumb.rsplit(['\\', '/']).next() else {
            return Ok(None);
        };
        let Some(matches) = legacy_candidates.get(name) else {
            return Ok(None);
        };
        if matches.len() != 1 {
            return Ok(None);
        }
        &matches[0]
    };
    let relative = candidate
        .strip_prefix(staging_thumbs)
        .map_err(|error| error.to_string())?;
    Ok(Some(
        live_thumbs.join(relative).to_string_lossy().to_string(),
    ))
}

fn validate_archive(archive: &mut zip::ZipArchive<fs::File>) -> AppResult<ValidatedArchive> {
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("Backup archive contains too many entries".into());
    }
    let mut expanded_bytes = 0u64;
    let mut destinations = HashSet::new();
    let mut entries = Vec::new();
    let mut manifest = None;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let safe_path = sanitize_zip_entry_path(entry.name())?;
        let size = entry.size();
        if size > MAX_ENTRY_BYTES {
            return Err(format!("Backup entry exceeds the size limit: {}", entry.name()).into());
        }
        expanded_bytes = expanded_bytes
            .checked_add(size)
            .ok_or("Backup expanded size overflow")?;
        if expanded_bytes > MAX_EXPANDED_BYTES {
            return Err("Backup archive exceeds the expanded size limit".into());
        }
        let compressed = entry.compressed_size();
        if size > 0
            && (compressed == 0
                || size > compressed.saturating_mul(MAX_COMPRESSION_RATIO))
        {
            return Err(format!("Backup entry exceeds the compression ratio limit: {}", entry.name()).into());
        }
        if entry.is_dir() {
            continue;
        }
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return Err("Backup archive contains a symbolic-link entry".into());
        }

        let normalized = safe_path.to_string_lossy().replace('\\', "/");
        let recognized = matches!(normalized.as_str(), "manifest.json" | "media.db" | "media.db-wal" | "media.db-shm")
            || normalized.starts_with("thumbs/");
        if !recognized {
            continue;
        }
        let destination_key = normalized.to_lowercase();
        if !destinations.insert(destination_key) {
            return Err(format!("Backup archive contains a duplicate destination: {normalized}").into());
        }
        if normalized == "manifest.json" {
            if size > MAX_MANIFEST_BYTES {
                return Err("Backup manifest exceeds the size limit".into());
            }
            let mut bytes = Vec::with_capacity(size as usize);
            entry
                .by_ref()
                .take(MAX_MANIFEST_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 != size {
                return Err("Backup manifest size does not match its ZIP metadata".into());
            }
            let parsed: BundleManifest =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            validate_manifest(&parsed)?;
            manifest = Some(parsed);
        } else {
            entries.push(ValidatedArchiveEntry {
                index,
                path: safe_path,
                size,
            });
        }
    }

    if !entries.iter().any(|entry| entry.path == Path::new("media.db")) {
        return Err("Backup archive does not contain media.db".into());
    }
    Ok(ValidatedArchive { entries, manifest })
}

fn validate_manifest(manifest: &BundleManifest) -> AppResult<()> {
    match manifest.format_version {
        1 => {
            if manifest.application_id.is_some() || manifest.schema_version.is_some() {
                return Err("Legacy backup manifest contains unsupported identity fields".into());
            }
        }
        BUNDLE_FORMAT_VERSION => {
            if manifest.application_id.as_deref() != Some(BUNDLE_APPLICATION_ID) {
                return Err("Backup manifest belongs to another application".into());
            }
            if manifest.schema_version != Some(db::SCHEMA_VERSION as u32) {
                return Err("Backup manifest has an unsupported database schema version".into());
            }
        }
        version if version > BUNDLE_FORMAT_VERSION => {
            return Err(format!("Backup format version {version} is newer than supported").into())
        }
        version => return Err(format!("Unsupported backup format version {version}").into()),
    }
    if !matches!(manifest.source_platform.as_str(), "windows" | "linux" | "macos") {
        return Err("Backup manifest has an unsupported source platform".into());
    }
    let unique_roots = manifest.roots.iter().collect::<HashSet<_>>();
    if unique_roots.len() != manifest.roots.len()
        || manifest.roots.iter().any(|root| root.trim().is_empty())
    {
        return Err("Backup manifest contains invalid or duplicate scan roots".into());
    }
    if manifest.source_thumbs_dir.trim().is_empty() {
        return Err("Backup manifest has an empty thumbnail directory".into());
    }
    for root in &manifest.roots {
        validate_stored_path(root)?;
    }
    validate_stored_path(&manifest.source_thumbs_dir)?;
    Ok(())
}

fn validate_manifest_roots(manifest: &BundleManifest, database_roots: &[String]) -> AppResult<()> {
    let mut manifest_roots = manifest.roots.clone();
    let mut database_roots = database_roots.to_vec();
    manifest_roots.sort();
    database_roots.sort();
    if manifest_roots != database_roots {
        return Err("Backup manifest scan roots do not match the database".into());
    }
    Ok(())
}

fn extract_database_entries(
    archive: &mut zip::ZipArchive<fs::File>,
    entries: &[ValidatedArchiveEntry],
    staging_db: &Path,
) -> AppResult<()> {
    for validated in entries.iter().filter(|entry| {
        matches!(
            entry.path.to_string_lossy().as_ref(),
            "media.db" | "media.db-wal" | "media.db-shm"
        )
    }) {
        let destination = match validated.path.to_string_lossy().as_ref() {
            "media.db" => staging_db.to_path_buf(),
            "media.db-wal" => sqlite_sidecar_path(staging_db, "-wal"),
            "media.db-shm" => sqlite_sidecar_path(staging_db, "-shm"),
            _ => continue,
        };
        let mut entry = archive.by_index(validated.index)?;
        write_archive_entry_to_file(&mut entry, &destination, validated.size)?;
    }
    Ok(())
}

fn extract_archive_entries(
    archive: &mut zip::ZipArchive<fs::File>,
    entries: &[ValidatedArchiveEntry],
    staging_db: &Path,
    staging_thumbs: &Path,
) -> AppResult<(usize, usize)> {
    let mut restored_files = 0usize;
    let mut restored_thumbnails = 0usize;
    for validated in entries {
        let destination = match validated.path.to_string_lossy().as_ref() {
            "media.db" => {
                restored_files += 1;
                staging_db.to_path_buf()
            }
            "media.db-wal" => {
                restored_files += 1;
                sqlite_sidecar_path(staging_db, "-wal")
            }
            "media.db-shm" => {
                restored_files += 1;
                sqlite_sidecar_path(staging_db, "-shm")
            }
            _ => {
                let relative = validated
                    .path
                    .strip_prefix("thumbs")
                    .map_err(|error| error.to_string())?;
                restored_thumbnails += 1;
                staging_thumbs.join(relative)
            }
        };
        let mut entry = archive.by_index(validated.index)?;
        write_archive_entry_to_file(&mut entry, &destination, validated.size)?;
    }
    Ok((restored_files, restored_thumbnails))
}

fn validate_sqlite_header(path: &Path) -> AppResult<()> {
    let metadata = path.metadata()?;
    if metadata.len() < 512 {
        return Err("Backup media.db is smaller than a valid SQLite database".into());
    }
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 100];
    file.read_exact(&mut header)?;
    if &header[..16] != b"SQLite format 3\0" {
        return Err("Backup media.db does not have a SQLite header".into());
    }
    let encoded_page_size = u16::from_be_bytes([header[16], header[17]]) as u64;
    let page_size = if encoded_page_size == 1 {
        65_536
    } else {
        encoded_page_size
    };
    if !(512..=65_536).contains(&page_size)
        || !page_size.is_power_of_two()
        || metadata.len() % page_size != 0
    {
        return Err("Backup media.db has an invalid SQLite page size or file length".into());
    }
    Ok(())
}

fn sqlite_sidecar_path(base_db_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", base_db_path.to_string_lossy(), suffix))
}

fn add_file_to_zip(
    zip: &mut zip::ZipWriter<fs::File>,
    source_path: &Path,
    entry_name: &str,
) -> AppResult<()> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    zip.start_file(entry_name, options)?;
    let mut source = fs::File::open(source_path)?;
    std::io::copy(&mut source, zip)?;
    Ok(())
}

fn sanitize_zip_entry_path(entry_name: &str) -> AppResult<PathBuf> {
    let normalized = entry_name.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute() {
        return Err("Backup archive contains absolute path entry".into());
    }

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => out.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("Backup archive contains unsafe path entry".into())
            }
        }
    }

    Ok(out)
}

fn write_archive_entry_to_file(
    entry: &mut zip::read::ZipFile<'_>,
    destination: &Path,
    expected_size: u64,
) -> AppResult<()> {
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut file = fs::File::create(destination)?;
    let copied = std::io::copy(&mut entry.take(expected_size + 1), &mut file)?;
    if copied != expected_size {
        return Err("Backup entry size does not match its ZIP metadata".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        sync::{atomic::AtomicBool, Mutex, RwLock},
    };

    use tempfile::tempdir;

    use crate::{
        app::state::AppState, db, models::NewAsset, services::thumb_scheduler::ThumbnailScheduler,
    };

    use super::{export_db_bundle, import_db_bundle, sanitize_zip_entry_path};

    fn create_test_state(db_path: &Path, thumbs_dir: &Path) -> AppState {
        AppState {
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
        }
    }

    fn as_state<'a>(state: &'a AppState) -> tauri::State<'a, AppState> {
        unsafe { std::mem::transmute::<&'a AppState, tauri::State<'a, AppState>>(state) }
    }

    fn new_asset(path: &Path, modified_at: i64) -> NewAsset {
        NewAsset {
            path: path.to_string_lossy().to_string(),
            kind: "image".to_string(),
            size_bytes: 10,
            modified_at,
            width: Some(100),
            height: Some(100),
            duration_ms: None,
            thumb_path: None,
        }
    }

    #[test]
    fn sanitize_rejects_parent_segments() {
        assert!(sanitize_zip_entry_path("../media.db").is_err());
        assert!(sanitize_zip_entry_path("thumbs/../../x").is_err());
    }

    #[test]
    fn sanitize_accepts_relative_file() {
        let safe = sanitize_zip_entry_path("thumbs/a/b.jpg").expect("must pass");
        assert_eq!(safe.to_string_lossy().replace('\\', "/"), "thumbs/a/b.jpg");
    }

    #[test]
    fn export_db_bundle_includes_database_sidecars_and_thumbnails() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        drop(conn);

        let wal_path = PathBuf::from(format!("{}-wal", db_path.to_string_lossy()));
        let shm_path = PathBuf::from(format!("{}-shm", db_path.to_string_lossy()));
        fs::write(&wal_path, b"wal").expect("write wal");
        fs::write(&shm_path, b"shm").expect("write shm");

        let nested_thumb_dir = thumbs_dir.join("nested");
        fs::create_dir_all(&nested_thumb_dir).expect("create nested thumbs");
        let thumb_file = nested_thumb_dir.join("thumb.jpg");
        fs::write(&thumb_file, b"thumb").expect("write thumb file");

        let state = create_test_state(&db_path, &thumbs_dir);
        let state_ref = as_state(&state);
        let archive_path = tmp.path().join("backup.zip");

        let summary = export_db_bundle(archive_path.to_string_lossy().to_string(), &state_ref)
            .expect("export bundle");

        assert_eq!(summary.copied_files, 3);
        assert_eq!(summary.copied_thumbnails, 1);

        let file = fs::File::open(&archive_path).expect("open archive");
        let mut archive = zip::ZipArchive::new(file).expect("read archive");
        let mut names = (0..archive.len())
            .map(|idx| {
                archive
                    .by_index(idx)
                    .expect("archive entry")
                    .name()
                    .to_string()
            })
            .collect::<Vec<_>>();
        names.sort();

        assert!(names.iter().any(|name| name == "media.db"));
        assert!(names.iter().any(|name| name == "media.db-wal"));
        assert!(names.iter().any(|name| name == "media.db-shm"));
        assert!(names.iter().any(|name| name == "thumbs/nested/thumb.jpg"));
    }

    #[test]
    fn import_db_bundle_rejects_archive_without_media_db() {
        let tmp = tempdir().expect("tempdir");
        let db_path = tmp.path().join("media.db");
        let thumbs_dir = tmp.path().join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        db::add_scan_root(&conn, "C:/existing").expect("seed root");

        let archive_path = tmp.path().join("broken.zip");
        {
            let file = fs::File::create(&archive_path).expect("create archive");
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("thumbs/file.jpg", zip::write::SimpleFileOptions::default())
                .expect("start file");
            zip.write_all(b"thumb").expect("write thumb");
            zip.finish().expect("finish archive");
        }

        let state = create_test_state(&db_path, &thumbs_dir);
        let state_ref = as_state(&state);
        let error = import_db_bundle(archive_path.to_string_lossy().to_string(), Vec::new(), &state_ref)
            .expect_err("must fail without media.db");

        assert!(
            error.to_string().contains("does not contain media.db"),
            "unexpected error: {error}"
        );

        let conn = db::open_connection(&db_path).expect("reopen db");
        let roots = db::list_scan_roots(&conn).expect("list roots");
        assert_eq!(roots, vec!["C:/existing".to_string()]);
    }

    #[test]
    fn import_db_bundle_restores_database_and_thumbnails() {
        let tmp = tempdir().expect("tempdir");
        let source_db_path = tmp.path().join("source.db");
        let source_thumb_bytes = b"thumb-bytes";

        {
            let conn = db::open_connection(&source_db_path).expect("open source db");
            db::init_schema(&conn).expect("init source schema");
            db::add_scan_root(&conn, "/restored-root").expect("add restored root");
            let asset_path = tmp.path().join("asset.jpg");
            fs::write(&asset_path, b"asset").expect("write source asset");
            db::upsert_asset(&conn, &new_asset(&asset_path, 10)).expect("upsert source asset");
        }

        let archive_path = tmp.path().join("restore.zip");
        {
            let file = fs::File::create(&archive_path).expect("create archive");
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("media.db", zip::write::SimpleFileOptions::default())
                .expect("start media db");
            let db_bytes = fs::read(&source_db_path).expect("read source db");
            zip.write_all(&db_bytes).expect("write media db");

            zip.start_file(
                "thumbs/restored/thumb.jpg",
                zip::write::SimpleFileOptions::default(),
            )
            .expect("start thumb");
            zip.write_all(source_thumb_bytes)
                .expect("write thumb bytes");
            zip.finish().expect("finish archive");
        }

        let target_db_path = tmp.path().join("target.db");
        let target_thumbs_dir = tmp.path().join("target-thumbs");
        fs::create_dir_all(&target_thumbs_dir).expect("create target thumbs dir");
        let conn = db::open_connection(&target_db_path).expect("open target db");
        db::init_schema(&conn).expect("init target schema");
        db::add_scan_root(&conn, "C:/old-root").expect("seed old root");
        drop(conn);

        let state = create_test_state(&target_db_path, &target_thumbs_dir);
        let state_ref = as_state(&state);
        let summary = import_db_bundle(archive_path.to_string_lossy().to_string(), Vec::new(), &state_ref)
            .expect("import bundle");

        assert_eq!(summary.restored_files, 1);
        assert_eq!(summary.restored_thumbnails, 1);

        let restored_thumb = target_thumbs_dir.join("restored").join("thumb.jpg");
        assert_eq!(
            fs::read(&restored_thumb).expect("read restored thumb"),
            source_thumb_bytes
        );

        let conn = db::open_connection(&target_db_path).expect("open restored db");
        let roots = db::list_scan_roots(&conn).expect("list restored roots");
        assert_eq!(roots, vec!["/restored-root".to_string()]);
        let page = db::list_assets(&conn, 0, 50, &[], &[], None, false).expect("list assets");
        assert_eq!(page.total, 1);
    }

    #[cfg(not(windows))]
    #[test]
    fn imports_windows_bundle_with_root_mapping_and_preserves_asset_metadata() {
        let tmp = tempdir().expect("tempdir");
        let source_db = tmp.path().join("windows.db");
        let source_root = r"C:\Users\Example\Pictures";
        {
            let conn = db::open_connection(&source_db).expect("open source db");
            db::init_schema(&conn).expect("init source schema");
            db::add_scan_root(&conn, source_root).expect("add source root");
            let asset = NewAsset {
                path: format!(r"{source_root}\album\photo.jpg"),
                kind: "image".to_string(),
                size_bytes: 10,
                modified_at: 12,
                width: Some(20),
                height: Some(30),
                duration_ms: None,
                thumb_path: Some(r"C:\old-profile\thumbs\legacy.jpg".to_string()),
            };
            db::upsert_asset(&conn, &asset).expect("insert asset");
            db::set_asset_tags(&conn, 1, &["travel".to_string()]).expect("set tags");
            db::set_asset_favorite(&conn, 1, true).expect("set favorite");
            db::set_asset_media_group(&conn, 1, Some("album"), Some(2.0))
                .expect("set group");
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .expect("checkpoint source");
        }

        let archive_path = tmp.path().join("windows-backup.zip");
        {
            let file = fs::File::create(&archive_path).expect("create archive");
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file("media.db", zip::write::SimpleFileOptions::default())
                .expect("start db");
            zip.write_all(&fs::read(&source_db).expect("read source db"))
                .expect("write db");
            zip.start_file("thumbs/legacy.jpg", zip::write::SimpleFileOptions::default())
                .expect("start thumb");
            zip.write_all(b"legacy-thumbnail").expect("write thumb");
            zip.finish().expect("finish archive");
        }

        let target_root = tmp.path().join("linux-media");
        fs::create_dir_all(target_root.join("album")).expect("create mapped root");
        fs::write(target_root.join("album/photo.jpg"), b"media").expect("write mapped media");
        let target_db = tmp.path().join("target.db");
        let target_thumbs = tmp.path().join("target-thumbs");
        fs::create_dir_all(&target_thumbs).expect("create target thumbs");
        let conn = db::open_connection(&target_db).expect("open target");
        db::init_schema(&conn).expect("init target");
        drop(conn);
        let state = create_test_state(&target_db, &target_thumbs);
        let mapping = crate::models::DbRootMapping {
            source_root: source_root.to_string(),
            target_root: target_root.to_string_lossy().to_string(),
        };

        import_db_bundle(
            archive_path.to_string_lossy().to_string(),
            vec![mapping],
            &as_state(&state),
        )
        .expect("import mapped bundle");

        let conn = db::open_connection(&target_db).expect("open restored db");
        let page = db::list_assets(&conn, 0, 10, &[], &[], None, false).expect("list assets");
        assert_eq!(page.total, 1);
        let asset = &page.items[0];
        assert_eq!(asset.id, 1);
        assert_eq!(asset.path, target_root.join("album/photo.jpg").to_string_lossy());
        assert_eq!(asset.tags, vec!["travel".to_string()]);
        assert!(asset.is_favorite);
        assert_eq!(asset.media_group_key.as_deref(), Some("album"));
        assert_eq!(asset.media_group_order, Some(2.0));
        let expected_thumb = target_thumbs.join("legacy.jpg").to_string_lossy().to_string();
        assert_eq!(asset.thumb_path.as_deref(), Some(expected_thumb.as_str()));
        assert_eq!(
            db::list_scan_roots(&conn).expect("roots"),
            vec![target_root.to_string_lossy().to_string()]
        );
        assert_eq!(fs::read(target_thumbs.join("legacy.jpg")).expect("restored thumb"), b"legacy-thumbnail");
    }
}
