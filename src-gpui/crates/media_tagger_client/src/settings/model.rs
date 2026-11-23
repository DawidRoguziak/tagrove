//! The settings model (`createSettings`, `createSettingsOperationRunner`,
//! `createScanSettings`, `createDangerZoneSettings`): the exclusive operation runner
//! and the operations of the scan, thumbnail and danger-zone sections.
//!
//! # The runner
//!
//! One operation runs at a time. A command that starts one while another runs is
//! refused in the same turn, without a backend call and without changing anything
//! ([`Settings::is_locked`] is the synchronous lock of `operationLock`). An operation
//! marks its section as loading with its pending line, and, for the long ones, the
//! runner subscribes to the progress stream before the call is made
//! ([`SettingsCall::progress`]); the section accepts only the operation's own phases
//! ([`PhaseFilter`]) and only while the operation runs. Success is decided from the
//! command's result alone, never from a progress phase. A failure shows the
//! operation's error prefix with the backend's text.
//!
//! An operation ends after the refreshes its result calls for (the library, the scan
//! roots, or both): only then is its summary shown and the lock released. After a
//! success a failed refresh becomes the operation's error, as an `await` that rejects
//! does in the Solid action; after a failure the refreshes run for what may already
//! have been committed, and the original error stays.
//!
//! The startup scan is an operation of the runner as well: the runtime begins it at
//! start ([`Settings::begin_startup`]) and hands it the scan's answer
//! ([`Settings::startup_scanned`]). It then ends as a manual rescan does, after the
//! refreshes its result calls for; until then every other operation is refused.
//!
//! # Embedding
//!
//! The runtime owns the model. Library refreshes and the mutation barrier of "clear
//! library" are [`SettingsEffect::Library`] inputs the runtime applies in the same
//! turn; their answers come back as [`SettingsInput::Library`] for the tickets of
//! this model's range ([`Settings::owns`]). The scan roots belong to the runtime:
//! the model hands it every canonical list it read ([`SettingsEffect::Roots`]). The
//! folder dialog is the adapter's: [`Settings::take_folder_picker_request`] asks for
//! it and [`SettingsInput::FoldersPicked`] answers.

use media_tagger_core::models::{ScanProgress, ScanRoot, ScanSummary, VideoToolStatus};

use super::{
    progress::{PhaseFilter, ThumbnailCounts},
    scan::{added_paths, normalize_folder_selection},
    status::{OperationSection, ProgressEvent, SectionStatus, StatusMessage},
};
use crate::{
    backend::BackendResult,
    i18n::Params,
    library::{LibraryEvent, LibraryInput, LibraryReset, RefreshError, Ticket},
    lightbox::LIGHTBOX_TICKET_BASE,
    model::Model,
};

/// The first ticket of the settings model's library commands. Its range ends where the
/// lightbox model's begins.
pub const SETTINGS_TICKET_BASE: u64 = 1 << 59;

/// Identifies one operation of the runner; replies and progress of another are
/// dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperationId(u64);

impl OperationId {
    pub fn value(self) -> u64 {
        self.0
    }
}

/// What the sections ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsCommand {
    /// "Choose folder": the folder dialog, then every chosen folder added.
    PickFolder,
    /// The startup-scan switch of a root.
    SetAutoScan {
        path: String,
        enabled: bool,
    },
    RescanRoot(String),
    RescanAll,
    /// "Remove" on a root: opens the confirmation.
    RequestRemoveRoot(String),
    CancelRemoveRoot,
    ConfirmRemoveRoot,
    RenderAllThumbnails,
    RenderFailedThumbnails,
    /// "Stop thumbnail render": outside the runner, for the render that runs.
    CancelThumbnailRender,
    OpenClearConfirm,
    CloseClearConfirm,
    /// "YES" in the clear confirmation.
    ConfirmClear,
    /// Reads whether ffmpeg and ffprobe were found, once.
    LoadVideoTools,
}

/// The folders "Choose folder" added: the roots before and after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderAddition {
    pub previous: Vec<String>,
    pub roots: Vec<ScanRoot>,
}

/// What removing a root removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedRoot {
    pub assets: usize,
    pub thumbnails: usize,
}

