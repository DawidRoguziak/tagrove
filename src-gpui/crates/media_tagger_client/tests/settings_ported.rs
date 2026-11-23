//! The settings page's TypeScript specification (`src/features/settings/__tests__`,
//! epic E17) ported to the settings model inside the application runtime: the
//! exclusive runner (`operationSafety.test.tsx`), the startup scan and the manual scans
//! (`startupScan.test.tsx`), the status lines and their language
//! (`statusLanguage.test.tsx`), thumbnail Stop (`thumbnailStop.test.tsx`), the actions
//! (`createSettingsActions.test.tsx`) and the pure helpers (`progressService.test.ts`,
//! `operationStateService.test.ts`, `SectionOperationStatus.test.tsx`,
//! `pluralCopy.test.tsx`). The scripted backend runs on the deterministic executor;
//! "clear library" also runs against the real core on a fresh temporary profile.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use media_tagger_client::{
    backend::{
        core_backend::test_support::TempCore,
        fake::{sequence, Call},
        Backend, BackendError, FakeBackend,
    },
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    library::{LibraryInput, Mutation, Ticket},
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner, StartupScanOutcome},
    settings::{
        progress::{
            is_duplicate_scan_progress_phase, is_library_clear_progress_phase,
            is_scan_progress_phase, is_thumbnail_progress_phase, progress_label, thumbnail_summary,
            ThumbnailCounts,
        },
        scan::video_tool_line,
        status::progress_bar,
        OperationSection, SectionStatus, SettingsCommand, SettingsInput, StatusMessage,
    },
    theme::ThemeName,
};
use media_tagger_core::models::{
    ClearLibrarySummary, RemoveRootSummary, ScanCompletion, ScanProgress, ScanRoot, ScanSummary,
    StartAssetQueryResult, TagListPage, ThumbnailRenderSummary, VideoToolStatus,
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

fn root(path: &str, auto: bool) -> ScanRoot {
    ScanRoot {
        path: path.to_owned(),
        auto_scan_on_startup: auto,
    }
}

fn summary(indexed: usize, removed: usize) -> ScanSummary {
    ScanSummary {
        completion: ScanCompletion::Complete,
        indexed,
        removed,
        failed: 0,
    }
}

fn render_summary(processed: usize, total: usize, cancelled: bool) -> ThumbnailRenderSummary {
    ThumbnailRenderSummary {
        stale: 0,
        generated: processed,
        failed: 0,
        skipped_failed: 0,
        processed,
        total,
        cancelled,
    }
}

fn progress(phase: &str, processed: usize, total: usize, message: &str) -> ScanProgress {
    ScanProgress {
        phase: phase.into(),
        processed,
        total,
        message: message.into(),
    }
}

fn empty_query() -> StartAssetQueryResult {
    StartAssetQueryResult::Ready {
        session_id: 1,
        revision: 1,
        total: 0,
        offset: 0,
        items: Vec::new(),
    }
}

impl Fixture {
    /// A runtime over the scripted backend, not started yet: one root without the
    /// startup-scan switch and a launch without startup work, unless `script` says
    /// otherwise.
    fn new(script: impl FnOnce(&FakeBackend)) -> Self {
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(|_| Ok(empty_query()))
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: Vec::new(),
                    total: 0,
                })
            })
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| Ok(vec![root("/synthetic/media", false)]))
            .on_get_video_tool_status(|| {
                Ok(VideoToolStatus {
                    ffmpeg_available: true,
                    ffprobe_available: true,
                })
            });
        script(&backend);
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            }),
            RuntimeRunner::new(backend.clone(), Arc::new(MemoryPrefsStore::default())),
            executor.clone() as Arc<dyn Executor>,
        );
        Self {
            backend,
            executor,
            driver,
        }
    }

    /// Started and settled.
    fn started() -> Self {
        let mut fx = Self::new(|_| {});
        fx.start();
        fx
    }

    fn start(&mut self) {
        self.send(RuntimeInput::Start);
        self.settle();
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn command(&mut self, command: SettingsCommand) {
        self.send(RuntimeInput::settings(command));
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    /// Runs the oldest waiting backend call and delivers what it sent.
    fn answer_next(&mut self) {
        assert!(self.executor.run_next(), "a call waits");
        while self.driver.pump() > 0 {}
    }

    /// Runs the waiting call at `index` and delivers what it sent.
    fn answer_nth(&mut self, index: usize) {
        assert!(self.executor.run_nth(index), "call {index} waits");
        while self.driver.pump() > 0 {}
    }

    /// Answers every waiting call but the oldest one, until only that one is left.
    fn settle_all_but_oldest(&mut self) {
        while self.executor.pending() > 1 {
            self.answer_nth(1);
        }
    }

    fn status(&self, section: OperationSection) -> SectionStatus {
        self.runtime().settings().status(section).clone()
    }

    fn scan_progress(&self) -> Option<usize> {
        self.status(OperationSection::Scan)
            .progress
            .map(|progress| progress.processed)
    }

    fn line_in(&self, section: OperationSection, language: Language) -> String {
        self.runtime()
            .settings()
            .status(section)
            .message
            .resolve(&I18n::new(language))
    }

    fn line(&self, section: OperationSection) -> String {
        self.line_in(section, Language::En)
    }

    fn scan_line(&self) -> String {
        self.line(OperationSection::Scan)
    }

    fn locked(&self) -> bool {
        self.runtime().settings().is_locked()
    }

    fn calls(&self, method: &str) -> usize {
        self.backend
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }

    fn progress_for_active(&mut self, event: ScanProgress) {
        let op = self
            .runtime()
            .settings()
            .active_operation()
            .expect("an operation runs");
        self.send(RuntimeInput::Settings(SettingsInput::Progress {
            op,
            progress: event,
        }));
    }
}

// progressService.test.ts, operationStateService.test.ts,
// SectionOperationStatus.test.tsx, pluralCopy.test.tsx ("localized video tool status")

/// progressService.test.ts "formats thumbnail summary for completed render" and "... for
/// cancelled render".
#[test]
fn a_thumbnail_summary_reads_as_a_completed_or_a_cancelled_render() {
    let en = I18n::new(Language::En);
    let completed = ThumbnailCounts {
        generated: 8,
        failed: 1,
        skipped_failed: 2,
        processed: 11,
        total: 11,
        cancelled: false,
    };
    assert_eq!(
        thumbnail_summary("All thumbnails", &completed, &en),
        "All thumbnails. Ready: 8, failed: 1, skipped failed: 2. Processed: 11/11."
    );
    let cancelled = ThumbnailCounts {
        generated: 3,
        failed: 0,
        skipped_failed: 5,
        processed: 8,
        total: 20,
        cancelled: true,
    };
    assert_eq!(
        thumbnail_summary("Retry failed", &cancelled, &en),
        "Retry failed. Ready: 3, failed: 0, skipped failed: 5. Cancelled at 8/20."
    );
}

/// progressService.test.ts "formats progress label from processed and total".
#[test]
fn the_progress_label_is_processed_over_total() {
    assert_eq!(
        progress_label(&progress("scanning", 12, 40, "Scanning")),
        "12/40"
    );
}

/// progressService.test.ts "matches scan, thumbnail and library clear phases".
#[test]
fn each_operation_matches_only_its_own_phases() {
    assert!(is_scan_progress_phase("counting"));
    assert!(is_scan_progress_phase("cleanup"));
    assert!(!is_scan_progress_phase("library-clear"));

    assert!(is_thumbnail_progress_phase("thumbs-rendering"));
    assert!(!is_thumbnail_progress_phase("scanning"));

    assert!(is_library_clear_progress_phase("library-clear"));
    assert!(is_library_clear_progress_phase("library-clear-done"));
    assert!(!is_library_clear_progress_phase("thumbs-batch"));

    assert!(is_duplicate_scan_progress_phase("duplicates-scan"));
    assert!(is_duplicate_scan_progress_phase("duplicates-scan-done"));
    assert!(!is_duplicate_scan_progress_phase("scanning"));
}

/// operationStateService.test.ts "creates default state for each settings section", and
/// the ready lines of statusLanguage.test.tsx "re-translates the idle ready lines after
/// a language switch".
#[test]
fn every_section_starts_idle_on_its_ready_line_in_either_language() {
    let fx = Fixture::new(|_| {});
    let expected = [
        (
            OperationSection::Scan,
            "Ready to scan",
            "Gotowe do skanowania",
        ),
        (
            OperationSection::ImportExport,
            "Ready for import/export",
            "Gotowe do importu/eksportu",
        ),
        (
            OperationSection::Danger,
            "No destructive operation running",
            "Brak uruchomionej operacji destrukcyjnej",
        ),
        (
            OperationSection::Duplicates,
            "Ready to scan duplicates",
            "Gotowe do skanowania duplikatów",
        ),
    ];
    for (section, en, pl) in expected {
        let status = fx.status(section);
        assert_eq!(
            status,
            SectionStatus {
                loading: false,
                message: StatusMessage::Ready(section),
                progress: None,
            }
        );
        assert_eq!(fx.line_in(section, Language::En), en);
        assert_eq!(fx.line_in(section, Language::Pl), pl);
        assert_eq!(fx.line_in(section, Language::En), en, "and back");
    }
}

/// SectionOperationStatus.test.tsx "renders progress bar with formatted label" and
/// "renders indeterminate progress when total equals zero".
#[test]
fn the_progress_bar_is_determinate_only_with_a_total() {
    let known = progress("scanning", 4, 10, "Scanning 4/10");
    assert_eq!(progress_bar(&known), (Some(4), 10));
    assert_eq!(progress_label(&known), "4/10");
    let unknown = progress("counting", 0, 0, "Counting");
    assert_eq!(progress_bar(&unknown), (None, 1));
}

/// pluralCopy.test.tsx "localized video tool status" (en, pl).
#[test]
fn the_video_tool_lines_are_translated() {
    for (language, first, second) in [
        (Language::En, "ffmpeg: available", "ffprobe: not available"),
        (Language::Pl, "ffmpeg: dostępny", "ffprobe: niedostępny"),
    ] {
        let i18n = I18n::new(language);
        assert_eq!(video_tool_line(&i18n, "ffmpeg", true), first);
        assert_eq!(video_tool_line(&i18n, "ffprobe", false), second);
    }
}

// operationSafety.test.tsx

/// "takes the lock synchronously and subscribes before dispatching": the lock is taken
/// in the turn of the command, a second operation is refused without a call, the
/// progress subscription exists when the backend call runs, and loading goes on and off.
#[test]
fn the_lock_is_taken_in_the_same_turn_and_progress_is_subscribed_before_the_call() {
    let mut fx = Fixture::started();
    let seen = Arc::new(AtomicUsize::new(usize::MAX));
    let backend = Arc::downgrade(&fx.backend);
    let listeners = seen.clone();
    fx.backend.on_render_all_thumbnails(move || {
        let backend = backend.upgrade().expect("the fake");
        listeners.store(backend.progress_listeners(), Ordering::SeqCst);
        Ok(render_summary(1, 1, false))
    });

    assert!(!fx.runtime().maintenance_loading());
    fx.command(SettingsCommand::RenderAllThumbnails);
    assert!(fx.locked(), "locked in the same turn");
    assert!(fx.runtime().maintenance_loading(), "loading on");
    let pending = fx.executor.pending();
    fx.command(SettingsCommand::RenderFailedThumbnails);
    fx.command(SettingsCommand::RescanAll);
    assert_eq!(
        fx.executor.pending(),
        pending,
        "the second operation was refused"
    );

    fx.settle();
    assert_eq!(seen.load(Ordering::SeqCst), 1, "subscribed before the call");
    assert_eq!(fx.backend.progress_listeners(), 0, "unsubscribed");
    assert!(!fx.locked());
    assert!(!fx.runtime().maintenance_loading(), "loading off");
    assert_eq!(fx.calls("render_all_thumbnails"), 1);
    assert_eq!(fx.calls("render_failed_thumbnails"), 0);
    assert_eq!(fx.calls("rescan_all_roots"), 0);
}

/// "filters phases and ignores callbacks after completion or disposal": an unrelated
/// phase leaves the pending line, the own phase shows, and progress that arrives for
/// the operation after it ended changes nothing.
#[test]
fn foreign_phases_and_late_progress_are_ignored() {
    let mut fx = Fixture::started();
    fx.backend.on_rescan_all_roots(|| Ok(summary(1, 0)));
    fx.command(SettingsCommand::RescanAll);
    let op = fx.runtime().settings().active_operation().expect("runs");

    fx.progress_for_active(progress("thumbs", 9, 10, "unrelated"));
    assert_eq!(fx.scan_line(), "Rescanning all folders...");
    assert_eq!(fx.scan_progress(), None);

    fx.progress_for_active(progress("scanning", 2, 10, "raw"));
    assert_eq!(fx.scan_progress(), Some(2));

    fx.settle();
    let finished = fx.status(OperationSection::Scan);
    fx.send(RuntimeInput::Settings(SettingsInput::Progress {
        op,
        progress: progress("scanning", 10, 10, "late"),
    }));
    assert_eq!(
        fx.status(OperationSection::Scan),
        finished,
        "late progress dropped"
    );
}

/// "translates backend errors %s without changing other sections".
#[test]
fn backend_errors_are_translated_and_other_sections_stay_as_they_were() {
    for raw in [
        "i18n:common.cancel",
        r#"{"key":"common.cancel"}"#,
        "common.cancel",
        "{malformed",
        "free form",
    ] {
        let mut fx = Fixture::started();
        let before = fx.status(OperationSection::ImportExport);
        let error = raw.to_owned();
        fx.backend
            .on_rescan_all_roots(move || Err(BackendError::rejected(error.clone())));
        fx.command(SettingsCommand::RescanAll);
        fx.settle();
        let detail = if raw.contains("common.cancel") {
            "Cancel"
        } else {
            raw
        };
        assert_eq!(fx.scan_line(), format!("Rescan error: {detail}"), "{raw}");
        assert_eq!(fx.status(OperationSection::ImportExport), before, "{raw}");
        assert_eq!(
            fx.status(OperationSection::Danger),
            SectionStatus::ready(OperationSection::Danger),
            "{raw}"
        );
    }
}

fn merge(ticket: u64, asset_id: i64) -> RuntimeInput {
    RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(ticket),
        mutation: Mutation::MergeTags {
            asset_ids: vec![asset_id],
            tags: vec!["synthetic".into()],
        },
    })
}

