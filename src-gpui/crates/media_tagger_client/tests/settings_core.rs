//! The settings model inside the application runtime (epic E17), against the scripted
//! backend on the deterministic executor: the exclusive runner, the progress filter,
//! the refreshes an operation ends with, and clear-library behind the mutation barrier.

use std::{sync::Arc, time::Duration};

use media_tagger_client::{
    backend::{BackendError, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    settings::{OperationSection, SettingsCommand, StatusMessage},
    theme::ThemeName,
};
use media_tagger_core::models::{
    ClearLibrarySummary, ScanCompletion, ScanProgress, ScanRoot, ScanSummary,
    StartAssetQueryResult, TagListPage, VideoToolStatus,
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

fn root(path: &str) -> ScanRoot {
    ScanRoot {
        path: path.to_owned(),
        auto_scan_on_startup: true,
    }
}

fn summary(indexed: usize) -> ScanSummary {
    ScanSummary {
        completion: ScanCompletion::Complete,
        indexed,
        removed: 0,
        failed: 0,
    }
}

impl Fixture {
    /// A started runtime over one root, the startup scan without work.
    fn started() -> Self {
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(|_| {
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total: 0,
                    offset: 0,
                    items: Vec::new(),
                })
            })
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: Vec::new(),
                    total: 0,
                })
            })
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| Ok(vec![root("/media")]))
            .on_get_video_tool_status(|| {
                Ok(VideoToolStatus {
                    ffmpeg_available: true,
                    ffprobe_available: false,
                })
            });
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            }),
            RuntimeRunner::new(backend.clone(), Arc::new(MemoryPrefsStore::default())),
            executor.clone() as Arc<dyn Executor>,
        );
        let mut fixture = Self {
            backend,
            executor,
            driver,
        };
        fixture.send(RuntimeInput::Start);
        fixture.settle();
        fixture
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

    fn line(&self, section: OperationSection) -> String {
        self.runtime()
            .settings()
            .status(section)
            .message
            .resolve(&I18n::new(Language::En))
    }

    fn calls(&self, method: &str) -> usize {
        self.backend
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }
}

#[test]
fn the_startup_scan_holds_the_lock_and_a_launch_without_work_leaves_the_line_empty() {
    let fx = Fixture::started();
    let settings = fx.runtime().settings();
    assert!(!settings.is_locked());
    assert!(!settings.status(OperationSection::Scan).loading);
    assert_eq!(
        settings.status(OperationSection::Scan).message,
        StatusMessage::Text(String::new())
    );
    let tools = settings.video_tools().expect("read at start");
    assert!(tools.ffmpeg && !tools.ffprobe);
    assert_eq!(fx.calls("get_video_tool_status"), 1);
}

#[test]
fn a_second_operation_is_refused_in_the_same_turn() {
    let mut fx = Fixture::started();
    fx.backend.on_rescan_all_roots(|| Ok(summary(2)));

    fx.command(SettingsCommand::RescanAll);
    assert!(fx.runtime().settings().is_locked());
    assert!(fx.runtime().maintenance_loading());
    let pending = fx.executor.pending();
    for command in [
        SettingsCommand::RenderAllThumbnails,
        SettingsCommand::RescanRoot("/media".into()),
        SettingsCommand::SetAutoScan {
            path: "/media".into(),
            enabled: false,
        },
        SettingsCommand::PickFolder,
        SettingsCommand::ConfirmClear,
    ] {
        fx.command(command);
        assert_eq!(fx.executor.pending(), pending, "nothing else was started");
    }
    assert_eq!(fx.line(OperationSection::Scan), "Rescanning all folders...");
    assert_eq!(
        fx.line(OperationSection::Danger),
        "No destructive operation running"
    );
    assert!(fx.runtime().settings().folder_picker_request().is_none());

    fx.settle();
    assert!(!fx.runtime().settings().is_locked());
    assert_eq!(
        fx.line(OperationSection::Scan),
        "Done. Indexed: 2, removed: 0, failed: 0"
    );
    assert_eq!(fx.calls("rescan_all_roots"), 1);
    assert_eq!(fx.calls("render_all_thumbnails"), 0);
}

