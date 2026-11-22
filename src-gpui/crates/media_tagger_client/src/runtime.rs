//! The application runtime (`createAppRuntime` of the Solid frontend): the state that lives
//! as long as the application, whatever view is shown.
//!
//! Views come and go: switching between the gallery and the settings page drops the
//! view that is left and every bit of state local to it (scroll position, focus,
//! hover). What must survive lives here: the current view, the theme and the
//! language, the tile size, the library model ([`Library`]: the query with its
//! session, page cache and applied filters, the canonical tags, the details cache,
//! the known tags and the metadata writes), the bulk selection ([`Selection`]), the
//! startup sequence and what it learned (the scan roots, the outcome of the startup
//! scan). Later epics hang their models (lightbox, settings) next to these.
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
//! The search header's model ([`Search`]: the draft and the applied filters) and the
//! suggestion client ([`SuggestionClient`]) live here too, so a draft survives the
//! gallery view. A valid submit ([`RuntimeInput::Search`]) hands its filters to the
//! query ([`QueryInput::ApplyFilters`]) in the same turn. Suggestion searches are
//! [`RuntimeEffect::Suggest`], run on the executor with the index of the known tags
//! cached by the runner; a field reads its answers with
//! [`AppRuntime::take_suggestion_events_for`].
//!
//! The selection is embedded the same way: its inputs travel as
//! [`RuntimeInput::Selection`]. It reads the query model through ticketed reads,
//! which the runtime hands to the query in the same turn, and it is told the query's
//! epochs and session after every input the library took, before it gets the answers
//! to its reads; so a read that a refresh overtook is forgotten, not reported as a
//! failure. The answers to its tickets ([`Selection::owns`]) never show up in
//! [`AppRuntime::take_query_events`]. Its keepalive timer is the only selection
//! effect that reaches the runner ([`RuntimeEffect::SelectionKeepAlive`]).
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

use std::{sync::Arc, time::Duration};

use media_tagger_core::models::{ScanRoot, ScanSummary};