/// What clearing the library removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearedLibrary {
    pub assets: usize,
    pub roots: usize,
    pub thumbnails: usize,
}

/// The answer of an operation's backend call.
#[derive(Debug, Clone)]
pub enum SettingsReply {
    FoldersAdded(BackendResult<FolderAddition>),
    AutoScanSaved(BackendResult<()>),
    Scanned(BackendResult<ScanSummary>),
    RootRemoved(BackendResult<RemovedRoot>),
    Thumbnails(BackendResult<ThumbnailCounts>),
    Cleared(BackendResult<ClearedLibrary>),
    RootsListed(BackendResult<Vec<ScanRoot>>),
}

/// Whether ffmpeg and ffprobe were found (`VideoToolStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoTools {
    pub ffmpeg: bool,
    pub ffprobe: bool,
}

impl From<&VideoToolStatus> for VideoTools {
    fn from(status: &VideoToolStatus) -> Self {
        Self {
            ffmpeg: status.ffmpeg_available,
            ffprobe: status.ffprobe_available,
        }
    }
}

/// Commands, the dialog's answer, progress and backend replies.
#[derive(Debug)]
pub enum SettingsInput {
    Command(SettingsCommand),
    /// The folder dialog of `op` closed: the chosen paths, `None` when cancelled, or
    /// why it could not open.
    FoldersPicked {
        op: OperationId,
        result: Result<Option<Vec<String>>, String>,
    },
    /// A progress event that arrived while `op` was subscribed.
    Progress {
        op: OperationId,
        progress: ScanProgress,
    },
    Reply {
        op: OperationId,
        reply: SettingsReply,
    },
    /// `cancel_render_all_thumbnails` answered for the render `render`.
    ThumbnailCancelAnswered {
        render: OperationId,
        result: BackendResult<bool>,
    },
    VideoToolsLoaded(BackendResult<VideoTools>),
    /// An answer of the library to a command of this model.
    Library(LibraryEvent),
}

/// A backend call for the runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsCall {
    /// `list_scan_roots`, `add_scan_root` for each path, `list_scan_roots`;
    /// answered with [`SettingsReply::FoldersAdded`].
    AddFolders {
        op: OperationId,
        paths: Vec<String>,
    },
    SetAutoScan {
        op: OperationId,
        path: String,
        enabled: bool,
    },
    /// `scan_folder(path)`, or `rescan_all_roots` without one.
    Rescan {
        op: OperationId,
        path: Option<String>,
    },
    RemoveRoot {
        op: OperationId,
        path: String,
    },
    /// `render_all_thumbnails`, or `render_failed_thumbnails`.
    RenderThumbnails {
        op: OperationId,
        failed_only: bool,
    },
    ClearLibrary {
        op: OperationId,
    },
    ListRoots {
        op: OperationId,
    },
    CancelThumbnailRender {
        render: OperationId,
    },
    LoadVideoTools,
}

impl SettingsCall {
    /// The operation whose progress the runner subscribes to before the call.
    pub fn progress(&self) -> Option<OperationId> {
        match self {
            Self::Rescan { op, .. }
            | Self::RenderThumbnails { op, .. }
            | Self::ClearLibrary { op } => Some(*op),
            _ => None,
        }
    }
}

/// What the model asks of the runtime.
#[derive(Debug)]
pub enum SettingsEffect {
    /// A backend call for the runner.
    Call(SettingsCall),
    /// A command for the library, applied in the same turn. Boxed: a library input is
    /// several times the size of every other effect.
    Library(Box<LibraryInput>),
    /// The scan roots as the backend listed them.
    Roots(Vec<ScanRoot>),
    /// The startup-scan preference of `path` was saved.
    RootAutoScanSaved { path: String, enabled: bool },
}

/// The folder dialog an operation waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderPickerRequest {
    pub op: OperationId,
    /// The dialog's title (`settings.actions.dialogs.selectMediaFolder`).
    pub title_key: &'static str,
}

