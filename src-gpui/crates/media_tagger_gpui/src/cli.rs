//! Command line of the binary.
//!
//! `media_tagger_gpui` starts the application, `--identifier` prints the identifier
//! baked into the binary, and `--scenario <id>` starts the application in a named
//! comparison state (`crate::scenario`). Everything else is refused, so a mistyped
//! flag never starts the application in its default state.

use std::fmt::{Display, Formatter};

pub const IDENTIFIER_FLAG: &str = "--identifier";
pub const SCENARIO_FLAG: &str = "--scenario";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print the baked identifier and exit; touches neither a display nor the disk.
    PrintIdentifier,
    /// Start the application, in the named scenario when one is given.
    Run { scenario: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// `--scenario` without an id.
    MissingScenarioId,
    /// An argument next to `--identifier`, or a second `--scenario`.
    Conflict(String),
    Unknown(String),
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingScenarioId => write!(f, "{SCENARIO_FLAG} needs a scenario id"),
            Self::Conflict(argument) => {
                write!(
                    f,
                    "argument {argument:?} cannot be combined with the others"
                )
            }
            Self::Unknown(argument) => write!(
                f,
                "unknown argument {argument:?}; expected {IDENTIFIER_FLAG} or {SCENARIO_FLAG} <id>"
            ),
        }
    }
}

impl std::error::Error for CliError {}

/// Parses the arguments after the program name.
pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Command, CliError> {
    let mut arguments = arguments.into_iter();
    let mut scenario = None;
    let mut first = true;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            IDENTIFIER_FLAG if first => {
                return match arguments.next() {
                    None => Ok(Command::PrintIdentifier),
                    Some(extra) => Err(CliError::Conflict(extra)),
                };
            }
            IDENTIFIER_FLAG => return Err(CliError::Conflict(argument)),
            SCENARIO_FLAG if scenario.is_some() => return Err(CliError::Conflict(argument)),
            SCENARIO_FLAG => {
                let id = arguments
                    .next()
                    .filter(|id| !id.is_empty() && !id.starts_with('-'))
                    .ok_or(CliError::MissingScenarioId)?;
                scenario = Some(id);
            }
            _ => return Err(CliError::Unknown(argument)),
        }
        first = false;
    }
    Ok(Command::Run { scenario })
}

#[cfg(test)]
mod tests {
    use super::{parse, CliError, Command};

    fn parsed(arguments: &[&str]) -> Result<Command, CliError> {
        parse(arguments.iter().map(|argument| (*argument).to_owned()))
    }

    #[test]
    fn no_argument_starts_the_application_in_its_default_state() {
        assert_eq!(parsed(&[]), Ok(Command::Run { scenario: None }));
    }

    #[test]
    fn identifier_flag_stands_alone() {
        assert_eq!(parsed(&["--identifier"]), Ok(Command::PrintIdentifier));
        assert_eq!(
            parsed(&["--identifier", "--scenario"]),
            Err(CliError::Conflict("--scenario".to_owned()))
        );
        assert_eq!(
            parsed(&["--scenario", "shell-empty", "--identifier"]),
            Err(CliError::Conflict("--identifier".to_owned()))
        );
    }

    #[test]
    fn scenario_flag_takes_exactly_one_id() {
        assert_eq!(
            parsed(&["--scenario", "shell-empty"]),
            Ok(Command::Run {
                scenario: Some("shell-empty".to_owned())
            })
        );
        for arguments in [
            &["--scenario"][..],
            &["--scenario", ""],
            &["--scenario", "--identifier"],
        ] {
            assert_eq!(
                parsed(arguments),
                Err(CliError::MissingScenarioId),
                "{arguments:?}"
            );
        }
        assert_eq!(
            parsed(&[
                "--scenario",
                "shell-empty",
                "--scenario",
                "shell-empty-light"
            ]),
            Err(CliError::Conflict("--scenario".to_owned()))
        );
    }

    #[test]
    fn anything_else_is_refused_with_its_text() {
        for argument in ["--help", "shell-empty", "--scenario=shell-empty", "-s"] {
            let error = parsed(&[argument]).expect_err("refused");

            assert_eq!(error, CliError::Unknown(argument.to_owned()));
            assert!(error.to_string().contains(argument), "{error}");
        }
        assert_eq!(
            parsed(&["--scenario", "shell-empty", "extra"]),
            Err(CliError::Unknown("extra".to_owned()))
        );
    }
}