/// "drains writes, invalidates under the barrier and unlocks after uncertain clear
/// failure": a write admitted before "clear library" ends before the clear runs, writes
/// are refused while the barrier stands, a failed clear still refreshes the library and
/// lists the roots, and writes are admitted again afterwards.
#[test]
fn clear_library_drains_writes_and_unlocks_after_an_uncertain_failure() {
    let mut fx = Fixture::started();
    fx.backend
        .on_merge_asset_tags_bulk(|_, _| Err(BackendError::rejected("disk")))
        .on_clear_library_data(|| Err(BackendError::rejected("uncertain IPC failure")));
    let queries = fx.calls("start_asset_query");
    let listings = fx.calls("list_scan_roots");

    fx.send(merge(1, 1));
    assert_eq!(fx.executor.pending(), 1, "the write is in flight");
    fx.command(SettingsCommand::OpenClearConfirm);
    fx.command(SettingsCommand::ConfirmClear);
    assert!(fx.locked());
    assert!(fx.runtime().library().maintenance_active());
    assert_eq!(fx.executor.pending(), 1, "the clear waits for the write");

    fx.send(merge(2, 2));
    assert_eq!(
        fx.executor.pending(),
        1,
        "a write under the barrier is refused"
    );

    fx.answer_next();
    assert_eq!(fx.calls("merge_asset_tags_bulk"), 1);
    assert_eq!(fx.calls("clear_library_data"), 0, "not run yet");
    assert_eq!(fx.executor.pending(), 1, "the clear is queued now");
    fx.answer_next();
    assert_eq!(fx.calls("clear_library_data"), 1);
    assert!(!fx.runtime().library().maintenance_active(), "barrier down");

    fx.settle();
    assert!(
        fx.calls("start_asset_query") > queries,
        "the library was refreshed"
    );
    assert_eq!(
        fx.calls("list_scan_roots"),
        listings + 1,
        "the roots were listed"
    );
    assert!(!fx.locked());
    assert!(fx
        .line(OperationSection::Danger)
        .contains("uncertain IPC failure"));

    fx.send(merge(3, 3));
    assert_eq!(
        fx.executor.pending(),
        1,
        "a later write reaches the backend"
    );
    fx.settle();
    assert_eq!(fx.calls("merge_asset_tags_bulk"), 2);
}