#[derive(Debug)]
enum Kind {
    Startup,
    PickFolder { selected: Vec<String> },
    AutoScan { path: String, enabled: bool },
    RemoveRoot,
    Rescan,
    Thumbnails { label_key: &'static str },
    Clear { ticket: Ticket },
}

#[derive(Debug)]
enum Stage {
    /// Waiting for the runtime's answer of the startup scan.
    External,
    /// Waiting for the folder dialog.
    Dialog,
    /// Waiting for the mutation barrier.
    Barrier,
    /// Waiting for the backend call.
    Call,
    /// Waiting for the refreshes the result called for.
    Settling(Settling),
}

#[derive(Debug)]
struct Settling {
    library: Option<Ticket>,
    roots: bool,
    /// A failed refresh becomes the operation's error (after a success).
    strict: bool,
    refresh_error: Option<String>,
    outcome: StatusMessage,
}

#[derive(Debug)]
struct Active {
    op: OperationId,
    kind: Kind,
    section: OperationSection,
    global: bool,
    filter: Option<PhaseFilter>,
    error_prefix: &'static str,
    stage: Stage,
}

/// How an operation starts.
struct Start {
    kind: Kind,
    section: OperationSection,
    pending: StatusMessage,
    error_prefix: &'static str,
    filter: Option<PhaseFilter>,
    global: bool,
}

/// The settings model.
#[derive(Debug)]
pub struct Settings {
    statuses: [SectionStatus; 4],
    active: Option<Active>,
    last_op: u64,
    last_ticket: u64,
    thumbnail_render: Option<OperationId>,
    cancel_pending: Option<OperationId>,
    remove_root_confirm: Option<String>,
    clear_confirm_open: bool,
    picker_request: Option<FolderPickerRequest>,
    video_tools: Option<VideoTools>,
    video_tools_requested: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}

fn refresh_error_text(error: &RefreshError) -> String {
    match error {
        RefreshError::Query(error) => error.to_string(),
        RefreshError::Vocabulary(error) => error.text().to_owned(),
    }
}

fn count_params(pairs: &[(&str, usize)]) -> Params {
    pairs.iter().fold(Params::new(), |params, (name, value)| {
        params.with(*name, value)
    })
}

impl Settings {
    pub fn new() -> Self {
        Self {
            statuses: OperationSection::ALL.map(SectionStatus::ready),
            active: None,
            last_op: 0,
            last_ticket: 0,
            thumbnail_render: None,
            cancel_pending: None,
            remove_root_confirm: None,
            clear_confirm_open: false,
            picker_request: None,
            video_tools: None,
            video_tools_requested: false,
        }
    }

    /// Whether a ticket of the library belongs to this model.
    pub fn owns(ticket: Ticket) -> bool {
        (SETTINGS_TICKET_BASE..LIGHTBOX_TICKET_BASE).contains(&ticket.0)
    }

    /// `isOperationLocked`: an operation runs; every other one is refused.
    pub fn is_locked(&self) -> bool {
        self.active.is_some()
    }

    /// `setLoading` of the runner: an operation that counts as maintenance runs
    /// (every one except choosing a folder and saving a startup-scan switch).
    pub fn global_loading(&self) -> bool {
        self.active.as_ref().is_some_and(|active| active.global)
    }

    /// The operation that runs.
    pub fn active_operation(&self) -> Option<OperationId> {
        self.active.as_ref().map(|active| active.op)
    }

    /// The status of a section: loading, its line and its progress.
    pub fn status(&self, section: OperationSection) -> &SectionStatus {
        &self.statuses[section.index()]
    }

    /// `thumbnailBulkRunning`: a bulk render runs; its Stop button is shown.
    pub fn thumbnail_render_running(&self) -> bool {
        self.thumbnail_render.is_some()
    }

    /// `cancelThumbnailRunning`: a stop request waits for its answer.
    pub fn cancel_thumbnail_pending(&self) -> bool {
        self.cancel_pending.is_some()
    }

    /// The root the remove confirmation asks about.
    pub fn remove_root_confirm(&self) -> Option<&str> {
        self.remove_root_confirm.as_deref()
    }

    pub fn clear_confirm_open(&self) -> bool {
        self.clear_confirm_open
    }

    /// The video tools, once read; a failed read shows both missing.
    pub fn video_tools(&self) -> Option<VideoTools> {
        self.video_tools
    }

    /// The folder dialog to open, once.
    pub fn take_folder_picker_request(&mut self) -> Option<FolderPickerRequest> {
        self.picker_request.take()
    }

