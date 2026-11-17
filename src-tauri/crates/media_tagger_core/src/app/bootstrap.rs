//! Framework-free startup phases of the desktop process.
//!
//! The Tauri adapter (`lib.rs`) composes them in a fixed order:
//! identifier guard → app-data directory → `InstanceLock` → [`open_storage`]
//! → managed startup snapshots → `resource_dir` → [`resolve_ffmpeg_path`] →
//! video phase → [`start_workers`] → `manage(AppState)` as the last step.
//! The `thumb://` scheme answers 503 until that last step, so nothing here
//! manages state or publishes `AppState` through another channel.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, atomic::AtomicU64, Mutex, RwLock},
};

use crate::{
    app::state::AppState,
    db,
    models::ScanRoot,
    services::{self, asset_mutation_service, thumb_scheduler::ThumbnailScheduler},
};

/// The error type of the Tauri `setup` hook. Every step converts its error
/// with the same `?` as the hook did, so a failed startup reports the same
/// text, and the adapter's `?` is the identity conversion.
pub type BootstrapError = Box<dyn std::error::Error>;
pub type BootstrapResult<T> = Result<T, BootstrapError>;

/// The opened profile: `media.db` after recovery and `thumbs/`. Holds the
/// FULL-sync startup connection; the adapter keeps this value until the end
/// of `setup`, after `AppState` is managed.
pub struct Storage {
    db_path: PathBuf,
    thumbs_dir: PathBuf,
    /// Only held open; tests read it.
    #[cfg_attr(not(test), allow(dead_code))]
    connection: rusqlite::Connection,
}

/// Launch snapshots taken after database initialization and recovery.
pub struct StartupSnapshots {
    /// `db::list_scan_root_settings`; the adapter builds `StartupScanState`.
    pub scan_roots: Vec<ScanRoot>,
}

/// Restore recovery → abandoned backup temporaries (best effort, ADR 0016) →
/// `thumbs/` → `media.db` + `init_schema` (FULL sync) → pending file-operation
/// recovery → scan-root snapshot. The order is a startup
/// invariant: nothing opens `thumbs/` or `media.db` before restore recovery.
pub fn open_storage(app_data_dir: &Path) -> BootstrapResult<(Storage, StartupSnapshots)> {
    services::backup_service::recover_interrupted_restore(app_data_dir)?;
    services::backup_service::remove_abandoned_backup_temporaries(app_data_dir);

    let thumbs_dir = app_data_dir.join("thumbs");
    fs::create_dir_all(&thumbs_dir)?;

    let db_path = app_data_dir.join("media.db");
    let conn = db::open_connection(&db_path)?;
    db::init_schema(&conn)?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    asset_mutation_service::recover_pending_file_operations(&conn)?;
    let scan_roots = db::list_scan_root_settings(&conn)?;

    Ok((
        Storage {
            db_path,
            thumbs_dir,
            connection: conn,
        },
        StartupSnapshots { scan_roots },
    ))
}

/// Starts the thumbnail scheduler and builds `AppState`. The adapter manages
/// the result as the last step of `setup`.
pub fn start_workers(storage: &Storage, ffmpeg_path: PathBuf) -> BootstrapResult<AppState> {
    let db_path = storage.db_path.clone();
    let thumbs_dir = storage.thumbs_dir.clone();
    let thumb_scheduler = ThumbnailScheduler::from_available_parallelism(ffmpeg_path.clone());
    // Demand calls block on scheduler results; a scheduler whose
    // workers never started would hang every thumbnail command.
    // Refuse the startup instead of entering a wedged state.
    let worker_spawn_failures = thumb_scheduler.worker_spawn_failures();
    if worker_spawn_failures > 0 {
        return Err(format!(
            "failed to start {worker_spawn_failures} thumbnail worker thread(s); refusing to run with an unhealthy thumbnail scheduler"
        )
        .into());
    }

    Ok(AppState {
        database: crate::services::db_pool::DatabaseRuntime::new(db_path.clone()),
        db_path,
        thumbs_dir,
        ffmpeg_path,
        scan_lock: Mutex::new(()),
        thumb_lock: RwLock::new(()),
        thumb_scheduler,
        thumbnail_render_all_running: AtomicBool::new(false),
        thumbnail_render_all_cancel_requested: AtomicBool::new(false),
        thumbnail_generation: AtomicU64::new(0),
    })
}

pub fn resolve_ffmpeg_path(resource_dir: &Path) -> PathBuf {
    let mut candidates = Vec::new();
    for directory in collect_ffmpeg_candidate_dirs(resource_dir) {
        extend_ffmpeg_candidates(&mut candidates, &directory);
    }

    candidates.push(PathBuf::from("/usr/bin/ffmpeg"));

    for candidate in candidates {
        if is_executable_file(&candidate) {
            return candidate;
        }
    }

    PathBuf::from("ffmpeg")
}