/// "keeps the original render lock while admitting only one stop request".
#[test]
fn one_stop_request_is_admitted_and_the_render_keeps_its_lock() {
    let mut fx = Fixture::started();
    fx.backend
        .on_render_all_thumbnails(|| Err(BackendError::rejected("render failed")))
        .on_cancel_render_all_thumbnails(|| Ok(true));
    fx.command(SettingsCommand::RenderAllThumbnails);
    assert!(fx.runtime().settings().thumbnail_render_running());

    fx.command(SettingsCommand::CancelThumbnailRender);
    fx.command(SettingsCommand::CancelThumbnailRender);
    assert!(fx.runtime().settings().cancel_thumbnail_pending());
    assert_eq!(fx.executor.pending(), 2, "the render and one stop request");
    fx.answer_nth(1);
    assert_eq!(fx.calls("cancel_render_all_thumbnails"), 1);
    assert!(fx.locked());
    assert!(fx.runtime().settings().thumbnail_render_running());
    assert!(!fx.runtime().settings().cancel_thumbnail_pending());

    fx.settle();
    assert!(!fx.locked());
    assert!(!fx.runtime().settings().thumbnail_render_running());
    assert!(fx.scan_line().contains("render failed"));
}

// startupScan.test.tsx

/// "saves the checkbox without scanning or refreshing the library", and
/// statusLanguage.test.tsx "keeps the toggle on after a successful save".
#[test]
fn the_startup_switch_is_saved_without_a_scan_or_a_refresh() {
    let mut fx = Fixture::started();
    fx.backend.on_set_scan_root_auto_scan(|_, _| Ok(()));
    let queries = fx.calls("start_asset_query");
    let listings = fx.calls("list_scan_roots");

    fx.command(SettingsCommand::SetAutoScan {
        path: "/synthetic/media".into(),
        enabled: true,
    });
    assert!(!fx.runtime().maintenance_loading(), "no global loading");
    fx.settle();

    assert!(fx.backend.calls().contains(&Call::SetScanRootAutoScan {
        path: "/synthetic/media".into(),
        enabled: true,
    }));
    assert_eq!(fx.runtime().scan_roots(), [root("/synthetic/media", true)]);
    assert_eq!(fx.calls("scan_startup_roots"), 1, "only the launch's");
    assert_eq!(fx.calls("scan_folder"), 0);
    assert_eq!(fx.calls("start_asset_query"), queries, "no library refresh");
    assert_eq!(
        fx.calls("list_scan_roots"),
        listings,
        "no reload of the roots"
    );
    assert_eq!(fx.scan_line(), "Saved. Applies on the next app launch.");
}

/// "keeps the saved checkbox value when persistence fails", and statusLanguage.test.tsx
/// "shows the saved value again when the canonical roots reload".
#[test]
fn a_failed_switch_save_keeps_the_saved_value_and_reloads_the_roots() {
    let mut fx = Fixture::started();
    fx.backend
        .on_set_scan_root_auto_scan(|_, _| Err(BackendError::rejected("write failed")));
    let listings = fx.calls("list_scan_roots");

    fx.command(SettingsCommand::SetAutoScan {
        path: "/synthetic/media".into(),
        enabled: true,
    });
    fx.settle();

    assert_eq!(fx.runtime().scan_roots(), [root("/synthetic/media", false)]);
    assert!(fx.scan_line().contains("write failed"));
    assert!(fx
        .scan_line()
        .starts_with("Could not save startup scan preference"));
    assert_eq!(fx.calls("list_scan_roots"), listings + 1, "roots reloaded");
    assert!(!fx.locked());
}

/// statusLanguage.test.tsx "shows the saved value again when the reload fails as well".
#[test]
fn a_failed_switch_save_keeps_the_saved_value_when_the_reload_fails_too() {
    let mut fx = Fixture::started();
    fx.backend
        .on_set_scan_root_auto_scan(|_, _| Err(BackendError::rejected("write failed")))
        .on_list_scan_roots(|| Err(BackendError::rejected("read failed")));

    fx.command(SettingsCommand::SetAutoScan {
        path: "/synthetic/media".into(),
        enabled: true,
    });
    fx.settle();

    assert_eq!(fx.runtime().scan_roots(), [root("/synthetic/media", false)]);
    assert!(fx.scan_line().contains("write failed"));
    assert!(!fx.locked());
}

/// "runs once per owner even when requested repeatedly, subscribes first, and refreshes
/// after completion".
#[test]
fn the_startup_scan_runs_once_subscribed_first_and_refreshes_after_it() {
    let seen = Arc::new(AtomicUsize::new(usize::MAX));
    let listeners = seen.clone();
    let mut fx = Fixture::new(|backend| {
        backend.on_list_scan_roots(|| Ok(vec![root("/synthetic/media", true)]));
    });
    let backend = Arc::downgrade(&fx.backend);
    fx.backend.on_scan_startup_roots(move || {
        let backend = backend.upgrade().expect("the fake");
        listeners.store(backend.progress_listeners(), Ordering::SeqCst);
        Ok(Some(summary(2, 0)))
    });

    fx.send(RuntimeInput::Start);
    assert!(fx.locked(), "the startup scan holds the runner");
    assert_eq!(fx.scan_line(), "Scanning startup folders…");
    fx.settle();
    fx.send(RuntimeInput::Start);
    fx.settle();

    assert_eq!(seen.load(Ordering::SeqCst), 1, "subscribed before the call");
    assert_eq!(fx.calls("scan_startup_roots"), 1);
    assert_eq!(
        fx.calls("start_asset_query"),
        2,
        "the launch's and one refresh"
    );
    assert_eq!(fx.backend.progress_listeners(), 0);
    assert!(!fx.locked());
    assert!(fx.scan_line().contains("Indexed: 2"));
}