    /// The folder dialog an operation waits for, without taking it.
    pub fn folder_picker_request(&self) -> Option<FolderPickerRequest> {
        self.picker_request
    }

    /// Begins the startup scan as an operation of the scan section. `None` when
    /// another operation runs.
    pub fn begin_startup(&mut self) -> Option<OperationId> {
        let op = self.begin(Start {
            kind: Kind::Startup,
            section: OperationSection::Scan,
            pending: StatusMessage::key("settings.scan.startupPending"),
            error_prefix: "settings.actions.errorPrefix.rescan",
            filter: Some(PhaseFilter::Scan),
            global: true,
        })?;
        self.stage(Stage::External);
        Some(op)
    }

    /// Whether the startup scan is the operation that runs: its scan, or the refreshes
    /// after it.
    pub fn startup_running(&self) -> bool {
        matches!(
            self.active,
            Some(Active {
                kind: Kind::Startup,
                ..
            })
        )
    }

    /// `scan_startup_roots` answered (`scanOnStartup` of `createScanSettings`). A
    /// launch without work ends at once with an empty line and refreshes nothing. A
    /// scan that found work ends with its summary once the library and the roots are
    /// read again, and a refresh that fails then is the operation's error; a failed
    /// scan refreshes both as well, for the batches it committed, and keeps its own
    /// error whatever the refreshes answer.
    pub fn startup_scanned(
        &mut self,
        result: &BackendResult<Option<ScanSummary>>,
    ) -> Vec<SettingsEffect> {
        let waiting = matches!(
            self.active,
            Some(Active {
                kind: Kind::Startup,
                stage: Stage::External,
                ..
            })
        );
        if !waiting {
            return Vec::new();
        }
        match result {
            Ok(None) => {
                self.finish(StatusMessage::Text(String::new()));
                Vec::new()
            }
            Ok(Some(summary)) => {
                self.settle(true, true, true, StatusMessage::scan_summary(summary))
            }
            Err(error) => {
                let message = self.failure(error.text());
                self.settle(true, true, false, message)
            }
        }
    }

    fn next_ticket(&mut self) -> Ticket {
        self.last_ticket += 1;
        Ticket(SETTINGS_TICKET_BASE + self.last_ticket)
    }

    fn begin(&mut self, start: Start) -> Option<OperationId> {
        if self.active.is_some() {
            return None;
        }
        self.last_op += 1;
        let op = OperationId(self.last_op);
        self.statuses[start.section.index()] = SectionStatus {
            loading: true,
            message: start.pending,
            progress: None,
        };
        self.active = Some(Active {
            op,
            kind: start.kind,
            section: start.section,
            global: start.global,
            filter: start.filter,
            error_prefix: start.error_prefix,
            stage: Stage::Call,
        });
        Some(op)
    }

    fn stage(&mut self, stage: Stage) {
        if let Some(active) = self.active.as_mut() {
            active.stage = stage;
        }
    }

    /// `setSectionMessage`: a new line, without progress.
    fn set_message(&mut self, section: OperationSection, message: StatusMessage) {
        let status = &mut self.statuses[section.index()];
        status.message = message;
        status.progress = None;
    }

    /// Ends the operation with `message` and releases the lock.
    fn finish(&mut self, message: StatusMessage) {
        let Some(active) = self.active.take() else {
            return;
        };
        if matches!(active.kind, Kind::Thumbnails { .. }) {
            self.thumbnail_render = None;
            self.cancel_pending = None;
        }
        let status = &mut self.statuses[active.section.index()];
        status.message = message;
        status.progress = None;
        status.loading = false;
    }

    /// Ends the operation with its error prefix and `detail`.
    fn fail(&mut self, detail: impl Into<String>) {
        let Some(prefix) = self.active.as_ref().map(|active| active.error_prefix) else {
            return;
        };
        self.finish(StatusMessage::failed(prefix, detail));
    }

