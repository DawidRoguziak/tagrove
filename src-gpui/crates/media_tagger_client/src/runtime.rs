//! The application runtime (`createAppRuntime` of the Solid frontend): the state that lives
//! as long as the application, whatever view is shown.
//!
//! Views come and go: switching between the gallery and the settings page drops the
//! view that is left and every bit of state local to it (scroll position, focus,
//! hover). What must survive lives here: the current view, the theme and the
//! language, the tile size, the library model ([`Library`]: the query with its
//! session, page cache and applied filters, the canonical tags, the details cache,
//! the known tags and the metadata writes), the startup sequence and what it learned
//! (the scan roots, the outcome of the startup scan). Later epics hang their models
//! (selection, lightbox, settings) next to these.
//!
//! The library is a model of its own, embedded here. The inputs of its query model
//! travel as [`RuntimeInput::Query`] and the query's effects as
//! [`RuntimeEffect::Query`], which the runner hands to [`run_query_effect`]; every
//! other input of the library travels as [`RuntimeInput::Library`] and its other
//! effects as [`RuntimeEffect::Library`] ([`run_library_effect`]). Both kinds of
//! input pass through the library, which empties its details cache when the query
//! epoch changes. A view sends `RuntimeInput::Query` (the gallery's viewport, Retry)
//! and reads the rows through [`AppRuntime::query`]; an editor sends
//! `RuntimeInput::Library` (details, pins, writes) and reads tags and details
//! through [`AppRuntime::library`].
//!
//! [`AppRuntime`] is a model in the convention of [`crate::model`]: an input gives a new
//! state and effects, replies come back as inputs with the token they were started
//! with, and a reply that has been superseded changes nothing. State coming from
//! the backend is published only from the backend's answer. [`RuntimeRunner`] runs the
//! effects on an executor through the `Backend` port and the `PrefsStore`.
//!
//! # Start
//!
//! [`RuntimeInput::Start`] runs once per runtime (later starts are ignored), as
//! `createAppRuntime().start` does: it refreshes the library (the query model's
//! refresh, with its own owner ID and generations, and the known tags), joins the
//! startup scan
//! and lists the scan roots, all at once, and stores the start theme and language
//! (the Solid frontend writes both to `localStorage` when it boots). The startup scan is
//! single-flight in the backend as well (`scan_startup_roots` joins the one scan of the
//! launch); here it is requested once. Its outcome never blocks the window: while
//! it runs, [`AppRuntime::maintenance_loading`] is set; when it found work, or failed,
//! the library and the scan roots are refreshed again, and only after both answered
//! is the scan [`StartupScanPhase::Finished`]. A launch without work refreshes nothing.

use std::sync::Arc;

use media_tagger_core::models::{ScanRoot, ScanSummary};

use crate::{
    backend::{Backend, BackendError, BackendResult},
    i18n::Language,
    library::{
        run_library_effect, run_query_effect, AssetQuery, Library, LibraryEffect, LibraryEvent,
        LibraryInput, QueryConfig, QueryEffect, QueryEvent, QueryInput,
    },
    model::{Dispatch, Model, RequestSlot, RequestToken, Runner, Tokens},
    prefs::{Prefs, PrefsStore},
    theme::ThemeName,
};

#[cfg(test)]
mod tests;

/// The two views of the main window (`view` of `createAppRuntime`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Gallery,
    Settings,
}

/// The tile size of the gallery (`tileSizeService.ts`), in CSS pixels.
pub const DEFAULT_TILE_SIZE: u32 = 188;
pub const TILE_SIZE_MIN: u32 = 140;
pub const TILE_SIZE_MAX: u32 = 340;
pub const TILE_SIZE_STEP: u32 = 8;

/// `clampTileSize`: a size inside the slider's range. A value that is not a number
/// keeps `current`.
pub fn clamp_tile_size(next: f64, current: u32) -> u32 {
    if !next.is_finite() {
        return current;
    }
    next.round()
        .clamp(f64::from(TILE_SIZE_MIN), f64::from(TILE_SIZE_MAX)) as u32
}

/// `getNextTileSizeByWheel`: one step smaller for a wheel turned down (positive
/// `delta_y`), one step larger for a wheel turned up, unchanged for no movement.
pub fn next_tile_size_by_wheel(current: u32, delta_y: f64) -> u32 {
    if delta_y == 0.0 || !delta_y.is_finite() {
        return current;
    }
    let step = f64::from(TILE_SIZE_STEP);
    let next = if delta_y > 0.0 {
        f64::from(current) - step
    } else {
        f64::from(current) + step
    };
    clamp_tile_size(next, current)
}

