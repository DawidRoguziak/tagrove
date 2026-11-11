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
    let token = temporary_token();
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
    let app_data = state
        .db_path
        .parent()
        .ok_or("Cannot resolve app data directory")?;
    crate::services::export_target::prepare_export_target(
        raw_path,
        app_data,
        conn,
        "DB export archive",
    )
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
        .join(format!("restore-inspect-{}", temporary_token()));
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

    // Maintenance keeps every other database user out, so this cannot change before the swap.
    let previous_db_existed = state.db_path.exists();
    let staged_result = (|| -> AppResult<(usize, usize, Vec<db::PendingFileOperation>)> {
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
        let carried = {
            let conn = db::open_connection_untracked(&staging_db)?;
            let carried = if previous_db_existed {
                let live = maintenance.connection()?;
                carry_live_file_operations(&live, &conn)?
            } else {
                Vec::new()
            };
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            carried
        };
        remove_database_sidecars(&staging_db)?;
        {
            let conn = db::open_connection_read_only_untracked(&staging_db)?;
            db::validate_restored_generation(&conn, &carried)?;
        }
        Ok((restored_files, restored_thumbnails, carried))
    })();
    let (restored_files, restored_thumbnails, carried) = match staged_result {
        Ok(summary) => summary,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(error);
        }
    };

    let previous_thumbs_existed = state.thumbs_dir.exists();
    let mut journal = RestoreJournal {
        phase: RestorePhase::Swapping,
        previous_db_existed,
        previous_thumbs_existed,
    };
    let prepared = (|| -> AppResult<()> {
        if previous_db_existed {
            let current = maintenance.connection()?;
            current.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        }
        remove_database_sidecars(&state.db_path)?;
        write_restore_journal(app_data_dir, &journal)
    })();
    if let Err(error) = prepared {
        return Err(abandon_staged_restore(app_data_dir, &staging_root, error));
    }

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
        db::validate_restored_generation(&conn, &carried)?;
        db::bump_library_revision(&conn)?;
        let _ = db::current_library_revision(&conn)?;
        let _ = db::list_scan_roots(&conn)?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        drop(conn);
        remove_database_sidecars(&state.db_path)?;

        // The commit point (ADR 0031): once this rename lands, recovery keeps the new
        // generation, so nothing after it may turn the restore into an error.
        journal.phase = RestorePhase::Committed;
        publish_restore_journal(app_data_dir, &journal)
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

    finish_committed_restore(app_data_dir);
    state.database.queries.clear();

    Ok(DbBundleImportSummary {
        restored_files,
        restored_thumbnails,
    })
}

/// Copies the live file-mutation journal into the staged database (ADR 0030).
///
/// A `pending_file_operations` row is the only record of a hidden
/// `.mediatagger-*.pending` file next to the user's media: a committed delete
/// whose staged file could not be removed, or a batch whose rollback failed.
/// The row describes files on this machine, not library content, so it has to
/// outlive the database generation that wrote it. The rows are copied as they
/// are, with their `created_at`; the bundle's own journal was validated as
/// empty, so no key can collide. The copy sits in the staged file before any
/// live path moves: a rolled-back restore keeps the previous generation with
/// its journal untouched, and a committed one starts with the same rows, which
/// startup recovery then handles as it would have without the restore.
fn carry_live_file_operations(
    live: &rusqlite::Connection,
    staged: &rusqlite::Connection,
) -> AppResult<Vec<db::PendingFileOperation>> {
    // Same durability as the connection that wrote the rows.
    staged.pragma_update(None, "synchronous", "FULL")?;
    Ok(db::copy_pending_file_operations(live, staged)?)
}

/// Runs what is left of a restore after its `committed` journal was published
/// (ADR 0031). The new generation is live and recovery keeps it, so a failure
/// here is logged and never returned: the journal stays and the next start
/// finishes the work.
///
/// The previous generation is removed only after the directory sync of the
/// `committed` journal succeeded. When that sync fails, the rename may not be
/// durable yet: after a power loss the `swapping` journal could be the one on
/// disk, and startup needs a complete previous generation to roll back to. A
/// repeated sync that passes would not prove that the first one's entry
/// reached the disk, so the sync is not retried here.
fn finish_committed_restore(app_data_dir: &Path) {
    let journal_path = restore_journal_path(app_data_dir);
    if let Err(error) = sync_parent_directory(&journal_path) {
        eprintln!(
            "Restore installed the new library, but its committed journal could not be synced ({error}); the previous generation is kept until the next start"
        );
        return;
    }
    if let Err(error) = recover_interrupted_restore(app_data_dir) {
        eprintln!(
            "Restore installed the new library, but cleanup of the previous generation failed ({error}); the next start finishes it"
        );
    }
}

/// Cleans up after a failure between a fully staged restore and the first live
/// rename. Nothing live has moved yet. Normally no journal exists and only the
/// staging tree is removed; a journal that was published but whose directory
/// sync failed is rolled back through the same recovery as a failed swap.
fn abandon_staged_restore(
    app_data_dir: &Path,
    staging_root: &Path,
    error: crate::error::AppError,
) -> crate::error::AppError {
    if restore_journal_path(app_data_dir).exists() {
        return match recover_interrupted_restore(app_data_dir) {
            Ok(()) => error,
            Err(recovery_error) => {
                format!("Restore failed: {error}. Automatic rollback also failed: {recovery_error}")
                    .into()
            }
        };
    }
    let _ = fs::remove_dir_all(staging_root);
    error
}

/// Startup-only sweep of the temporaries that bundle export, bundle inspection,
/// journal writes and pre-swap restore staging leave in the profile directory
/// when the process dies before its own cleanup runs. Only direct children of
/// `app_data_dir` whose exact name and file type match what this module
/// creates are removed; symbolic links and every other entry are left alone.
/// It must run after `recover_interrupted_restore` succeeded and before any
/// command can start a new export, inspection or import. Failures are logged
/// and never block startup. Returns the removed paths.
pub fn remove_abandoned_backup_temporaries(app_data_dir: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let entries = match fs::read_dir(app_data_dir) {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("Cannot list the profile directory for backup temporaries: {error}");
            return removed;
        }
    };
    // Without a journal a staging tree never belongs to a live or previous
    // generation. Unexplained previous generations keep it for manual review.
    let staging_is_abandoned = !restore_journal_path(app_data_dir).exists()
        && !app_data_dir.join("media.db.restore-previous").exists()
        && !app_data_dir.join("thumbs.restore-previous").exists();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let directory = match abandoned_backup_temporary_kind(name) {
            Some(TemporaryKind::File) if file_type.is_file() => false,
            Some(TemporaryKind::Directory) if file_type.is_dir() => true,
            Some(TemporaryKind::StagingDirectory) if file_type.is_dir() && staging_is_abandoned => {
                true
            }
            _ => continue,
        };
        let path = entry.path();
        let result = if directory {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match result {
            Ok(()) => removed.push(path),
            Err(error) => eprintln!(
                "Cannot remove abandoned backup temporary '{}': {error}",
                path.display()
            ),
        }
    }
    if !removed.is_empty() {
        if let Err(error) = fs::File::open(app_data_dir).and_then(|dir| dir.sync_all()) {
            eprintln!(
                "Cannot sync the profile directory after removing backup temporaries: {error}"
            );
        }
    }
    removed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TemporaryKind {
    File,
    Directory,
    StagingDirectory,
}

/// Exact names: `backup-snapshot-<pid>-<n>.db` and its `-journal`/`-wal`/`-shm`
/// sidecars, `.restore-journal-<pid>-<n>.tmp`, `restore-inspect-<pid>-<n>` and
/// the fixed `restore-staging`.
fn abandoned_backup_temporary_kind(name: &str) -> Option<TemporaryKind> {
    if name == "restore-staging" {
        return Some(TemporaryKind::StagingDirectory);
    }
    if let Some(token) = name.strip_prefix("restore-inspect-") {
        return is_temporary_token(token).then_some(TemporaryKind::Directory);
    }
    if let Some(token) = name
        .strip_prefix(".restore-journal-")
        .and_then(|rest| rest.strip_suffix(".tmp"))
    {
        return is_temporary_token(token).then_some(TemporaryKind::File);
    }
    let rest = name.strip_prefix("backup-snapshot-")?;
    ["", "-journal", "-wal", "-shm"].iter().find_map(|suffix| {
        let token = rest.strip_suffix(suffix)?.strip_suffix(".db")?;
        is_temporary_token(token).then_some(TemporaryKind::File)
    })
}