/// "handles a launch with no enabled roots without a library refresh".
#[test]
fn a_launch_without_startup_work_refreshes_nothing() {
    let fx = Fixture::started();
    assert_eq!(fx.scan_line(), "");
    assert_eq!(fx.calls("start_asset_query"), 1, "only the launch's query");
    assert_eq!(fx.calls("list_scan_roots"), 1, "only the launch's listing");
    assert_eq!(fx.backend.progress_listeners(), 0);
    assert!(!fx.locked());
}

/// "refreshes committed scan batches after failure and does not retry".
#[test]
fn a_failed_startup_scan_refreshes_once_and_is_not_retried() {
    let mut fx = Fixture::new(|backend| {
        backend.on_scan_startup_roots(|| Err(BackendError::rejected("scan failed")));
    });
    fx.start();
    assert!(fx.scan_line().contains("scan failed"));
    assert_eq!(
        fx.calls("start_asset_query"),
        2,
        "one refresh after the failure"
    );
    assert_eq!(
        fx.calls("list_scan_roots"),
        2,
        "one listing after the failure"
    );

    fx.start();
    assert_eq!(fx.calls("scan_startup_roots"), 1, "no retry");
    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(fx.backend.progress_listeners(), 0);
    assert!(!fx.locked());
}

/// "reports missing folders as a partial result".
#[test]
fn a_partial_startup_scan_is_reported_as_partial() {
    let mut fx = Fixture::new(|backend| {
        backend.on_scan_startup_roots(|| {
            Ok(Some(ScanSummary {
                completion: ScanCompletion::Partial,
                indexed: 2,
                removed: 0,
                failed: 1,
            }))
        });
    });
    fx.start();
    assert!(
        fx.scan_line().to_lowercase().contains("partial"),
        "{}",
        fx.scan_line()
    );
    assert_eq!(fx.calls("start_asset_query"), 2);
}

/// `scanOnStartup` awaits `refreshLibrary()` and `refreshScanRoots()` inside its
/// exclusive operation after a scan with a summary (`createScanSettings.ts`): a library
/// refresh that fails then is the scan line's error, with the rescan prefix, in place of
/// the summary.
#[test]
fn a_failed_library_refresh_after_a_startup_scan_with_work_is_the_scan_lines_error() {
    let mut fx = Fixture::new(|backend| {
        backend.on_scan_startup_roots(|| Ok(Some(summary(2, 0))));
        let mut queries = sequence([
            Ok(empty_query()),
            Err(BackendError::rejected("refresh failed")),
        ]);
        backend.on_start_asset_query(move |_| queries());
    });
    fx.start();
    assert!(
        fx.scan_line().starts_with("Rescan error: "),
        "{}",
        fx.scan_line()
    );
    assert!(fx.scan_line().contains("refresh failed"));
    assert!(!fx.scan_line().contains("Indexed"), "{}", fx.scan_line());
    assert_eq!(fx.calls("list_scan_roots"), 2, "the roots are read again");
    assert!(!fx.locked());
    assert!(!fx.runtime().maintenance_loading());
    assert!(
        matches!(
            fx.runtime().startup_scan().outcome(),
            Some(StartupScanOutcome::Scanned(_))
        ),
        "the scan itself succeeded"
    );
}

/// The same for the root listing after the scan: `await refreshScanRoots()` rejecting
/// is the operation's error, and the roots of the launch's listing stay.
#[test]
fn a_failed_root_listing_after_a_startup_scan_with_work_is_the_scan_lines_error() {
    let mut fx = Fixture::new(|backend| {
        backend.on_scan_startup_roots(|| Ok(Some(summary(2, 0))));
        backend.on_list_scan_roots(sequence([
            Ok(vec![root("/synthetic/media", true)]),
            Err(BackendError::rejected("listing failed")),
        ]));
    });
    fx.start();
    assert!(
        fx.scan_line().starts_with("Rescan error: "),
        "{}",
        fx.scan_line()
    );
    assert!(fx.scan_line().contains("listing failed"));
    assert_eq!(
        fx.calls("start_asset_query"),
        2,
        "the library is read again"
    );
    assert_eq!(
        fx.runtime().scan_roots(),
        [root("/synthetic/media", true)],
        "the launch's roots stay"
    );
    assert!(!fx.locked());
    assert!(!fx.runtime().maintenance_loading());
}

/// A failed startup scan settles both refreshes (`Promise.allSettled`) and throws its
/// own error: a refresh that fails as well does not replace it.
#[test]
fn a_failed_startup_scan_keeps_its_error_when_the_refreshes_fail_too() {
    let mut fx = Fixture::new(|backend| {
        backend.on_scan_startup_roots(|| Err(BackendError::rejected("scan failed")));
        let mut queries = sequence([
            Ok(empty_query()),
            Err(BackendError::rejected("refresh failed")),
        ]);
        backend.on_start_asset_query(move |_| queries());
        backend.on_list_scan_roots(sequence([
            Ok(vec![root("/synthetic/media", true)]),
            Err(BackendError::rejected("listing failed")),
        ]));
    });
    fx.start();
    assert!(
        fx.scan_line().starts_with("Rescan error: "),
        "{}",
        fx.scan_line()
    );
    assert!(fx.scan_line().contains("scan failed"), "{}", fx.scan_line());
    assert!(!fx.scan_line().contains("refresh failed"));
    assert!(!fx.scan_line().contains("listing failed"));
    assert!(!fx.locked());
    assert!(matches!(
        fx.runtime().startup_scan().outcome(),
        Some(StartupScanOutcome::Failed(_))
    ));
}

/// `rootsGeneration` of `createScanSettings.ts`: the launch's root listing is still on
/// its way when a launch without startup work releases the runner. The folders an
/// operation adds then publish the canonical roots, and the launch's listing, answering
/// last with what it read before, does not replace them.
#[test]
fn a_late_launch_listing_does_not_replace_the_roots_an_operation_published() {
    let before = || vec![root("/synthetic/library", false)];
    let after = || {
        vec![
            root("/synthetic/library", false),
            root("/synthetic/media", false),
        ]
    };
    let mut fx = Fixture::new(|backend| {
        // In the order the calls run: the two listings of the folder operation, then
        // the launch's listing, held back until the end.
        backend.on_list_scan_roots(sequence([Ok(before()), Ok(after()), Ok(before())]));
        backend.on_add_scan_root(|_| Ok(()));
    });

    fx.send(RuntimeInput::Start);
    // The query, the known tags and the startup scan answer; the listing asked for
    // next stays out, and so stays the oldest call while everything else answers.
    for _ in 0..3 {
        fx.answer_next();
    }
    assert_eq!(fx.calls("scan_startup_roots"), 1);
    fx.settle_all_but_oldest();
    assert_eq!(fx.calls("list_scan_roots"), 0, "the launch's listing waits");
    assert_eq!(fx.executor.pending(), 1);
    assert!(!fx.locked(), "a launch without work released the runner");
    assert!(fx.runtime().scan_roots().is_empty());

    fx.command(SettingsCommand::PickFolder);
    let request = fx
        .driver
        .model_mut()
        .take_folder_picker_request()
        .expect("the dialog is asked for");
    fx.send(RuntimeInput::Settings(SettingsInput::FoldersPicked {
        op: request.op,
        result: Ok(Some(vec!["/synthetic/media".into()])),
    }));
    fx.settle_all_but_oldest();
    assert_eq!(
        fx.calls("list_scan_roots"),
        2,
        "the operation's two listings"
    );
    assert_eq!(fx.runtime().scan_roots(), after());
    assert_eq!(
        fx.scan_line(),
        "Folder added to scan list: /synthetic/media"
    );
    assert!(!fx.locked());

    // The launch's listing answers now, with the roots from before the operation.
    fx.answer_next();
    assert_eq!(fx.calls("list_scan_roots"), 3);
    assert_eq!(fx.executor.pending(), 0);
    assert_eq!(
        fx.runtime().scan_roots(),
        after(),
        "the operation's roots stay"
    );
    assert!(fx.runtime().scan_roots_error().is_none());
}

