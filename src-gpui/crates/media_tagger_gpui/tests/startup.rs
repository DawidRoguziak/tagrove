//! Startup guards of the GPUI adapter, in the order `run` promises: identifier guard →
//! display check → app-data directory → instance lock → window. Every case here stops
//! before a window would open, so none of them needs or touches a display: where the
//! display check has to pass, a socket in a temporary directory stands in for the
//! compositor. All app data lives in temporary directories.

use std::{
    collections::HashMap,
    ffi::OsString,
    os::unix::net::UnixListener,
    path::Path,
    process::{Command, Stdio},
};

use media_tagger_gpui::{boot, profile, run_with};
use tempfile::{tempdir, TempDir};

/// A local X display that no server owns: its number has no TCP port (6000 + N does
/// not fit) and no socket in `/tmp/.X11-unix`.
const DEAD_X11_DISPLAY: &str = ":65000";

/// A stand-in for a Wayland compositor: a socket that accepts every connection and
/// closes it at once. The display check finds it answering; a startup that wrongly went
/// on to open a window would fail on it instead of waiting for a reply.
struct FakeCompositor {
    _dir: TempDir,
    /// The absolute socket path, as a `WAYLAND_DISPLAY` value.
    wayland_display: OsString,
}

impl FakeCompositor {
    fn start() -> Self {
        let dir = tempdir().expect("tempdir");
        let socket = dir.path().join("wayland-fake");
        let listener = UnixListener::bind(&socket).expect("bind the compositor socket");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                drop(stream);
            }
        });
        Self {
            _dir: dir,
            wayland_display: socket.into(),
        }
    }
}

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

#[test]
fn an_identifier_off_the_allowlist_is_refused_before_anything_touches_the_disk() {
    let tmp = tempdir().expect("tempdir");
    let data_home = OsString::from(tmp.path().join("data"));
    let home = OsString::from(tmp.path().join("home"));

    for identifier in [
        "com.example.mediatagger",
        "com.example.mediatagger.dev",
        "com.example.mediatagger.e2e",
        "com.example.mediatagger.gpui",
        "",
    ] {
        // No display either: the refusal must come from the identifier guard, which
        // runs before the display check.
        let env = env_of(&[("XDG_DATA_HOME", &data_home), ("HOME", &home)]);

        let error = error_of(run_with(identifier, &env));

        assert!(
            error.starts_with(&format!("Refusing to start with identifier {identifier}.")),
            "{identifier:?}: {error}"
        );
    }
    assert!(
        is_empty_dir(tmp.path()),
        "a refused identifier created app data"
    );
}

#[test]
fn an_isolated_profile_without_a_display_stops_before_its_data_directory() {
    let tmp = tempdir().expect("tempdir");
    let data_home = OsString::from(tmp.path());
    let empty = OsString::new();

    for identifier in [profile::DEV.identifier, profile::E2E.identifier] {
        let env = env_of(&[
            ("XDG_DATA_HOME", &data_home),
            ("WAYLAND_DISPLAY", &empty),
            ("DISPLAY", &empty),
        ]);

        let error = error_of(run_with(identifier, &env));

        assert_eq!(
            error, "no display server: neither WAYLAND_DISPLAY nor DISPLAY is set",
            "{identifier}"
        );
    }
    assert!(is_empty_dir(tmp.path()), "no app data without a display");
}