/// How the startup scan ended.
#[derive(Debug, Clone)]
pub enum StartupScanOutcome {
    /// The launch had no root with `auto_scan_on_startup`; nothing was scanned.
    NoWork,
    Scanned(ScanSummary),
    Failed(BackendError),
}

/// Where the startup scan of this runtime is.
#[derive(Debug, Clone, Default)]
pub enum StartupScanPhase {
    /// [`RuntimeInput::Start`] has not come yet.
    #[default]
    NotStarted,
    /// Waiting for `scan_startup_roots`.
    Scanning,
    /// The scan answered; the library and the scan roots are being refreshed.
    Refreshing(StartupScanOutcome),
    Finished(StartupScanOutcome),
}

impl StartupScanPhase {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Scanning | Self::Refreshing(_))
    }

    /// The outcome once the scan has finished.
    pub fn outcome(&self) -> Option<&StartupScanOutcome> {
        match self {
            Self::Finished(outcome) => Some(outcome),
            _ => None,
        }
    }
}

/// How the runtime starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// The start theme: stored, else dark (or the scenario's).
    pub theme: ThemeName,
    /// The start language: stored, else the system's, else English.
    pub language: Language,
}

/// Commands, backend replies and preference saves.
#[derive(Debug)]
pub enum RuntimeInput {
    /// Runs the start sequence once; later starts are ignored.
    Start,
    OpenSettings,
    /// Back to the gallery; also ends the scan-section highlight.
    CloseSettings,
    /// "Add first folder": the settings page with the scan section highlighted.
    OpenFirstFolderSettings,
    SetTheme(ThemeName),
    SetLanguage(Language),
    /// The slider's value (`setTileSize`).
    SetTileSize(f64),
    /// Ctrl and the wheel over the gallery (`zoom`), with the wheel's `deltaY`.
    Zoom(f64),
    /// Asks the backend for the library again with the applied filters and for the
    /// known tags ([`LibraryInput::Refresh`]); an earlier query refresh still running
    /// is superseded.
    RefreshLibrary,
    /// A command, a backend reply or a timer of the library's query model.
    Query(QueryInput),
    /// Any other command or backend reply of the library model: details, tag pins,
    /// metadata writes, the known tags, maintenance ([`RuntimeInput::library`]). Boxed:
    /// a reply with details or a bulk summary is several times the size of every
    /// other input.
    Library(Box<LibraryInput>),
    /// Lists the scan roots again; an earlier listing still running is superseded.
    RefreshScanRoots,
    ScanRootsLoaded {
        token: RequestToken,
        result: BackendResult<Vec<ScanRoot>>,
    },
    StartupScanFinished {
        result: BackendResult<Option<ScanSummary>>,
    },
    /// A preference save ended; `Err` holds why it failed.
    PrefsSaved {
        result: Result<(), String>,
    },
}

impl RuntimeInput {
    /// `input` for the library model.
    pub fn library(input: LibraryInput) -> Self {
        Self::Library(Box::new(input))
    }
}

/// Work for the [`RuntimeRunner`].
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEffect {
    /// Work of the library's query model.
    Query(QueryEffect),
    /// Other work of the library model; never a [`LibraryEffect::Query`].
    Library(LibraryEffect),
    ListScanRoots(RequestToken),
    ScanStartupRoots,
    SavePrefs(Prefs),
}

/// The application runtime.
#[derive(Debug)]
pub struct AppRuntime {
    view: View,
    highlight_scan: bool,
    theme: ThemeName,
    language: Language,
    tile_size: u32,
    started: bool,
    tokens: Tokens,
    library: Library,
    scan_roots: Vec<ScanRoot>,
    scan_roots_error: Option<BackendError>,
    scan_roots_request: RequestSlot,
    startup_scan: StartupScanPhase,
    prefs_saving: bool,
    prefs_dirty: bool,
    prefs_error: Option<String>,
}