/// The `describe.each` "manual rescan all" and "manual rescan one root".
#[derive(Clone, Copy, Debug)]
enum Manual {
    All,
    Root,
}

impl Manual {
    fn command(self) -> SettingsCommand {
        match self {
            Self::All => SettingsCommand::RescanAll,
            Self::Root => SettingsCommand::RescanRoot("/synthetic/media".into()),
        }
    }

    fn script(self, backend: &FakeBackend, result: Result<ScanSummary, &'static str>) {
        let answer = move || result.clone().map_err(BackendError::rejected);
        match self {
            Self::All => {
                backend.on_rescan_all_roots(answer);
            }
            Self::Root => {
                backend.on_scan_folder(move |_| answer());
            }
        }
    }
}

/// "refreshes the library and roots after a rejection and keeps the error status".
#[test]
fn a_failed_manual_scan_refreshes_the_library_and_roots_and_keeps_its_error() {
    for manual in [Manual::All, Manual::Root] {
        let mut fx = Fixture::started();
        manual.script(&fx.backend, Err("scan exploded"));
        let queries = fx.calls("start_asset_query");
        let listings = fx.calls("list_scan_roots");
        fx.command(manual.command());
        fx.settle();
        assert_eq!(fx.calls("start_asset_query"), queries + 1, "{manual:?}");
        assert_eq!(fx.calls("list_scan_roots"), listings + 1, "{manual:?}");
        assert!(fx.scan_line().contains("scan exploded"), "{manual:?}");
        assert!(!fx.locked(), "{manual:?}");
    }
}

/// "still refreshes roots and keeps the error when the library refresh fails".
#[test]
fn a_failed_manual_scan_keeps_its_error_when_the_library_refresh_fails() {
    for manual in [Manual::All, Manual::Root] {
        let mut fx = Fixture::started();
        manual.script(&fx.backend, Err("scan exploded"));
        fx.backend
            .on_start_asset_query(|_| Err(BackendError::rejected("refresh failed")));
        let listings = fx.calls("list_scan_roots");
        fx.command(manual.command());
        fx.settle();
        assert_eq!(fx.calls("list_scan_roots"), listings + 1, "{manual:?}");
        assert!(
            fx.scan_line().contains("scan exploded"),
            "{manual:?}: {}",
            fx.scan_line()
        );
        assert!(!fx.scan_line().contains("refresh failed"), "{manual:?}");
        assert!(!fx.locked(), "{manual:?}");
    }
}

/// "keeps the success path: one refresh each and the summary".
#[test]
fn a_manual_scan_refreshes_once_each_and_shows_its_summary() {
    for manual in [Manual::All, Manual::Root] {
        let mut fx = Fixture::started();
        manual.script(&fx.backend, Ok(summary(2, 0)));
        let queries = fx.calls("start_asset_query");
        let listings = fx.calls("list_scan_roots");
        fx.command(manual.command());
        fx.settle();
        assert_eq!(fx.calls("start_asset_query"), queries + 1, "{manual:?}");
        assert_eq!(fx.calls("list_scan_roots"), listings + 1, "{manual:?}");
        assert!(fx.scan_line().contains("Indexed: 2"), "{manual:?}");
    }
}

/// The runner's contract (an `await refreshLibrary()` that rejects after a successful
/// command is the operation's error): a scan that succeeded but whose library refresh
/// failed ends with the refresh's error.
#[test]
fn a_failed_refresh_after_a_successful_scan_is_the_operations_error() {
    let mut fx = Fixture::started();
    fx.backend.on_rescan_all_roots(|| Ok(summary(2, 0)));
    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("refresh failed")));
    fx.command(SettingsCommand::RescanAll);
    fx.settle();
    assert!(
        fx.scan_line().starts_with("Rescan error: "),
        "{}",
        fx.scan_line()
    );
    assert!(fx.scan_line().contains("refresh failed"));
    assert!(!fx.locked());
}

// statusLanguage.test.tsx

/// "re-translates a finished operation summary after a language switch".
#[test]
fn a_finished_summary_follows_the_language() {
    let mut fx = Fixture::started();
    fx.backend.on_rescan_all_roots(|| Ok(summary(3, 0)));
    fx.command(SettingsCommand::RescanAll);
    fx.settle();
    assert_eq!(fx.scan_line(), "Done. Indexed: 3, removed: 0, failed: 0");
    assert_eq!(
        fx.line_in(OperationSection::Scan, Language::Pl),
        "Gotowe. Zindeksowano: 3, usunięto: 0, błędy: 0"
    );
}

/// "re-translates a failed operation prefix and a backend i18n detail".
#[test]
fn a_failed_prefix_and_a_backend_key_follow_the_language() {
    let mut fx = Fixture::started();
    fx.backend
        .on_rescan_all_roots(|| Err(BackendError::rejected("i18n:settings.status.ready.danger")));
    fx.command(SettingsCommand::RescanAll);
    fx.settle();
    assert_eq!(
        fx.scan_line(),
        "Rescan error: No destructive operation running"
    );
    assert_eq!(
        fx.line_in(OperationSection::Scan, Language::Pl),
        "Błąd ponownego skanu: Brak uruchomionej operacji destrukcyjnej"
    );
}

/// "re-translates the pending line and the live progress line while an operation runs".
#[test]
fn the_pending_and_the_live_progress_line_follow_the_language() {
    let mut fx = Fixture::started();
    fx.backend.on_rescan_all_roots(|| Ok(summary(1, 0)));
    fx.command(SettingsCommand::RescanAll);
    assert_eq!(fx.scan_line(), "Rescanning all folders...");
    assert_eq!(
        fx.line_in(OperationSection::Scan, Language::Pl),
        "Ponowne skanowanie wszystkich folderów..."
    );
    fx.progress_for_active(progress("scanning", 2, 7, "x"));
    assert_eq!(
        fx.line_in(OperationSection::Scan, Language::Pl),
        "Skanowanie 2/7"
    );
    assert_eq!(fx.scan_line(), "Scanning 2/7");
    fx.settle();
    assert!(!fx.locked());
}

// thumbnailStop.test.tsx

#[derive(Clone, Copy, Debug)]
enum StopReply {
    Accepted,
    Idle,
    Rejected,
}

const STOP_REPLIES: [StopReply; 3] = [StopReply::Accepted, StopReply::Idle, StopReply::Rejected];

impl StopReply {
    fn answer(self) -> Result<bool, BackendError> {
        match self {
            Self::Accepted => Ok(true),
            Self::Idle => Ok(false),
            Self::Rejected => Err(BackendError::rejected("stop failed")),
        }
    }
}

fn render_command(failed_only: bool) -> SettingsCommand {
    if failed_only {
        SettingsCommand::RenderFailedThumbnails
    } else {
        SettingsCommand::RenderAllThumbnails
    }
}

fn script_render(
    fx: &Fixture,
    failed_only: bool,
    result: Result<(usize, usize, bool), &'static str>,
) {
    let answer = move || {
        result
            .map(|(processed, total, cancelled)| render_summary(processed, total, cancelled))
            .map_err(BackendError::rejected)
    };
    if failed_only {
        fx.backend.on_render_failed_thumbnails(answer);
    } else {
        fx.backend.on_render_all_thumbnails(answer);
    }
}

