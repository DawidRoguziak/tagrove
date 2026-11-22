//! Deterministic start states for the comparison harness (`bun run compare:ui`).
//!
//! `--scenario <id>` starts the application in a state of
//! `media_tagger_client::scenario`. Only the isolated E2E profile accepts it: a scenario
//! presets what the user's own preferences decide in every other profile, and the
//! harness that asks for one always runs the E2E build on a private display with
//! scratch app data. The guard runs before the display check, so a refused scenario
//! touches neither a display nor the disk.
//!
//! A scenario with media ([`Seed::Control`]) indexes the directory the harness seeded
//! and named in `MEDIATAGGER_GPUI_SCENARIO_MEDIA`, as `app:control start` indexes the
//! same files for the Tauri application: [`seed_library`] adds it as the one scan root
//! and scans it before the window opens, so the first query already lists the assets.

use std::{
    ffi::OsString,
    fmt::{Display, Formatter},
    path::Path,
};

use media_tagger_client::{
    backend::Backend,
    runtime::DEFAULT_TILE_SIZE,
    scenario::{self, Scenario, ScenarioClick, Seed, SCENARIO_MEDIA_VARIABLE},
    theme::ThemeName,
};

use crate::profile::{self, Profile};

/// What the main window shows first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartState {
    pub theme: ThemeName,
    pub screen: StartScreen,
    /// The media the library starts with.
    pub seed: Seed,
    /// The tile size of the gallery.
    pub tile_size: u32,
    /// The clicks the gallery starts with, in selection mode, once their tiles are
    /// loaded; none: selection mode stays off.
    pub selection: &'static [ScenarioClick],
}

impl Default for StartState {
    /// The application's own start: the stored theme decides, the library is what the
    /// profile holds, and tiles have their default size.
    fn default() -> Self {
        Self {
            theme: ThemeName::default(),
            screen: StartScreen::default(),
            seed: Seed::Empty,
            tile_size: DEFAULT_TILE_SIZE,
            selection: &[],
        }
    }
}

/// The view of the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartScreen {
    /// The application's own start.
    #[default]
    Main,
    /// The board of every shared control in both themes (`--scenario kit`).
    Kit,
}

impl From<Scenario> for StartState {
    fn from(scenario: Scenario) -> Self {
        Self {
            theme: scenario.theme,
            screen: StartScreen::Main,
            seed: scenario.seed,
            tile_size: scenario.tile_size,
            selection: scenario.selection,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioError {
    /// A scenario was requested on a profile other than the isolated E2E one.
    NotE2e {
        identifier: &'static str,
    },
    Unknown(String),
    /// A scenario with media was started without the seeded directory.
    MediaUnset,
    /// The seeded directory is not one, or its path is not UTF-8.
    MediaInvalid,
    /// The backend refused to index the seeded directory.
    SeedFailed(String),
}

impl Display for ScenarioError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotE2e { identifier } => write!(
                f,
                "Refusing a scenario on profile {identifier}. Scenarios start only on the \
                 isolated E2E profile {}.",
                profile::E2E.identifier
            ),
            Self::Unknown(id) => {
                let known: Vec<_> = scenario::SCENARIOS
                    .iter()
                    .map(|s| s.id)
                    .chain([scenario::KIT])
                    .collect();
                write!(f, "unknown scenario {id:?}; known: {}", known.join(", "))
            }
            Self::MediaUnset => write!(
                f,
                "this scenario needs seeded media: {SCENARIO_MEDIA_VARIABLE} is not set"
            ),
            Self::MediaInvalid => write!(
                f,
                "{SCENARIO_MEDIA_VARIABLE} does not name a directory with a UTF-8 path"
            ),
            Self::SeedFailed(reason) => {
                write!(f, "cannot index the scenario's media: {reason}")
            }
        }
    }
}

impl std::error::Error for ScenarioError {}