impl AppRuntime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            view: View::default(),
            highlight_scan: false,
            theme: config.theme,
            language: config.language,
            tile_size: DEFAULT_TILE_SIZE,
            started: false,
            tokens: Tokens::default(),
            library: Library::new(QueryConfig::default()),
            scan_roots: Vec::new(),
            scan_roots_error: None,
            scan_roots_request: RequestSlot::default(),
            startup_scan: StartupScanPhase::default(),
            prefs_saving: false,
            prefs_dirty: false,
            prefs_error: None,
        }
    }

    pub fn view(&self) -> View {
        self.view
    }

    /// Whether the settings page highlights its scan section (opened through "Add first
    /// folder").
    pub fn highlight_scan(&self) -> bool {
        self.highlight_scan
    }

    pub fn theme(&self) -> ThemeName {
        self.theme
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    /// The library's query model: the result's `total`, the cached rows by global
    /// index, the session and its load state.
    pub fn query(&self) -> &AssetQuery {
        self.library.query()
    }

    /// The library model: the canonical tags and details of assets, the known tags,
    /// and whether writes are pending.
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// The answers to ticketed reads of the query model since the last call
    /// ([`AssetQuery::take_events`]).
    pub fn take_query_events(&mut self) -> Vec<QueryEvent> {
        self.library.take_query_events()
    }

    /// The answers to ticketed library commands and the commits of writes since the
    /// last call ([`Library::take_events`]).
    pub fn take_library_events(&mut self) -> Vec<LibraryEvent> {
        self.library.take_events()
    }

    /// Whether the query's refresh is waiting for the backend.
    pub fn library_loading(&self) -> bool {
        self.library.query().query_pending()
    }

    /// The scan roots as the backend last listed them.
    pub fn scan_roots(&self) -> &[ScanRoot] {
        &self.scan_roots
    }

    pub fn scan_roots_error(&self) -> Option<&BackendError> {
        self.scan_roots_error.as_ref()
    }

    /// `hasScanRoots`: the gallery's empty state offers "Add first folder" without
    /// one.
    pub fn has_scan_roots(&self) -> bool {
        !self.scan_roots.is_empty()
    }

    pub fn startup_scan(&self) -> &StartupScanPhase {
        &self.startup_scan
    }

    /// `maintenanceLoading`: a maintenance operation runs (for now, the startup scan
    /// with its refreshes).
    pub fn maintenance_loading(&self) -> bool {
        self.startup_scan.is_running()
    }

    /// Why the last preference save failed; `None` after one that succeeded. A
    /// failed save never undoes the change of the running session.
    pub fn prefs_error(&self) -> Option<&str> {
        self.prefs_error.as_deref()
    }

    /// The preferences this runtime stores: the current theme and language.
    pub fn prefs(&self) -> Prefs {
        Prefs::default()
            .with_theme(self.theme)
            .with_language(self.language)
    }

    /// Hands `input` to the library model and wraps what it asks for.
    fn library_input(&mut self, input: LibraryInput) -> Vec<RuntimeEffect> {
        let effects = self
            .library
            .handle(input)
            .into_iter()
            .map(|effect| match effect {
                LibraryEffect::Query(effect) => RuntimeEffect::Query(effect),
                effect => RuntimeEffect::Library(effect),
            })
            .collect();
        // A reply may have been the library refresh the startup scan waits for.
        self.settle_startup_scan();
        effects
    }

    fn refresh_library(&mut self) -> Vec<RuntimeEffect> {
        self.library_input(LibraryInput::Refresh { ticket: None })
    }

    fn refresh_scan_roots(&mut self) -> RuntimeEffect {
        RuntimeEffect::ListScanRoots(self.scan_roots_request.begin(&mut self.tokens))
    }

    /// Saves the preferences, one save at a time: a change while a save runs is saved
    /// after it, with the values current then, so an older save never lands last.
    fn save_prefs(&mut self) -> Vec<RuntimeEffect> {
        if self.prefs_saving {
            self.prefs_dirty = true;
            return Vec::new();
        }
        self.prefs_saving = true;
        vec![RuntimeEffect::SavePrefs(self.prefs())]
    }

    /// Ends the startup scan once the refreshes it asked for have answered.
    fn settle_startup_scan(&mut self) {
        if !matches!(self.startup_scan, StartupScanPhase::Refreshing(_))
            || self.library.refreshing()
            || self.scan_roots_request.is_pending()
        {
            return;
        }
        if let StartupScanPhase::Refreshing(outcome) = std::mem::take(&mut self.startup_scan) {
            self.startup_scan = StartupScanPhase::Finished(outcome);
        }
    }
}

impl Model for AppRuntime {
    type Input = RuntimeInput;
    type Effect = RuntimeEffect;

