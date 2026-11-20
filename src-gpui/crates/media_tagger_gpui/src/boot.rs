//! Startup composition: the one file of this crate that reaches `media_tagger_core`
//! beyond `models` and `error` (`architecture_boundaries`, ADR 0036).
//!
//! It runs the framework-free phases of `media_tagger_core::app::bootstrap` in the
//! order of the Tauri adapter (`src-tauri/src/lib.rs`), without the phases that only
//! exist there (managed state, native video): app-data directory → `InstanceLock` →
//! `open_storage` → `resolve_ffmpeg_path` → `start_workers`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use media_tagger_core::{
    app::{
        bootstrap::{self, BootstrapResult},
        instance_lock::InstanceLock,
        state::AppState,
    },
    models::ScanRoot,
};

/// The running backend of one profile. Fields drop in declaration order, so the
/// database pool closes before the instance lock is released. Dropping it does not stop
/// the thumbnail workers: `ThumbnailScheduler` has no shutdown, and its threads wait
/// for jobs until the process exits.
pub struct Boot {
    pub state: AppState,
    /// Scan roots as stored at launch; the startup scan (a later epic) reads them.
    pub startup_scan_roots: Vec<ScanRoot>,
    _instance_lock: InstanceLock,
}

/// Opens the profile in `data_dir` and starts the workers. The instance lock is
/// taken before anything opens `media.db`; the startup connection of
/// `open_storage` lives until the workers have started.
pub fn start(data_dir: &Path, resource_dir: &Path) -> BootstrapResult<Boot> {
    fs::create_dir_all(data_dir)?;
    let instance_lock = InstanceLock::acquire(data_dir)?;
    let (storage, snapshots) = bootstrap::open_storage(data_dir)?;
    let ffmpeg_path = bootstrap::resolve_ffmpeg_path(resource_dir);
    let state = bootstrap::start_workers(&storage, ffmpeg_path)?;
    Ok(Boot {
        state,
        startup_scan_roots: snapshots.scan_roots,
        _instance_lock: instance_lock,
    })
}

/// Where bundled resources such as `ffmpeg` would live: the directory of the
/// executable. Packaging (epic E22) decides the final layout.
pub fn resource_dir() -> BootstrapResult<PathBuf> {
    let executable = std::env::current_exe()?;
    Ok(executable
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(executable))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use media_tagger_core::app::instance_lock::InstanceLock;
    use tempfile::tempdir;

    use super::{resource_dir, start};

    fn error_of<T>(result: Result<T, Box<dyn std::error::Error>>) -> String {
        match result {
            Ok(_) => panic!("expected the startup to fail"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn start_creates_the_profile_and_reports_its_scan_roots() {
        let tmp = tempdir().expect("tempdir");
        let data_dir = tmp.path().join("data").join("profile");

        let boot = start(&data_dir, tmp.path()).expect("start");

        assert!(data_dir.join("instance.lock").is_file());
        assert!(data_dir.join("media.db").is_file());
        assert!(data_dir.join("thumbs").is_dir());
        assert_eq!(boot.state.db_path, data_dir.join("media.db"));
        assert_eq!(boot.state.thumbs_dir, data_dir.join("thumbs"));
        assert!(boot.startup_scan_roots.is_empty());
    }

    #[test]
    fn a_second_instance_on_the_same_profile_is_refused_until_the_first_stops() {
        let tmp = tempdir().expect("tempdir");
        let data_dir = tmp.path().join("profile");

        let first = start(&data_dir, tmp.path()).expect("first instance");
        let refused = error_of(start(&data_dir, tmp.path()));
        assert_eq!(
            refused,
            "another Tagrove instance is already using this data profile"
        );

        drop(first);
        start(&data_dir, tmp.path()).expect("the lock is released when the first instance stops");
    }

    #[test]
    fn profiles_in_different_directories_run_side_by_side() {
        let tmp = tempdir().expect("tempdir");

        let dev = start(&tmp.path().join("dev"), tmp.path()).expect("dev profile");
        let e2e = start(&tmp.path().join("e2e"), tmp.path()).expect("e2e profile");

        assert_ne!(dev.state.db_path, e2e.state.db_path);
    }

    #[test]
    fn a_corrupt_database_stops_the_startup_and_releases_the_lock() {
        let tmp = tempdir().expect("tempdir");
        let data_dir = tmp.path().join("profile");
        std::fs::create_dir_all(&data_dir).unwrap();
        let garbage = vec![0x5a_u8; 8192];
        std::fs::write(data_dir.join("media.db"), &garbage).unwrap();

        let error = error_of(start(&data_dir, tmp.path()));

        assert!(error.contains("not a database"), "{error}");
        assert_eq!(std::fs::read(data_dir.join("media.db")).unwrap(), garbage);
        let again = error_of(start(&data_dir, tmp.path()));
        assert!(
            again.contains("not a database"),
            "the failed startup must not keep the instance lock: {again}"
        );
    }

    #[test]
    fn the_instance_lock_is_taken_before_the_database_is_opened() {
        let tmp = tempdir().expect("tempdir");
        let data_dir = tmp.path().join("profile");
        std::fs::create_dir_all(&data_dir).unwrap();
        let other_instance = InstanceLock::acquire(&data_dir).expect("lock the profile");

        let refused = error_of(start(&data_dir, tmp.path()));

        assert_eq!(
            refused,
            "another Tagrove instance is already using this data profile"
        );
        assert!(
            !data_dir.join("media.db").exists(),
            "a refused instance must not open the database of the running one"
        );
        assert!(!data_dir.join("thumbs").exists());
        drop(other_instance);
    }

    #[test]
    fn a_bundled_ffmpeg_in_the_resource_dir_is_handed_to_the_workers() {
        let tmp = tempdir().expect("tempdir");
        let resources = tmp.path().join("resources");
        std::fs::create_dir_all(resources.join("binaries")).unwrap();
        let bundled = resources.join("binaries").join("ffmpeg");
        std::fs::write(&bundled, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&bundled, std::fs::Permissions::from_mode(0o755)).unwrap();

        let boot = start(&tmp.path().join("profile"), &resources).expect("start");

        assert_eq!(boot.state.ffmpeg_path, bundled);
    }

    #[test]
    fn resources_are_looked_up_next_to_the_executable() {
        let executable = std::env::current_exe().expect("current executable");

        assert_eq!(
            resource_dir().expect("resource dir"),
            executable.parent().expect("the executable has a directory")
        );
    }
}
