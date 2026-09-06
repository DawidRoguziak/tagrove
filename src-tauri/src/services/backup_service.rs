use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use crate::{
    app::state::AppState,
    db,
    error::AppResult,
    models::{DbBundleExportSummary, DbBundleImportSummary, DbBundleInspection, DbRootMapping},
    utils::text::canonical_key,
};

const BUNDLE_APPLICATION_ID: &str = "io.github.mediatagger.bundle";
const BUNDLE_FORMAT_VERSION: u32 = 2;
const MAX_ARCHIVE_BYTES: u64 = 128 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 2_000_000;
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024 * 1024;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum RestorePhase {
    Swapping,
    Committed,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct RestoreJournal {
    phase: RestorePhase,
    previous_db_existed: bool,
    previous_thumbs_existed: bool,
}

pub fn export_db_bundle(
    path: String,
    state: &AppState,
    maintenance: &crate::services::db_pool::MaintenancePermit,
) -> AppResult<DbBundleExportSummary> {
    let conn = maintenance.connection()?;
    db::ensure_no_pending_file_operations(&conn)?;
    let target_file = validate_export_target(path.trim(), state, &conn)?;
    let manifest = BundleManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        application_id: Some(BUNDLE_APPLICATION_ID.to_string()),
        schema_version: Some(db::SCHEMA_VERSION as u32),
        source_platform: std::env::consts::OS.to_string(),
        roots: db::list_scan_roots(&conn)?,
        source_thumbs_dir: state.thumbs_dir.to_string_lossy().to_string(),
    };
    let app_data_dir = state
        .db_path
        .parent()
        .ok_or("Cannot resolve app data directory")?;
    let token = format!("{}-{}", std::process::id(), rand::random::<u64>());
    let snapshot_path = app_data_dir.join(format!("backup-snapshot-{token}.db"));
    let target_name = target_file
        .file_name()
        .ok_or("DB export archive path has no file name")?
        .to_string_lossy();
    let temporary_target = target_file
        .parent()
        .ok_or("DB export archive path has no parent")?
        .join(format!(".{target_name}.{token}.tmp"));

    let export_result = (|| -> AppResult<DbBundleExportSummary> {
        create_database_snapshot(&conn, &snapshot_path)?;
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_target)?;
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file(
            "manifest.json",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644),
        )?;
        serde_json::to_writer(&mut zip, &manifest).map_err(|error| error.to_string())?;
        add_file_to_zip(&mut zip, &snapshot_path, "media.db")?;

        let mut copied_thumbnails = 0usize;
        if state.thumbs_dir.exists() {
            for entry in walkdir::WalkDir::new(&state.thumbs_dir) {
                let entry = entry.map_err(|e| e.to_string())?;
                if !entry.file_type().is_file() {
                    continue;
                }

                let source_path = entry.path();
                if source_path == target_file || source_path == temporary_target {
                    continue;
                }
                let relative = source_path
                    .strip_prefix(&state.thumbs_dir)
                    .map_err(|e| e.to_string())?;
                let relative_name = relative.to_string_lossy().replace('\\', "/");
                add_file_to_zip(&mut zip, source_path, &format!("thumbs/{relative_name}"))?;
                copied_thumbnails += 1;
            }
        }

        let archive = zip.finish()?;
        archive.sync_all()?;
        if archive.metadata()?.len() > MAX_ARCHIVE_BYTES {
            return Err("Backup archive exceeds the size limit".into());
        }
        drop(archive);
        validate_archive(&mut zip::ZipArchive::new(fs::File::open(
            &temporary_target,
        )?)?)?;
        publish_archive(&temporary_target, &target_file)?;
        sync_parent_directory(&target_file)?;
        Ok(DbBundleExportSummary {
            copied_files: 1,
            copied_thumbnails,
        })
    })();

    let _ = fs::remove_file(&snapshot_path);
    if export_result.is_err() {
        let _ = fs::remove_file(&temporary_target);
    }
    export_result
}

fn validate_export_target(
    raw_path: &str,
    state: &AppState,
    conn: &rusqlite::Connection,
) -> AppResult<PathBuf> {
    let requested = PathBuf::from(raw_path);
    if requested.as_os_str().is_empty() {
        return Err("DB export archive path is empty".into());
    }
    let parent = requested
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let file_name = requested
        .file_name()
        .ok_or("DB export archive path has no file name")?;
    let target = parent.join(file_name);
    let app_data = state
        .db_path
        .parent()
        .ok_or("Cannot resolve app data directory")?
        .canonicalize()?;
    if target.starts_with(&app_data) {
        return Err("DB export archive cannot be stored inside the active profile".into());
    }

    for root in db::list_scan_roots(conn)? {
        if let Ok(root) = Path::new(&root).canonicalize() {
            if target.starts_with(root) {
                return Err(
                    "DB export archive cannot be stored inside an indexed source root".into(),
                );
            }
        }
    }
    for asset in db::list_backup_asset_paths(conn)? {
        if let Ok(source) = Path::new(&asset.path).canonicalize() {
            if target == source {
                return Err("DB export archive cannot overwrite an indexed source file".into());
            }
        }
    }
    Ok(target)
}