    /// Asks for the refreshes a result calls for and ends the operation once they
    /// answered.
    fn settle(
        &mut self,
        library: bool,
        roots: bool,
        strict: bool,
        outcome: StatusMessage,
    ) -> Vec<SettingsEffect> {
        let Some(op) = self.active_operation() else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        let library = library.then(|| {
            let ticket = self.next_ticket();
            effects.push(SettingsEffect::Library(Box::new(LibraryInput::Refresh {
                ticket: Some(ticket),
            })));
            ticket
        });
        if roots {
            effects.push(SettingsEffect::Call(SettingsCall::ListRoots { op }));
        }
        self.stage(Stage::Settling(Settling {
            library,
            roots,
            strict,
            refresh_error: None,
            outcome,
        }));
        self.try_settle();
        effects
    }

    fn try_settle(&mut self) {
        let Some(Active {
            stage: Stage::Settling(settling),
            error_prefix,
            ..
        }) = self.active.as_mut()
        else {
            return;
        };
        if settling.library.is_some() || settling.roots {
            return;
        }
        let message = match (settling.strict, settling.refresh_error.take()) {
            (true, Some(error)) => StatusMessage::failed(error_prefix, error),
            _ => std::mem::replace(&mut settling.outcome, StatusMessage::Text(String::new())),
        };
        self.finish(message);
    }

    fn settling(&mut self) -> Option<&mut Settling> {
        match self.active.as_mut() {
            Some(Active {
                stage: Stage::Settling(settling),
                ..
            }) => Some(settling),
            _ => None,
        }
    }

    fn command(&mut self, command: SettingsCommand) -> Vec<SettingsEffect> {
        match command {
            SettingsCommand::PickFolder => {
                let Some(op) = self.begin(Start {
                    kind: Kind::PickFolder {
                        selected: Vec::new(),
                    },
                    section: OperationSection::Scan,
                    pending: StatusMessage::key("settings.actions.pending.waitFolderSelection"),
                    error_prefix: "settings.actions.errorPrefix.addFolder",
                    filter: None,
                    global: false,
                }) else {
                    return Vec::new();
                };
                self.stage(Stage::Dialog);
                self.picker_request = Some(FolderPickerRequest {
                    op,
                    title_key: "settings.actions.dialogs.selectMediaFolder",
                });
                Vec::new()
            }
            SettingsCommand::SetAutoScan { path, enabled } => {
                let Some(op) = self.begin(Start {
                    kind: Kind::AutoScan {
                        path: path.clone(),
                        enabled,
                    },
                    section: OperationSection::Scan,
                    pending: StatusMessage::key("settings.scan.savingAutoScan"),
                    error_prefix: "settings.scan.saveAutoScanError",
                    filter: None,
                    global: false,
                }) else {
                    return Vec::new();
                };
                vec![SettingsEffect::Call(SettingsCall::SetAutoScan {
                    op,
                    path,
                    enabled,
                })]
            }
            SettingsCommand::RescanRoot(path) => self.rescan(Some(path)),
            SettingsCommand::RescanAll => self.rescan(None),
            SettingsCommand::RequestRemoveRoot(path) => {
                if !self.is_locked() {
                    self.remove_root_confirm = Some(path);
                }
                Vec::new()
            }
            SettingsCommand::CancelRemoveRoot => {
                if !self.is_locked() {
                    self.remove_root_confirm = None;
                }
                Vec::new()
            }
            SettingsCommand::ConfirmRemoveRoot => {
                if self.is_locked() {
                    return Vec::new();
                }
                let Some(path) = self.remove_root_confirm.take() else {
                    return Vec::new();
                };
                let Some(op) = self.begin(Start {
                    kind: Kind::RemoveRoot,
                    section: OperationSection::Scan,
                    pending: StatusMessage::key_with(
                        "settings.actions.pending.removeFolder",
                        Params::new().with("path", &path),
                    ),
                    error_prefix: "settings.actions.errorPrefix.removeFolder",
                    filter: None,
                    global: true,
                }) else {
                    return Vec::new();
                };
                vec![SettingsEffect::Call(SettingsCall::RemoveRoot { op, path })]
            }
            SettingsCommand::RenderAllThumbnails => self.render_thumbnails(false),
            SettingsCommand::RenderFailedThumbnails => self.render_thumbnails(true),
            SettingsCommand::CancelThumbnailRender => {
                let Some(render) = self.thumbnail_render else {
                    return Vec::new();
                };
                if self.cancel_pending.is_some() {
                    return Vec::new();
                }
                self.cancel_pending = Some(render);
                vec![SettingsEffect::Call(SettingsCall::CancelThumbnailRender {
                    render,
                })]
            }
            SettingsCommand::OpenClearConfirm => {
                self.clear_confirm_open = true;
                Vec::new()
            }
            SettingsCommand::CloseClearConfirm => {
                self.clear_confirm_open = false;
                Vec::new()
            }
            SettingsCommand::ConfirmClear => {
                self.clear_confirm_open = false;
                let ticket = Ticket(SETTINGS_TICKET_BASE + self.last_ticket + 1);
                if self
                    .begin(Start {
                        kind: Kind::Clear { ticket },
                        section: OperationSection::Danger,
                        pending: StatusMessage::key("settings.actions.pending.clearLibrary"),
                        error_prefix: "settings.actions.errorPrefix.clearLibrary",
                        filter: Some(PhaseFilter::LibraryClear),
                        global: true,
                    })
                    .is_none()
                {
                    return Vec::new();
                }
                self.last_ticket += 1;
                self.stage(Stage::Barrier);
                vec![SettingsEffect::Library(Box::new(
                    LibraryInput::BeginMaintenance { ticket },
                ))]
            }
            SettingsCommand::LoadVideoTools => {
                if std::mem::replace(&mut self.video_tools_requested, true) {
                    return Vec::new();
                }
                vec![SettingsEffect::Call(SettingsCall::LoadVideoTools)]
            }
        }
    }