#[test]
fn a_display_that_does_not_answer_stops_before_the_profile_is_opened() {
    let tmp = tempdir().expect("tempdir");
    let data_home = OsString::from(tmp.path().join("data"));
    let dead_wayland = OsString::from(tmp.path().join("no-compositor"));
    let dead_wayland_name = OsString::from("no-compositor");
    let runtime_dir = OsString::from(tmp.path());
    let dead_x11 = OsString::from(DEAD_X11_DISPLAY);

    for display in [
        vec![("WAYLAND_DISPLAY", &dead_wayland)],
        vec![
            ("WAYLAND_DISPLAY", &dead_wayland_name),
            ("XDG_RUNTIME_DIR", &runtime_dir),
        ],
        vec![("DISPLAY", &dead_x11)],
        vec![("WAYLAND_DISPLAY", &dead_wayland), ("DISPLAY", &dead_x11)],
    ] {
        let mut vars = vec![("XDG_DATA_HOME", &data_home)];
        vars.extend(display.iter().copied());

        let error = error_of(run_with(profile::DEV.identifier, &env_of(&vars)));

        assert_eq!(
            error, "no display server: the one WAYLAND_DISPLAY or DISPLAY names does not answer",
            "{display:?}"
        );
    }
    assert!(
        is_empty_dir(tmp.path()),
        "an unreachable display must not open the profile"
    );
}

#[test]
fn an_isolated_profile_needs_an_absolute_data_home() {
    let compositor = FakeCompositor::start();
    let display = &compositor.wayland_display;
    let relative = OsString::from("relative/data");

    let env = env_of(&[
        ("WAYLAND_DISPLAY", display),
        ("XDG_DATA_HOME", &relative),
        ("HOME", &relative),
    ]);

    let error = error_of(run_with(profile::DEV.identifier, &env));

    assert!(
        error.starts_with("cannot resolve the app-data directory"),
        "{error}"
    );
}

#[test]
fn a_profile_held_by_another_instance_stops_before_the_window() {
    let tmp = tempdir().expect("tempdir");
    let data_home = OsString::from(tmp.path());
    let compositor = FakeCompositor::start();
    let data_dir = tmp.path().join(profile::DEV.identifier);
    let running = boot::start(&data_dir, tmp.path()).expect("the first instance starts");

    let env = env_of(&[
        ("XDG_DATA_HOME", &data_home),
        ("WAYLAND_DISPLAY", &compositor.wayland_display),
    ]);
    let error = error_of(run_with(profile::DEV.identifier, &env));

    assert_eq!(
        error,
        "another Tagrove instance is already using this data profile"
    );
    drop(running);
}

const BINARY: &str = env!("CARGO_BIN_EXE_media_tagger_gpui");

#[test]
fn the_binary_reports_its_baked_isolated_identifier() {
    let output = Command::new(BINARY)
        .arg("--identifier")
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .expect("run the binary");

    assert!(output.status.success(), "{output:?}");
    let identifier = String::from_utf8(output.stdout).expect("utf-8");
    assert_eq!(identifier, format!("{}\n", profile::BUILT_IDENTIFIER));
    assert!(profile::resolve(identifier.trim()).is_ok(), "{identifier}");
}

#[test]
fn the_binary_exits_with_failure_and_no_app_data_without_a_display() {
    let tmp = tempdir().expect("tempdir");

    let output = Command::new(BINARY)
        .env_clear()
        .env("XDG_DATA_HOME", tmp.path().join("data"))
        .env("HOME", tmp.path().join("home"))
        .stdin(Stdio::null())
        .output()
        .expect("run the binary");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Tagrove GPUI cannot start: no display server"),
        "{stderr}"
    );
    assert!(is_empty_dir(tmp.path()), "no app data without a display");
}

#[test]
fn the_binary_exits_with_failure_and_no_app_data_when_its_display_does_not_answer() {
    let tmp = tempdir().expect("tempdir");

    let output = Command::new(BINARY)
        .env_clear()
        .env("XDG_DATA_HOME", tmp.path().join("data"))
        .env("HOME", tmp.path().join("home"))
        .env("WAYLAND_DISPLAY", tmp.path().join("no-compositor"))
        .env("DISPLAY", DEAD_X11_DISPLAY)
        .stdin(Stdio::null())
        .output()
        .expect("run the binary");

    // A panic inside GPUI would exit with 101 after the profile had been opened.
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "Tagrove GPUI cannot start: no display server: the one WAYLAND_DISPLAY or DISPLAY \
             names does not answer"
        ),
        "{stderr}"
    );
    assert!(
        is_empty_dir(tmp.path()),
        "an unreachable display must not open the profile"
    );
}
