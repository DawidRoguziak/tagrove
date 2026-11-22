//! The lightbox model (epic E15) in the application runtime against the real core on
//! a fresh temporary profile: the details it shows, its writes, the deletion of a
//! file, and the deletion the core refuses while a scan holds the scan lock (ADR
//! 0022). Nothing here touches a directory outside the temporary root of `TempCore`.
//! The backend runs on the deterministic executor.

use std::sync::Arc;

use media_tagger_client::{
    backend::{core_backend::test_support::TempCore, Backend},
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    library::MutationError,
    lightbox::{DeleteConfirm, DeletedSource, LightboxCommand, LightboxEvent, NavigationStatus},
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    theme::ThemeName,
};

struct Fixture {
    core: TempCore,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl Fixture {
    /// A scanned library of `count` synthetic images in the temporary media
    /// directory, loaded into a runtime.
    fn library(count: usize) -> Self {
        let core = TempCore::start();
        for index in 0..count {
            core.write_image(&format!("image-{index}.png"));
        }
        core.backend()
            .scan_folder(TempCore::text(&core.media_dir()))
            .expect("scan the media directory");
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            }),
            RuntimeRunner::new(core.shared(), Arc::new(MemoryPrefsStore::default())),
            executor.clone() as Arc<dyn Executor>,
        );
        let mut fixture = Self {
            core,
            executor,
            driver,
        };
        fixture.driver.handle(RuntimeInput::RefreshLibrary);
        fixture.settle();
        fixture
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn lightbox(&mut self, command: LightboxCommand) {
        self.driver.handle(RuntimeInput::lightbox(command));
    }

    /// Opens the asset of the gallery tile at `index` and lets every read end.
    fn open(&mut self, index: usize) -> i64 {
        let row = self
            .driver
            .model()
            .query()
            .asset_at(index)
            .expect("a row")
            .clone();
        let id = row.id;
        self.lightbox(LightboxCommand::Select {
            asset: Box::new(row),
            index: Some(index),
        });
        self.settle();
        id
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }
}

#[test]
fn the_lightbox_shows_details_and_saves_tags_favorite_and_group_on_the_real_core() {
    let mut fx = Fixture::library(3);
    let id = fx.open(0);
    let row = fx.runtime().query().asset_at(0).expect("a row").clone();

    let view = fx.runtime().lightbox();
    let selected = view.selected().expect("an open lightbox").clone();
    assert_eq!(selected.id(), id);
    // The details complete the row and do not contradict it.
    assert_eq!(selected.summary, row);
    let path = selected.path.clone().expect("the path of the details");
    assert!(path.starts_with(&TempCore::text(&fx.core.media_dir())));
    assert!(selected.size_bytes.is_some_and(|size| size > 0));
    assert!(!view.tag_editing_disabled());
    assert!(view.tag_editor().is_empty());
    assert_eq!(view.navigation_status(), NavigationStatus::Ready);

    fx.lightbox(LightboxCommand::SaveTags(vec!["Trip".into(), "cat".into()]));
    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.lightbox(LightboxCommand::SetMediaGroupKeyDraft(" holiday ".into()));
    fx.lightbox(LightboxCommand::SetMediaGroupOrderDraft("07".into()));
    fx.lightbox(LightboxCommand::SaveMediaGroup);
    fx.settle();

    let stored = fx
        .core
        .backend()
        .get_asset_details(id)
        .expect("read the details")
        .expect("the asset exists");
    assert_eq!(stored.tags, ["cat", "trip"]);
    assert!(stored.summary.is_favorite);
    assert_eq!(stored.summary.media_group_key.as_deref(), Some("holiday"));
    assert_eq!(stored.summary.media_group_order, Some(7.0));

    let view = fx.runtime().lightbox();
    let selected = view.selected().expect("still open");
    assert_eq!(selected.id(), id);
    assert_eq!(view.tag_editor(), ["cat", "trip"]);
    assert!(selected.summary.is_favorite);
    assert_eq!(view.media_group_key_editor(), "holiday");
    assert_eq!(view.media_group_order_editor(), "7");
    assert!(!view.tag_saving() && !view.tag_failed());
    assert!(!view.favorite_pending() && !view.favorite_failed());
    assert!(!view.group_pending() && !view.group_failed());
    // Grouping reorders the result: the position is found again.
    assert_eq!(view.navigation_status(), NavigationStatus::Ready);
    assert!(view.selected_index().is_some());
    assert_eq!(selected.path.as_deref(), Some(path.as_str()));
}