/// "ignores late %s Stop" for both renders, after a render success or failure, with and
/// without a newer scan: the Stop's answer arrives after its render ended and changes
/// nothing.
#[test]
fn a_late_stop_answer_changes_nothing() {
    for failed_only in [false, true] {
        for render_ok in [true, false] {
            for newer_scan in [false, true] {
                for reply in STOP_REPLIES {
                    let case = format!(
                        "failed_only={failed_only} render_ok={render_ok} newer_scan={newer_scan} {reply:?}"
                    );
                    let mut fx = Fixture::started();
                    script_render(
                        &fx,
                        failed_only,
                        if render_ok {
                            Ok((2, 2, false))
                        } else {
                            Err("render failed")
                        },
                    );
                    fx.backend
                        .on_cancel_render_all_thumbnails(move || reply.answer())
                        .on_rescan_all_roots(|| Ok(summary(3, 0)));
                    let queries = fx.calls("start_asset_query");

                    fx.command(render_command(failed_only));
                    fx.command(SettingsCommand::CancelThumbnailRender);
                    // The render answers first; then its refreshes, the stop stays oldest.
                    fx.answer_next();
                    fx.settle_all_but_oldest();
                    assert!(!fx.locked(), "{case}");
                    assert_eq!(fx.calls("cancel_render_all_thumbnails"), 0, "{case}");
                    if render_ok {
                        assert_eq!(fx.calls("start_asset_query"), queries + 1, "{case}");
                    } else {
                        assert!(fx.scan_line().contains("render failed"), "{case}");
                    }
                    if newer_scan {
                        fx.command(SettingsCommand::RescanAll);
                        fx.progress_for_active(progress("scanning", 2, 7, "scan"));
                        assert_eq!(fx.scan_progress(), Some(2), "{case}");
                    }
                    let authoritative = fx.status(OperationSection::Scan);
                    fx.answer_next();
                    assert_eq!(fx.calls("cancel_render_all_thumbnails"), 1, "{case}");
                    assert_eq!(fx.status(OperationSection::Scan), authoritative, "{case}");
                    assert!(
                        !fx.runtime().settings().cancel_thumbnail_pending(),
                        "{case}"
                    );
                    fx.settle();
                    assert!(!fx.locked(), "{case}");
                }
            }
        }
    }
}

/// "publishes active-render %s Stop and allows another request".
#[test]
fn an_active_renders_stop_answer_is_shown_and_another_stop_is_allowed() {
    for failed_only in [false, true] {
        for reply in STOP_REPLIES {
            let case = format!("failed_only={failed_only} {reply:?}");
            let mut fx = Fixture::started();
            script_render(&fx, failed_only, Ok((2, 2, false)));
            fx.backend
                .on_cancel_render_all_thumbnails(move || reply.answer());
            fx.command(render_command(failed_only));
            fx.command(SettingsCommand::CancelThumbnailRender);
            fx.command(SettingsCommand::CancelThumbnailRender);
            assert_eq!(fx.executor.pending(), 2, "{case}: one stop");
            fx.answer_nth(1);
            let expected = match reply {
                StopReply::Rejected => "stop failed",
                StopReply::Accepted => "Stop requested",
                StopReply::Idle => "No thumbnail bulk render",
            };
            assert!(
                fx.scan_line().contains(expected),
                "{case}: {}",
                fx.scan_line()
            );
            assert!(fx.locked(), "{case}");
            assert!(
                !fx.runtime().settings().cancel_thumbnail_pending(),
                "{case}"
            );

            fx.backend.on_cancel_render_all_thumbnails(|| Ok(true));
            fx.command(SettingsCommand::CancelThumbnailRender);
            assert_eq!(fx.executor.pending(), 2, "{case}: the second stop");
            fx.answer_nth(1);
            assert_eq!(fx.calls("cancel_render_all_thumbnails"), 2, "{case}");

            fx.settle();
            assert!(
                !fx.runtime().settings().thumbnail_render_running(),
                "{case}"
            );
            assert!(!fx.locked(), "{case}");
        }
    }
}

/// "a retired %s Stop cannot clear a newer render's pending Stop".
#[test]
fn a_retired_stop_answer_does_not_clear_a_newer_renders_stop() {
    for reply in STOP_REPLIES {
        let case = format!("{reply:?}");
        let mut fx = Fixture::started();
        script_render(&fx, false, Ok((2, 2, false)));
        script_render(&fx, true, Ok((1, 1, false)));
        // Popped from the end: the retired stop's answer first, then the newer one's.
        let answers = Arc::new(Mutex::new(vec![Ok(true), reply.answer()]));
        fx.backend.on_cancel_render_all_thumbnails(move || {
            answers
                .lock()
                .unwrap()
                .pop()
                .expect("a scripted stop answer")
        });

        fx.command(SettingsCommand::RenderAllThumbnails);
        fx.command(SettingsCommand::CancelThumbnailRender);
        fx.answer_next();
        fx.settle_all_but_oldest();
        assert!(!fx.locked(), "{case}");
        assert!(
            !fx.runtime().settings().cancel_thumbnail_pending(),
            "{case}"
        );

        fx.command(SettingsCommand::RenderFailedThumbnails);
        fx.command(SettingsCommand::CancelThumbnailRender);
        assert_eq!(
            fx.executor.pending(),
            3,
            "{case}: old stop, render, new stop"
        );
        assert!(fx.runtime().settings().cancel_thumbnail_pending(), "{case}");
        let state = fx.status(OperationSection::Scan);

        fx.answer_next();
        assert_eq!(fx.status(OperationSection::Scan), state, "{case}");
        assert!(fx.runtime().settings().cancel_thumbnail_pending(), "{case}");
        fx.command(SettingsCommand::CancelThumbnailRender);
        assert_eq!(fx.executor.pending(), 2, "{case}: no third stop");

        fx.answer_nth(1);
        assert!(
            !fx.runtime().settings().cancel_thumbnail_pending(),
            "{case}"
        );
        assert_eq!(fx.calls("cancel_render_all_thumbnails"), 2, "{case}");
        fx.settle();
        assert!(!fx.locked(), "{case}");
    }
}

/// "rejects Stop without an active render": no request without a render, nor while
/// another operation runs.
#[test]
fn stop_without_a_render_makes_no_request() {
    let mut fx = Fixture::started();
    fx.command(SettingsCommand::CancelThumbnailRender);
    assert_eq!(fx.executor.pending(), 0);
    fx.backend.on_rescan_all_roots(|| Ok(summary(0, 0)));
    fx.command(SettingsCommand::RescanAll);
    fx.command(SettingsCommand::CancelThumbnailRender);
    fx.settle();
    assert_eq!(fx.calls("cancel_render_all_thumbnails"), 0);
}

