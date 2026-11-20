//! Deterministic start states for the comparison harness (`bun run compare:ui`).
//!
//! `--scenario <id>` starts the application in a state of
//! `media_tagger_client::scenario`. Only the isolated E2E profile accepts it: a scenario
//! presets what the user's own preferences decide in every other profile, and the
//! harness that asks for one always runs the E2E build on a private display with
//! scratch app data. The guard runs before the display check, so a refused scenario
//! touches neither a display nor the disk.

use std::fmt::{Display, Formatter};

use media_tagger_client::{
    scenario::{self, Scenario},
    theme::ThemeName,
};

use crate::profile::{self, Profile};

/// What the main window shows first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StartState {
    pub theme: ThemeName,
}

impl From<Scenario> for StartState {
    fn from(scenario: Scenario) -> Self {
        Self {
            theme: scenario.theme,
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
                let known: Vec<_> = scenario::SCENARIOS.iter().map(|s| s.id).collect();
                write!(f, "unknown scenario {id:?}; known: {}", known.join(", "))
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
    scenario::find(id)
        .map(StartState::from)
        .ok_or_else(|| ScenarioError::Unknown(id.to_owned()))
}

#[cfg(test)]
mod tests {
    use media_tagger_client::{scenario::SCENARIOS, theme::ThemeName};

    use super::{start_state, ScenarioError, StartState};
    use crate::profile::{DEV, E2E};

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
                    theme: scenario.theme
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
    fn the_dev_profile_refuses_every_scenario_before_looking_it_up() {
        for id in ["shell-empty", "no-such-scenario"] {
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
            message.contains("shell-empty, shell-empty-light"),
            "{message}"
        );
    }
}
