//! `--scenario <id>`: the start states of the comparison harness are accepted only by
//! the isolated E2E profile, and a refused scenario stops before the display check, so
//! nothing here needs or touches a display. All app data would live in temporary
//! directories; every case asserts that none was created.

use std::{
    collections::HashMap,
    ffi::OsString,
    path::Path,
    process::{Command, Output, Stdio},
};

use media_tagger_gpui::{profile, run_scenario_with};
use tempfile::tempdir;

/// A process environment made of exactly `vars`.
fn env_of(vars: &[(&str, &OsString)]) -> impl Fn(&str) -> Option<OsString> {
    let vars: HashMap<String, OsString> = vars
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).clone()))
        .collect();
    move |name| vars.get(name).cloned()
}

fn error_of(result: Result<(), Box<dyn std::error::Error>>) -> String {
    match result {
        Ok(()) => panic!("expected the startup to stop"),
        Err(error) => error.to_string(),
    }
}

fn is_empty_dir(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
}

const NO_DISPLAY: &str = "no display server: neither WAYLAND_DISPLAY nor DISPLAY is set";

#[test]
fn the_dev_profile_refuses_a_scenario_before_the_display_check() {
    let tmp = tempdir().expect("tempdir");
    let data_home = OsString::from(tmp.path());
    let env = env_of(&[("XDG_DATA_HOME", &data_home)]);

    for scenario in ["shell-empty", "shell-empty-light", "no-such-scenario"] {
        let error = error_of(run_scenario_with(
            profile::DEV.identifier,
            Some(scenario),
            &env,
        ));

        assert!(
            error.starts_with("Refusing a scenario on profile com.example.mediatagger.gpui.dev."),
            "{scenario}: {error}"
        );
    }
    assert!(
        is_empty_dir(tmp.path()),
        "a refused scenario created app data"
    );
}

#[test]
fn an_identifier_off_the_allowlist_is_refused_before_its_scenario_is_looked_at() {
    let env = env_of(&[]);

    for identifier in ["com.example.mediatagger", "com.example.mediatagger.e2e"] {
        let error = error_of(run_scenario_with(identifier, Some("shell-empty"), &env));

        assert!(
            error.starts_with(&format!("Refusing to start with identifier {identifier}.")),
            "{identifier}: {error}"
        );
    }
}

#[test]
fn the_e2e_profile_refuses_an_unknown_scenario_before_the_display_check() {
    let tmp = tempdir().expect("tempdir");
    let data_home = OsString::from(tmp.path());
    let env = env_of(&[("XDG_DATA_HOME", &data_home)]);

    let error = error_of(run_scenario_with(
        profile::E2E.identifier,
        Some("no-such-scenario"),
        &env,
    ));

    assert_eq!(
        error,
        "unknown scenario \"no-such-scenario\"; known: shell-empty, shell-empty-light, \
         gallery-default, gallery-tiles-small, gallery-tiles-large, gallery-selection, kit"
    );
    assert!(
        is_empty_dir(tmp.path()),
        "a refused scenario created app data"
    );
}

#[test]
fn the_e2e_profile_accepts_its_scenarios_and_goes_on_to_the_display_check() {
    let tmp = tempdir().expect("tempdir");
    let data_home = OsString::from(tmp.path());
    let env = env_of(&[("XDG_DATA_HOME", &data_home)]);

    // A scenario with media gets as far as well: its seed is indexed only after a
    // display has answered and the profile is open.
    for scenario in [
        None,
        Some("shell-empty"),
        Some("shell-empty-light"),
        Some("gallery-default"),
        Some("gallery-tiles-small"),
        Some("gallery-tiles-large"),
        Some("gallery-selection"),
    ] {
        let error = error_of(run_scenario_with(profile::E2E.identifier, scenario, &env));

        assert_eq!(error, NO_DISPLAY, "{scenario:?}");
    }
    assert!(is_empty_dir(tmp.path()), "no app data without a display");
}

const BINARY: &str = env!("CARGO_BIN_EXE_media_tagger_gpui");

/// Runs the built binary with no display and a temporary data home.
fn binary(arguments: &[&str], data_home: &Path) -> Output {
    Command::new(BINARY)
        .args(arguments)
        .env_clear()
        .env("XDG_DATA_HOME", data_home.join("data"))
        .env("HOME", data_home.join("home"))
        .stdin(Stdio::null())
        .output()
        .expect("run the binary")
}

#[test]
fn the_binary_starts_a_scenario_only_when_it_is_the_e2e_build() {
    let tmp = tempdir().expect("tempdir");

    let output = binary(&["--scenario", "shell-empty"], tmp.path());

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The test binary is the dev build unless the whole test run was built for E2E.
    let expected = if profile::BUILT_IDENTIFIER == profile::E2E.identifier {
        format!("Tagrove GPUI cannot start: {NO_DISPLAY}")
    } else {
        format!(
            "Tagrove GPUI cannot start: Refusing a scenario on profile {}.",
            profile::BUILT_IDENTIFIER
        )
    };
    assert!(stderr.contains(&expected), "{stderr}");
    assert!(is_empty_dir(tmp.path()), "no app data was expected");
}

#[test]
fn the_binary_refuses_arguments_it_does_not_know() {
    let tmp = tempdir().expect("tempdir");

    for (arguments, message) in [
        (&["--scenario"][..], "--scenario needs a scenario id"),
        (&["--help"], "unknown argument \"--help\""),
        (&["shell-empty"], "unknown argument \"shell-empty\""),
        (
            &["--identifier", "--scenario", "shell-empty"],
            "argument \"--scenario\" cannot be combined with the others",
        ),
    ] {
        let output = binary(arguments, tmp.path());

        assert_eq!(output.status.code(), Some(1), "{arguments:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("Tagrove GPUI cannot start: {message}")),
            "{arguments:?}: {stderr}"
        );
    }
    assert!(
        is_empty_dir(tmp.path()),
        "a refused argument created app data"
    );
}