/// "keeps rendered scan progress after Stop and render failure, with an attached root":
/// Stop, the render fails, a rescan starts and reports progress, then the Stop's answer
/// arrives and changes neither the line nor the bar.
#[test]
fn a_stop_answer_after_a_failed_render_keeps_the_newer_scans_progress() {
    let mut fx = Fixture::started();
    fx.backend
        .on_render_all_thumbnails(|| Err(BackendError::rejected("render failed")))
        .on_cancel_render_all_thumbnails(|| Ok(true))
        .on_rescan_all_roots(|| Ok(summary(3, 0)));
    fx.command(SettingsCommand::RenderAllThumbnails);
    fx.command(SettingsCommand::CancelThumbnailRender);
    assert!(fx.runtime().settings().cancel_thumbnail_pending());
    fx.answer_next();
    assert!(fx.scan_line().contains("render failed"));
    assert!(!fx.locked(), "Rescan all is enabled again");
    assert!(!fx.runtime().settings().thumbnail_render_running());

    fx.command(SettingsCommand::RescanAll);
    fx.progress_for_active(progress("scanning", 2, 7, "scan"));
    let line = fx.scan_line();
    fx.answer_next();
    assert_eq!(fx.calls("cancel_render_all_thumbnails"), 1);
    assert_eq!(fx.scan_line(), line);
    assert_eq!(fx.scan_progress(), Some(2));
    fx.settle();
    assert!(!fx.locked());
    assert!(!fx.runtime().settings().thumbnail_render_running());
}

// createSettingsActions.test.tsx

/// "refreshes scan roots and adds a root from picker selection", and the folder half of
/// "handles cancelled export/import dialog selections": the chosen folder is added
/// without global loading, and a cancelled dialog says so.
#[test]
fn a_picked_folder_is_added_and_a_cancelled_dialog_says_so() {
    let mut fx = Fixture::new(|backend| {
        backend.on_list_scan_roots(|| Ok(vec![root("/synthetic/library", false)]));
    });
    fx.start();
    assert_eq!(
        fx.runtime().scan_roots(),
        [root("/synthetic/library", false)]
    );
    let added = Arc::new(AtomicUsize::new(0));
    let flag = added.clone();
    fx.backend.on_add_scan_root(move |_| {
        flag.store(1, Ordering::SeqCst);
        Ok(())
    });
    fx.backend.on_list_scan_roots(move || {
        let mut roots = vec![root("/synthetic/library", false)];
        if added.load(Ordering::SeqCst) == 1 {
            roots.push(root("/synthetic/media", false));
        }
        Ok(roots)
    });

    fx.command(SettingsCommand::PickFolder);
    assert!(!fx.runtime().maintenance_loading(), "no global loading");
    assert_eq!(fx.scan_line(), "Waiting for folder selection...");
    let request = fx
        .driver
        .model_mut()
        .take_folder_picker_request()
        .expect("the dialog is asked for");
    assert_eq!(
        request.title_key,
        "settings.actions.dialogs.selectMediaFolder"
    );
    fx.send(RuntimeInput::Settings(SettingsInput::FoldersPicked {
        op: request.op,
        result: Ok(Some(vec!["/synthetic/media".into()])),
    }));
    assert!(!fx.runtime().maintenance_loading(), "no global loading");
    fx.settle();
    assert!(fx.backend.calls().contains(&Call::AddScanRoot {
        path: "/synthetic/media".into()
    }));
    assert_eq!(
        fx.scan_line(),
        "Folder added to scan list: /synthetic/media"
    );
    assert_eq!(fx.runtime().scan_roots().len(), 2);

    fx.command(SettingsCommand::PickFolder);
    let request = fx
        .driver
        .model_mut()
        .take_folder_picker_request()
        .expect("the dialog is asked for");
    fx.send(RuntimeInput::Settings(SettingsInput::FoldersPicked {
        op: request.op,
        result: Ok(None),
    }));
    fx.settle();
    assert_eq!(fx.calls("add_scan_root"), 1);
    assert_eq!(fx.scan_line(), "Folder selection cancelled.");
    assert!(!fx.locked());
}

/// "rescans single root and refreshes library".
#[test]
fn rescanning_one_root_scans_it_and_refreshes_the_library() {
    let mut fx = Fixture::started();
    fx.backend.on_scan_folder(|_| Ok(summary(3, 1)));
    let queries = fx.calls("start_asset_query");
    fx.command(SettingsCommand::RescanRoot("/synthetic/media".into()));
    assert_eq!(fx.scan_line(), "Rescanning folder: /synthetic/media");
    fx.settle();
    assert!(fx.backend.calls().contains(&Call::ScanFolder {
        path: "/synthetic/media".into()
    }));
    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert_eq!(fx.scan_line(), "Done. Indexed: 3, removed: 1, failed: 0");
}

/// "updates scan section from progress events during rescan".
#[test]
fn a_rescan_shows_its_progress_and_then_its_summary() {
    let mut fx = Fixture::started();
    let backend = Arc::downgrade(&fx.backend);
    fx.backend.on_rescan_all_roots(move || {
        backend.upgrade().expect("the fake").emit_progress(progress(
            "scanning",
            2,
            5,
            "Scanning 2/5",
        ));
        Ok(summary(5, 1))
    });
    fx.command(SettingsCommand::RescanAll);
    assert!(fx.locked());
    assert!(fx.status(OperationSection::Scan).loading);
    assert!(fx.executor.run_next());
    assert!(fx.driver.pump_one(Duration::ZERO), "the progress event");
    assert_eq!(fx.scan_line(), "Scanning 2/5");
    assert_eq!(fx.scan_progress(), Some(2));
    fx.settle();
    assert!(!fx.locked());
    assert!(!fx.status(OperationSection::Scan).loading);
    assert_eq!(fx.scan_line(), "Done. Indexed: 5, removed: 1, failed: 0");
}

/// "blocks other operations while exclusive operation is running": a render holds the
/// runner (the TS case's CSV import belongs to E18; here the danger zone's and the scan
/// section's operations), and a cancelled render's summary says where it stopped.
#[test]
fn a_render_blocks_other_operations_and_a_cancelled_one_says_where_it_stopped() {
    let mut fx = Fixture::started();
    fx.backend.on_render_all_thumbnails(|| {
        Ok(ThumbnailRenderSummary {
            generated: 3,
            stale: 0,
            failed: 1,
            skipped_failed: 2,
            processed: 6,
            total: 10,
            cancelled: true,
        })
    });
    fx.command(SettingsCommand::RenderAllThumbnails);
    assert!(fx.runtime().settings().thumbnail_render_running());
    assert!(fx.locked());
    fx.command(SettingsCommand::OpenClearConfirm);
    fx.command(SettingsCommand::ConfirmClear);
    fx.command(SettingsCommand::RescanRoot("/synthetic/media".into()));
    fx.settle();
    assert_eq!(fx.calls("clear_library_data"), 0);
    assert_eq!(fx.calls("scan_folder"), 0);
    assert!(!fx.runtime().settings().thumbnail_render_running());
    assert!(!fx.locked());
    assert_eq!(
        fx.scan_line(),
        "All thumbnails. Ready: 3, failed: 1, skipped failed: 2. Cancelled at 6/10."
    );
}

/// "closes confirmation and clears library data inside the tag mutation barrier": the
/// barrier is up before the clear runs and down after its reply, and the summary names
/// what was removed.
#[test]
fn clear_library_closes_its_confirmation_and_runs_inside_the_barrier() {
    let mut fx = Fixture::started();
    fx.backend
        .on_clear_library_data(|| {
            Ok(ClearLibrarySummary {
                removed_assets: 7,
                removed_roots: 2,
                removed_thumbnails: 11,
            })
        })
        .on_list_scan_roots(|| Ok(Vec::new()));
    fx.command(SettingsCommand::OpenClearConfirm);
    assert!(fx.runtime().settings().clear_confirm_open());
    fx.command(SettingsCommand::ConfirmClear);
    assert!(!fx.runtime().settings().clear_confirm_open());
    assert!(fx.runtime().library().maintenance_active(), "barrier:start");
    assert_eq!(
        fx.line(OperationSection::Danger),
        "Clearing library data..."
    );
    assert_eq!(
        fx.executor.pending(),
        1,
        "the clear, the barrier granted at once"
    );
    assert_eq!(fx.calls("clear_library_data"), 0);
    fx.answer_next();
    assert_eq!(fx.calls("clear_library_data"), 1, "clear");
    assert!(!fx.runtime().library().maintenance_active(), "barrier:end");
    fx.settle();
    assert_eq!(
        fx.line(OperationSection::Danger),
        "Library cleared. Assets: 7, roots: 2, thumbnails: 11"
    );
    assert!(!fx.runtime().has_scan_roots());
    assert!(!fx.locked());
}