    fn rescan(&mut self, path: Option<String>) -> Vec<SettingsEffect> {
        let (pending, error_prefix) = match &path {
            Some(path) => (
                StatusMessage::key_with(
                    "settings.actions.pending.rescanFolder",
                    Params::new().with("path", path),
                ),
                "settings.actions.errorPrefix.rescanFolder",
            ),
            None => (
                StatusMessage::key("settings.actions.pending.rescanAll"),
                "settings.actions.errorPrefix.rescan",
            ),
        };
        let Some(op) = self.begin(Start {
            kind: Kind::Rescan,
            section: OperationSection::Scan,
            pending,
            error_prefix,
            filter: Some(PhaseFilter::Scan),
            global: true,
        }) else {
            return Vec::new();
        };
        vec![SettingsEffect::Call(SettingsCall::Rescan { op, path })]
    }

    fn render_thumbnails(&mut self, failed_only: bool) -> Vec<SettingsEffect> {
        let (pending, error_prefix, label_key) = if failed_only {
            (
                "settings.actions.pending.retryFailedThumbs",
                "settings.actions.errorPrefix.retryThumbs",
                "settings.actions.summary.failedRetryLabel",
            )
        } else {
            (
                "settings.actions.pending.renderAllThumbs",
                "settings.actions.errorPrefix.renderThumbs",
                "settings.actions.summary.allThumbnailsLabel",
            )
        };
        let Some(op) = self.begin(Start {
            kind: Kind::Thumbnails { label_key },
            section: OperationSection::Scan,
            pending: StatusMessage::key(pending),
            error_prefix,
            filter: Some(PhaseFilter::Thumbnails),
            global: true,
        }) else {
            return Vec::new();
        };
        self.thumbnail_render = Some(op);
        vec![SettingsEffect::Call(SettingsCall::RenderThumbnails {
            op,
            failed_only,
        })]
    }

    fn folders_picked(
        &mut self,
        op: OperationId,
        result: Result<Option<Vec<String>>, String>,
    ) -> Vec<SettingsEffect> {
        let waiting = matches!(
            &self.active,
            Some(Active { op: active, stage: Stage::Dialog, .. }) if *active == op
        );
        if !waiting {
            return Vec::new();
        }
        if self.picker_request.is_some_and(|request| request.op == op) {
            self.picker_request = None;
        }
        let selection = match result {
            Err(error) => {
                self.fail(error);
                return Vec::new();
            }
            Ok(selection) => normalize_folder_selection(selection),
        };
        let Some(paths) = selection else {
            self.finish(StatusMessage::key(
                "settings.actions.summary.folderSelectionCancelled",
            ));
            return Vec::new();
        };
        if let Some(Active {
            kind: Kind::PickFolder { selected },
            ..
        }) = self.active.as_mut()
        {
            selected.clone_from(&paths);
        }
        self.stage(Stage::Call);
        vec![SettingsEffect::Call(SettingsCall::AddFolders { op, paths })]
    }