    fn handle(&mut self, input: RuntimeInput) -> Vec<RuntimeEffect> {
        match input {
            RuntimeInput::Start => {
                if self.started {
                    return Vec::new();
                }
                self.started = true;
                self.startup_scan = StartupScanPhase::Scanning;
                let mut effects = self.refresh_library();
                effects.push(RuntimeEffect::ScanStartupRoots);
                effects.push(self.refresh_scan_roots());
                effects.extend(self.save_prefs());
                effects
            }
            RuntimeInput::OpenSettings => {
                self.view = View::Settings;
                Vec::new()
            }
            RuntimeInput::CloseSettings => {
                self.view = View::Gallery;
                self.highlight_scan = false;
                Vec::new()
            }
            RuntimeInput::OpenFirstFolderSettings => {
                self.highlight_scan = true;
                self.view = View::Settings;
                Vec::new()
            }
            RuntimeInput::SetTheme(theme) => {
                if theme == self.theme {
                    return Vec::new();
                }
                self.theme = theme;
                self.save_prefs()
            }
            RuntimeInput::SetLanguage(language) => {
                if language == self.language {
                    return Vec::new();
                }
                self.language = language;
                self.save_prefs()
            }
            RuntimeInput::SetTileSize(size) => {
                self.tile_size = clamp_tile_size(size, self.tile_size);
                Vec::new()
            }
            RuntimeInput::Zoom(delta_y) => {
                self.tile_size = next_tile_size_by_wheel(self.tile_size, delta_y);
                Vec::new()
            }
            RuntimeInput::RefreshLibrary => self.refresh_library(),
            RuntimeInput::Query(input) => self.library_input(LibraryInput::Query(input)),
            RuntimeInput::Library(input) => self.library_input(*input),
            RuntimeInput::RefreshScanRoots => vec![self.refresh_scan_roots()],
            RuntimeInput::ScanRootsLoaded { token, result } => {
                if !self.scan_roots_request.finish(token) {
                    return Vec::new();
                }
                match result {
                    Ok(roots) => {
                        self.scan_roots = roots;
                        self.scan_roots_error = None;
                    }
                    Err(error) => self.scan_roots_error = Some(error),
                }
                self.settle_startup_scan();
                Vec::new()
            }
            RuntimeInput::StartupScanFinished { result } => {
                if !matches!(self.startup_scan, StartupScanPhase::Scanning) {
                    return Vec::new();
                }
                let outcome = match result {
                    Ok(None) => {
                        self.startup_scan = StartupScanPhase::Finished(StartupScanOutcome::NoWork);
                        return Vec::new();
                    }
                    Ok(Some(summary)) => StartupScanOutcome::Scanned(summary),
                    Err(error) => StartupScanOutcome::Failed(error),
                };
                self.startup_scan = StartupScanPhase::Refreshing(outcome);
                let mut effects = self.refresh_library();
                effects.push(self.refresh_scan_roots());
                effects
            }
            RuntimeInput::PrefsSaved { result } => {
                self.prefs_saving = false;
                self.prefs_error = result.err();
                if std::mem::take(&mut self.prefs_dirty) {
                    self.save_prefs()
                } else {
                    Vec::new()
                }
            }
        }
    }
}

/// Runs the runtime's effects through the backend port and the preference store.
pub struct RuntimeRunner {
    backend: Arc<dyn Backend>,
    prefs: Arc<dyn PrefsStore>,
}

impl RuntimeRunner {
    pub fn new(backend: Arc<dyn Backend>, prefs: Arc<dyn PrefsStore>) -> Self {
        Self { backend, prefs }
    }
}

impl Runner<AppRuntime> for RuntimeRunner {
    fn run(&mut self, effect: RuntimeEffect, dispatch: &Dispatch<RuntimeInput>) {
        match effect {
            RuntimeEffect::Query(effect) => {
                run_query_effect(&self.backend, effect, dispatch, RuntimeInput::Query);
            }
            RuntimeEffect::Library(effect) => {
                run_library_effect(&self.backend, effect, dispatch, RuntimeInput::library);
            }
            RuntimeEffect::ListScanRoots(token) => {
                let backend = Arc::clone(&self.backend);
                dispatch.call(
                    "list_scan_roots",
                    move || backend.list_scan_roots(),
                    move |result| RuntimeInput::ScanRootsLoaded { token, result },
                );
            }
            RuntimeEffect::ScanStartupRoots => {
                let backend = Arc::clone(&self.backend);
                dispatch.call(
                    "scan_startup_roots",
                    move || backend.scan_startup_roots(),
                    |result| RuntimeInput::StartupScanFinished { result },
                );
            }
            RuntimeEffect::SavePrefs(prefs) => {
                let store = Arc::clone(&self.prefs);
                dispatch.call(
                    "save preferences",
                    move || Ok(store.save(&prefs).map_err(|error| error.to_string())),
                    |result: BackendResult<Result<(), String>>| RuntimeInput::PrefsSaved {
                        result: result.unwrap_or_else(|error| Err(error.text().to_owned())),
                    },
                );
            }
        }
    }
}
