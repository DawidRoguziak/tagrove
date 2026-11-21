//! Tagrove GPUI adapter (ADR 0036): the second UI adapter over `media_tagger_core`,
//! next to the Tauri adapter in `src-tauri/`.
//!
//! Dependency direction: `media_tagger_gpui` → `media_tagger_client` →
//! `media_tagger_core`. This crate holds views and executor glue. It reaches the core
//! only in `boot.rs` (startup composition); everything else goes through the client
//! crate. `src-tauri/tests/architecture_boundaries.rs` enforces both.

pub mod boot;
pub mod cli;
pub mod profile;
pub mod runtime;
pub mod scenario;
pub mod ui;
pub mod views;
pub mod window;

use std::{error::Error, ffi::OsString, sync::Arc};

use media_tagger_client::{
    backend::CoreBackend,
    exec::ThreadExecutor,
    i18n::{Language, LocaleEnvironment},
    prefs::{FilePrefsStore, PrefsStore},
    runtime::RuntimeConfig,
};
use runtime::RuntimeParts;
use scenario::StartScreen;
use window::MainScreen;

/// Starts the application and returns after its last window has closed.
///
/// Order: identifier guard → scenario guard → display check (a named display server
/// must answer) → app-data directory → `boot::start` (instance lock, storage, ffmpeg,
/// workers) → preferences → backend → window, whose runtime then starts (library,
/// startup scan, scan roots). Nothing touches the disk before the identifier and
/// scenario guards have passed, and nothing opens the profile before a display server
/// has answered.
///
/// The start theme and language are the stored preferences (else dark, and the
/// system's language). A scenario starts in its own theme and in English, as the
/// Tauri E2E specs start.
///
/// `scenario` is the id given with `--scenario`; see [`scenario::start_state`].
pub fn run(scenario: Option<&str>) -> Result<(), Box<dyn Error>> {
    run_scenario_with(profile::BUILT_IDENTIFIER, scenario, &|name| {
        std::env::var_os(name)
    })
}

/// [`run`] without a scenario, with the identifier and the environment passed in, so
/// tests can drive the startup guards without a baked identifier or the process
/// environment.
pub fn run_with(
    identifier: &str,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Result<(), Box<dyn Error>> {
    run_scenario_with(identifier, None, env)
}

/// [`run`] with the identifier and the environment passed in.
pub fn run_scenario_with(
    identifier: &str,
    scenario: Option<&str>,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Result<(), Box<dyn Error>> {
    let profile = profile::resolve(identifier)?;
    let start = scenario::start_state(profile, scenario)?;
    let display = window::require_display(&window::DisplayEnvironment {
        wayland_display: env("WAYLAND_DISPLAY"),
        wayland_socket: env("WAYLAND_SOCKET"),
        xdg_runtime_dir: env("XDG_RUNTIME_DIR"),
        x11_display: env("DISPLAY"),
    })?;
    let data_dir = profile::data_dir(
        profile,
        env("XDG_DATA_HOME").as_deref(),
        env("HOME").as_deref(),
    )?;

    // Held until the event loop has returned; dropping it releases the profile's
    // instance lock. It does not stop the thumbnail workers: the core's scheduler has
    // no shutdown, so its idle threads end with the process.
    let mut boot = boot::start(&data_dir, &boot::resource_dir()?)?;
    // Backend calls run here, never on the UI thread; replies wake the UI thread.
    let executor = Arc::new(ThreadExecutor::new());
    let (theme, screen) = match start.screen {
        StartScreen::Main => {
            let prefs = Arc::new(FilePrefsStore::new(&data_dir));
            let config = if scenario.is_some() {
                RuntimeConfig {
                    theme: start.theme,
                    language: Language::En,
                }
            } else {
                let stored = prefs.load();
                RuntimeConfig {
                    theme: stored.theme_or_default(),
                    language: stored.language_or_system(&LocaleEnvironment::read(env)),
                }
            };
            let backend = CoreBackend::new(
                Arc::clone(&boot.state),
                std::mem::take(&mut boot.startup_scan_roots),
            )?;
            let parts = RuntimeParts {
                config,
                backend: Arc::new(backend),
                prefs,
                executor: executor.clone(),
            };
            (config.theme, MainScreen::Shell(parts))
        }
        StartScreen::Kit => (start.theme, MainScreen::Kit),
    };

    let outcome = window::run_until_closed(profile, theme, screen, display);
    // Queued backend calls are dropped; one still running ends with the process, as a
    // command of the Tauri application does when its window closes.
    executor.shutdown();
    drop(boot);
    outcome
}