#[test]
fn deleting_in_the_lightbox_removes_the_file_and_closes_the_selection() {
    let mut fx = Fixture::library(3);
    let id = fx.open(1);
    let path = fx
        .runtime()
        .lightbox()
        .selected()
        .and_then(|selected| selected.path.clone())
        .expect("the path of the details");
    assert!(std::path::Path::new(&path).is_file());
    assert_eq!(fx.runtime().query().total(), 3);

    fx.lightbox(LightboxCommand::DeleteSelected { request: 1 });
    fx.settle();

    assert!(!std::path::Path::new(&path).exists());
    assert!(fx.runtime().lightbox().selected().is_none());
    assert_eq!(fx.runtime().query().total(), 2);
    assert!(fx.runtime().query().assets().all(|row| row.id != id));
    assert!(fx
        .core
        .backend()
        .get_asset_details(id)
        .expect("read the details")
        .is_none());
    let events = fx.driver.model_mut().take_lightbox_events();
    let [LightboxEvent::DeleteSettled {
        request: 1,
        result: Ok(Some(deleted)),
    }] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(deleted.source, DeletedSource::Deleted);
    assert_eq!(deleted.removed_assets, 1);

    // The neighbours are still there and can be opened.
    let next = fx.open(0);
    assert_ne!(next, id);
    fx.lightbox(LightboxCommand::Next);
    fx.settle();
    let after = fx
        .runtime()
        .lightbox()
        .selected()
        .map(|selected| selected.id());
    assert!(after.is_some_and(|after| after != id && after != next));
}

#[test]
fn a_file_that_is_already_gone_is_reported_as_a_missing_source() {
    let mut fx = Fixture::library(2);
    let id = fx.open(0);
    let path = fx
        .runtime()
        .lightbox()
        .selected()
        .and_then(|selected| selected.path.clone())
        .expect("the path of the details");
    std::fs::remove_file(&path).expect("remove the synthetic image");

    fx.lightbox(LightboxCommand::DeleteSelected { request: 2 });
    fx.settle();

    assert!(fx.runtime().lightbox().selected().is_none());
    let events = fx.driver.model_mut().take_lightbox_events();
    let [LightboxEvent::DeleteSettled {
        request: 2,
        result: Ok(Some(deleted)),
    }] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(deleted.source, DeletedSource::Missing);
    assert!(fx.runtime().query().assets().all(|row| row.id != id));
}

/// ADR 0022 on the real core: while a scan holds the scan lock, a deletion in the
/// lightbox is refused at once with the busy-library text, and nothing changes: the
/// file stays in the temporary media directory, the asset stays indexed and selected,
/// and the confirmation shows the translated reason with its exits usable. After the
/// scan the same deletion goes through.
#[test]
fn a_deletion_during_a_scan_is_refused_as_busy_and_changes_nothing() {
    let mut fx = Fixture::library(2);
    let id = fx.open(0);
    let path = fx
        .runtime()
        .lightbox()
        .selected()
        .and_then(|selected| selected.path.clone())
        .expect("the path of the details");
    let path = std::path::Path::new(&path);
    assert!(path.starts_with(fx.core.media_dir()));
    let i18n = I18n::new(Language::En);
    let word = i18n.t("lightbox.deleteConfirm.confirmWord");
    let mut confirm = DeleteConfirm::new();
    confirm.sync(Some(id));
    confirm.open();
    confirm.set_text(word.as_str());
    let request = confirm.submit(&word).expect("the word was typed");

    let scan = fx.core.hold_scan_lock();
    fx.lightbox(LightboxCommand::DeleteSelected { request });
    fx.settle();

    let events = fx.driver.model_mut().take_lightbox_events();
    let [LightboxEvent::DeleteSettled {
        request: answered,
        result,
    }] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(*answered, request);
    let Err(MutationError::Backend(error)) = result else {
        panic!("{result:?}");
    };
    assert_eq!(error.text(), "i18n:common.libraryBusy");
    assert!(path.is_file(), "the file is neither staged nor removed");
    let selected = fx.runtime().lightbox().selected().map(|asset| asset.id());
    assert_eq!(selected, Some(id), "the asset stays selected");
    assert_eq!(fx.runtime().query().total(), 2);
    assert_eq!(fx.runtime().lightbox().lightbox.pending_writes(), 0);
    assert!(fx.runtime().query().assets().any(|row| row.id == id));

    confirm.settled(*answered, result);
    assert!(confirm.is_open() && !confirm.deleting());
    let shown = confirm.error_text(&i18n).expect("the refusal is shown");
    assert!(shown.contains(&i18n.t("common.libraryBusy")), "{shown}");
    assert!(confirm.close(), "every exit is usable again");
    drop(scan);

    // The scan has ended: the asset is still indexed, and now it can be deleted.
    assert!(fx
        .core
        .backend()
        .get_asset_details(id)
        .expect("read the details")
        .is_some());
    fx.lightbox(LightboxCommand::DeleteSelected { request: 99 });
    fx.settle();
    let events = fx.driver.model_mut().take_lightbox_events();
    let [LightboxEvent::DeleteSettled {
        request: 99,
        result: Ok(Some(deleted)),
    }] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(deleted.source, DeletedSource::Deleted);
    assert!(!path.exists());
    assert!(fx.runtime().lightbox().selected().is_none());
    assert_eq!(fx.runtime().query().total(), 1);
}
