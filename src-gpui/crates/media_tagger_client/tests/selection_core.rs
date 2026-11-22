//! A range selection on the real core (epic E11): the selection reads the IDs of a
//! small range through the query session, a select-all stays a range, and a bulk
//! write goes through `apply_selection_mutation` with the selection's spec and
//! reaches exactly the assets it denotes. The core runs on a fresh temporary profile
//! with synthetic images (`TempCore`).

use std::sync::Arc;

use media_tagger_client::{
    backend::{core_backend::test_support::TempCore, AssetIdRange, Backend},
    exec::{Executor, ManualExecutor},
    gallery::{Interaction, SelectionInput, SelectionNotice, SelectionWrite, MATERIALIZE_LIMIT},
    i18n::Language,
    library::{LibraryEvent, LibraryInput, MutationOutcome, Ticket},
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    theme::ThemeName,
};
use media_tagger_core::models::SelectionMutationResult;

/// One more than `MATERIALIZE_LIMIT`: a select-all stays a range.
const COUNT: usize = MATERIALIZE_LIMIT + 1;

struct Fixture {
    core: TempCore,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl Fixture {
    /// A runtime on a core with `COUNT` indexed synthetic images, its library loaded and
    /// selection mode on.
    fn library() -> Self {
        let core = TempCore::start();
        for index in 0..COUNT {
            core.write_image(&format!("image-{index:05}.png"));
        }
        let summary = core
            .backend()
            .scan_folder(TempCore::text(&core.media_dir()))
            .expect("scan the media directory");
        assert_eq!(summary.indexed, COUNT);
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
        fixture.send(RuntimeInput::RefreshLibrary);
        fixture.settle();
        assert_eq!(fixture.runtime().query().total(), COUNT);
        fixture.select(SelectionInput::ToggleMode);
        fixture
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn select(&mut self, input: SelectionInput) {
        self.send(RuntimeInput::Selection(input));
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    /// The asset IDs of the current session, in gallery order, read from the core.
    fn session_ids(&self) -> Vec<i64> {
        let session = self.runtime().query().session_id().expect("a session");
        match self
            .core
            .backend()
            .get_asset_query_id_range(session, 0, COUNT - 1)
            .expect("read the session")
        {
            AssetIdRange::Ready(ids) => ids,
            AssetIdRange::Stale => panic!("the session is current"),
        }
    }

    fn click(&mut self, ids: &[i64], index: usize, ctrl_like: bool, shift: bool) {
        self.select(SelectionInput::Interact(Interaction::Click {
            asset_id: ids[index],
            asset_index: index,
            ctrl_like,
            shift,
        }));
    }

    fn favorite(&self, asset_id: i64) -> bool {
        self.core
            .backend()
            .get_asset_details(asset_id)
            .expect("read the details")
            .expect("the asset exists")
            .summary
            .is_favorite
    }
}

/// Acceptance: a Shift range is resolved to IDs with one read of the real session and
/// the IDs are the ones at those positions in gallery order.
#[test]
fn a_shift_range_is_resolved_to_the_ids_of_the_real_session() {
    let mut fx = Fixture::library();
    let ids = fx.session_ids();
    fx.click(&ids, 1, false, false);
    fx.click(&ids, 4, false, true);
    assert!(fx.runtime().selection().busy());
    fx.settle();

    let selection = fx.runtime().selection();
    assert!(!selection.busy() && !selection.ranged());
    assert_eq!(selection.error(), None);
    let mut selected = selection.ids().to_vec();
    selected.sort_unstable();
    let mut expected = ids[1..=4].to_vec();
    expected.sort_unstable();
    assert_eq!(selected, expected);
}

/// Acceptance "Mutacja selekcji przez port (`apply_selection_mutation`)": select all
/// minus one Ctrl-click is a spec of the real session, and toggling favorites through
/// it favorites every asset but the excluded one.
#[test]
fn a_range_selection_writes_favorites_through_its_spec_on_the_real_core() {
    let mut fx = Fixture::library();
    let ids = fx.session_ids();
    fx.select(SelectionInput::Interact(Interaction::SelectAll));
    fx.click(&ids, 3, true, false);
    fx.settle();

    let capture = fx.runtime().selection().capture();
    let spec = capture.spec.clone().expect("a range selection");
    assert_eq!(spec.ranges, [[0, COUNT - 1]]);
    assert_eq!(spec.excluded, [ids[3]]);
    assert_eq!(capture.count, COUNT - 1);
    assert_eq!(Some(spec.session_id), fx.runtime().query().session_id());

    let mutation = capture
        .spec_mutation(SelectionWrite::ToggleFavorites)
        .expect("a range selection writes through its spec");
    fx.send(RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(1),
        mutation,
    }));
    fx.settle();

    let settled = fx
        .driver
        .model_mut()
        .take_library_events()
        .into_iter()
        .find_map(|event| match event {
            LibraryEvent::MutationSettled { ticket, result } => Some((ticket, result)),
            _ => None,
        })
        .expect("the write was answered");
    assert_eq!(settled.0, Ticket(1));
    assert!(
        matches!(
            settled.1,
            Ok(MutationOutcome::Selection(
                SelectionMutationResult::Ready { .. }
            ))
        ),
        "{:?}",
        settled.1
    );
    for (index, id) in ids.iter().enumerate() {
        assert_eq!(fx.favorite(*id), index != 3, "asset at {index}");
    }
    // Favorites change neither the members nor the order of the unfiltered result:
    // the session stays and so does the selection.
    assert_eq!(fx.runtime().query().session_id(), Some(spec.session_id));
    let selection = fx.runtime().selection();
    assert_eq!(selection.spec(), Some(spec));
    assert_eq!(selection.count(), COUNT - 1);
    assert_eq!(selection.notice(), None);
}

/// Acceptance "the selection survives a query refresh by the epochs": a refresh that
/// returns the same session keeps the ranges; one whose result changed (a scan added
/// an asset, so the core builds another session) clears them with the notice.
#[test]
fn a_range_selection_survives_a_refresh_of_the_same_session_and_not_a_changed_result() {
    let mut fx = Fixture::library();
    fx.select(SelectionInput::Interact(Interaction::SelectAll));
    fx.settle();
    let before = fx.runtime().query().session_id();
    assert!(fx.runtime().selection().ranged());

    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    assert_eq!(fx.runtime().query().session_id(), before);
    let selection = fx.runtime().selection();
    assert!(selection.ranged());
    assert_eq!(selection.count(), COUNT);
    assert_eq!(selection.notice(), None);

    fx.core.write_image("image-new.png");
    let summary = fx
        .core
        .backend()
        .scan_folder(TempCore::text(&fx.core.media_dir()))
        .expect("scan the media directory again");
    assert_eq!(summary.indexed, COUNT + 1);
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    assert_ne!(fx.runtime().query().session_id(), before);
    assert_eq!(fx.runtime().query().total(), COUNT + 1);
    let selection = fx.runtime().selection();
    assert!(!selection.ranged());
    assert_eq!(selection.count(), 0);
    assert_eq!(selection.notice(), Some(SelectionNotice::RangeCleared));
    assert!(selection.enabled());
    assert_eq!(selection.keepalive_session(), None);
}