/// The start state of `profile`: the default one, or the one of the named scenario.
/// The profile is checked first, so a build of another profile does not even say which
/// scenarios exist.
pub fn start_state(profile: Profile, scenario: Option<&str>) -> Result<StartState, ScenarioError> {
    let Some(id) = scenario else {
        return Ok(StartState::default());
    };
    if profile != profile::E2E {
        return Err(ScenarioError::NotE2e {
            identifier: profile.identifier,
        });
    }
    if id == scenario::KIT {
        return Ok(StartState {
            screen: StartScreen::Kit,
            ..StartState::default()
        });
    }
    scenario::find(id)
        .map(StartState::from)
        .ok_or_else(|| ScenarioError::Unknown(id.to_owned()))
}

/// Gives the library the media of `seed` before the window opens. `media` is the
/// value of [`SCENARIO_MEDIA_VARIABLE`]: the directory the harness seeded. It becomes
/// the library's one scan root and is scanned, as `app:control start` does for the
/// Tauri application. [`Seed::Empty`] touches nothing.
///
/// Only a start state of the E2E profile carries a seed ([`start_state`]), and this
/// runs before the event loop exists, so the scan blocks no UI thread.
pub fn seed_library(
    seed: Seed,
    media: Option<OsString>,
    backend: &dyn Backend,
) -> Result<(), ScenarioError> {
    match seed {
        Seed::Empty => Ok(()),
        Seed::Control => {
            let media = media
                .filter(|media| !media.is_empty())
                .ok_or(ScenarioError::MediaUnset)?;
            if !Path::new(&media).is_dir() {
                return Err(ScenarioError::MediaInvalid);
            }
            let media = media
                .into_string()
                .map_err(|_| ScenarioError::MediaInvalid)?;
            backend
                .scan_folder(media)
                .map(|_| ())
                .map_err(|error| ScenarioError::SeedFailed(error.text().to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use media_tagger_client::{
        backend::{fake::Call, BackendError, FakeBackend},
        runtime::{DEFAULT_TILE_SIZE, TILE_SIZE_MAX, TILE_SIZE_MIN},
        scenario::{Seed, SCENARIOS},
        theme::ThemeName,
    };
    use media_tagger_core::models::{ScanCompletion, ScanSummary};

    use super::{seed_library, start_state, ScenarioError, StartScreen, StartState};
    use crate::profile::{DEV, E2E};

    fn scanned() -> ScanSummary {
        ScanSummary {
            completion: ScanCompletion::Complete,
            indexed: 7,
            removed: 0,
            failed: 0,
        }
    }

    #[test]
    fn without_a_scenario_every_profile_starts_in_the_default_state() {
        for profile in [DEV, E2E] {
            assert_eq!(start_state(profile, None), Ok(StartState::default()));
        }
        assert_eq!(StartState::default().theme, ThemeName::Dark);
    }

    #[test]
    fn the_e2e_profile_starts_in_every_catalogued_scenario() {
        for scenario in SCENARIOS {
            assert_eq!(
                start_state(E2E, Some(scenario.id)),
                Ok(StartState {
                    theme: scenario.theme,
                    screen: StartScreen::Main,
                    seed: scenario.seed,
                    tile_size: scenario.tile_size,
                    selection: scenario.selection,
                }),
                "{}",
                scenario.id
            );
        }
        assert_eq!(
            start_state(E2E, Some("shell-empty-light")).map(|state| state.theme),
            Ok(ThemeName::Light)
        );
    }

    #[test]
    fn the_e2e_profile_starts_on_the_kit_board() {
        assert_eq!(
            start_state(E2E, Some("kit")).map(|state| state.screen),
            Ok(StartScreen::Kit)
        );
        assert_eq!(StartState::default().screen, StartScreen::Main);
    }

    #[test]
    fn the_dev_profile_refuses_every_scenario_before_looking_it_up() {
        for id in ["shell-empty", "kit", "no-such-scenario"] {
            let error = start_state(DEV, Some(id)).expect_err("refused");

            assert_eq!(
                error,
                ScenarioError::NotE2e {
                    identifier: DEV.identifier
                }
            );
            let message = error.to_string();
            assert!(
                message.starts_with(
                    "Refusing a scenario on profile com.example.mediatagger.gpui.dev."
                ),
                "{message}"
            );
            assert!(message.contains(E2E.identifier), "{message}");
        }
    }

    #[test]
    fn an_unknown_scenario_is_refused_with_the_known_ids() {
        let error = start_state(E2E, Some("no-such-scenario")).expect_err("refused");

        assert_eq!(error, ScenarioError::Unknown("no-such-scenario".to_owned()));
        let message = error.to_string();
        assert!(message.contains("\"no-such-scenario\""), "{message}");
        assert!(
            message.contains("shell-empty, shell-empty-light, gallery-default"),
            "{message}"
        );
        assert!(message.ends_with(", kit"), "{message}");
    }

    #[test]
    fn the_gallery_scenarios_start_with_the_control_seed_at_their_tile_size() {
        for (id, tile_size) in [
            ("gallery-default", DEFAULT_TILE_SIZE),
            ("gallery-tiles-small", TILE_SIZE_MIN),
            ("gallery-tiles-large", TILE_SIZE_MAX),
        ] {
            let start = start_state(E2E, Some(id)).expect("a scenario of the E2E profile");
            assert_eq!(start.seed, Seed::Control, "{id}");
            assert_eq!(start.tile_size, tile_size, "{id}");
            assert_eq!(start.screen, StartScreen::Main, "{id}");
        }
        let default = StartState::default();
        assert_eq!(
            (default.seed, default.tile_size),
            (Seed::Empty, DEFAULT_TILE_SIZE)
        );
    }

    #[test]
    fn the_selection_scenario_starts_with_its_clicks_and_every_other_start_without() {
        let start = start_state(E2E, Some("gallery-selection")).expect("a scenario of E2E");
        assert_eq!(start.seed, Seed::Control);
        assert_eq!(start.tile_size, DEFAULT_TILE_SIZE);
        assert_eq!(
            start.selection,
            &media_tagger_client::scenario::GALLERY_SELECTION_CLICKS[..]
        );
        assert!(StartState::default().selection.is_empty());
        assert!(start_state(E2E, Some("gallery-default"))
            .expect("a scenario of E2E")
            .selection
            .is_empty());
    }

    #[test]
    fn the_control_seed_scans_the_seeded_directory_as_the_one_root() {
        let media = tempfile::tempdir().expect("tempdir");
        let backend = FakeBackend::new();
        backend.on_scan_folder(|_| Ok(scanned()));

        seed_library(Seed::Control, Some(OsString::from(media.path())), &backend).expect("seeded");

        assert_eq!(
            backend.calls(),
            [Call::ScanFolder {
                path: media.path().to_str().expect("UTF-8").to_owned()
            }]
        );
    }

    #[test]
    fn the_empty_seed_touches_nothing() {
        let backend = FakeBackend::new();

        seed_library(Seed::Empty, None, &backend).expect("nothing to do");
        seed_library(Seed::Empty, Some(OsString::from("/nowhere")), &backend)
            .expect("nothing to do");

        assert!(backend.calls().is_empty());
    }

    #[test]
    fn a_seed_without_its_directory_is_refused_before_the_backend_is_called() {
        let backend = FakeBackend::new();
        let missing = tempfile::tempdir().expect("tempdir").path().join("gone");

        assert_eq!(
            seed_library(Seed::Control, None, &backend),
            Err(ScenarioError::MediaUnset)
        );
        assert_eq!(
            seed_library(Seed::Control, Some(OsString::new()), &backend),
            Err(ScenarioError::MediaUnset)
        );
        assert_eq!(
            seed_library(Seed::Control, Some(missing.into_os_string()), &backend),
            Err(ScenarioError::MediaInvalid)
        );
        assert!(backend.calls().is_empty());
        assert!(ScenarioError::MediaUnset
            .to_string()
            .contains("MEDIATAGGER_GPUI_SCENARIO_MEDIA"));
    }

    #[test]
    fn a_refused_scan_fails_the_start() {
        let media = tempfile::tempdir().expect("tempdir");
        let backend = FakeBackend::new();
        backend.on_scan_folder(|_| Err(BackendError::rejected("i18n:common.libraryBusy")));

        let error = seed_library(Seed::Control, Some(OsString::from(media.path())), &backend)
            .expect_err("refused");

        assert_eq!(
            error,
            ScenarioError::SeedFailed("i18n:common.libraryBusy".to_owned())
        );
    }
}
