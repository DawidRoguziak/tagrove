//! The tag list (epic E13) as a workflow over the application runtime: on the real core
//! in a temporary profile, where its pages, its filter and the search it applies go
//! through SQLite (`list_tags`, the asset query), and on the scripted backend for the
//! Apply cases of `TagListModal.test.tsx` that the model tests leave to the runtime.
//! Every library here is a fresh temporary directory.

use std::sync::Arc;

use media_tagger_client::{
    backend::{core_backend::test_support::TempCore, AssetQueryRequest, Backend, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    model::Driver,
    prefs::{MemoryPrefsStore, PrefsStore},
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    search::tag_list::{TagListInput, TagSelectionMode, SINGLE_CLICK_DELAY},
    theme::ThemeName,
};
use media_tagger_core::models::{StartAssetQueryResult, TagListPage};

const DARK_EN: RuntimeConfig = RuntimeConfig {
    theme: ThemeName::Dark,
    language: Language::En,
};

struct Fixture {
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl Fixture {
    fn new(backend: Arc<dyn Backend>) -> Self {
        let executor = Arc::new(ManualExecutor::new());
        let prefs: Arc<dyn PrefsStore> = Arc::new(MemoryPrefsStore::default());
        let driver = Driver::new(
            AppRuntime::new(DARK_EN),
            RuntimeRunner::new(backend, prefs),
            executor.clone() as Arc<dyn Executor>,
        );
        Self { executor, driver }
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn tag_list(&mut self, input: TagListInput) {
        self.send(RuntimeInput::TagList(input));
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn items(&self) -> Vec<String> {
        self.runtime().tag_list().page().items.clone()
    }
}

/// The IDs of every asset of the library, in query order.
fn asset_ids(backend: &dyn Backend) -> Vec<i64> {
    match backend
        .start_asset_query(AssetQueryRequest::new("tag-list-check", 1))
        .expect("start the query")
    {
        StartAssetQueryResult::Ready { items, .. } => items.iter().map(|item| item.id).collect(),
        StartAssetQueryResult::Superseded => panic!("a fresh owner is never superseded"),
    }
}

/// Three synthetic images: the first carries 150 numbered tags, the second
/// `camera`, the third `camera` and `cat`.
fn tagged_library() -> TempCore {
    let core = TempCore::start();
    for index in 0..3 {
        core.write_image(&format!("image-{index:02}.png"));
    }
    let backend = core.backend();
    let summary = backend
        .scan_folder(TempCore::text(&core.media_dir()))
        .expect("scan the media directory");
    assert_eq!(summary.indexed, 3);
    let ids = asset_ids(backend);
    assert_eq!(ids.len(), 3);
    let numbered: Vec<String> = (0..150).map(|index| format!("number-{index:03}")).collect();
    backend
        .set_asset_tags(ids[0], numbered)
        .expect("tag the first image");
    backend
        .set_asset_tags(ids[1], vec!["camera".into()])
        .expect("tag the second image");
    backend
        .set_asset_tags(ids[2], vec!["camera".into(), "cat".into()])
        .expect("tag the third image");
    core
}

/// createTagList "deduplicates same-turn page requests" and TagListModal "loads the next
/// page on scroll and merges it without duplicates", on SQLite: the first page holds 100
/// of the 152 tags, the next page the rest, and no tag twice; a filter starts over from
/// the first page of the matching tags.
#[test]
fn the_tag_list_pages_and_filters_the_tags_of_the_real_library() {
    let core = tagged_library();
    let mut fx = Fixture::new(core.shared());
    fx.send(RuntimeInput::Start);
    fx.settle();

    fx.tag_list(TagListInput::Open);
    fx.settle();
    assert!(fx.runtime().tag_list().is_open());
    assert_eq!(fx.items().len(), 100);
    assert_eq!(fx.runtime().tag_list().page().total, 152);

    fx.tag_list(TagListInput::LoadMore);
    fx.tag_list(TagListInput::LoadMore);
    fx.settle();
    let items = fx.items();
    assert_eq!(items.len(), 152);
    let mut unique = items.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 152, "no tag twice");
    assert!(items.contains(&"camera".to_owned()) && items.contains(&"cat".to_owned()));
    assert!(!fx.runtime().tag_list().loading_more());

    fx.tag_list(TagListInput::SetQuery(" CA ".into()));
    fx.settle();
    let mut matching = fx.items();
    matching.sort();
    assert_eq!(matching, ["camera", "cat"]);
    assert_eq!(fx.runtime().tag_list().page().total, 2);
}

/// TagListModal "tracks single and double click selections and applies a sorted filter
/// string" through to the query: `camera` included and `cat` excluded find the one
/// image tagged `camera` only, and the list closes once that query answered.
#[test]
fn applying_the_choices_searches_the_real_library_and_closes_the_list() {
    let core = tagged_library();
    let mut fx = Fixture::new(core.shared());
    fx.send(RuntimeInput::Start);
    fx.settle();
    assert_eq!(fx.runtime().query().total(), 3);

    fx.tag_list(TagListInput::Open);
    fx.tag_list(TagListInput::SetQuery("ca".into()));
    fx.settle();
    fx.tag_list(TagListInput::Click {
        tag: "camera".into(),
        count: 1,
    });
    fx.settle();
    fx.executor.advance_and_run(SINGLE_CLICK_DELAY);
    fx.settle();
    fx.tag_list(TagListInput::DoubleClick { tag: "cat".into() });
    assert_eq!(
        fx.runtime().tag_list().selections().mode("camera"),
        Some(TagSelectionMode::Include)
    );

    fx.tag_list(TagListInput::Apply);
    assert!(fx.runtime().tag_list().applying());
    fx.settle();
    assert!(!fx.runtime().tag_list().is_open());
    assert!(!fx.runtime().tag_list().applying());
    assert_eq!(fx.runtime().search().filter_input(), "camera -cat");
    assert_eq!(fx.runtime().query().total(), 1);
}

fn scripted(tags: &[&str]) -> Arc<FakeBackend> {
    let backend = Arc::new(FakeBackend::new());
    let items: Vec<String> = tags.iter().map(|tag| (*tag).to_owned()).collect();
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
        .on_list_tags(move |_, _, _| {
            Ok(TagListPage {
                items: items.clone(),
                total: items.len() as i64,
            })
        });
    backend
}

/// `handleApply` with nothing chosen: `onApplySearch("")` clears the search, and the
/// list closes like after any other successful Apply.
#[test]
fn apply_without_choices_clears_the_search_and_closes_the_list() {
    let backend = scripted(&["cat"]);
    let mut fx = Fixture::new(backend.clone());
    fx.send(RuntimeInput::Start);
    fx.settle();
    fx.send(RuntimeInput::Search(
        media_tagger_client::search::SearchCommand::ApplyInput("dog".into()),
    ));
    fx.settle();
    assert_eq!(fx.runtime().search().filter_input(), "dog");

    fx.tag_list(TagListInput::Open);
    fx.settle();
    fx.tag_list(TagListInput::Apply);
    fx.settle();
    assert!(!fx.runtime().tag_list().is_open());
    assert_eq!(fx.runtime().search().filter_input(), "");
    assert!(fx.runtime().search().applied().include.is_empty());
}

/// Apply twice with the same choices: the second time the filters do not change, and
/// the list must still close (`onApplySearch` resolves, `onClose` runs).
#[test]
fn applying_the_same_choices_again_closes_the_list_again() {
    let backend = scripted(&["cat"]);
    let mut fx = Fixture::new(backend.clone());
    fx.send(RuntimeInput::Start);
    fx.settle();
    for round in 0..2 {
        fx.tag_list(TagListInput::Open);
        fx.settle();
        assert!(fx.runtime().tag_list().is_open(), "round {round}");
        fx.tag_list(TagListInput::DoubleClick { tag: "cat".into() });
        fx.tag_list(TagListInput::Apply);
        fx.settle();
        assert!(!fx.runtime().tag_list().is_open(), "round {round}");
        assert_eq!(fx.runtime().search().filter_input(), "-cat");
    }
}

/// createTagList "rejects old queries and finalizers ... and stops on close" across the
/// runtime: a page that answers after the list was closed and opened again is not
/// shown, and the new opening shows its own first page.
#[test]
fn a_page_of_an_earlier_opening_is_dropped() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let backend = scripted(&[]);
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    backend.on_list_tags(move |_, _, _| {
        let call = counter.fetch_add(1, Ordering::SeqCst);
        let tag = if call == 0 { "stale" } else { "fresh" };
        Ok(TagListPage {
            items: vec![tag.into()],
            total: 1,
        })
    });
    let mut fx = Fixture::new(backend.clone());
    fx.send(RuntimeInput::Start);
    fx.settle();
    // The library's own vocabulary load is not the tag list's: count from here.
    calls.store(0, Ordering::SeqCst);

    fx.tag_list(TagListInput::Open);
    // Close before the first page answered, then open again.
    fx.tag_list(TagListInput::Close);
    fx.tag_list(TagListInput::Open);
    fx.settle();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(fx.runtime().tag_list().is_open());
    assert_eq!(fx.items(), ["fresh"]);
    assert!(!fx.runtime().tag_list().loading());
}