    fn reply(&mut self, op: OperationId, reply: SettingsReply) -> Vec<SettingsEffect> {
        if self.active_operation() != Some(op) {
            return Vec::new();
        }
        if let SettingsReply::RootsListed(result) = reply {
            let Some(settling) = self.settling().filter(|settling| settling.roots) else {
                return Vec::new();
            };
            settling.roots = false;
            let effects = match result {
                Ok(roots) => vec![SettingsEffect::Roots(roots)],
                Err(error) => {
                    settling
                        .refresh_error
                        .get_or_insert(error.text().to_owned());
                    Vec::new()
                }
            };
            self.try_settle();
            return effects;
        }
        if !matches!(
            self.active.as_ref().map(|active| &active.stage),
            Some(Stage::Call)
        ) {
            return Vec::new();
        }
        match reply {
            SettingsReply::FoldersAdded(result) => match result {
                Ok(addition) => {
                    let selected = match self.active.as_ref().map(|active| &active.kind) {
                        Some(Kind::PickFolder { selected }) => selected.clone(),
                        _ => Vec::new(),
                    };
                    let added = added_paths(&addition.previous, &addition.roots);
                    let skipped = selected.len().saturating_sub(added.len());
                    self.finish(StatusMessage::FoldersAdded {
                        selected,
                        added,
                        skipped,
                    });
                    vec![SettingsEffect::Roots(addition.roots)]
                }
                Err(error) => {
                    self.fail(error.text());
                    Vec::new()
                }
            },
            SettingsReply::AutoScanSaved(result) => match result {
                Ok(()) => {
                    let Some(Kind::AutoScan { path, enabled }) =
                        self.active.as_ref().map(|active| &active.kind)
                    else {
                        return Vec::new();
                    };
                    let effect = SettingsEffect::RootAutoScanSaved {
                        path: path.clone(),
                        enabled: *enabled,
                    };
                    self.finish(StatusMessage::key("settings.scan.autoScanSaved"));
                    vec![effect]
                }
                Err(error) => {
                    // The switch shows the saved value again once the canonical
                    // roots are read.
                    let prefix = "settings.scan.saveAutoScanError";
                    self.settle(
                        false,
                        true,
                        false,
                        StatusMessage::failed(prefix, error.text()),
                    )
                }
            },
            SettingsReply::Scanned(result) => match result {
                Ok(summary) => self.settle(true, true, true, StatusMessage::scan_summary(&summary)),
                Err(error) => {
                    // Batches committed before the failure already changed the library.
                    let message = self.failure(error.text());
                    self.settle(true, true, false, message)
                }
            },
            SettingsReply::RootRemoved(result) => match result {
                Ok(removed) => self.settle(
                    true,
                    true,
                    true,
                    StatusMessage::key_with(
                        "settings.actions.summary.removedRoot",
                        count_params(&[
                            ("assets", removed.assets),
                            ("thumbnails", removed.thumbnails),
                        ]),
                    ),
                ),
                Err(error) => {
                    self.fail(error.text());
                    Vec::new()
                }
            },
            SettingsReply::Thumbnails(result) => match result {
                Ok(counts) => {
                    let label_key = match self.active.as_ref().map(|active| &active.kind) {
                        Some(Kind::Thumbnails { label_key }) => *label_key,
                        _ => "settings.actions.summary.allThumbnailsLabel",
                    };
                    self.settle(
                        true,
                        false,
                        true,
                        StatusMessage::ThumbnailSummary { label_key, counts },
                    )
                }
                Err(error) => {
                    self.fail(error.text());
                    Vec::new()
                }
            },
            SettingsReply::Cleared(result) => {
                let Some(Kind::Clear { ticket }) = self.active.as_ref().map(|active| &active.kind)
                else {
                    return Vec::new();
                };
                // The library is reset before the barrier comes down, whatever the
                // result: a failed clear may have committed part of its work.
                let mut effects = vec![SettingsEffect::Library(Box::new(
                    LibraryInput::EndMaintenance {
                        ticket: *ticket,
                        reset: Some(LibraryReset::Cleared),
                    },
                ))];
                effects.extend(match result {
                    Ok(cleared) => self.settle(
                        false,
                        true,
                        true,
                        StatusMessage::key_with(
                            "settings.actions.summary.libraryCleared",
                            count_params(&[
                                ("assets", cleared.assets),
                                ("roots", cleared.roots),
                                ("thumbnails", cleared.thumbnails),
                            ]),
                        ),
                    ),
                    Err(error) => {
                        let message = self.failure(error.text());
                        self.settle(true, true, false, message)
                    }
                });
                effects
            }
            SettingsReply::RootsListed(_) => Vec::new(),
        }
    }