use crate::{
    backend::{Backend, BackendError, BackendResult},
    gallery::selection::{QuerySnapshot, Selection, SelectionEffect, SelectionInput},
    i18n::Language,
    library::{
        run_library_effect, run_query_effect, AssetQuery, Library, LibraryEffect, LibraryEvent,
        LibraryInput, QueryConfig, QueryEffect, QueryEvent, QueryInput,
    },
    model::{Dispatch, Model, RequestSlot, RequestToken, Runner, Tokens},
    prefs::{Prefs, PrefsStore},
    search::{
        run_suggestion_effect,
        suggestions::SuggestionIndexCache,
        tag_list::{TagList, TagListEffect, TagListInput},
        EditorId, Search, SearchCommand, SuggestionClient, SuggestionEffect, SuggestionEvent,
        SuggestionInput,
    },
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
    /// A command or the keepalive timer of the bulk selection. The query's state and
    /// the answers to the selection's reads are passed on by the runtime itself.
    Selection(SelectionInput),
    /// A command of the search header; a valid submit applies its filters to the
    /// query.
    Search(SearchCommand),
    /// A request or answer of the suggestion client.
    Suggestions(SuggestionInput),
    /// A command, page or timer of the tag list. Its Apply replaces the search
    /// field's text and submits it; the tag list hears back once the query that
    /// started has answered.
    TagList(TagListInput),
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
    /// A search of the suggestion client.
    Suggest(SuggestionEffect),
    /// A page or the click timer of the tag list; never its
    /// [`TagListEffect::ApplySearch`], which the runtime runs itself.
    TagList(TagListEffect),
    /// Send [`SelectionInput::KeepAliveDue`] with this token after `delay`.
    SelectionKeepAlive {
        token: RequestToken,
        delay: Duration,
    },
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
    selection: Selection,
    search: Search,
    suggestions: SuggestionClient,
    tag_list: TagList,
    /// The tag list's Apply waits for the query it started.
    tag_list_apply: bool,
    /// The answers to ticketed query reads of other callers than the selection.
    query_events: Vec<QueryEvent>,
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
        let library = Library::new(QueryConfig::default());
        let selection = Selection::new(query_snapshot(&library));
        Self {
            view: View::default(),
            highlight_scan: false,
            theme: config.theme,
            language: config.language,
            tile_size: DEFAULT_TILE_SIZE,
            started: false,
            tokens: Tokens::default(),
            library,
            selection,
            search: Search::new(),
            suggestions: SuggestionClient::new(),
            tag_list: TagList::new(),
            tag_list_apply: false,
            query_events: Vec::new(),
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

    /// The bulk selection of the gallery: selection mode, what is selected, and
    /// whether a range is being read.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// The search header: the draft, the applied filters and the validation error.
    pub fn search(&self) -> &Search {
        &self.search
    }

    /// The suggestion client shared by every suggesting field.
    pub fn suggestions(&self) -> &SuggestionClient {
        &self.suggestions
    }

    /// The tag list: whether it is open, its pages, the choices and whether Apply
    /// runs.
    pub fn tag_list(&self) -> &TagList {
        &self.tag_list
    }

    /// The answers of the suggestion client to `editor` since the last call.
    pub fn take_suggestion_events_for(&mut self, editor: EditorId) -> Vec<SuggestionEvent> {
        self.suggestions.take_events_for(editor)
    }

    /// Whether the gallery shows its load error banner (`selection.error ??
    /// library.query.loadError`): a range of the selection could not be read, or the
    /// query or one of its pages failed. Retry sends [`QueryInput::RetryLoad`] and
    /// then [`SelectionInput::Retry`].
    pub fn gallery_load_failed(&self) -> bool {
        self.selection.error().is_some() || self.library.query().load_error().is_some()
    }

    /// The answers to ticketed reads of the query model since the last call
    /// ([`AssetQuery::take_events`]), without the answers to the selection's own reads
    /// (tickets from `SELECTION_TICKET_BASE` up).
    pub fn take_query_events(&mut self) -> Vec<QueryEvent> {
        std::mem::take(&mut self.query_events)
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
    fn feed_library(&mut self, input: LibraryInput, effects: &mut Vec<RuntimeEffect>) {
        effects.extend(
            self.library
                .handle(input)
                .into_iter()
                .map(|effect| match effect {
                    LibraryEffect::Query(effect) => RuntimeEffect::Query(effect),
                    effect => RuntimeEffect::Library(effect),
                }),
        );
    }

    /// Runs what the selection asked for and brings it in line with the library: its
    /// reads go to the query model in this turn, it is told the query's state before
    /// it gets the answers that came with it, and whatever that makes it ask for is
    /// run as well.
    fn drive_selection(
        &mut self,
        mut pending: Vec<SelectionEffect>,
        effects: &mut Vec<RuntimeEffect>,
    ) {
        loop {
            for effect in std::mem::take(&mut pending) {
                let read = match effect {
                    SelectionEffect::ReadIdRange { ticket, start, end } => {
                        QueryInput::ReadIdRange { ticket, start, end }
                    }
                    SelectionEffect::KeepSessionAlive { ticket, session_id } => {
                        QueryInput::KeepSessionAlive { ticket, session_id }
                    }
                    SelectionEffect::KeepAliveTimer { token, delay } => {
                        effects.push(RuntimeEffect::SelectionKeepAlive { token, delay });
                        continue;
                    }
                };
                self.feed_library(LibraryInput::Query(read), effects);
            }
            if self.library.is_disposed() {
                self.selection.handle(SelectionInput::Dispose);
            }
            // The epochs first: an answer that a refresh overtook belongs to a read
            // the selection has forgotten by then.
            let snapshot = query_snapshot(&self.library);
            pending.extend(self.selection.handle(SelectionInput::Sync(snapshot)));
            for event in self.library.take_query_events() {
                let input = match event {
                    QueryEvent::IdRangeRead { ticket, result } if Selection::owns(ticket) => {
                        SelectionInput::IdRangeRead { ticket, result }
                    }
                    QueryEvent::SessionKeptAlive { ticket, result } if Selection::owns(ticket) => {
                        SelectionInput::SessionKeptAlive { ticket, result }
                    }
                    other => {
                        self.query_events.push(other);
                        continue;
                    }
                };
                pending.extend(self.selection.handle(input));
            }
            if pending.is_empty() {
                break;
            }
        }
    }

    /// Hands `input` to the library model, then to the selection what changed for it.
    fn library_input(&mut self, input: LibraryInput) -> Vec<RuntimeEffect> {
        let mut effects = Vec::new();
        self.feed_library(input, &mut effects);
        self.drive_selection(Vec::new(), &mut effects);
        // A reply may have been the library refresh the startup scan waits for.
        self.settle_startup_scan();
        self.sync_tag_list(&mut effects);
        effects
    }

    /// Hands `input` to the tag list and runs what it asks for: its Apply here, its
    /// pages and timer through the runner.
    fn feed_tag_list(&mut self, input: TagListInput, effects: &mut Vec<RuntimeEffect>) {
        for effect in self.tag_list.handle(input) {
            match effect {
                TagListEffect::ApplySearch(filter_input) => {
                    self.apply_tag_list_search(filter_input, effects);
                }
                effect => effects.push(RuntimeEffect::TagList(effect)),
            }
        }
    }

    /// `onApplySearch` of the tag list: `search.applyInput`. A valid input starts the
    /// query and the tag list waits for it; an invalid one is shown under the header
    /// and counts as applied, as the Solid submit returns without an error.
    fn apply_tag_list_search(&mut self, filter_input: String, effects: &mut Vec<RuntimeEffect>) {
        match self.search.handle(SearchCommand::ApplyInput(filter_input)) {
            Some(filters) => {
                self.tag_list_apply = true;
                effects.extend(
                    self.library_input(LibraryInput::Query(QueryInput::ApplyFilters(filters))),
                );
            }
            None => self.feed_tag_list(TagListInput::ApplySettled { ok: true }, effects),
        }
    }

    /// Gives the tag list the current known tags, and answers its Apply once the query
    /// is no longer pending: ok unless the query failed to start.
    fn sync_tag_list(&mut self, effects: &mut Vec<RuntimeEffect>) {
        let known_tags = self.library.known_tags().clone();
        self.feed_tag_list(TagListInput::SetVocabulary(known_tags), effects);
        if self.tag_list_apply && !self.library.query().query_pending() {
            self.tag_list_apply = false;
            let ok = self.library.query().load_error().is_none();
            self.feed_tag_list(TagListInput::ApplySettled { ok }, effects);
        }
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
            RuntimeInput::Selection(input) => {
                let pending = self.selection.handle(input);
                let mut effects = Vec::new();
                self.drive_selection(pending, &mut effects);
                self.settle_startup_scan();
                effects
            }
            RuntimeInput::Search(command) => match self.search.handle(command) {
                Some(filters) => {
                    self.library_input(LibraryInput::Query(QueryInput::ApplyFilters(filters)))
                }
                None => Vec::new(),
            },
            RuntimeInput::Suggestions(input) => self
                .suggestions
                .handle(input)
                .into_iter()
                .map(RuntimeEffect::Suggest)
                .collect(),
            RuntimeInput::TagList(input) => {
                let mut effects = Vec::new();
                let known_tags = self.library.known_tags().clone();
                self.feed_tag_list(TagListInput::SetVocabulary(known_tags), &mut effects);
                self.feed_tag_list(input, &mut effects);
                effects
            }
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

/// What the selection is told of the library: the query's epochs, session and total,
/// and the identity epoch of the tag state.
fn query_snapshot(library: &Library) -> QuerySnapshot {
    let query = library.query();
    QuerySnapshot {
        query_epoch: query.query_epoch(),
        identity_epoch: library.tags().epoch(),
        failure_epoch: query.page_failure_epoch(),
        session_id: query.session_id(),
        total: query.total(),
    }
}

/// Runs the runtime's effects through the backend port and the preference store.
pub struct RuntimeRunner {
    backend: Arc<dyn Backend>,
    prefs: Arc<dyn PrefsStore>,
    /// The search index of the known tags, built again only for a new vocabulary.
    suggestion_index: Arc<SuggestionIndexCache>,
}

impl RuntimeRunner {
    pub fn new(backend: Arc<dyn Backend>, prefs: Arc<dyn PrefsStore>) -> Self {
        Self {
            backend,
            prefs,
            suggestion_index: Arc::new(SuggestionIndexCache::new()),
        }
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
            RuntimeEffect::Suggest(effect) => {
                run_suggestion_effect(
                    &self.suggestion_index,
                    effect,
                    dispatch,
                    RuntimeInput::Suggestions,
                );
            }
            RuntimeEffect::TagList(effect) => match effect {
                TagListEffect::LoadPage {
                    token,
                    query,
                    offset,
                    limit,
                } => {
                    let backend = Arc::clone(&self.backend);
                    dispatch.call(
                        "list_tags",
                        move || backend.list_tags(query, offset, limit),
                        move |result| {
                            RuntimeInput::TagList(TagListInput::PageLoaded { token, result })
                        },
                    );
                }
                TagListEffect::ClickTimer { token, delay } => {
                    dispatch.after(
                        delay,
                        RuntimeInput::TagList(TagListInput::ClickTimerFired(token)),
                    );
                }
                // The runtime applies the search itself (`AppRuntime::feed_tag_list`).
                TagListEffect::ApplySearch(_) => {}
            },
            RuntimeEffect::SelectionKeepAlive { token, delay } => {
                dispatch.after(
                    delay,
                    RuntimeInput::Selection(SelectionInput::KeepAliveDue(token)),
                );
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