fn collect_ffmpeg_candidate_dirs(resource_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![resource_dir.to_path_buf(), resource_dir.join("binaries")];

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            dirs.push(exe_dir.to_path_buf());
            dirs.push(exe_dir.join("binaries"));
            dirs.push(exe_dir.join("resources"));
            dirs.push(exe_dir.join("resources").join("binaries"));

            if let Some(exe_parent) = exe_dir.parent() {
                dirs.push(exe_parent.join("resources"));
                dirs.push(exe_parent.join("resources").join("binaries"));
            }
        }
    }

    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|directory| seen.insert(directory.clone()))
        .collect()
}

fn extend_ffmpeg_candidates(candidates: &mut Vec<PathBuf>, directory: &Path) {
    candidates.push(directory.join("ffmpeg"));
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(test)]
mod tests {
    use std::{os::unix::fs::PermissionsExt, path::Path, sync::atomic::Ordering};

    use tempfile::tempdir;

    use super::{open_storage, resolve_ffmpeg_path, start_workers};
    use crate::{db, models::NewAsset};

    fn library_db(db_path: &Path, roots: &[(&Path, bool)]) {
        let conn = db::open_connection(db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        for (root, auto_scan) in roots {
            let root_text = root.to_string_lossy();
            db::add_scan_root(&conn, &root_text).expect("add root");
            db::set_scan_root_auto_scan(&conn, &root_text, *auto_scan).expect("auto scan");
        }
    }

    fn root_paths(roots: &[crate::models::ScanRoot]) -> Vec<String> {
        roots.iter().map(|root| root.path.clone()).collect()
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("list dir")
            .map(|entry| entry.expect("entry").file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn open_storage_rolls_back_an_interrupted_restore_before_opening_the_database() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path().join("profile");
        std::fs::create_dir_all(&profile).expect("profile");
        let kept_root = tmp.path().join("kept-library");
        let partial_root = tmp.path().join("partial-library");
        // The previous generation, renamed aside by the swap.
        library_db(
            &profile.join("media.db.restore-previous"),
            &[(&kept_root, true)],
        );
        std::fs::create_dir_all(profile.join("thumbs.restore-previous")).unwrap();
        std::fs::write(profile.join("thumbs.restore-previous/kept.jpg"), b"kept").unwrap();
        // The partially installed new generation.
        library_db(&profile.join("media.db"), &[(&partial_root, true)]);
        std::fs::create_dir_all(profile.join("thumbs")).unwrap();
        std::fs::write(profile.join("thumbs/partial.jpg"), b"partial").unwrap();
        std::fs::create_dir_all(profile.join("restore-staging")).unwrap();
        std::fs::write(
            profile.join("restore-journal.json"),
            br#"{"phase":"swapping","previous_db_existed":true,"previous_thumbs_existed":true}"#,
        )
        .unwrap();

        let (storage, snapshots) = open_storage(&profile).expect("open storage");

        assert_eq!(
            root_paths(&snapshots.scan_roots),
            [kept_root.to_string_lossy()]
        );
        assert_eq!(names_in(&profile.join("thumbs")), ["kept.jpg"]);
        let mut names = names_in(&profile);
        names.retain(|name| !name.starts_with("media.db-"));
        assert_eq!(names, ["media.db", "thumbs"]);
        drop(storage);
    }

    #[test]
    fn open_storage_sweeps_abandoned_backup_temporaries() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path();
        std::fs::write(profile.join("backup-snapshot-123-456.db"), b"snapshot").unwrap();
        std::fs::write(profile.join("backup-snapshot-123-456.db-wal"), b"wal").unwrap();
        std::fs::write(profile.join(".restore-journal-123-789.tmp"), b"{}").unwrap();
        std::fs::create_dir_all(profile.join("restore-inspect-123-42/thumbs")).unwrap();
        std::fs::create_dir_all(profile.join("restore-staging/thumbs")).unwrap();
        std::fs::write(profile.join("notes.txt"), b"not ours").unwrap();

        let (storage, _snapshots) = open_storage(profile).expect("open storage");

        let mut names = names_in(profile);
        names.retain(|name| !name.starts_with("media.db-"));
        assert_eq!(names, ["media.db", "notes.txt", "thumbs"]);
        drop(storage);
    }

    #[test]
    fn open_storage_recovers_a_pending_file_operation_and_takes_snapshots() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path().join("profile");
        std::fs::create_dir_all(&profile).expect("profile");
        let enabled_root = tmp.path().join("a-library");
        let disabled_root = tmp.path().join("b-library");
        std::fs::create_dir_all(&enabled_root).unwrap();
        let original = enabled_root.join("photo.jpg");
        let staging = enabled_root.join(".mediatagger-abcdefghijklmn0123456789.pending");
        std::fs::write(&original, b"source").unwrap();
        let db_path = profile.join("media.db");
        library_db(&db_path, &[(&enabled_root, true), (&disabled_root, false)]);
        {
            let conn = db::open_connection(&db_path).expect("open db");
            let root_text = enabled_root.to_string_lossy();
            db::upsert_scanned_asset(
                &conn,
                &NewAsset {
                    path: original.to_string_lossy().into_owned(),
                    kind: "image".to_string(),
                    size_bytes: 6,
                    modified_at: 1,
                    width: None,
                    height: None,
                    duration_ms: None,
                    thumb_path: None,
                },
                1,
                &root_text,
                1,
            )
            .expect("index asset");
            db::set_asset_tags(&conn, 1, &["keep".to_string()]).expect("tag asset");
            db::insert_pending_file_operations(
                &conn,
                &[db::PendingFileOperation {
                    operation_id: "interrupted-delete".to_string(),
                    asset_id: 1,
                    action: "delete".to_string(),
                    original_path: original.to_string_lossy().into_owned(),
                    staging_path: staging.to_string_lossy().into_owned(),
                    final_path: None,
                    committed: false,
                }],
            )
            .expect("journal the delete");
            // The process dies after staging and before the database commit.
            std::fs::rename(&original, &staging).expect("stage the source");
        }

        let (storage, snapshots) = open_storage(&profile).expect("open storage");

        assert_eq!(
            std::fs::read(&original).expect("restored source"),
            b"source"
        );
        assert!(!staging.exists());
        assert!(db::list_pending_file_operations(&storage.connection)
            .expect("list journal")
            .is_empty());
        let roots: Vec<(String, bool)> = snapshots
            .scan_roots
            .iter()
            .map(|root| (root.path.clone(), root.auto_scan_on_startup))
            .collect();
        assert_eq!(
            roots,
            [
                (enabled_root.to_string_lossy().into_owned(), true),
                (disabled_root.to_string_lossy().into_owned(), false),
            ]
        );
        assert!(profile.join("thumbs").is_dir());
        let synchronous: i64 = storage
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .expect("synchronous");
        assert_eq!(synchronous, 2, "the startup connection runs with FULL sync");
    }

