//! The preferences file in the profile directory, written atomically: a staging file
//! next to it is written and flushed, then renamed over the old file, and the
//! directory is flushed, so a crash leaves either the old or the new file.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{Prefs, PrefsError, PrefsStore};

/// The file name inside the profile directory.
pub const PREFS_FILE_NAME: &str = "ui-preferences.json";

/// Staging files of concurrent saves in one process never share a name.
static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);

/// [`PrefsStore`] over `<profile directory>/ui-preferences.json`.
#[derive(Debug, Clone)]
pub struct FilePrefsStore {
    path: PathBuf,
}

impl FilePrefsStore {
    /// The store of the profile in `profile_dir` (the app-data directory of the
    /// identifier, next to `media.db`).
    pub fn new(profile_dir: impl AsRef<Path>) -> Self {
        Self {
            path: profile_dir.as_ref().join(PREFS_FILE_NAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The stored preferences, or why the file cannot be read. A missing file is
    /// `Ok(None)`; a file that is not valid preferences reads as nothing stored.
    pub fn read(&self) -> io::Result<Option<Prefs>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(Prefs::parse(&String::from_utf8_lossy(&bytes)))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn staging_path(&self, directory: &Path) -> PathBuf {
        let sequence = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
        directory.join(format!(
            ".mediatagger-ui-preferences-{}-{sequence}.pending",
            process::id()
        ))
    }

    fn write_staging(staging: &Path, text: &str) -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staging)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    }
}

impl PrefsStore for FilePrefsStore {
    fn load(&self) -> Prefs {
        self.read().ok().flatten().unwrap_or_default()
    }

    fn save(&self, prefs: &Prefs) -> Result<(), PrefsError> {
        let directory = self
            .path
            .parent()
            .ok_or_else(|| {
                PrefsError::new(
                    "save",
                    io::Error::new(io::ErrorKind::InvalidInput, "no profile directory"),
                )
            })?
            .to_path_buf();
        fs::create_dir_all(&directory).map_err(|error| PrefsError::new("save", error))?;
        let staging = self.staging_path(&directory);
        let written = Self::write_staging(&staging, &prefs.to_json())
            .and_then(|()| fs::rename(&staging, &self.path));
        if let Err(error) = written {
            // Best effort: a staging file that cannot be removed is only clutter.
            let _ = fs::remove_file(&staging);
            return Err(PrefsError::new("save", error));
        }
        // The rename is durable once the directory entry is.
        File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| PrefsError::new("save", error))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, sync::Arc, thread};

    use tempfile::tempdir;

    use super::{FilePrefsStore, PREFS_FILE_NAME};
    use crate::{
        i18n::Language,
        prefs::{Prefs, PrefsStore},
        theme::ThemeName,
    };

    fn entries(directory: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn saved_preferences_load_again() {
        let profile = tempdir().unwrap();
        let store = FilePrefsStore::new(profile.path());
        let prefs = Prefs::default()
            .with_theme(ThemeName::Light)
            .with_language(Language::Ru);

        store.save(&prefs).expect("save");

        assert_eq!(store.load(), prefs);
        assert_eq!(
            FilePrefsStore::new(profile.path()).load(),
            prefs,
            "a new store reads the file"
        );
        assert_eq!(store.path(), profile.path().join(PREFS_FILE_NAME));
        assert_eq!(
            entries(profile.path()),
            [PREFS_FILE_NAME],
            "no staging file is left"
        );
    }

    #[test]
    fn a_save_replaces_the_previous_preferences() {
        let profile = tempdir().unwrap();
        let store = FilePrefsStore::new(profile.path());

        store
            .save(
                &Prefs::default()
                    .with_theme(ThemeName::Light)
                    .with_language(Language::De),
            )
            .unwrap();
        store
            .save(&Prefs::default().with_theme(ThemeName::Dark))
            .unwrap();

        assert_eq!(store.load(), Prefs::default().with_theme(ThemeName::Dark));
    }

    #[test]
    fn a_missing_file_or_directory_reads_as_nothing_stored() {
        let profile = tempdir().unwrap();

        assert_eq!(FilePrefsStore::new(profile.path()).read().unwrap(), None);
        assert_eq!(FilePrefsStore::new(profile.path()).load(), Prefs::default());
        assert_eq!(
            FilePrefsStore::new(profile.path().join("missing")).load(),
            Prefs::default()
        );
    }

    #[test]
    fn a_damaged_file_reads_as_defaults_and_is_replaced_by_the_next_save() {
        let profile = tempdir().unwrap();
        let store = FilePrefsStore::new(profile.path());
        for damaged in [
            &b"{\"theme\": \"li"[..],
            b"\xff\xfe\x00garbage",
            b"",
            b"[1, 2]",
        ] {
            fs::write(store.path(), damaged).unwrap();

            assert_eq!(store.load(), Prefs::default(), "{damaged:?}");
        }

        store
            .save(&Prefs::default().with_language(Language::Ko))
            .unwrap();
        assert_eq!(store.load(), Prefs::default().with_language(Language::Ko));
    }

    #[test]
    fn an_unreadable_file_reads_as_defaults() {
        let profile = tempdir().unwrap();
        let store = FilePrefsStore::new(profile.path());
        // A directory where the file belongs cannot be read as a file.
        fs::create_dir(store.path()).unwrap();

        assert!(store.read().is_err());
        assert_eq!(store.load(), Prefs::default());
    }

    #[test]
    fn a_failed_save_keeps_the_old_file_and_leaves_no_staging_file() {
        let profile = tempdir().unwrap();
        let store = FilePrefsStore::new(profile.path());
        let old = Prefs::default().with_theme(ThemeName::Light);
        store.save(&old).unwrap();
        fs::set_permissions(profile.path(), fs::Permissions::from_mode(0o500)).unwrap();

        let result = store.save(&Prefs::default().with_theme(ThemeName::Dark));
        fs::set_permissions(profile.path(), fs::Permissions::from_mode(0o700)).unwrap();

        // Root ignores directory permissions; the failure path is then not reached.
        if let Err(error) = result {
            assert!(
                error
                    .to_string()
                    .starts_with("cannot save the UI preferences: "),
                "{error}"
            );
            assert_eq!(store.load(), old);
        }
        assert_eq!(
            entries(profile.path())
                .into_iter()
                .filter(|name| name.ends_with(".pending"))
                .count(),
            0
        );
    }

    #[test]
    fn the_profile_directory_is_created_when_missing() {
        let root = tempdir().unwrap();
        let store = FilePrefsStore::new(root.path().join("profile"));

        store
            .save(&Prefs::default().with_theme(ThemeName::Light))
            .unwrap();

        assert_eq!(store.load(), Prefs::default().with_theme(ThemeName::Light));
    }

    #[test]
    fn concurrent_saves_leave_one_complete_file() {
        let profile = tempdir().unwrap();
        let store = Arc::new(FilePrefsStore::new(profile.path()));
        let candidates: Vec<Prefs> = Language::ALL
            .into_iter()
            .map(|language| Prefs::default().with_language(language))
            .collect();

        let handles: Vec<_> = candidates
            .iter()
            .copied()
            .map(|prefs| {
                let store = Arc::clone(&store);
                thread::spawn(move || store.save(&prefs).unwrap())
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        assert!(candidates.contains(&store.load()));
        assert_eq!(entries(profile.path()), [PREFS_FILE_NAME]);
    }
}