fn create_database_snapshot(source: &rusqlite::Connection, target: &Path) -> AppResult<()> {
    let mut destination = rusqlite::Connection::open(target)?;
    let backup = rusqlite::backup::Backup::new(source, &mut destination)?;
    backup.run_to_completion(256, Duration::from_millis(10), None)?;
    drop(backup);
    destination.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(destination);
    fs::File::open(target)?.sync_all()?;
    Ok(())
}

pub fn inspect_db_bundle(path: String, state: &AppState) -> AppResult<DbBundleInspection> {
    let source_file = validate_bundle_source(&path)?;
    let file = fs::File::open(&source_file)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let validated = validate_archive(&mut archive)?;
    let inspect_root = state
        .db_path
        .parent()
        .ok_or("Cannot resolve app data directory")?
        .join(format!(
            "restore-inspect-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
    let _ = fs::remove_dir_all(&inspect_root);
    fs::create_dir_all(&inspect_root)?;
    let inspect_db = inspect_root.join("media.db");
    let result = (|| -> AppResult<DbBundleInspection> {
        extract_database_entries(&mut archive, &validated.entries, &inspect_db)?;
        validate_sqlite_header(&inspect_db)?;
        let allow_legacy = validated
            .manifest
            .as_ref()
            .is_none_or(|manifest| manifest.format_version == 1);
        let conn = db::open_connection_read_only(&inspect_db)?;
        db::validate_backup_database(&conn, allow_legacy)?;
        let roots = db::list_scan_roots(&conn)?;
        if let Some(manifest) = &validated.manifest {
            validate_manifest_database(manifest, &conn)?;
        }
        let requires_mapping =
            cfg!(not(windows)) && roots.iter().any(|root| is_windows_absolute(root));
        Ok(DbBundleInspection {
            format_version: validated
                .manifest
                .as_ref()
                .map(|manifest| manifest.format_version),
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
    state: &AppState,
    maintenance: &crate::services::db_pool::MaintenancePermit,
) -> AppResult<DbBundleImportSummary> {
    let source_file = validate_bundle_source(&path)?;

    let file = fs::File::open(&source_file)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let validated = validate_archive(&mut archive)?;

    let app_data_dir = state
        .db_path
        .parent()
        .ok_or_else(|| "Cannot resolve app data directory".to_string())?;
    let previous_db = app_data_dir.join("media.db.restore-previous");
    let previous_thumbs = app_data_dir.join("thumbs.restore-previous");
    if previous_db.exists()
        || previous_thumbs.exists()
        || restore_journal_path(app_data_dir).exists()
    {
        return Err(
            "Restore recovery artifacts already exist; restart Tagrove to recover them before importing again"
                .into(),
        );
    }
    if source_file
        .canonicalize()?
        .starts_with(app_data_dir.canonicalize()?)
    {
        return Err("DB import archive cannot be stored inside the active profile".into());
    }
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
        let allow_legacy = validated
            .manifest
            .as_ref()
            .is_none_or(|manifest| manifest.format_version == 1);
        {
            let conn = db::open_connection_read_only_untracked(&staging_db)?;
            db::validate_backup_database(&conn, allow_legacy)?;
            if let Some(manifest) = &validated.manifest {
                validate_manifest_database(manifest, &conn)?;
            }
        }
        {
            let conn = db::open_connection_untracked(&staging_db)?;
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
            let conn = db::open_connection_untracked(&staging_db)?;
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        }
        remove_database_sidecars(&staging_db)?;
        {
            let conn = db::open_connection_read_only_untracked(&staging_db)?;
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

    let previous_db_existed = state.db_path.exists();
    let previous_thumbs_existed = state.thumbs_dir.exists();
    if state.db_path.exists() {
        let current = maintenance.connection()?;
        current.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    remove_database_sidecars(&state.db_path)?;

    let mut journal = RestoreJournal {
        phase: RestorePhase::Swapping,
        previous_db_existed,
        previous_thumbs_existed,
    };
    write_restore_journal(app_data_dir, &journal)?;

    let install_result = (|| -> AppResult<()> {
        if previous_db_existed {
            rename_durable(&state.db_path, &previous_db)?;
            sync_parent_directory(&state.db_path)?;
        }
        if previous_thumbs_existed {
            rename_durable(&state.thumbs_dir, &previous_thumbs)?;
            sync_parent_directory(&state.thumbs_dir)?;
        }
        rename_durable(&staging_db, &state.db_path)?;
        sync_parent_directory(&state.db_path)?;
        rename_durable(&staging_thumbs, &state.thumbs_dir)?;
        sync_parent_directory(&state.thumbs_dir)?;

        let conn = maintenance.connection()?;
        db::init_schema(&conn)?;
        db::validate_backup_database(&conn, false)?;
        db::bump_library_revision(&conn)?;
        let _ = db::current_library_revision(&conn)?;
        let _ = db::list_scan_roots(&conn)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        drop(conn);
        remove_database_sidecars(&state.db_path)?;

        journal.phase = RestorePhase::Committed;
        write_restore_journal(app_data_dir, &journal)?;
        Ok(())
    })();
    if let Err(error) = install_result {
        return match recover_interrupted_restore(app_data_dir) {
            Ok(()) => Err(error),
            Err(recovery_error) => Err(format!(
                "Restore failed: {error}. Automatic rollback also failed: {recovery_error}"
            )
            .into()),
        };
    }

    recover_interrupted_restore(app_data_dir)?;
    state.database.queries.clear();

    Ok(DbBundleImportSummary {
        restored_files,
        restored_thumbnails,
    })
}

pub fn recover_interrupted_restore(app_data_dir: &Path) -> AppResult<()> {
    let journal_path = restore_journal_path(app_data_dir);
    if !journal_path.exists() {
        return Ok(());
    }
    let journal: RestoreJournal = serde_json::from_reader(fs::File::open(&journal_path)?)
        .map_err(|error| format!("Cannot read restore recovery journal: {error}"))?;
    let live_db = app_data_dir.join("media.db");
    let live_thumbs = app_data_dir.join("thumbs");
    let previous_db = app_data_dir.join("media.db.restore-previous");
    let previous_thumbs = app_data_dir.join("thumbs.restore-previous");
    let staging_root = app_data_dir.join("restore-staging");

    match journal.phase {
        RestorePhase::Swapping => {
            remove_database_sidecars(&live_db)?;
            restore_previous_component(&live_db, &previous_db, journal.previous_db_existed, false)?;
            restore_previous_component(
                &live_thumbs,
                &previous_thumbs,
                journal.previous_thumbs_existed,
                true,
            )?;
        }
        RestorePhase::Committed => {
            if !live_db.is_file() || !live_thumbs.is_dir() {
                return Err(
                    "Committed restore journal points to an incomplete live generation".into(),
                );
            }
            remove_path_if_exists(&previous_db, false)?;
            remove_path_if_exists(&previous_thumbs, true)?;
        }
    }

    remove_path_if_exists(&staging_root, true)?;
    fs::remove_file(&journal_path)?;
    sync_parent_directory(&journal_path)?;
    Ok(())
}

fn restore_previous_component(
    live: &Path,
    previous: &Path,
    previously_existed: bool,
    directory: bool,
) -> AppResult<()> {
    if previously_existed {
        if previous.exists() {
            remove_path_if_exists(live, directory)?;
            rename_durable(previous, live)?;
            sync_parent_directory(live)?;
        } else if !live.exists() {
            return Err(format!(
                "Restore recovery cannot find either live or previous path '{}'",
                live.display()
            )
            .into());
        }
    } else {
        remove_path_if_exists(live, directory)?;
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path, directory: bool) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    if directory {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    sync_parent_directory(path)?;
    Ok(())
}

fn restore_journal_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("restore-journal.json")
}

fn write_restore_journal(app_data_dir: &Path, journal: &RestoreJournal) -> AppResult<()> {
    let journal_path = restore_journal_path(app_data_dir);
    let temporary = app_data_dir.join(format!(
        ".restore-journal-{}-{}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    serde_json::to_writer(&mut file, journal).map_err(|error| error.to_string())?;
    file.flush()?;
    file.sync_all()?;
    drop(file);
    publish_archive(&temporary, &journal_path)?;
    sync_parent_directory(&journal_path)?;
    Ok(())
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
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || path.starts_with("\\\\")
}

fn mapped_path(path: &str, mappings: &[DbRootMapping]) -> Option<String> {
    mappings.iter().find_map(|mapping| {
        let normalized_path = path.replace('\\', "/");
        let normalized_root = mapping
            .source_root
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string();
        if normalized_path.eq_ignore_ascii_case(&normalized_root) {
            return Some(mapping.target_root.clone());
        }
        let suffix = normalized_path.get(normalized_root.len()..)?;
        if normalized_path[..normalized_root.len()].eq_ignore_ascii_case(&normalized_root)
            && suffix.starts_with('/')
        {
            let suffix = suffix.trim_start_matches('/');
            return Some(
                Path::new(&mapping.target_root)
                    .join(suffix)
                    .to_string_lossy()
                    .to_string(),
            );
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
    let conn = db::open_connection_untracked(staging_db)?;
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

    conn.pragma_update(None, "temp_store", "FILE")?;
    conn.execute_batch("CREATE TEMP TABLE rewritten_assets (id INTEGER PRIMARY KEY, path TEXT NOT NULL, file_name TEXT NOT NULL, thumb_path TEXT, roots TEXT NOT NULL, collision_key TEXT NOT NULL UNIQUE, canonical_path BLOB UNIQUE)")?;
    conn.execute_batch("CREATE TEMP TABLE imported_thumbnails (relative TEXT PRIMARY KEY, path TEXT NOT NULL, basename TEXT); CREATE INDEX imported_thumbnail_basename ON imported_thumbnails(basename)")?;
    if staging_thumbs.exists() {
        for entry in walkdir::WalkDir::new(staging_thumbs) {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.file_type().is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(staging_thumbs)
                    .map_err(|error| error.to_string())?;
                let key = normalized_relative_path(relative)?;
                conn.execute(
                    "INSERT INTO imported_thumbnails VALUES (?1, ?2, ?3)",
                    rusqlite::params![
                        key,
                        entry.path().to_string_lossy(),
                        entry.file_name().to_str()
                    ],
                )
                .map_err(|error| {
                    format!("Duplicate thumbnail destination or staging error: {error}")
                })?;
            }
        }
    }
    let mut after = 0;
    loop {
        let assets = {
            let mut stmt = conn.prepare(
                "SELECT id, path, thumb_path FROM assets WHERE id > ?1 ORDER BY id LIMIT 512",
            )?;
            let rows = stmt.query_map([after], |row| {
                Ok(db::BackupAssetPath {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    thumb_path: row.get(2)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if assets.is_empty() {
            break;
        }
        after = assets.last().expect("nonempty page").id;
        for asset in assets {
            validate_stored_path(&asset.path)?;
            let assigned_roots = db::assigned_asset_roots(&conn, asset.id)?
                .into_iter()
                .map(|(_, root)| root)
                .filter(|root| path_is_within_root(&asset.path, root))
                .collect::<Vec<_>>();
            if assigned_roots.is_empty() {
                return Err(format!(
                    "Asset path is outside its declared root mappings: {}",
                    asset.path
                )
                .into());
            }
            let next_path = if mappings.is_empty() {
                asset.path.clone()
            } else {
                mapped_path(&asset.path, &mappings)
                    .ok_or_else(|| format!("Asset path has no root mapping: {}", asset.path))?
            };
            validate_stored_path(&next_path)?;
            let mut rewritten_roots = Vec::with_capacity(assigned_roots.len());
            for assigned_root in assigned_roots {
                let target_root = if mappings.is_empty() {
                    assigned_root.as_str()
                } else {
                    mappings
                        .iter()
                        .find(|mapping| path_strings_equal(&mapping.source_root, &assigned_root))
                        .map(|mapping| mapping.target_root.as_str())
                        .ok_or_else(|| {
                            format!("Missing target mapping for scan root: {assigned_root}")
                        })?
                };
                if !path_is_within_root(&next_path, target_root) {
                    return Err(format!("Mapped asset path escapes scan root: {next_path}").into());
                }
                validate_existing_path_within_root(&next_path, target_root)?;
                rewritten_roots.push(target_root.to_string());
            }
            let target_key = if cfg!(any(windows, target_os = "macos")) {
                next_path.to_lowercase()
            } else {
                next_path.clone()
            };
            let canonical = if Path::new(&next_path).exists() {
                Some(
                    Path::new(&next_path)
                        .canonicalize()?
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec(),
                )
            } else {
                None
            };
            let file_name = next_path
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or("")
                .to_string();
            let next_thumb = match asset.thumb_path.as_deref() {
                Some(value) => {
                    resolve_imported_thumbnail(value, manifest, staging_thumbs, live_thumbs, &conn)?
                }
                None => None,
            };
            conn.execute(
                "INSERT INTO rewritten_assets VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    asset.id,
                    next_path,
                    file_name,
                    next_thumb,
                    serde_json::to_string(&rewritten_roots).map_err(|e| e.to_string())?,
                    target_key,
                    canonical
                ],
            )
            .map_err(|error| {
                format!("Path mapping collision or staging error for {next_path}: {error}")
            })?;
        }
    }

    let root_settings = db::list_scan_root_settings(&conn)?;
    let tx = conn.unchecked_transaction()?;
    if !mappings.is_empty() {
        tx.execute("DELETE FROM asset_scan_roots", [])?;
        tx.execute("DELETE FROM scan_roots", [])?;
        for mapping in &mappings {
            tx.execute(
                "INSERT INTO scan_roots(path, auto_scan_on_startup) VALUES (?1, ?2)",
                rusqlite::params![
                    mapping.target_root,
                    root_settings
                        .iter()
                        .any(|root| root.path == mapping.source_root && root.auto_scan_on_startup)
                ],
            )?;
        }
    }
    let mut rewritten = tx.prepare(
        "SELECT id, path, file_name, thumb_path, roots FROM rewritten_assets ORDER BY id",
    )?;
    let mut rows = rewritten.query([])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let path: String = row.get(1)?;
        let file_name: String = row.get(2)?;
        let thumb_path: Option<String> = row.get(3)?;
        let root_paths: Vec<String> =
            serde_json::from_str(&row.get::<_, String>(4)?).map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE assets SET path=?1, file_name=?2, file_name_key=?3, thumb_path=?4 WHERE id=?5",
            rusqlite::params![path, file_name, canonical_key(&file_name), thumb_path, id],
        )?;
        if !mappings.is_empty() {
            for root_path in root_paths {
                tx.execute(
                    "INSERT INTO asset_scan_roots(asset_id, root_path, last_seen_generation) VALUES (?1, ?2, 0)",
                    rusqlite::params![id, root_path],
                )?;
            }
        }
    }
    if !mappings.is_empty() {
        let assets_without_roots: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assets a WHERE NOT EXISTS (
               SELECT 1 FROM asset_scan_roots ar WHERE ar.asset_id = a.id
             )",
            [],
            |row| row.get(0),
        )?;
        if assets_without_roots != 0 {
            return Err("Path rewrite left assets without scan-root mappings".into());
        }
    }
    drop(rows);
    drop(rewritten);
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
            return Err(format!(
                "Invalid or duplicate source root mapping: {}",
                mapping.source_root
            )
            .into());
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

fn path_strings_equal(left: &str, right: &str) -> bool {
    left.replace('\\', "/")
        .trim_end_matches('/')
        .eq_ignore_ascii_case(right.replace('\\', "/").trim_end_matches('/'))
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
                    return Err(format!(
                        "Media path escapes its scan root through a symlink: {path}"
                    )
                    .into());
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
            path.get(root.len()..)
                .filter(|_| lower_path.starts_with(&lower_root))?
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
    conn: &rusqlite::Connection,
) -> AppResult<Option<String>> {
    use rusqlite::OptionalExtension;
    let candidate: String = if let Some(manifest) =
        manifest.filter(|value| value.format_version >= 2)
    {
        let relative = stored_relative_path(old_thumb, &manifest.source_thumbs_dir)
            .ok_or("Format-v2 thumbnail path is outside the declared source thumbnail directory")?;
        conn.query_row(
            "SELECT path FROM imported_thumbnails WHERE relative = ?1",
            [relative],
            |row| row.get(0),
        )
        .optional()?
        .ok_or("Format-v2 thumbnail reference has no matching archive entry")?
    } else {
        let Some(name) = old_thumb.rsplit(['\\', '/']).next() else {
            return Ok(None);
        };
        let mut stmt =
            conn.prepare("SELECT path FROM imported_thumbnails WHERE basename = ?1 LIMIT 2")?;
        let candidates = stmt
            .query_map([name], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if candidates.len() != 1 {
            return Ok(None);
        }
        candidates.into_iter().next().expect("one candidate")
    };
    let relative = Path::new(&candidate)
        .strip_prefix(staging_thumbs)
        .map_err(|error| error.to_string())?;
    Ok(Some(
        live_thumbs.join(relative).to_string_lossy().into_owned(),
    ))
}

fn validate_archive(archive: &mut zip::ZipArchive<fs::File>) -> AppResult<ValidatedArchive> {
    validate_archive_with_limits(
        archive,
        MAX_ARCHIVE_ENTRIES,
        MAX_ENTRY_BYTES,
        MAX_EXPANDED_BYTES,
    )
}

fn validate_archive_with_limits(
    archive: &mut zip::ZipArchive<fs::File>,
    max_entries: usize,
    max_entry_bytes: u64,
    max_expanded_bytes: u64,
) -> AppResult<ValidatedArchive> {
    if archive.len() > max_entries {
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
        if size > max_entry_bytes {
            return Err(format!("Backup entry exceeds the size limit: {}", entry.name()).into());
        }
        expanded_bytes = expanded_bytes
            .checked_add(size)
            .ok_or("Backup expanded size overflow")?;
        if expanded_bytes > max_expanded_bytes {
            return Err("Backup archive exceeds the expanded size limit".into());
        }
        let compressed = entry.compressed_size();
        if size > 0 && (compressed == 0 || size > compressed.saturating_mul(MAX_COMPRESSION_RATIO))
        {
            return Err(format!(
                "Backup entry exceeds the compression ratio limit: {}",
                entry.name()
            )
            .into());
        }
        if entry.is_dir() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Backup archive contains a symbolic-link entry".into());
        }

        let normalized = safe_path.to_string_lossy().replace('\\', "/");
        let recognized = matches!(
            normalized.as_str(),
            "manifest.json" | "media.db" | "media.db-wal" | "media.db-shm"
        ) || normalized.starts_with("thumbs/");
        if !recognized {
            continue;
        }
        let destination_key = normalized.to_lowercase();
        if !destinations.insert(destination_key) {
            return Err(
                format!("Backup archive contains a duplicate destination: {normalized}").into(),
            );
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

    if !entries
        .iter()
        .any(|entry| entry.path == Path::new("media.db"))
    {
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
            if !manifest
                .schema_version
                .is_some_and(|version| (1..=db::SCHEMA_VERSION as u32).contains(&version))
            {
                return Err("Backup manifest has an unsupported database schema version".into());
            }
        }
        version if version > BUNDLE_FORMAT_VERSION => {
            return Err(format!("Backup format version {version} is newer than supported").into())
        }
        version => return Err(format!("Unsupported backup format version {version}").into()),
    }
    if !matches!(
        manifest.source_platform.as_str(),
        "windows" | "linux" | "macos"
    ) {
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

fn validate_manifest_database(
    manifest: &BundleManifest,
    conn: &rusqlite::Connection,
) -> AppResult<()> {
    if let Some(expected) = manifest.schema_version {
        let actual: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if expected != actual {
            return Err("Backup manifest schema version does not match the database".into());
        }
    }
    let mut manifest_roots = manifest.roots.clone();
    let mut database_roots = db::list_scan_roots(conn)?;
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

fn remove_database_sidecars(db_path: &Path) -> AppResult<()> {
    for suffix in ["-wal", "-shm"] {
        let sidecar = sqlite_sidecar_path(db_path, suffix);
        if sidecar.exists() {
            fs::remove_file(&sidecar)?;
        }
    }
    sync_parent_directory(db_path)?;
    Ok(())
}

fn publish_archive(source: &Path, target: &Path) -> AppResult<()> {
    fs::rename(source, target)?;
    Ok(())
}

fn rename_durable(source: &Path, target: &Path) -> AppResult<()> {
    fs::rename(source, target)?;
    Ok(())
}

fn sync_parent_directory(path: &Path) -> AppResult<()> {
    let parent = path.parent().ok_or("Path has no parent directory")?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn add_file_to_zip(
    zip: &mut zip::ZipWriter<fs::File>,
    source_path: &Path,
    entry_name: &str,
) -> AppResult<()> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        // The importer permits entries above ZIP32's 4 GiB boundary.
        .large_file(true)
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

    use super::{export_db_bundle, import_db_bundle, mapped_path, sanitize_zip_entry_path};

    fn create_test_state(db_path: &Path, thumbs_dir: &Path) -> AppState {
        AppState {
            database: crate::services::db_pool::DatabaseRuntime::new(db_path.to_path_buf()),
            db_path: db_path.to_path_buf(),
            thumbs_dir: thumbs_dir.to_path_buf(),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            scan_lock: Mutex::new(()),
            thumb_lock: RwLock::new(()),
            thumb_scheduler: ThumbnailScheduler::new(1, PathBuf::from("ffmpeg")),
            thumbnail_render_all_running: AtomicBool::new(false),
            thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            thumbnail_generation: std::sync::atomic::AtomicU64::new(0),
        }
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
    fn mapped_path_accepts_backslash_and_slash_source_separators() {
        let target = PathBuf::from("/tmp/mapped-media");
        let mappings = vec![crate::models::DbRootMapping {
            source_root: r"C:\Users\Example\Pictures".to_string(),
            target_root: target.to_string_lossy().into_owned(),
        }];

        assert_eq!(
            mapped_path(r"C:\Users\Example\Pictures\album\photo.jpg", &mappings),
            Some(
                target
                    .join("album/photo.jpg")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        assert_eq!(
            mapped_path("C:/Users/Example/Pictures/album/photo.jpg", &mappings),
            Some(
                target
                    .join("album/photo.jpg")
                    .to_string_lossy()
                    .into_owned()
            )
        );
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
    fn export_db_bundle_uses_standalone_snapshot_and_includes_thumbnails() {
        let tmp = tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("profile");
        let db_path = profile_dir.join("media.db");
        let thumbs_dir = profile_dir.join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");

        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        conn.pragma_update(None, "wal_autocheckpoint", 0)
            .expect("disable auto checkpoint");
        db::add_scan_root(&conn, "/library").expect("write WAL content");

        let nested_thumb_dir = thumbs_dir.join("nested");
        fs::create_dir_all(&nested_thumb_dir).expect("create nested thumbs");
        let thumb_file = nested_thumb_dir.join("thumb.jpg");
        fs::write(&thumb_file, b"thumb").expect("write thumb file");

        let state = create_test_state(&db_path, &thumbs_dir);
        let state_ref = &state;
        let archive_path = tmp.path().join("backup.zip");

        let summary = export_db_bundle(
            archive_path.to_string_lossy().to_string(),
            state_ref,
            &state.database.maintenance().unwrap(),
        )
        .expect("export bundle");

        assert_eq!(summary.copied_files, 1);
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
        assert!(!names.iter().any(|name| name == "media.db-wal"));
        assert!(!names.iter().any(|name| name == "media.db-shm"));
        assert!(names.iter().any(|name| name == "thumbs/nested/thumb.jpg"));
    }

    #[test]
    fn import_db_bundle_rejects_archive_without_media_db() {
        let tmp = tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("profile");
        let db_path = profile_dir.join("media.db");
        let thumbs_dir = profile_dir.join("thumbs");
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
        let state_ref = &state;
        let error = import_db_bundle(
            archive_path.to_string_lossy().to_string(),
            Vec::new(),
            state_ref,
            &state.database.maintenance().unwrap(),
        )
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
    fn imports_version_one_bundle_with_startup_scanning_disabled() {
        let tmp = tempdir().unwrap();
        let source_db = tmp.path().join("source.db");
        let root = tmp.path().join("media");
        fs::create_dir_all(&root).unwrap();
        {
            let conn = db::open_connection(&source_db).unwrap();
            db::init_schema(&conn).unwrap();
            db::add_scan_root(&conn, &root.to_string_lossy()).unwrap();
            conn.execute_batch("ALTER TABLE scan_roots DROP COLUMN auto_scan_on_startup; PRAGMA user_version = 1; PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        }
        let archive = tmp.path().join("old.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.start_file("media.db", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&fs::read(&source_db).unwrap()).unwrap();
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            serde_json::to_string(&serde_json::json!({
                "format_version": 2, "application_id": super::BUNDLE_APPLICATION_ID,
                "schema_version": 1, "source_platform": "linux", "roots": [root.to_string_lossy()],
                "source_thumbs_dir": tmp.path().join("old-thumbs").to_string_lossy()
            }))
            .unwrap()
            .as_bytes(),
        )
        .unwrap();
        zip.finish().unwrap();
        let profile = tmp.path().join("profile");
        let thumbs = profile.join("thumbs");
        fs::create_dir_all(&thumbs).unwrap();
        let target_db = profile.join("media.db");
        let conn = db::open_connection(&target_db).unwrap();
        db::init_schema(&conn).unwrap();
        drop(conn);
        let state = create_test_state(&target_db, &thumbs);
        import_db_bundle(
            archive.to_string_lossy().into_owned(),
            vec![],
            &state,
            &state.database.maintenance().unwrap(),
        )
        .unwrap();
        let conn = db::open_connection(&target_db).unwrap();
        assert!(!db::list_scan_root_settings(&conn).unwrap()[0].auto_scan_on_startup);
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            db::SCHEMA_VERSION
        );
    }

    #[test]
    fn import_db_bundle_restores_database_and_thumbnails() {
        let tmp = tempdir().expect("tempdir");
        let source_db_path = tmp.path().join("source.db");
        let source_root = tmp.path().join("restored-root");
        fs::create_dir_all(&source_root).expect("create restored root");
        let source_thumb_bytes = b"thumb-bytes";

        {
            let conn = db::open_connection(&source_db_path).expect("open source db");
            db::init_schema(&conn).expect("init source schema");
            db::add_scan_root(&conn, &source_root.to_string_lossy()).expect("add restored root");
            let asset_path = source_root.join("asset.jpg");
            fs::write(&asset_path, b"asset").expect("write source asset");
            db::upsert_scanned_asset(
                &conn,
                &new_asset(&asset_path, 10),
                10,
                &source_root.to_string_lossy(),
                1,
            )
            .expect("upsert source asset");
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .expect("checkpoint source");
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

        let profile_dir = tmp.path().join("profile");
        let target_db_path = profile_dir.join("media.db");
        let target_thumbs_dir = profile_dir.join("thumbs");
        fs::create_dir_all(&target_thumbs_dir).expect("create target thumbs dir");
        let conn = db::open_connection(&target_db_path).expect("open target db");
        db::init_schema(&conn).expect("init target schema");
        db::add_scan_root(&conn, "C:/old-root").expect("seed old root");
        drop(conn);

        let state = create_test_state(&target_db_path, &target_thumbs_dir);
        let state_ref = &state;
        let summary = import_db_bundle(
            archive_path.to_string_lossy().to_string(),
            Vec::new(),
            state_ref,
            &state.database.maintenance().unwrap(),
        )
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
        assert_eq!(roots, vec![source_root.to_string_lossy().to_string()]);
        let page = db::list_assets(&conn, 0, 50, &[], &[], None, false).expect("list assets");
        assert_eq!(page.total, 1);
    }

    #[test]
    fn imports_windows_bundle_with_root_mapping_and_preserves_asset_metadata() {
        let tmp = tempdir().expect("tempdir");
        let source_db = tmp.path().join("windows.db");
        let source_root = r"C:\Users\Example\Pictures";
        {
            let conn = db::open_connection(&source_db).expect("open source db");
            db::init_schema(&conn).expect("init source schema");
            db::add_scan_root(&conn, source_root).expect("add source root");
            db::set_scan_root_auto_scan(&conn, source_root, true).expect("enable startup scan");
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
            // Precise nanosecond fingerprint must match the seconds-derived
            // approximation stored by upsert_asset, mirroring the production
            // invariant after any scanned upsert.
            db::upsert_scanned_asset(&conn, &asset, 12_000_000_000, source_root, 1)
                .expect("insert asset");
            db::set_asset_tags(&conn, 1, &["travel".to_string()]).expect("set tags");
            db::set_asset_favorite(&conn, 1, true).expect("set favorite");
            db::set_asset_media_group(&conn, 1, Some("album"), Some(2.0)).expect("set group");
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
            zip.start_file(
                "thumbs/legacy.jpg",
                zip::write::SimpleFileOptions::default(),
            )
            .expect("start thumb");
            zip.write_all(b"legacy-thumbnail").expect("write thumb");
            zip.finish().expect("finish archive");
        }

        let target_root = tmp.path().join("linux-media");
        fs::create_dir_all(target_root.join("album")).expect("create mapped root");
        fs::write(target_root.join("album/photo.jpg"), b"media").expect("write mapped media");
        let profile_dir = tmp.path().join("profile");
        let target_db = profile_dir.join("media.db");
        let target_thumbs = profile_dir.join("thumbs");
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
            &state,
            &state.database.maintenance().unwrap(),
        )
        .expect("import mapped bundle");

        let conn = db::open_connection(&target_db).expect("open restored db");
        let page = db::list_assets(&conn, 0, 10, &[], &[], None, false).expect("list assets");
        assert_eq!(page.total, 1);
        let asset = &page.items[0];
        assert_eq!(asset.id, 1);
        assert_eq!(
            asset.path,
            target_root.join("album/photo.jpg").to_string_lossy()
        );
        assert!(db::list_scan_root_settings(&conn).unwrap()[0].auto_scan_on_startup);
        assert_eq!(asset.tags, vec!["travel".to_string()]);
        assert!(asset.is_favorite);
        assert_eq!(asset.media_group_key.as_deref(), Some("album"));
        assert_eq!(asset.media_group_order, Some(2.0));
        let expected_thumb = target_thumbs
            .join("legacy.jpg")
            .to_string_lossy()
            .to_string();
        assert_eq!(asset.thumb_path.as_deref(), Some(expected_thumb.as_str()));
        assert_eq!(
            db::list_scan_roots(&conn).expect("roots"),
            vec![target_root.to_string_lossy().to_string()]
        );
        assert_eq!(
            db::list_backup_asset_root_mappings(&conn).expect("asset root mappings"),
            vec![(1, target_root.to_string_lossy().into_owned())]
        );
        assert_eq!(
            fs::read(target_thumbs.join("legacy.jpg")).expect("restored thumb"),
            b"legacy-thumbnail"
        );
    }
    #[test]
    fn exported_files_use_zip64_and_roundtrip_through_the_import_reader() {
        use std::io::Read;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("media.db");
        fs::write(&source, b"small ZIP64 fixture").unwrap();
        let path = dir.path().join("bundle.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        super::add_file_to_zip(&mut writer, &source, "media.db").unwrap();
        writer.finish().unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        // Local-header ZIP32 sizes are sentinels; the actual sizes live in ZIP64 extras.
        assert_eq!(&bytes[18..26], &[0xff; 8]);
        let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        super::validate_archive(&mut archive).unwrap();
        let mut restored = Vec::new();
        archive
            .by_name("media.db")
            .unwrap()
            .read_to_end(&mut restored)
            .unwrap();
        assert_eq!(restored, b"small ZIP64 fixture");
    }

    #[test]
    fn archive_limits_share_the_export_import_validator() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bundle.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file("media.db", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"database").unwrap();
        zip.finish().unwrap();
        for (entries, entry_bytes, expanded) in [(0, 100, 100), (10, 1, 100), (10, 100, 1)] {
            let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
            assert!(super::validate_archive_with_limits(
                &mut archive,
                entries,
                entry_bytes,
                expanded
            )
            .is_err());
        }
        assert!(super::validate_archive(
            &mut zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap()
        )
        .is_ok());
    }

    #[test]
    fn interrupted_restore_recovery_selects_the_generation_at_the_commit_boundary() {
        for phase in [
            super::RestorePhase::Swapping,
            super::RestorePhase::Committed,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            fs::write(root.join("media.db"), b"new database").unwrap();
            fs::write(root.join("media.db.restore-previous"), b"old database").unwrap();
            fs::create_dir(root.join("thumbs")).unwrap();
            fs::write(root.join("thumbs/new.jpg"), b"new").unwrap();
            fs::create_dir(root.join("thumbs.restore-previous")).unwrap();
            fs::write(root.join("thumbs.restore-previous/old.jpg"), b"old").unwrap();
            super::write_restore_journal(
                root,
                &super::RestoreJournal {
                    phase,
                    previous_db_existed: true,
                    previous_thumbs_existed: true,
                },
            )
            .unwrap();
            super::recover_interrupted_restore(root).unwrap();
            let committed = phase == super::RestorePhase::Committed;
            assert_eq!(
                fs::read(root.join("media.db")).unwrap(),
                if committed {
                    b"new database"
                } else {
                    b"old database"
                }
            );
            assert_eq!(root.join("thumbs/new.jpg").exists(), committed);
            assert_eq!(root.join("thumbs/old.jpg").exists(), !committed);
            assert!(!super::restore_journal_path(root).exists());
            super::recover_interrupted_restore(root).unwrap();
        }
    }
}