    #[test]
    fn open_storage_fails_on_a_corrupt_database_and_leaves_it_in_place() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path();
        let garbage = vec![0x5a_u8; 8192];
        std::fs::write(profile.join("media.db"), &garbage).unwrap();

        let error = match open_storage(profile) {
            Ok(_) => panic!("a corrupt database must stop startup"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("not a database"), "{error}");
        assert_eq!(std::fs::read(profile.join("media.db")).unwrap(), garbage);
    }

    #[test]
    fn open_storage_refuses_a_database_with_a_newer_schema() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path();
        {
            let conn = rusqlite::Connection::open(profile.join("media.db")).unwrap();
            conn.pragma_update(None, "user_version", 9_999).unwrap();
        }

        let error = match open_storage(profile) {
            Ok(_) => panic!("a database from a newer release must stop startup"),
            Err(error) => error.to_string(),
        };

        assert!(
            error.contains("database schema version 9999 is newer than supported"),
            "{error}"
        );
        let conn = rusqlite::Connection::open(profile.join("media.db")).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 9_999);
        let tables: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tables, 0, "nothing was written to the newer database");
    }

    #[test]
    fn start_workers_builds_app_state_for_the_opened_profile() {
        let tmp = tempdir().expect("tempdir");
        let profile = tmp.path();

        let (storage, snapshots) = open_storage(profile).expect("open storage");
        let state = start_workers(&storage, "ffmpeg".into()).expect("start workers");

        assert!(snapshots.scan_roots.is_empty());
        assert_eq!(state.db_path, profile.join("media.db"));
        assert_eq!(state.thumbs_dir, profile.join("thumbs"));
        assert!(state.thumbs_dir.is_dir());
        assert_eq!(state.ffmpeg_path, Path::new("ffmpeg"));
        assert_eq!(state.thumb_scheduler.worker_spawn_failures(), 0);
        assert!(!state.thumbnail_render_all_running.load(Ordering::SeqCst));
        assert!(!state
            .thumbnail_render_all_cancel_requested
            .load(Ordering::SeqCst));
        assert_eq!(state.thumbnail_generation.load(Ordering::SeqCst), 0);
        let permit = state.database.admit().expect("admit");
        let assets: i64 = permit
            .connection()
            .expect("connection")
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .expect("count assets");
        assert_eq!(assets, 0);
        drop(permit);
        drop(state);
        drop(storage);
    }

    #[test]
    fn resolve_ffmpeg_path_prefers_an_executable_in_the_resource_dir() {
        let tmp = tempdir().expect("tempdir");
        let resources = tmp.path();
        std::fs::create_dir_all(resources.join("binaries")).unwrap();
        let skipped = resources.join("ffmpeg");
        std::fs::write(&skipped, b"not executable").unwrap();
        std::fs::set_permissions(&skipped, std::fs::Permissions::from_mode(0o644)).unwrap();
        let bundled = resources.join("binaries/ffmpeg");
        std::fs::write(&bundled, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&bundled, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(resolve_ffmpeg_path(resources), bundled);

        std::fs::set_permissions(&skipped, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(resolve_ffmpeg_path(resources), skipped);
    }
}