/// Unique `<pid>-<n>` part of every temporary name in the profile directory.
fn temporary_token() -> String {
    format!("{}-{}", std::process::id(), rand::random::<u64>())
}

/// Matches exactly what `temporary_token` produces.
fn is_temporary_token(token: &str) -> bool {
    fn decimal(value: &str) -> bool {
        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
    }
    token.split_once('-').is_some_and(|(pid, random)| {
        decimal(pid)
            && decimal(random)
            && pid.parse::<u32>().is_ok()
            && random.parse::<u64>().is_ok()
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
    publish_restore_journal(app_data_dir, journal)?;
    sync_parent_directory(&restore_journal_path(app_data_dir))?;
    Ok(())
}

/// Writes the journal to a synced temporary file and renames it over
/// `restore-journal.json`. An error means the rename did not happen and the
/// previous journal, if any, is still the one in place. The caller syncs the
/// directory.
fn publish_restore_journal(app_data_dir: &Path, journal: &RestoreJournal) -> AppResult<()> {
    let journal_path = restore_journal_path(app_data_dir);
    let temporary = app_data_dir.join(format!(".restore-journal-{}.tmp", temporary_token()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let written = (|| -> AppResult<()> {
        serde_json::to_writer(&mut file, journal).map_err(|error| error.to_string())?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        publish_archive(&temporary, &journal_path)
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
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
    // A thumbnail is a derived cache entry. A reference that cannot be matched
    // to an extracted file is dropped (the asset gets a new thumbnail on
    // demand) and never rejects the bundle: export archives the files it finds
    // and does not check the rows against them.
    let candidate: String = if let Some(manifest) =
        manifest.filter(|value| value.format_version >= 2)
    {
        let Some(relative) = stored_relative_path(old_thumb, &manifest.source_thumbs_dir) else {
            return Ok(None);
        };
        let Some(path) = conn
            .query_row(
                "SELECT path FROM imported_thumbnails WHERE relative = ?1",
                [relative],
                |row| row.get(0),
            )
            .optional()?
        else {
            return Ok(None);
        };
        path
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
        // zip 9 decodes entry names fallibly; an undecodable name rejects the archive.
        let name = entry.name()?.into_owned();
        let safe_path = sanitize_zip_entry_path(&name)?;
        let size = entry.size();
        if size > max_entry_bytes {
            return Err(format!("Backup entry exceeds the size limit: {name}").into());
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
            return Err(format!("Backup entry exceeds the compression ratio limit: {name}").into());
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
    #[cfg(test)]
    sync_fault::check()?;
    let parent = path.parent().ok_or("Path has no parent directory")?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// Test-only fault injection at this module's directory syncs. Every rename and
/// removal of the restore sequence is followed by one, so the n-th sync marks
/// the point right after the n-th step landed on disk.
#[cfg(test)]
mod sync_fault {
    use std::cell::Cell;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum SyncFault {
        None,
        /// The n-th sync from now returns an error once (1 is the next sync).
        Fail(usize),
        /// The n-th sync from now unwinds without running any cleanup code,
        /// which leaves the disk as a killed process would.
        Crash(usize),
        /// The n-th sync from now and every later one return an error, until
        /// the fault is disarmed.
        FailFrom(usize),
    }

    thread_local! {
        static FAULT: Cell<SyncFault> = const { Cell::new(SyncFault::None) };
        static FIRED: Cell<bool> = const { Cell::new(false) };
    }

    pub(super) fn arm(fault: SyncFault) {
        FAULT.with(|current| current.set(fault));
        FIRED.with(|fired| fired.set(false));
    }

    /// Disarms the fault and reports whether it had fired.
    pub(super) fn disarm() -> bool {
        FAULT.with(|current| current.set(SyncFault::None));
        FIRED.with(|fired| fired.replace(false))
    }

    pub(super) fn check() -> crate::error::AppResult<()> {
        match FAULT.with(Cell::get) {
            SyncFault::None => Ok(()),
            SyncFault::Fail(1) => {
                FAULT.with(|current| current.set(SyncFault::None));
                FIRED.with(|fired| fired.set(true));
                Err("injected directory sync failure".into())
            }
            SyncFault::Crash(1) => {
                FAULT.with(|current| current.set(SyncFault::None));
                FIRED.with(|fired| fired.set(true));
                std::panic::resume_unwind(Box::new("injected crash at a directory sync"))
            }
            SyncFault::FailFrom(0 | 1) => {
                FAULT.with(|current| current.set(SyncFault::FailFrom(1)));
                FIRED.with(|fired| fired.set(true));
                Err("injected persistent directory sync failure".into())
            }
            SyncFault::FailFrom(remaining) => {
                FAULT.with(|current| current.set(SyncFault::FailFrom(remaining - 1)));
                Ok(())
            }
            SyncFault::Fail(remaining) => {
                FAULT.with(|current| current.set(SyncFault::Fail(remaining - 1)));
                Ok(())
            }
            SyncFault::Crash(remaining) => {
                FAULT.with(|current| current.set(SyncFault::Crash(remaining - 1)));
                Ok(())
            }
        }
    }
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
    entry: &mut zip::read::ZipFile<'_, fs::File>,
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

    use super::{
        export_db_bundle, import_db_bundle, inspect_db_bundle, mapped_path, sanitize_zip_entry_path,
    };

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
                    .expect("entry name")
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
    fn imports_version_three_bundle_and_backfills_sort_keys() {
        let tmp = tempdir().unwrap();
        let source_db = tmp.path().join("source.db");
        let root = tmp.path().join("media");
        fs::create_dir_all(&root).unwrap();
        {
            let conn = db::open_connection(&source_db).unwrap();
            db::init_schema(&conn).unwrap();
            db::add_scan_root(&conn, &root.to_string_lossy()).unwrap();
            for (name, modified_at) in [("a.jpg", 10), ("b.jpg", 30), ("c.jpg", 20)] {
                let asset = crate::models::NewAsset {
                    path: root.join(name).to_string_lossy().into_owned(),
                    kind: "image".into(),
                    size_bytes: 1,
                    modified_at,
                    width: None,
                    height: None,
                    duration_ms: None,
                    thumb_path: None,
                };
                db::upsert_scanned_asset(&conn, &asset, modified_at, &root.to_string_lossy(), 1)
                    .unwrap();
            }
            db::set_asset_media_group(&conn, 1, Some("trip"), Some(1.0)).unwrap();
            db::set_asset_media_group(&conn, 2, Some("trip"), None).unwrap();
            // A package written before performance_schema_version 4.
            for name in db::SORT_KEY_TRIGGERS {
                conn.execute_batch(&format!("DROP TRIGGER {name};"))
                    .unwrap();
            }
            conn.execute_batch(
                "DROP INDEX idx_assets_sort_key;
                 DROP INDEX idx_assets_group_bucket;
                 ALTER TABLE assets DROP COLUMN sort_bucket_time;
                 ALTER TABLE assets DROP COLUMN sort_bucket_id;
                 ALTER TABLE assets DROP COLUMN sort_group_null_order;
                 UPDATE library_metadata SET value = 3 WHERE key = 'performance_schema_version';
                 PRAGMA wal_checkpoint(TRUNCATE);",
            )
            .unwrap();
        }
        let archive = tmp.path().join("v3.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        zip.start_file("media.db", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&fs::read(&source_db).unwrap()).unwrap();
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            serde_json::to_string(&serde_json::json!({
                "format_version": 2, "application_id": super::BUNDLE_APPLICATION_ID,
                "schema_version": 3, "source_platform": "linux", "roots": [root.to_string_lossy()],
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
        assert_eq!(db::count_inconsistent_sort_keys(&conn).unwrap(), 0);
        assert_eq!(
            db::list_ordered_asset_ids_with_meta(&conn, &[], &[], None, false, None).unwrap(),
            vec![1, 2, 3]
        );
        assert_eq!(
            conn.query_row(
                "SELECT value FROM library_metadata WHERE key = 'performance_schema_version'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            4
        );
    }

    #[test]
    fn import_db_bundle_restores_database_and_thumbnails() {
        for with_markers in [false, true] {
            let tmp = tempdir().expect("tempdir");
            let source_db_path = tmp.path().join("source.db");
            let source_root = tmp.path().join("restored-root");
            fs::create_dir_all(&source_root).expect("create restored root");
            let source_thumb_bytes = b"thumb-bytes";

            {
                let conn = db::open_connection(&source_db_path).expect("open source db");
                db::init_schema(&conn).expect("init source schema");
                db::add_scan_root(&conn, &source_root.to_string_lossy())
                    .expect("add restored root");
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
                conn.execute(
                    "DELETE FROM library_metadata WHERE substr(key,1,15) = 'query_revision:'",
                    [],
                )
                .unwrap();
                if with_markers {
                    conn.execute_batch("INSERT INTO library_metadata VALUES ('query_revision:general', 9999), ('query_revision:tag:obsolete', 9999)").unwrap();
                }
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
            let filters = crate::services::asset_query_service::AssetQueryFilters {
                tags_and: vec![],
                tags_not: vec![],
                kind: None,
                favorites_only: false,
                meta_filter: None,
            };
            let manager = &state.database.queries;
            let old_session = {
                let permit = state.database.admit().unwrap();
                let crate::models::StartAssetQueryResult::Ready { session_id, .. } = manager
                    .start(
                        &permit,
                        filters.clone(),
                        10,
                        manager.begin_request("backup-test", 1),
                        1,
                    )
                    .unwrap()
                else {
                    panic!("ready")
                };
                session_id
            };
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
            let page = db::test_support::session_page(&conn, 0, 50, &[], &[], None, false)
                .expect("list assets");
            assert_eq!(page.total, 1);
            let revision = db::current_library_revision(&conn).unwrap();
            assert_eq!(
                db::query_dependency_revision(&conn, &["general".into()]).unwrap(),
                revision
            );
            assert_eq!(
                db::query_dependency_revision(&conn, &["favorites".into(), "tag_count".into()])
                    .unwrap(),
                revision - 1
            );
            assert_eq!(
                db::query_dependency_revision(&conn, &["tag:obsolete".into()]).unwrap(),
                0
            );
            let permit = state.database.admit().unwrap();
            assert_eq!(
                manager.page(&permit, old_session, 0, 10).unwrap(),
                crate::models::AssetQueryPageResult::Stale
            );
            let crate::models::StartAssetQueryResult::Ready {
                total, session_id, ..
            } = manager
                .start(
                    &permit,
                    filters,
                    10,
                    manager.begin_request("backup-test", 2),
                    2,
                )
                .unwrap()
            else {
                panic!("ready")
            };
            assert_eq!(total, 1);
            assert_ne!(session_id, old_session);
        }
    }

    /// A markerless library from before `assets.file_name` existed: the legacy
    /// core tables only, with one tagged asset below `root`.
    fn create_legacy_library_without_file_name(db_path: &Path, root: &Path, thumb_path: &str) {
        let conn = rusqlite::Connection::open(db_path).expect("open legacy db");
        conn.execute_batch(
            "
            CREATE TABLE assets (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              path TEXT NOT NULL UNIQUE,
              kind TEXT NOT NULL,
              size_bytes INTEGER NOT NULL,
              modified_at INTEGER NOT NULL,
              width INTEGER,
              height INTEGER,
              duration_ms INTEGER,
              thumb_path TEXT,
              indexed_at INTEGER NOT NULL DEFAULT (unixepoch())
            );
            CREATE TABLE tags (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              name TEXT NOT NULL UNIQUE COLLATE NOCASE
            );
            CREATE TABLE asset_tags (
              asset_id INTEGER NOT NULL,
              tag_id INTEGER NOT NULL,
              PRIMARY KEY(asset_id, tag_id),
              FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE,
              FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
            );
            CREATE TABLE scan_roots (path TEXT PRIMARY KEY);
            CREATE TABLE thumbnail_failures (
              asset_id INTEGER PRIMARY KEY,
              failure_count INTEGER NOT NULL,
              last_error TEXT,
              last_failed_at INTEGER NOT NULL,
              asset_modified_at INTEGER NOT NULL
            );
            CREATE TABLE library_metadata (key TEXT PRIMARY KEY, value INTEGER NOT NULL);
            INSERT INTO library_metadata VALUES ('revision', 1), ('performance_schema_version', 0);
            INSERT INTO tags(name) VALUES ('travel');
            ",
        )
        .expect("legacy schema");
        conn.execute(
            "INSERT INTO scan_roots(path) VALUES (?1)",
            [root.to_string_lossy()],
        )
        .expect("legacy root");
        conn.execute(
            "INSERT INTO assets(path, kind, size_bytes, modified_at, thumb_path)
             VALUES (?1, 'image', 10, 20, ?2)",
            rusqlite::params![root.join("album/Photo.JPG").to_string_lossy(), thumb_path],
        )
        .expect("legacy asset");
        conn.execute("INSERT INTO asset_tags VALUES (1, 1)", [])
            .expect("legacy tag mapping");
    }

    fn empty_profile(dir: &Path) -> (PathBuf, PathBuf) {
        let db_path = dir.join("media.db");
        let thumbs_dir = dir.join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");
        let conn = db::open_connection(&db_path).expect("open profile db");
        db::init_schema(&conn).expect("init profile schema");
        (db_path, thumbs_dir)
    }

    fn restored_asset(
        conn: &rusqlite::Connection,
    ) -> (String, String, Option<String>, Vec<String>) {
        let (file_name, file_name_key, thumb_path) = conn
            .query_row(
                "SELECT file_name, file_name_key, thumb_path FROM assets WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("restored asset");
        (
            file_name,
            file_name_key,
            thumb_path,
            db::list_asset_tags(conn, 1).expect("restored tags"),
        )
    }

    #[test]
    fn restores_a_legacy_bundle_without_file_name_and_its_later_backup() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("media");
        fs::create_dir_all(&root).expect("create root");
        let legacy_db = tmp.path().join("legacy.db");
        create_legacy_library_without_file_name(&legacy_db, &root, "/old-profile/thumbs/photo.jpg");

        let legacy_archive = tmp.path().join("legacy.zip");
        {
            let mut zip = zip::ZipWriter::new(
                fs::File::create(&legacy_archive).expect("create legacy archive"),
            );
            zip.start_file("media.db", zip::write::SimpleFileOptions::default())
                .expect("start media db");
            zip.write_all(&fs::read(&legacy_db).expect("read legacy db"))
                .expect("write media db");
            zip.start_file("thumbs/photo.jpg", zip::write::SimpleFileOptions::default())
                .expect("start thumb");
            zip.write_all(b"thumb").expect("write thumb");
            zip.finish().expect("finish legacy archive");
        }

        let (db_path, thumbs_dir) = empty_profile(&tmp.path().join("profile"));
        let state = create_test_state(&db_path, &thumbs_dir);
        let inspection = inspect_db_bundle(legacy_archive.to_string_lossy().into_owned(), &state)
            .expect("inspect legacy bundle");
        assert_eq!(inspection.format_version, None);
        assert_eq!(inspection.roots, vec![root.to_string_lossy().into_owned()]);

        let summary = import_db_bundle(
            legacy_archive.to_string_lossy().into_owned(),
            Vec::new(),
            &state,
            &state.database.maintenance().expect("maintenance"),
        )
        .expect("import legacy bundle");
        assert_eq!(summary.restored_thumbnails, 1);

        let expected = (
            "Photo.JPG".to_string(),
            "photo.jpg".to_string(),
            Some(thumbs_dir.join("photo.jpg").to_string_lossy().into_owned()),
            vec!["travel".to_string()],
        );
        {
            let conn = db::open_connection(&db_path).expect("open restored db");
            assert_eq!(restored_asset(&conn), expected);
            assert!(
                conn.query_row(
                    "SELECT \"notnull\" FROM pragma_table_info('assets') WHERE name = 'file_name'",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .expect("file_name column"),
                "the migrated column must match the created schema"
            );
            assert_eq!(
                db::validate_backup_database(&conn, false).expect("restored library validates"),
                db::BackupSchemaCompatibility::Current
            );
        }

        // The library migrated from the legacy shape must restore its own backup.
        let later_archive = tmp.path().join("later.zip");
        export_db_bundle(
            later_archive.to_string_lossy().into_owned(),
            &state,
            &state.database.maintenance().expect("maintenance"),
        )
        .expect("export migrated library");
        let inspection = inspect_db_bundle(later_archive.to_string_lossy().into_owned(), &state)
            .expect("inspect later bundle");
        assert_eq!(
            inspection.format_version,
            Some(super::BUNDLE_FORMAT_VERSION)
        );
        import_db_bundle(
            later_archive.to_string_lossy().into_owned(),
            Vec::new(),
            &state,
            &state.database.maintenance().expect("maintenance"),
        )
        .expect("import later bundle");
        let conn = db::open_connection(&db_path).expect("open restored db");
        assert_eq!(restored_asset(&conn), expected);
        assert_eq!(
            fs::read(thumbs_dir.join("photo.jpg")).expect("restored thumb"),
            b"thumb"
        );
    }

    #[test]
    fn restores_a_backup_of_a_library_whose_file_name_column_an_earlier_build_added() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("media");
        fs::create_dir_all(&root).expect("create root");
        let profile = tmp.path().join("profile");
        let db_path = profile.join("media.db");
        let thumbs_dir = profile.join("thumbs");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");
        create_legacy_library_without_file_name(&db_path, &root, "/old-profile/thumbs/photo.jpg");
        {
            // Startup of a build that still added the column as nullable TEXT.
            let conn = db::open_connection(&db_path).expect("open live db");
            conn.execute_batch("ALTER TABLE assets ADD COLUMN file_name TEXT;")
                .expect("earlier migration");
            db::init_schema(&conn).expect("init schema");
            assert!(
                !conn
                    .query_row(
                        "SELECT \"notnull\" FROM pragma_table_info('assets') WHERE name = 'file_name'",
                        [],
                        |row| row.get::<_, bool>(0),
                    )
                    .expect("file_name column"),
                "the fixture must keep the nullable column"
            );
        }

        let state = create_test_state(&db_path, &thumbs_dir);
        let archive = tmp.path().join("backup.zip");
        export_db_bundle(
            archive.to_string_lossy().into_owned(),
            &state,
            &state.database.maintenance().expect("maintenance"),
        )
        .expect("export migrated library");
        inspect_db_bundle(archive.to_string_lossy().into_owned(), &state)
            .expect("inspect its own backup");

        {
            // The restore must bring this state back.
            let conn = db::open_connection(&db_path).expect("open live db");
            db::set_asset_tags(&conn, 1, &["changed".to_string()]).expect("change tags");
        }
        import_db_bundle(
            archive.to_string_lossy().into_owned(),
            Vec::new(),
            &state,
            &state.database.maintenance().expect("maintenance"),
        )
        .expect("import its own backup");

        let conn = db::open_connection(&db_path).expect("open restored db");
        assert_eq!(
            restored_asset(&conn),
            (
                "Photo.JPG".to_string(),
                "photo.jpg".to_string(),
                // The exported thumbnail directory was empty.
                None,
                vec!["travel".to_string()],
            )
        );
    }

    #[test]
    fn format_two_import_drops_unresolved_thumbnail_references_instead_of_rejecting() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("media");
        fs::create_dir_all(&root).expect("create root");
        let (source_db, source_thumbs) = empty_profile(&tmp.path().join("source-profile"));
        fs::create_dir_all(source_thumbs.join("kept")).expect("create kept dir");
        fs::write(source_thumbs.join("kept/a.jpg"), b"kept").expect("write kept thumb");
        let references = [
            // Archived: the only reference that can be resolved.
            source_thumbs.join("kept/a.jpg"),
            // Inside the thumbnail directory, but the file is gone.
            source_thumbs.join("gone/b.jpg"),
            // Left over from a profile that lived somewhere else.
            tmp.path().join("moved-profile/thumbs/kept/a.jpg"),
            // Lexically below the directory, but not a path inside it.
            source_thumbs.join("../kept/a.jpg"),
        ];
        {
            let conn = db::open_connection(&source_db).expect("open source db");
            db::add_scan_root(&conn, &root.to_string_lossy()).expect("add root");
            for (index, reference) in references.iter().enumerate() {
                let asset = new_asset(&root.join(format!("asset-{index}.jpg")), 10);
                db::upsert_scanned_asset(&conn, &asset, 10, &root.to_string_lossy(), 1)
                    .expect("insert asset");
                conn.execute(
                    "UPDATE assets SET thumb_path = ?1 WHERE path = ?2",
                    rusqlite::params![reference.to_string_lossy(), asset.path],
                )
                .expect("store thumbnail reference");
            }
            db::record_thumbnail_failure(&conn, 2, 10, Some("missing")).expect("failure marker");
        }
        let source_state = create_test_state(&source_db, &source_thumbs);
        let archive = tmp.path().join("backup.zip");
        let exported = export_db_bundle(
            archive.to_string_lossy().into_owned(),
            &source_state,
            &source_state.database.maintenance().expect("maintenance"),
        )
        .expect("export bundle");
        assert_eq!(exported.copied_thumbnails, 1);

        let (target_db, target_thumbs) = empty_profile(&tmp.path().join("target-profile"));
        let state = create_test_state(&target_db, &target_thumbs);
        let inspection = inspect_db_bundle(archive.to_string_lossy().into_owned(), &state)
            .expect("inspect bundle");
        assert_eq!(
            inspection.format_version,
            Some(super::BUNDLE_FORMAT_VERSION)
        );
        let summary = import_db_bundle(
            archive.to_string_lossy().into_owned(),
            Vec::new(),
            &state,
            &state.database.maintenance().expect("maintenance"),
        )
        .expect("import bundle with unresolved thumbnail references");
        assert_eq!(summary.restored_thumbnails, 1);

        let conn = db::open_connection(&target_db).expect("open restored db");
        let restored = conn
            .prepare("SELECT thumb_path FROM assets ORDER BY id")
            .expect("prepare")
            .query_map([], |row| row.get::<_, Option<String>>(0))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("thumbnail references");
        assert_eq!(
            restored,
            vec![
                Some(
                    target_thumbs
                        .join("kept/a.jpg")
                        .to_string_lossy()
                        .into_owned()
                ),
                None,
                None,
                None,
            ]
        );
        assert_eq!(
            fs::read(target_thumbs.join("kept/a.jpg")).expect("restored thumb"),
            b"kept"
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM thumbnail_failures", [], |row| row
                .get::<_, i64>(0))
                .expect("failure markers"),
            0
        );
        assert!(!tmp.path().join("target-profile/restore-staging").exists());
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
        let page = db::test_support::session_page(&conn, 0, 10, &[], &[], None, false)
            .expect("list assets");
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
    fn rejected_export_targets_leave_no_new_directory() {
        let tmp = tempdir().expect("tempdir");
        let profile_dir = tmp.path().join("profile");
        let db_path = profile_dir.join("media.db");
        let thumbs_dir = profile_dir.join("thumbs");
        let root = tmp.path().join("media");
        fs::create_dir_all(&thumbs_dir).expect("create thumbs dir");
        fs::create_dir_all(&root).expect("create root");
        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        db::add_scan_root(&conn, root.to_str().unwrap()).expect("add root");
        drop(conn);
        let state = create_test_state(&db_path, &thumbs_dir);
        let profile_before = names_in(&profile_dir);

        for (target, reason) in [
            (
                profile_dir.join("new/deeper/backup.zip"),
                "inside the active profile",
            ),
            (
                root.join("new/deeper/backup.zip"),
                "inside an indexed source root",
            ),
        ] {
            let error = export_db_bundle(
                target.to_string_lossy().to_string(),
                &state,
                &state.database.maintenance().unwrap(),
            )
            .expect_err("protected target");
            assert!(error.to_string().contains(reason), "{error}");
        }

        assert_eq!(names_in(&profile_dir), profile_before);
        assert!(names_in(&root).is_empty());
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("list dir")
            .map(|entry| entry.expect("entry").file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn temporary_names_written_by_this_module_match_the_startup_sweep() {
        use super::{abandoned_backup_temporary_kind, temporary_token, TemporaryKind};
        for _ in 0..64 {
            let token = temporary_token();
            for name in [
                format!("backup-snapshot-{token}.db"),
                format!("backup-snapshot-{token}.db-journal"),
                format!("backup-snapshot-{token}.db-wal"),
                format!("backup-snapshot-{token}.db-shm"),
                format!(".restore-journal-{token}.tmp"),
            ] {
                assert_eq!(
                    abandoned_backup_temporary_kind(&name),
                    Some(TemporaryKind::File),
                    "{name}"
                );
            }
            assert_eq!(
                abandoned_backup_temporary_kind(&format!("restore-inspect-{token}")),
                Some(TemporaryKind::Directory)
            );
        }
        for name in [
            "media.db",
            "media.db-wal",
            "media.db.restore-previous",
            "thumbs",
            "thumbs.restore-previous",
            "restore-journal.json",
            "instance.lock",
            "backup-snapshot-.db",
            "backup-snapshot-1.db",
            "backup-snapshot-1-2.db.bak",
            "backup-snapshot-1-2.dbx",
            "backup-snapshot-1-2-3.db",
            "backup-snapshot-a-2.db",
            "backup-snapshot--1-2.db",
            "backup-snapshot-+1-2.db",
            "backup-snapshot-4294967296-2.db",
            "backup-snapshot-1-18446744073709551616.db",
            "backup-snapshot-1-2.db-journal.keep",
            "Backup-snapshot-1-2.db",
            "restore-inspect-1",
            "restore-inspect-1-2-extra",
            "restore-inspect-1-2/",
            "restore-staging-old",
            ".restore-journal-1-2.tmp.keep",
            ".restore-journal-1-2",
            "restore-journal-1-2.tmp",
        ] {
            assert_eq!(abandoned_backup_temporary_kind(name), None, "{name}");
        }
    }

    #[test]
    fn startup_sweep_removes_only_abandoned_backup_temporaries() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path().join("profile");
        let (db_path, thumbs_dir) = empty_profile(&profile);
        fs::write(thumbs_dir.join("kept.jpg"), b"thumb").unwrap();
        fs::write(profile.join("instance.lock"), b"").unwrap();
        // Leftovers of a killed export, inspection, journal write and pre-swap staging.
        for name in [
            "backup-snapshot-123-456.db",
            "backup-snapshot-123-456.db-journal",
            "backup-snapshot-123-456.db-wal",
            "backup-snapshot-123-456.db-shm",
            ".restore-journal-123-789.tmp",
        ] {
            fs::write(profile.join(name), b"leftover").unwrap();
        }
        fs::create_dir_all(profile.join("restore-inspect-123-456")).unwrap();
        fs::write(profile.join("restore-inspect-123-456/media.db"), b"copy").unwrap();
        fs::create_dir_all(profile.join("restore-staging/thumbs/nested")).unwrap();
        fs::write(profile.join("restore-staging/media.db"), b"copy").unwrap();
        fs::write(profile.join("restore-staging/thumbs/nested/a.jpg"), b"a").unwrap();
        // Lookalikes and wrong types stay.
        fs::write(profile.join("backup-snapshot-123-456.db.bak"), b"user").unwrap();
        fs::write(profile.join("restore-inspect-1-2"), b"a file, not a dir").unwrap();
        fs::create_dir_all(profile.join("backup-snapshot-1-2.db")).unwrap();
        fs::write(profile.join("backup-snapshot-1-2.db/inside"), b"x").unwrap();
        fs::write(profile.join("restore-staging-old"), b"user").unwrap();
        fs::write(profile.join("notes.txt"), b"user").unwrap();

        // Symbolic links are never followed or removed.
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("precious.db"), b"precious").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&outside, profile.join("restore-inspect-7-8")).unwrap();
            symlink(
                outside.join("precious.db"),
                profile.join("backup-snapshot-7-8.db"),
            )
            .unwrap();
        }

        let mut removed: Vec<String> = super::remove_abandoned_backup_temporaries(&profile)
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        removed.sort();
        assert_eq!(
            removed,
            vec![
                ".restore-journal-123-789.tmp",
                "backup-snapshot-123-456.db",
                "backup-snapshot-123-456.db-journal",
                "backup-snapshot-123-456.db-shm",
                "backup-snapshot-123-456.db-wal",
                "restore-inspect-123-456",
                "restore-staging",
            ]
        );
        let mut expected = vec![
            "backup-snapshot-1-2.db",
            "backup-snapshot-123-456.db.bak",
            "instance.lock",
            "media.db",
            "notes.txt",
            "restore-inspect-1-2",
            "restore-staging-old",
            "thumbs",
        ];
        if cfg!(unix) {
            expected.extend(["backup-snapshot-7-8.db", "restore-inspect-7-8"]);
        }
        expected.sort();
        let survivors = names_in(&profile);
        let survivors: Vec<&str> = survivors
            .iter()
            .map(String::as_str)
            .filter(|name| !name.starts_with("media.db-"))
            .collect();
        assert_eq!(survivors, expected);
        assert_eq!(
            fs::read(profile.join("backup-snapshot-1-2.db/inside")).unwrap(),
            b"x"
        );
        assert_eq!(fs::read(outside.join("precious.db")).unwrap(), b"precious");
        assert!(thumbs_dir.join("kept.jpg").is_file());
        db::open_connection(&db_path).expect("live database still opens");

        assert!(super::remove_abandoned_backup_temporaries(&profile).is_empty());
        assert!(super::remove_abandoned_backup_temporaries(&tmp.path().join("missing")).is_empty());
    }

    #[test]
    fn startup_sweep_keeps_staging_while_restore_artifacts_exist() {
        for artifact in [
            "restore-journal.json",
            "media.db.restore-previous",
            "thumbs.restore-previous",
        ] {
            let tmp = tempdir().expect("tempdir");
            let profile = tmp.path();
            fs::create_dir_all(profile.join("restore-staging")).unwrap();
            fs::write(profile.join("restore-staging/media.db"), b"copy").unwrap();
            if artifact.starts_with("thumbs") {
                fs::create_dir_all(profile.join(artifact)).unwrap();
            } else {
                fs::write(profile.join(artifact), b"artifact").unwrap();
            }
            fs::create_dir_all(profile.join("restore-inspect-1-2")).unwrap();

            let removed = super::remove_abandoned_backup_temporaries(profile);
            assert_eq!(
                removed,
                vec![profile.join("restore-inspect-1-2")],
                "{artifact}"
            );
            assert!(
                profile.join("restore-staging/media.db").is_file(),
                "{artifact}"
            );
            assert!(profile.join(artifact).exists(), "{artifact}");
        }
    }

    #[test]
    fn failed_journal_write_removes_its_temporary_file() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path();
        // A non-empty directory in place of the journal makes the publishing rename fail.
        fs::create_dir_all(profile.join("restore-journal.json/blocker")).unwrap();
        let error = super::write_restore_journal(
            profile,
            &super::RestoreJournal {
                phase: super::RestorePhase::Swapping,
                previous_db_existed: true,
                previous_thumbs_existed: true,
            },
        );
        assert!(error.is_err());
        assert_eq!(names_in(profile), vec!["restore-journal.json".to_string()]);
    }

    #[test]
    fn pre_swap_failure_removes_restore_staging_and_keeps_the_live_library() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("media");
        fs::create_dir_all(&root).unwrap();
        let (source_db, source_thumbs) = empty_profile(&tmp.path().join("source"));
        fs::write(source_thumbs.join("t.jpg"), b"thumb").unwrap();
        let source = create_test_state(&source_db, &source_thumbs);
        let archive = tmp.path().join("bundle.zip");
        export_db_bundle(
            archive.to_string_lossy().into_owned(),
            &source,
            &source.database.maintenance().expect("maintenance"),
        )
        .expect("export bundle");

        let profile = tmp.path().join("profile");
        let (db_path, thumbs_dir) = empty_profile(&profile);
        {
            let conn = db::open_connection(&db_path).unwrap();
            db::add_scan_root(&conn, &root.to_string_lossy()).unwrap();
        }
        fs::write(thumbs_dir.join("live.jpg"), b"live").unwrap();
        // A directory where the live WAL belongs makes the live database unreadable:
        // reading its file-mutation journal fails after extraction, validation and
        // path rewriting of the staged database, and before any restore journal.
        let wal = profile.join("media.db-wal");
        let _ = fs::remove_file(&wal);
        let _ = fs::remove_file(profile.join("media.db-shm"));
        fs::create_dir_all(wal.join("blocker")).unwrap();

        let state = create_test_state(&db_path, &thumbs_dir);
        let result = import_db_bundle(
            archive.to_string_lossy().into_owned(),
            Vec::new(),
            &state,
            &state.database.maintenance().expect("maintenance"),
        );
        assert!(result.is_err(), "reading the live database must fail");
        assert_eq!(
            names_in(&profile),
            vec!["media.db", "media.db-wal", "thumbs"],
            "no staging, journal, journal temporary or previous generation may remain"
        );
        assert_eq!(fs::read(thumbs_dir.join("live.jpg")).unwrap(), b"live");

        fs::remove_dir_all(&wal).unwrap();
        {
            let conn = db::open_connection(&db_path).unwrap();
            assert_eq!(
                db::list_scan_roots(&conn).unwrap(),
                vec![root.to_string_lossy().into_owned()]
            );
        }
        // The same bundle installs once the live database can be checkpointed.
        import_db_bundle(
            archive.to_string_lossy().into_owned(),
            Vec::new(),
            &state,
            &state.database.maintenance().expect("maintenance"),
        )
        .expect("import after the fault is gone");
        let conn = db::open_connection(&db_path).unwrap();
        assert!(db::list_scan_roots(&conn).unwrap().is_empty());
        assert_eq!(fs::read(thumbs_dir.join("t.jpg")).unwrap(), b"thumb");
        assert!(!profile.join("restore-staging").exists());
    }

    #[test]
    fn abandoned_staged_restore_rolls_back_a_published_journal() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path();
        fs::write(profile.join("media.db"), b"live database").unwrap();
        fs::create_dir_all(profile.join("thumbs")).unwrap();
        fs::write(profile.join("thumbs/live.jpg"), b"live").unwrap();
        let staging = profile.join("restore-staging");
        fs::create_dir_all(staging.join("thumbs")).unwrap();
        fs::write(staging.join("media.db"), b"staged").unwrap();
        // The journal rename landed but its directory sync reported an error.
        super::write_restore_journal(
            profile,
            &super::RestoreJournal {
                phase: super::RestorePhase::Swapping,
                previous_db_existed: true,
                previous_thumbs_existed: true,
            },
        )
        .unwrap();

        let error =
            super::abandon_staged_restore(profile, &staging, "directory sync failed".into());
        assert_eq!(error.to_string(), "directory sync failed");
        assert_eq!(names_in(profile), vec!["media.db", "thumbs"]);
        assert_eq!(
            fs::read(profile.join("media.db")).unwrap(),
            b"live database"
        );
        assert_eq!(fs::read(profile.join("thumbs/live.jpg")).unwrap(), b"live");

        // Without a journal only the staging tree is removed.
        fs::create_dir_all(staging.join("thumbs")).unwrap();
        let error = super::abandon_staged_restore(profile, &staging, "checkpoint failed".into());
        assert_eq!(error.to_string(), "checkpoint failed");
        assert_eq!(names_in(profile), vec!["media.db", "thumbs"]);
    }

    /// What `lib.rs` setup does with the profile before any command can run:
    /// restore-journal recovery, the temporaries sweep, then `init_schema` and
    /// file-mutation journal recovery on a FULL-sync connection.
    fn start_profile(profile: &Path) -> (rusqlite::Connection, anyhow::Result<()>) {
        super::recover_interrupted_restore(profile).expect("restore journal recovery");
        super::remove_abandoned_backup_temporaries(profile);
        assert_eq!(
            names_in(profile),
            vec!["media.db", "thumbs"],
            "startup leaves exactly one generation"
        );
        let conn = db::open_connection(&profile.join("media.db")).expect("open profile db");
        db::init_schema(&conn).expect("init schema at startup");
        conn.pragma_update(None, "synchronous", "FULL").unwrap();
        let recovery =
            crate::services::asset_mutation_service::recover_pending_file_operations(&conn);
        (conn, recovery)
    }

    /// The journal with `created_at`, in the order recovery walks it.
    fn journal_rows(db_path: &Path) -> Vec<(db::PendingFileOperation, i64)> {
        let conn = db::open_connection(db_path).expect("open db");
        let operations = db::list_pending_file_operations(&conn).expect("list journal");
        let created_at: Vec<i64> = conn
            .prepare(
                "SELECT created_at FROM pending_file_operations
                 ORDER BY created_at, operation_id, asset_id",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        operations.into_iter().zip(created_at).collect()
    }

    /// Exports a bundle of a library that has `roots` and no assets.
    fn bundle_with_roots(dir: &Path, roots: &[&Path]) -> PathBuf {
        let (source_db, source_thumbs) = empty_profile(&dir.join("bundle-source"));
        fs::write(source_thumbs.join("bundled.jpg"), b"thumb").unwrap();
        {
            let conn = db::open_connection(&source_db).unwrap();
            for root in roots {
                db::add_scan_root(&conn, &root.to_string_lossy()).unwrap();
            }
        }
        let source = create_test_state(&source_db, &source_thumbs);
        let archive = dir.join("bundle.zip");
        export_db_bundle(
            archive.to_string_lossy().into_owned(),
            &source,
            &source.database.maintenance().expect("maintenance"),
        )
        .expect("export bundle");
        archive
    }

    /// A live profile whose library has `root`, one deleted asset and that
    /// delete's committed journal row; the hidden staging file still holds the
    /// bytes (`cleanup_pending`). Returns the staging path.
    fn profile_with_a_committed_delete_awaiting_cleanup(profile: &Path, root: &Path) -> PathBuf {
        fs::create_dir_all(root).unwrap();
        let original = root.join("photo.jpg");
        let staging = root.join(".mediatagger-0123456789abcdefghijklmn.pending");
        fs::write(&original, b"deleted content").unwrap();
        let (db_path, thumbs_dir) = empty_profile(profile);
        fs::write(thumbs_dir.join("live.jpg"), b"live").unwrap();
        let conn = db::open_connection(&db_path).unwrap();
        conn.pragma_update(None, "synchronous", "FULL").unwrap();
        let root_text = root.to_string_lossy();
        db::add_scan_root(&conn, &root_text).unwrap();
        db::upsert_scanned_asset(&conn, &new_asset(&original, 1), 1, &root_text, 1).unwrap();
        let asset = db::get_file_mutation_asset(&conn, 1).unwrap().unwrap();
        db::insert_pending_file_operations(
            &conn,
            &[db::PendingFileOperation {
                operation_id: "cleanup-pending-delete".to_string(),
                asset_id: asset.id,
                action: "delete".to_string(),
                original_path: asset.path.clone(),
                staging_path: staging.to_string_lossy().into_owned(),
                final_path: None,
                committed: false,
            }],
        )
        .unwrap();
        fs::rename(&original, &staging).unwrap();
        db::apply_file_mutations(
            &conn,
            "cleanup-pending-delete",
            &[db::DbFileMutation::Delete {
                asset_id: asset.id,
                expected_path: &asset.path,
                expected_record_version: asset.record_version,
            }],
        )
        .unwrap();
        // A distinct timestamp shows that the copy keeps it.
        conn.execute("UPDATE pending_file_operations SET created_at = 1234", [])
            .unwrap();
        staging
    }

    fn import(archive: &Path, state: &AppState) -> crate::error::AppResult<()> {
        import_db_bundle(
            archive.to_string_lossy().into_owned(),
            Vec::new(),
            state,
            &state.database.maintenance().expect("maintenance"),
        )
        .map(|_| ())
    }

    fn scan_roots_of(db_path: &Path) -> Vec<String> {
        db::list_scan_roots(&db::open_connection(db_path).unwrap()).unwrap()
    }

    #[test]
    fn restore_carries_a_committed_delete_and_a_later_start_finishes_its_cleanup() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("media");
        let other_root = tmp.path().join("other-media");
        fs::create_dir_all(&other_root).unwrap();
        let profile = tmp.path().join("profile");
        let staging = profile_with_a_committed_delete_awaiting_cleanup(&profile, &root);
        let archive = bundle_with_roots(tmp.path(), &[&root, &other_root]);
        let db_path = profile.join("media.db");
        let thumbs_dir = profile.join("thumbs");
        let before = journal_rows(&db_path);
        assert_eq!(before.len(), 1);
        assert!(before[0].0.committed);
        assert_eq!(before[0].1, 1234);

        let state = create_test_state(&db_path, &thumbs_dir);
        import(&archive, &state).expect("a retained journal row must not block restore");

        assert_eq!(scan_roots_of(&db_path).len(), 2, "the bundle is installed");
        assert!(thumbs_dir.join("bundled.jpg").exists());
        assert_eq!(journal_rows(&db_path), before, "the row moved unchanged");
        assert_eq!(fs::read(&staging).unwrap(), b"deleted content");
        assert_eq!(names_in(&profile), vec!["media.db", "thumbs"]);

        // The restored library is still not exportable, as before the restore.
        let export_target = tmp.path().join("after-restore.zip");
        let export = |state: &AppState| {
            export_db_bundle(
                export_target.to_string_lossy().into_owned(),
                state,
                &state.database.maintenance().expect("maintenance"),
            )
        };
        let error = export(&state).expect_err("pending rows keep rejecting export");
        assert!(error.to_string().contains("pending local file operations"));
        assert!(!export_target.exists());

        // A second restore carries the same row again, once.
        import(&archive, &state).expect("second restore");
        assert_eq!(journal_rows(&db_path), before);

        let (conn, recovery) = start_profile(&profile);
        recovery.expect("startup after restore");
        assert!(!staging.exists(), "the root is registered, so cleanup runs");
        assert!(db::list_pending_file_operations(&conn).unwrap().is_empty());
        drop(conn);

        export(&state).expect("export works once the journal is empty");
        // The exported bundle carries no journal and restores into a clean profile.
        let (clean_db, clean_thumbs) = empty_profile(&tmp.path().join("clean-profile"));
        let clean = create_test_state(&clean_db, &clean_thumbs);
        import(&export_target, &clean).expect("restore the later export");
        assert!(journal_rows(&clean_db).is_empty());
    }

    #[test]
    fn restore_carries_uncommitted_and_rename_rows_for_startup_recovery() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("media");
        fs::create_dir_all(&root).unwrap();
        let profile = tmp.path().join("profile");
        let (db_path, thumbs_dir) = empty_profile(&profile);
        let path = |name: &str| root.join(name).to_string_lossy().into_owned();
        // A batch whose rollback failed: the source is still under its staging name.
        fs::write(
            path(".mediatagger-aaaaaaaaaaaaaaaaaaaaaaaa.pending"),
            b"hidden",
        )
        .unwrap();
        // A rollback that restored the file but could not drop its row.
        fs::write(path("restored.jpg"), b"restored").unwrap();
        // A committed rename whose row removal failed: the file is at its final name.
        fs::write(path("renamed.jpg"), b"renamed").unwrap();
        let operation =
            |id: &str, asset_id: i64, action: &str, names: [&str; 2]| db::PendingFileOperation {
                operation_id: id.to_string(),
                asset_id,
                action: action.to_string(),
                original_path: path(names[0]),
                staging_path: path(names[1]),
                final_path: (action == "rename").then(|| path("renamed.jpg")),
                committed: action == "rename",
            };
        {
            let conn = db::open_connection(&db_path).unwrap();
            db::add_scan_root(&conn, &root.to_string_lossy()).unwrap();
            db::insert_pending_file_operations(
                &conn,
                &[
                    operation(
                        "rollback-failed",
                        7,
                        "delete",
                        [
                            "hidden.jpg",
                            ".mediatagger-aaaaaaaaaaaaaaaaaaaaaaaa.pending",
                        ],
                    ),
                    operation(
                        "row-left-behind",
                        8,
                        "delete",
                        [
                            "restored.jpg",
                            ".mediatagger-bbbbbbbbbbbbbbbbbbbbbbbb.pending",
                        ],
                    ),
                    operation(
                        "renamed",
                        9,
                        "rename",
                        [
                            "before.jpg",
                            ".mediatagger-cccccccccccccccccccccccc.pending",
                        ],
                    ),
                ],
            )
            .unwrap();
            // Reverse the insertion order to show that the copy keeps the journal order.
            conn.execute_batch(
                "UPDATE pending_file_operations SET created_at = 300 WHERE asset_id = 7;
                 UPDATE pending_file_operations SET created_at = 200 WHERE asset_id = 8;
                 UPDATE pending_file_operations SET created_at = 100 WHERE asset_id = 9;",
            )
            .unwrap();
        }
        let before = journal_rows(&db_path);
        assert_eq!(
            before.iter().map(|row| row.0.asset_id).collect::<Vec<_>>(),
            vec![9, 8, 7]
        );
        // The bundle knows neither the root nor any of the journaled assets.
        let archive = bundle_with_roots(tmp.path(), &[]);

        let state = create_test_state(&db_path, &thumbs_dir);
        import(&archive, &state).expect("uncommitted rows must not block restore");

        assert!(
            scan_roots_of(&db_path).is_empty(),
            "the bundle is installed"
        );
        assert_eq!(journal_rows(&db_path), before);
        assert!(!root.join("hidden.jpg").exists());

        let (conn, recovery) = start_profile(&profile);
        recovery.expect("startup after restore");
        assert_eq!(fs::read(root.join("hidden.jpg")).unwrap(), b"hidden");
        assert_eq!(
            names_in(&root),
            vec!["hidden.jpg", "renamed.jpg", "restored.jpg"],
            "the user's file is back under its own name and nothing hidden remains"
        );
        assert!(db::list_pending_file_operations(&conn).unwrap().is_empty());
    }

    /// Runs one restore of a profile with a committed delete awaiting cleanup
    /// under `fault`, then starts the profile. Returns whether the import
    /// returned `Ok`, whether the fault fired, and whether the bundle's
    /// generation is the live one afterwards.
    fn restore_under_fault(fault: super::sync_fault::SyncFault) -> (bool, bool, bool) {
        let run = faulted_restore(fault);
        (run.error.is_none() && !run.crashed, run.fired, run.restored)
    }

    struct FaultedRestore {
        /// The error the import returned, if it returned one.
        error: Option<String>,
        crashed: bool,
        fired: bool,
        /// The bundle's generation is the live one after the next start.
        restored: bool,
        /// The profile directory right after the import, before the next start.
        names_after_import: Vec<String>,
    }

    fn faulted_restore(fault: super::sync_fault::SyncFault) -> FaultedRestore {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("media");
        let other_root = tmp.path().join("other-media");
        fs::create_dir_all(&other_root).unwrap();
        let profile = tmp.path().join("profile");
        let staging = profile_with_a_committed_delete_awaiting_cleanup(&profile, &root);
        let archive = bundle_with_roots(tmp.path(), &[&root, &other_root]);
        let db_path = profile.join("media.db");
        let thumbs_dir = profile.join("thumbs");
        let before = journal_rows(&db_path);
        let state = create_test_state(&db_path, &thumbs_dir);

        super::sync_fault::arm(fault);
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| import(&archive, &state)));
        let fired = super::sync_fault::disarm();
        let crashed = outcome.is_err();
        let error = match outcome {
            Ok(Err(error)) => Some(error.to_string()),
            _ => None,
        };
        let names_after_import = names_in(&profile);
        let recovery_artifacts = names_after_import
            .iter()
            .any(|name| name == "restore-journal.json" || name.ends_with(".restore-previous"));
        if !crashed && recovery_artifacts {
            // What this attempt left for recovery stays until a restart resolves it.
            let refused = import(&archive, &state).expect_err("second import before a restart");
            assert!(
                refused
                    .to_string()
                    .contains("Restore recovery artifacts already exist"),
                "{fault:?}: {refused}"
            );
            assert_eq!(names_in(&profile), names_after_import, "{fault:?}");
        }
        drop(state);

        // The next start of the application.
        super::recover_interrupted_restore(&profile).expect("restore journal recovery");
        super::remove_abandoned_backup_temporaries(&profile);
        assert_eq!(names_in(&profile), vec!["media.db", "thumbs"], "{fault:?}");
        let restored = scan_roots_of(&db_path).len() == 2;
        assert_eq!(
            thumbs_dir.join("bundled.jpg").exists(),
            restored,
            "{fault:?}: database and thumbnails are one generation"
        );
        assert_eq!(thumbs_dir.join("live.jpg").exists(), !restored, "{fault:?}");
        if matches!(fault, super::sync_fault::SyncFault::Fail(_)) && !restored {
            // One failed sync before the commit point: the import itself removed the
            // staging tree or rolled the swap back, and left nothing for startup.
            assert_eq!(names_after_import, vec!["media.db", "thumbs"], "{fault:?}");
        }
        assert_eq!(
            journal_rows(&db_path),
            before,
            "{fault:?}: whichever generation is live holds the journal row"
        );
        assert_eq!(fs::read(&staging).unwrap(), b"deleted content", "{fault:?}");

        let (conn, recovery) = start_profile(&profile);
        recovery.unwrap_or_else(|error| panic!("{fault:?}: startup failed: {error:#}"));
        assert!(!staging.exists(), "{fault:?}: cleanup finished");
        assert!(db::list_pending_file_operations(&conn).unwrap().is_empty());
        FaultedRestore {
            error,
            crashed,
            fired,
            restored,
            names_after_import,
        }
    }

    /// The ninth directory sync of a restore over an existing database and
    /// thumbnail tree is the one after the `committed` journal rename.
    const COMMITTED_JOURNAL_SYNC: usize = 9;
    /// The syncs of the cleanup that follows it: previous database, previous
    /// thumbnails, staging tree, journal removal.
    const LAST_CLEANUP_SYNC: usize = 13;

    #[test]
    fn restore_with_a_published_committed_journal_reports_success() {
        use super::sync_fault::SyncFault;
        // The sync of the committed journal fails, once or for good: the new generation
        // is live and the import says so. The previous generation and the journal stay
        // for the next start, which `faulted_restore` runs and checks.
        for fault in [
            SyncFault::Fail(COMMITTED_JOURNAL_SYNC),
            SyncFault::FailFrom(COMMITTED_JOURNAL_SYNC),
        ] {
            let run = faulted_restore(fault);
            assert!(run.fired && !run.crashed, "{fault:?}");
            assert_eq!(run.error, None, "{fault:?}");
            assert!(run.restored, "{fault:?}");
            assert_eq!(
                run.names_after_import,
                vec![
                    "media.db",
                    "media.db.restore-previous",
                    "restore-journal.json",
                    "restore-staging",
                    "thumbs",
                    "thumbs.restore-previous",
                ],
                "{fault:?}: cleanup waits for the next start"
            );
        }
    }

    #[test]
    fn restore_cleanup_failure_after_the_commit_point_reports_success() {
        use super::sync_fault::SyncFault;
        for step in COMMITTED_JOURNAL_SYNC + 1..=LAST_CLEANUP_SYNC {
            for fault in [SyncFault::Fail(step), SyncFault::FailFrom(step)] {
                let run = faulted_restore(fault);
                assert!(run.fired && !run.crashed, "{fault:?}");
                assert_eq!(run.error, None, "{fault:?}");
                assert!(run.restored, "{fault:?}");
                // The last cleanup step removes the journal; only its sync is left to fail.
                assert_eq!(
                    run.names_after_import
                        .contains(&"restore-journal.json".to_string()),
                    step < LAST_CLEANUP_SYNC,
                    "{fault:?}: an unfinished cleanup keeps the journal: {:?}",
                    run.names_after_import
                );
            }
        }
        // One sync further nothing fires: the sequence ends at the journal removal.
        let run = faulted_restore(SyncFault::Fail(LAST_CLEANUP_SYNC + 1));
        assert!(!run.fired && run.restored && run.error.is_none());
        assert_eq!(run.names_after_import, vec!["media.db", "thumbs"]);
    }

    #[test]
    fn restore_failure_before_the_commit_point_keeps_the_previous_generation() {
        use super::sync_fault::SyncFault;
        for step in 1..COMMITTED_JOURNAL_SYNC {
            let run = faulted_restore(SyncFault::Fail(step));
            assert!(run.fired && !run.crashed && !run.restored, "Fail({step})");
            assert_eq!(
                run.error.as_deref(),
                Some("injected directory sync failure"),
                "Fail({step}): the rollback succeeded"
            );

            let run = faulted_restore(SyncFault::FailFrom(step));
            assert!(
                run.fired && !run.crashed && !run.restored,
                "FailFrom({step})"
            );
            let error = run.error.expect("a rolled-back restore is an error");
            // From the `swapping` journal on (the third sync) the rollback needs syncs too.
            assert_eq!(
                error.contains("Automatic rollback also failed"),
                step >= 3,
                "FailFrom({step}): {error}"
            );
            assert_eq!(
                run.names_after_import
                    .contains(&"restore-journal.json".to_string()),
                step >= 3,
                "FailFrom({step}): {:?}",
                run.names_after_import
            );
        }
    }

    #[test]
    fn restore_interrupted_at_any_step_keeps_the_journal_with_the_live_generation() {
        use super::sync_fault::SyncFault;
        for crash in [false, true] {
            let mut generations = std::collections::BTreeSet::new();
            let mut step = 1;
            loop {
                let fault = if crash {
                    SyncFault::Crash(step)
                } else {
                    SyncFault::Fail(step)
                };
                let (returned_ok, fired, restored) = restore_under_fault(fault);
                if !fired {
                    assert!(returned_ok && restored, "{fault:?}: an undisturbed restore");
                    break;
                }
                assert!(!returned_ok || restored, "{fault:?}: Ok means restored");
                generations.insert(restored);
                step += 1;
                assert!(step < 64, "the restore sequence has far fewer syncs");
            }
            assert!(
                step > 9,
                "every move-aside, install and cleanup step ran: {step}"
            );
            assert_eq!(
                generations.len(),
                2,
                "faults before and after the commit point were both exercised"
            );
        }
    }

    #[test]
    fn restore_fails_before_the_swap_when_the_journal_cannot_be_carried() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("media");
        let profile = tmp.path().join("profile");
        let staging = profile_with_a_committed_delete_awaiting_cleanup(&profile, &root);
        let db_path = profile.join("media.db");
        let thumbs_dir = profile.join("thumbs");
        let before = journal_rows(&db_path);
        // A hand-made bundle whose journal table rejects the copy: `action` has a CHECK
        // that no real row satisfies. Structure validation looks at columns, not at checks.
        let (source_db, _) = empty_profile(&tmp.path().join("bundle-source"));
        {
            let conn = db::open_connection(&source_db).unwrap();
            conn.execute_batch(
                "DROP TABLE pending_file_operations;
                 CREATE TABLE pending_file_operations (
                   operation_id TEXT NOT NULL,
                   asset_id INTEGER NOT NULL,
                   action TEXT NOT NULL CHECK (action = 'never'),
                   original_path TEXT NOT NULL,
                   staging_path TEXT NOT NULL,
                   final_path TEXT,
                   committed INTEGER NOT NULL DEFAULT 0,
                   created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                   PRIMARY KEY(operation_id, asset_id)
                 );
                 PRAGMA wal_checkpoint(TRUNCATE);",
            )
            .unwrap();
        }
        let archive = tmp.path().join("bundle.zip");
        {
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            zip.start_file("media.db", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&fs::read(&source_db).unwrap()).unwrap();
            zip.finish().unwrap();
        }

        let state = create_test_state(&db_path, &thumbs_dir);
        let error = import(&archive, &state).expect_err("the copy must fail");
        assert!(error.to_string().contains("CHECK constraint"), "{error}");

        assert_eq!(names_in(&profile), vec!["media.db", "thumbs"]);
        assert_eq!(scan_roots_of(&db_path).len(), 1, "the live library stays");
        assert_eq!(journal_rows(&db_path), before);
        assert_eq!(fs::read(&staging).unwrap(), b"deleted content");
        assert_eq!(fs::read(thumbs_dir.join("live.jpg")).unwrap(), b"live");
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