    /// The error line of the running operation.
    fn failure(&self, detail: &str) -> StatusMessage {
        let prefix = self
            .active
            .as_ref()
            .map_or("settings.actions.errorPrefix.rescan", |active| {
                active.error_prefix
            });
        StatusMessage::failed(prefix, detail)
    }

    fn library_event(&mut self, event: LibraryEvent) -> Vec<SettingsEffect> {
        let clear_ticket = match self.active.as_ref() {
            Some(Active {
                kind: Kind::Clear { ticket },
                stage: Stage::Barrier,
                ..
            }) => Some(*ticket),
            _ => None,
        };
        match event {
            LibraryEvent::MaintenanceReady { ticket } if clear_ticket == Some(ticket) => {
                let Some(op) = self.active_operation() else {
                    return Vec::new();
                };
                self.stage(Stage::Call);
                vec![SettingsEffect::Call(SettingsCall::ClearLibrary { op })]
            }
            LibraryEvent::MaintenanceRejected { ticket, error } if clear_ticket == Some(ticket) => {
                let message = self.failure(&error.to_string());
                self.settle(true, true, false, message)
            }
            LibraryEvent::Refreshed { ticket, result } => {
                let Some(settling) = self
                    .settling()
                    .filter(|settling| settling.library == Some(ticket))
                else {
                    return Vec::new();
                };
                settling.library = None;
                if let Err(error) = result {
                    settling
                        .refresh_error
                        .get_or_insert_with(|| refresh_error_text(&error));
                }
                self.try_settle();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

impl Model for Settings {
    type Input = SettingsInput;
    type Effect = SettingsEffect;

    fn handle(&mut self, input: SettingsInput) -> Vec<SettingsEffect> {
        match input {
            SettingsInput::Command(command) => self.command(command),
            SettingsInput::FoldersPicked { op, result } => self.folders_picked(op, result),
            SettingsInput::Progress { op, progress } => {
                if let Some(active) = self.active.as_ref().filter(|active| active.op == op) {
                    if active
                        .filter
                        .is_some_and(|filter| filter.accepts(&progress.phase))
                    {
                        let status = &mut self.statuses[active.section.index()];
                        let progress = ProgressEvent(progress);
                        status.message = StatusMessage::Progress(progress.clone());
                        status.progress = Some(progress);
                    }
                }
                Vec::new()
            }
            SettingsInput::Reply { op, reply } => self.reply(op, reply),
            SettingsInput::ThumbnailCancelAnswered { render, result } => {
                if self.thumbnail_render == Some(render) {
                    let message = match result {
                        Ok(true) => StatusMessage::key("settings.actions.summary.stopRequested"),
                        Ok(false) => {
                            StatusMessage::key("settings.actions.summary.noThumbRenderRunning")
                        }
                        Err(error) => StatusMessage::Failed {
                            prefix: "settings.actions.errorPrefix.cancelThumbRender",
                            detail: error.text().to_owned(),
                            raw: true,
                        },
                    };
                    self.set_message(OperationSection::Scan, message);
                }
                if self.cancel_pending == Some(render) {
                    self.cancel_pending = None;
                }
                Vec::new()
            }
            SettingsInput::VideoToolsLoaded(result) => {
                self.video_tools = Some(result.unwrap_or(VideoTools {
                    ffmpeg: false,
                    ffprobe: false,
                }));
                Vec::new()
            }
            SettingsInput::Library(event) => self.library_event(event),
        }
    }
}
