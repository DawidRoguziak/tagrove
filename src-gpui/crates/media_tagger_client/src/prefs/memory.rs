//! An in-memory [`PrefsStore`] for tests (feature `test-support`).

use std::{
    io,
    sync::{Mutex, MutexGuard},
};

use super::{Prefs, PrefsError, PrefsStore};

#[derive(Debug, Default)]
struct State {
    stored: Prefs,
    fail_saves: bool,
    saves: usize,
}

/// Keeps the preferences in memory; saves can be made to fail.
#[derive(Debug, Default)]
pub struct MemoryPrefsStore {
    state: Mutex<State>,
}

impl MemoryPrefsStore {
    pub fn new(stored: Prefs) -> Self {
        Self {
            state: Mutex::new(State {
                stored,
                ..State::default()
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Makes every following save fail (a full disk, a read-only profile).
    pub fn fail_saves(&self, fail: bool) {
        self.state().fail_saves = fail;
    }

    /// How many saves succeeded.
    pub fn saves(&self) -> usize {
        self.state().saves
    }
}

impl PrefsStore for MemoryPrefsStore {
    fn load(&self) -> Prefs {
        self.state().stored
    }

    fn save(&self, prefs: &Prefs) -> Result<(), PrefsError> {
        let mut state = self.state();
        if state.fail_saves {
            return Err(PrefsError::new(
                "save",
                io::Error::new(io::ErrorKind::StorageFull, "synthetic failure"),
            ));
        }
        state.stored = *prefs;
        state.saves += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryPrefsStore;
    use crate::{
        prefs::{Prefs, PrefsStore},
        theme::ThemeName,
    };

    #[test]
    fn keeps_saves_and_can_fail_them() {
        let store = MemoryPrefsStore::new(Prefs::default().with_theme(ThemeName::Light));
        assert_eq!(store.load().theme, Some(ThemeName::Light));

        store.fail_saves(true);
        assert!(store.save(&Prefs::default()).is_err());
        assert_eq!(store.load().theme, Some(ThemeName::Light));

        store.fail_saves(false);
        store.save(&Prefs::default()).unwrap();
        assert_eq!(store.load(), Prefs::default());
        assert_eq!(store.saves(), 1);
    }
}