#[test]
fn progress_is_subscribed_before_the_call_and_only_own_phases_count() {
    let mut fx = Fixture::started();
    let backend = Arc::downgrade(&fx.backend);
    fx.backend.on_rescan_all_roots(move || {
        let backend = backend.upgrade().expect("the fake");
        assert_eq!(
            backend.progress_listeners(),
            1,
            "subscribed before the call"
        );
        for (phase, processed) in [("thumbs", 1), ("scanning", 2)] {
            backend.emit_progress(ScanProgress {
                phase: phase.into(),
                processed,
                total: 5,
                message: String::new(),
            });
        }
        Ok(summary(5))
    });

    fx.command(SettingsCommand::RescanAll);
    assert!(fx.executor.run_next());
    // The two progress events, then the reply.
    assert!(fx.driver.pump_one(Duration::ZERO));
    assert_eq!(fx.line(OperationSection::Scan), "Rescanning all folders...");
    assert!(fx.driver.pump_one(Duration::ZERO));
    assert_eq!(fx.line(OperationSection::Scan), "Scanning 2/5");
    assert_eq!(
        fx.runtime()
            .settings()
            .status(OperationSection::Scan)
            .progress
            .as_ref()
            .map(|progress| progress.processed),
        Some(2)
    );
    assert_eq!(
        fx.backend.progress_listeners(),
        0,
        "unsubscribed with the reply"
    );
    fx.settle();
    assert_eq!(
        fx.line(OperationSection::Scan),
        "Done. Indexed: 5, removed: 0, failed: 0"
    );
    assert!(fx
        .runtime()
        .settings()
        .status(OperationSection::Scan)
        .progress
        .is_none());
}

#[test]
fn a_failed_rescan_still_refreshes_and_keeps_its_error() {
    let mut fx = Fixture::started();
    fx.backend
        .on_rescan_all_roots(|| Err(BackendError::rejected("i18n:common.libraryBusy")));
    let queries = fx.calls("start_asset_query");
    let listings = fx.calls("list_scan_roots");

    fx.command(SettingsCommand::RescanAll);
    fx.settle();

    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert_eq!(fx.calls("list_scan_roots"), listings + 1);
    assert_eq!(
        fx.line(OperationSection::Scan),
        "Rescan error: A scan or another library operation is running. Try again when it finishes."
    );
    assert!(!fx.runtime().settings().is_locked());
}

#[test]
fn clear_library_runs_behind_the_barrier_and_lists_the_roots_again() {
    let mut fx = Fixture::started();
    fx.backend.on_clear_library_data(|| {
        Ok(ClearLibrarySummary {
            removed_assets: 3,
            removed_roots: 1,
            removed_thumbnails: 2,
        })
    });
    fx.backend.on_list_scan_roots(|| Ok(Vec::new()));

    fx.command(SettingsCommand::OpenClearConfirm);
    assert!(fx.runtime().settings().clear_confirm_open());
    fx.command(SettingsCommand::ConfirmClear);
    assert!(!fx.runtime().settings().clear_confirm_open());
    assert!(fx.runtime().library().maintenance_active());
    fx.settle();

    assert_eq!(fx.calls("clear_library_data"), 1);
    assert!(!fx.runtime().library().maintenance_active());
    assert!(!fx.runtime().has_scan_roots());
    assert_eq!(
        fx.line(OperationSection::Danger),
        "Library cleared. Assets: 3, roots: 1, thumbnails: 2"
    );
}

#[test]
fn remove_waits_for_its_confirmation() {
    let mut fx = Fixture::started();
    fx.backend.on_remove_scan_root(|_| {
        Ok(media_tagger_core::models::RemoveRootSummary {
            removed_assets: 4,
            removed_thumbnails: 1,
        })
    });

    fx.command(SettingsCommand::RequestRemoveRoot("/media".into()));
    assert_eq!(
        fx.runtime().settings().remove_root_confirm(),
        Some("/media")
    );
    assert_eq!(fx.calls("remove_scan_root"), 0);
    fx.command(SettingsCommand::CancelRemoveRoot);
    assert_eq!(fx.runtime().settings().remove_root_confirm(), None);

    fx.command(SettingsCommand::RequestRemoveRoot("/media".into()));
    fx.command(SettingsCommand::ConfirmRemoveRoot);
    fx.settle();
    assert_eq!(fx.calls("remove_scan_root"), 1);
    assert_eq!(
        fx.line(OperationSection::Scan),
        "Removed root. Assets: 4, thumbnails: 1"
    );
}