/// "invalidates and refreshes library state when clear library rejects".
#[test]
fn a_rejected_clear_refreshes_the_library_and_the_roots_once() {
    let mut fx = Fixture::started();
    fx.backend
        .on_clear_library_data(|| Err(BackendError::rejected("partial clear failure")));
    let queries = fx.calls("start_asset_query");
    let listings = fx.calls("list_scan_roots");
    fx.command(SettingsCommand::ConfirmClear);
    fx.settle();
    assert_eq!(fx.calls("clear_library_data"), 1);
    assert_eq!(fx.calls("list_scan_roots"), listings + 1);
    assert!(
        fx.calls("start_asset_query") > queries,
        "the library was refreshed"
    );
    let line = fx.line(OperationSection::Danger);
    assert!(line.starts_with("Clear library error: "), "{line}");
    assert!(line.contains("partial clear failure"), "{line}");
    assert!(!fx.runtime().library().maintenance_active());
    assert!(!fx.locked());
}

/// "waits for remove confirmation before deleting root".
#[test]
fn removing_a_root_waits_for_its_confirmation_and_refreshes_after_it() {
    let mut fx = Fixture::started();
    fx.backend.on_remove_scan_root(|_| {
        Ok(RemoveRootSummary {
            removed_assets: 4,
            removed_thumbnails: 9,
        })
    });
    fx.command(SettingsCommand::RequestRemoveRoot(
        "/synthetic/media".into(),
    ));
    assert_eq!(
        fx.runtime().settings().remove_root_confirm(),
        Some("/synthetic/media")
    );
    fx.command(SettingsCommand::CancelRemoveRoot);
    assert_eq!(fx.runtime().settings().remove_root_confirm(), None);
    fx.settle();
    assert_eq!(fx.calls("remove_scan_root"), 0);

    let queries = fx.calls("start_asset_query");
    let listings = fx.calls("list_scan_roots");
    fx.command(SettingsCommand::RequestRemoveRoot(
        "/synthetic/media".into(),
    ));
    fx.command(SettingsCommand::ConfirmRemoveRoot);
    assert_eq!(fx.runtime().settings().remove_root_confirm(), None);
    fx.settle();
    assert!(fx.backend.calls().contains(&Call::RemoveScanRoot {
        path: "/synthetic/media".into()
    }));
    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert_eq!(fx.calls("list_scan_roots"), listings + 1);
    assert_eq!(fx.scan_line(), "Removed root. Assets: 4, thumbnails: 9");
}

/// RemoveScanRootConfirmDialog.test.tsx "disables actions when operation is locked", at
/// the model: while an operation runs the open confirmation is neither confirmed nor
/// cancelled.
#[test]
fn the_remove_confirmation_does_nothing_while_an_operation_runs() {
    let mut fx = Fixture::started();
    fx.backend.on_rescan_all_roots(|| Ok(summary(0, 0)));
    fx.command(SettingsCommand::RequestRemoveRoot(
        "/synthetic/media".into(),
    ));
    fx.command(SettingsCommand::RescanAll);
    fx.command(SettingsCommand::ConfirmRemoveRoot);
    fx.command(SettingsCommand::CancelRemoveRoot);
    assert_eq!(
        fx.runtime().settings().remove_root_confirm(),
        Some("/synthetic/media")
    );
    fx.settle();
    assert_eq!(fx.calls("remove_scan_root"), 0);
    fx.command(SettingsCommand::CancelRemoveRoot);
    assert_eq!(fx.runtime().settings().remove_root_confirm(), None);
}

/// "reports when an active render's stop request is rejected or fails".
#[test]
fn a_refused_or_failed_stop_request_is_reported() {
    let mut fx = Fixture::started();
    fx.backend
        .on_render_all_thumbnails(|| Ok(render_summary(0, 0, true)))
        .on_cancel_render_all_thumbnails(|| Ok(false));
    fx.command(SettingsCommand::RenderAllThumbnails);
    fx.command(SettingsCommand::CancelThumbnailRender);
    fx.answer_nth(1);
    assert_eq!(fx.scan_line(), "No thumbnail bulk render is running.");

    fx.backend
        .on_cancel_render_all_thumbnails(|| Err(BackendError::rejected("network")));
    fx.command(SettingsCommand::CancelThumbnailRender);
    fx.answer_nth(1);
    assert!(fx.scan_line().starts_with("Cancel thumbnail render error"));
    assert!(fx.scan_line().contains("network"));
    assert!(!fx.runtime().settings().cancel_thumbnail_pending());
    fx.settle();
    assert!(!fx.locked());
}

// Acceptance: "clear library" only on fresh temporary directories, on the real core.

/// Acceptance "Testy „wyczyść bibliotekę” tylko na świeżych katalogach tymczasowych":
/// on a fresh temporary profile with three synthetic images, "clear library" removes
/// the indexed assets and the root from the database, leaves the media files on disk,
/// and the runtime shows an empty library without roots.
#[test]
fn clear_library_on_a_fresh_temporary_profile_empties_the_library_and_keeps_the_files() {
    let core = TempCore::start();
    let root_path = TempCore::text(&core.media_dir());
    let images: Vec<_> = (0..3)
        .map(|index| core.write_image(&format!("image-{index:02}.png")))
        .collect();
    core.backend().add_scan_root(root_path.clone()).unwrap();
    assert_eq!(core.backend().scan_folder(root_path).unwrap().indexed, 3);

    let executor = Arc::new(ManualExecutor::new());
    let mut driver = Driver::new(
        AppRuntime::new(RuntimeConfig {
            theme: ThemeName::Dark,
            language: Language::En,
        }),
        RuntimeRunner::new(core.shared(), Arc::new(MemoryPrefsStore::default())),
        executor.clone() as Arc<dyn Executor>,
    );
    let settle = |driver: &mut Driver<AppRuntime, RuntimeRunner>| {
        while executor.run_all() > 0 || driver.pump() > 0 {}
    };
    driver.handle(RuntimeInput::Start);
    settle(&mut driver);
    assert_eq!(driver.model().query().total(), 3);
    assert!(driver.model().has_scan_roots());

    driver.handle(RuntimeInput::settings(SettingsCommand::OpenClearConfirm));
    driver.handle(RuntimeInput::settings(SettingsCommand::ConfirmClear));
    settle(&mut driver);

    let line = driver
        .model()
        .settings()
        .status(OperationSection::Danger)
        .message
        .resolve(&I18n::new(Language::En));
    assert!(
        line.starts_with("Library cleared. Assets: 3, roots: 1, thumbnails: "),
        "{line}"
    );
    assert!(!driver.model().settings().is_locked());
    assert!(!driver.model().library().maintenance_active());
    assert!(!driver.model().has_scan_roots());
    assert_eq!(driver.model().query().total(), 0);
    assert!(core.backend().list_scan_roots().unwrap().is_empty());
    for image in images {
        assert!(image.exists(), "clear library never deletes media files");
    }
}
