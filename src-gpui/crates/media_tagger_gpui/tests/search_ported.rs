//! The search header's view behaviour from the Solid specification (epic E12), at the
//! window layer: \`SearchTagatorWrapper.test.tsx\` (Ctrl/Meta+K, each control submits
//! once, Clear gives the focus back), \`SearchTagator.test.tsx\` (the list below the
//! field, closed by a click outside), \`suggestionRecovery.test.tsx\` (the search
//! variant: Retry by pointer and by keyboard) and the acceptance criterion that typing
//! with 100,000 known tags leaves the matching to the executor. The scripted backend
//! runs on the deterministic executor.

use std::sync::Arc;

use gpui::{
    point, px, Focusable as _, KeyUpEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext,
    WindowHandle,
};
use media_tagger_client::{
    backend::{fake::Call, AssetQueryRequest, FakeBackend, MediaKind},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    theme::ThemeName,
};
use media_tagger_core::models::{StartAssetQueryResult, TagListPage};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui::{
        layers::{self, LayerOptions},
        segmented::SegmentedEvent,
    },
    views::{search, shell::ShellView},
    window::{init_application, open_main_window, WINDOW_SIZE},
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
}

impl Fixture {
    fn new(tags: Vec<String>) -> Self {
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
            .on_list_tags(move |_, offset, limit| {
                let start = (offset.max(0) as usize).min(tags.len());
                let end = (start + limit.max(0) as usize).min(tags.len());
                Ok(TagListPage {
                    items: tags[start..end].to_vec(),
                    total: tags.len() as i64,
                })
            })
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| Ok(Vec::new()));
        Self {
            backend,
            executor: Arc::new(ManualExecutor::new()),
        }
    }

    fn with(tags: &[&str]) -> Self {
        Self::new(tags.iter().map(|tag| (*tag).to_owned()).collect())
    }

    fn parts(&self) -> RuntimeParts {
        RuntimeParts {
            config: RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            },
            backend: self.backend.clone(),
            prefs: Arc::new(MemoryPrefsStore::default()),
            executor: self.executor.clone() as Arc<dyn Executor>,
        }
    }

    fn queries(&self) -> Vec<AssetQueryRequest> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::StartAssetQuery { request } => Some(request),
                _ => None,
            })
            .collect()
    }
}

fn open(
    fixture: &Fixture,
    cx: &mut TestAppContext,
) -> (WindowHandle<ShellView>, VisualTestContext) {
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window(profile::E2E, fixture.parts(), cx)
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.update(|window, _| window.activate_window());
    settle(fixture, &mut cx);
    (window, cx)
}

fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    loop {
        cx.run_until_parked();
        if fixture.executor.run_all() == 0 {
            break;
        }
    }
    cx.run_until_parked();
}

fn field_focused(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, window, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery
                .search()
                .read(cx)
                .input()
                .focus_handle(cx)
                .is_focused(window)
        })
        .unwrap()
}

fn text(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).input().read(cx).text().to_owned()
        })
        .unwrap()
}

fn suggestions(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let combobox = gallery.search().read(cx).combobox();
            if combobox.popup() {
                combobox
                    .items()
                    .iter()
                    .map(|item| item.value.clone())
                    .collect()
            } else {
                Vec::new()
            }
        })
        .unwrap()
}

/// Takes the focus from every element.
fn blur(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
}

fn key(fixture: &Fixture, keystroke: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(keystroke);
    settle(fixture, cx);
}

fn click(fixture: &Fixture, selector: &'static str, cx: &mut VisualTestContext) {
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    settle(fixture, cx);
}

/// SearchTagatorWrapper "focuses search with Ctrl/Meta+K, ignores modified/composing
/// shortcuts, inert content and modal dialogs".
#[gpui::test]
fn ctrl_or_meta_k_focuses_the_field_unless_modified_or_under_a_modal(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&[]);
    let (window, mut cx) = open(&fixture, cx);
    for keys in ["ctrl-k", "cmd-k"] {
        key(&fixture, keys, &mut cx);
        assert!(field_focused(&window, &mut cx), "{keys}");
        blur(&mut cx);
        assert!(!field_focused(&window, &mut cx));
    }
    for keys in ["ctrl-shift-k", "ctrl-alt-k", "k"] {
        key(&fixture, keys, &mut cx);
        assert!(!field_focused(&window, &mut cx), "{keys}");
    }

    let modal = cx.update(|window, cx| {
        let container = cx.focus_handle();
        layers::register(LayerOptions::modal(container, |_, _| {}), window, cx)
    });
    cx.run_until_parked();
    key(&fixture, "ctrl-k", &mut cx);
    assert!(
        !field_focused(&window, &mut cx),
        "a modal layer keeps Ctrl+K from the search field"
    );
    cx.update(|window, cx| layers::release(modal, None, window, cx));
    cx.run_until_parked();
    key(&fixture, "ctrl-k", &mut cx);
    assert!(field_focused(&window, &mut cx));
}

/// SearchTagatorWrapper "submits each kind/favorite command once, shows validation and
/// returns focus on clear".
#[gpui::test]
fn each_control_submits_once_and_clear_returns_the_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&[]);
    let (window, mut cx) = open(&fixture, cx);
    let start = fixture.queries().len();

    let kind = window
        .update(&mut cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).kind_control().clone()
        })
        .unwrap();
    kind.update(&mut cx, |_, cx| {
        cx.emit(SegmentedEvent::Change("video".into()))
    });
    settle(&fixture, &mut cx);
    assert_eq!(fixture.queries().len(), start + 1, "the kind submits once");
    assert_eq!(
        fixture.queries().last().unwrap().kind,
        Some(MediaKind::Video)
    );

    click(&fixture, search::FAVORITES_ID, &mut cx);
    assert_eq!(fixture.queries().len(), start + 2, "favourites submit once");
    assert!(fixture.queries().last().unwrap().favorites_only);

    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input("gN:");
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(search::VALIDATION_SELECTOR).is_none());
    click(&fixture, search::SUBMIT_ID, &mut cx);
    assert!(cx.debug_bounds(search::VALIDATION_SELECTOR).is_some());
    assert_eq!(
        fixture.queries().len(),
        start + 2,
        "a refused submit starts nothing"
    );

    blur(&mut cx);
    click(&fixture, search::CLEAR_ID, &mut cx);
    assert_eq!(fixture.queries().len(), start + 3, "Clear submits once");
    assert!(cx.debug_bounds(search::VALIDATION_SELECTOR).is_none());
    assert_eq!(text(&window, &mut cx), "");
    assert!(
        field_focused(&window, &mut cx),
        "Clear gives the focus back"
    );
}

/// SearchTagator "positions suggestions below the input by default".
#[gpui::test]
fn the_list_hangs_below_the_field(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&["cat", "car"]);
    let (_window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input("ca");
    settle(&fixture, &mut cx);
    let field = cx.debug_bounds(search::FIELD_SELECTOR).expect("the field");
    let list = cx.debug_bounds(search::LIST_SELECTOR).expect("the list");
    assert!(
        list.top() >= field.bottom(),
        "the list {list:?} starts below the field {field:?}"
    );
    assert!(list.left() >= field.left() && list.right() <= field.right());
}

/// SearchTagator "closes suggestion list after input loses focus" (a press outside).
#[gpui::test]
fn a_press_outside_closes_the_list(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&["cat", "car"]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input("ca");
    settle(&fixture, &mut cx);
    assert_eq!(suggestions(&window, &mut cx).len(), 2);
    cx.simulate_click(
        point(px(WINDOW_SIZE.0 / 2.0), px(WINDOW_SIZE.1 - 120.0)),
        Modifiers::none(),
    );
    settle(&fixture, &mut cx);
    assert!(!field_focused(&window, &mut cx));
    assert!(cx.debug_bounds(search::LIST_SELECTOR).is_none());
    assert_eq!(text(&window, &mut cx), "ca", "the draft stays");
}

/// Types \`c\` and makes its suggestion search fail: the executor drops the task, as one
/// that shuts down does, so the search answers with a failure.
fn fail_the_suggestion_search(fixture: &Fixture, cx: &mut VisualTestContext) {
    key(fixture, "ctrl-k", cx);
    cx.simulate_input("c");
    cx.run_until_parked();
    assert_eq!(fixture.executor.pending(), 1, "the suggestion search waits");
    assert!(fixture.executor.drop_nth(0));
    settle(fixture, cx);
    assert!(
        cx.debug_bounds(search::RETRY_ID).is_some(),
        "Retry is shown"
    );
}

fn assert_recovered(
    fixture: &Fixture,
    window: &WindowHandle<ShellView>,
    cx: &mut VisualTestContext,
) {
    assert_eq!(suggestions(window, cx), ["cat"]);
    assert!(cx.debug_bounds(search::RETRY_ID).is_none());
    assert!(field_focused(window, cx), "the focus is back in the field");
    let before = fixture.queries().len();
    assert_eq!(before, 1, "Retry submits nothing");
    key(fixture, "escape", cx);
    key(fixture, "enter", cx);
    let queries = fixture.queries();
    assert_eq!(queries.len(), before + 1, "Escape, then Enter submits once");
    assert_eq!(queries.last().unwrap().tags_and, ["c"]);
}

/// suggestionRecovery "search recovers suggestions after pointer focus transfer".
#[gpui::test]
fn suggestions_recover_after_a_failure_by_pointer(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&["cat"]);
    let (window, mut cx) = open(&fixture, cx);
    fail_the_suggestion_search(&fixture, &mut cx);
    click(&fixture, search::RETRY_ID, &mut cx);
    assert_recovered(&fixture, &window, &mut cx);
}

/// suggestionRecovery "search recovers suggestions after keyboard focus transfer".
#[gpui::test]
fn suggestions_recover_after_a_failure_by_keyboard(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&["cat"]);
    let (window, mut cx) = open(&fixture, cx);
    fail_the_suggestion_search(&fixture, &mut cx);
    key(&fixture, "tab", &mut cx);
    assert!(!field_focused(&window, &mut cx), "Tab leaves the field");
    assert!(
        cx.debug_bounds(search::RETRY_ID).is_some(),
        "Retry stays while the focus is on it"
    );
    // A full press: GPUI turns Enter into a click on the key's release.
    cx.simulate_keystrokes("enter");
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse("enter").expect("a key"),
    });
    settle(&fixture, &mut cx);
    assert_recovered(&fixture, &window, &mut cx);
}

/// Acceptance criterion "typing does not block the UI with ≥ 100,000 tags": with that
/// vocabulary loaded, a keystroke only queues the matching on the executor; the list
/// appears when the executor has run it.
#[gpui::test]
fn typing_with_a_hundred_thousand_tags_leaves_the_matching_to_the_executor(
    cx: &mut TestAppContext,
) {
    let tags: Vec<String> = (0..100_000)
        .map(|i| format!("tag-{i:06}-{}", ["cat", "dog", "landscape"][i % 3]))
        .collect();
    let fixture = Fixture::new(tags);
    let (window, mut cx) = open(&fixture, cx);
    let known = window
        .update(&mut cx, |shell, _, cx| {
            shell
                .runtime()
                .read(cx)
                .model()
                .library()
                .known_tags()
                .len()
        })
        .unwrap();
    assert_eq!(known, 100_000);
    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input("lands");
    cx.run_until_parked();
    assert!(fixture.executor.pending() >= 1, "the search is queued");
    assert!(
        suggestions(&window, &mut cx).is_empty(),
        "nothing matched on the UI thread"
    );
    settle(&fixture, &mut cx);
    assert_eq!(suggestions(&window, &mut cx).len(), 8);
}

fn send(window: &WindowHandle<ShellView>, input: RuntimeInput, cx: &mut VisualTestContext) {
    window
        .update(cx, |shell, _, cx| {
            shell
                .runtime()
                .clone()
                .update(cx, |runtime, cx| runtime.send(input, cx))
        })
        .unwrap();
}

/// \`createAppRuntime\` owns the search model for the app's lifetime: the draft, the
/// media kind and favourites survive the gallery view being left and made anew, and
/// the new header suggests as the old one did.
#[gpui::test]
fn the_draft_survives_a_remount_of_the_gallery(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&["cat", "car"]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input("-cat");
    settle(&fixture, &mut cx);
    let kind = window
        .update(&mut cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).kind_control().clone()
        })
        .unwrap();
    kind.update(&mut cx, |_, cx| {
        cx.emit(SegmentedEvent::Change("video".into()))
    });
    settle(&fixture, &mut cx);
    // The open list hangs over the tools row; Escape closes it first.
    key(&fixture, "escape", &mut cx);
    click(&fixture, search::FAVORITES_ID, &mut cx);
    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input(" dog");
    settle(&fixture, &mut cx);
    let queries = fixture.queries().len();

    send(&window, RuntimeInput::OpenSettings, &mut cx);
    settle(&fixture, &mut cx);
    send(&window, RuntimeInput::CloseSettings, &mut cx);
    settle(&fixture, &mut cx);

    assert_eq!(
        text(&window, &mut cx),
        "-cat dog",
        "the unsubmitted draft stays"
    );
    let value = window
        .update(&mut cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let kind = gallery.search().read(cx).kind_control().read(cx);
            kind.value().map(|value| value.to_string())
        })
        .unwrap();
    assert_eq!(value.as_deref(), Some("video"));
    let search = window
        .update(&mut cx, |shell, _, cx| {
            let search = shell.runtime().read(cx).model().search();
            (search.favorites_only(), search.applied().exclude.clone())
        })
        .unwrap();
    assert_eq!(search, (true, vec!["cat".to_owned()]));
    assert_eq!(
        fixture.queries().len(),
        queries,
        "a remount applies nothing"
    );

    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input(" ca");
    settle(&fixture, &mut cx);
    assert_eq!(
        suggestions(&window, &mut cx),
        ["car"],
        "cat is already used"
    );
}

/// SearchTagator "closes suggestion list after input loses focus", by keyboard: Tab
/// takes the focus out of the field with the list open, and the list is gone from the
/// drawn frame (it no longer covers the tools row) without any further event.
#[gpui::test]
fn tab_out_of_the_field_closes_the_drawn_list(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&["cat", "car"]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input("ca");
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(search::LIST_SELECTOR).is_some());

    key(&fixture, "tab", &mut cx);
    assert!(!field_focused(&window, &mut cx), "Tab leaves the field");
    assert!(
        suggestions(&window, &mut cx).is_empty(),
        "the session closed"
    );
    assert!(
        cx.debug_bounds(search::LIST_SELECTOR).is_none(),
        "the closed list is still drawn: the focus-out handler's notify asks for no frame"
    );
}

/// The same after the focus leaves the field without an input event (the window
/// loses the focus, or a view moves it): the next frame must not show the list.
#[gpui::test]
fn a_programmatic_blur_closes_the_drawn_list(cx: &mut TestAppContext) {
    let fixture = Fixture::with(&["cat", "car"]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    cx.simulate_input("ca");
    settle(&fixture, &mut cx);
    blur(&mut cx);
    settle(&fixture, &mut cx);
    assert!(
        suggestions(&window, &mut cx).is_empty(),
        "the session closed"
    );
    assert!(
        cx.debug_bounds(search::LIST_SELECTOR).is_none(),
        "the closed list is still drawn"
    );
    // A press where the list was reaches the control below it.
    click(&fixture, search::FAVORITES_ID, &mut cx);
    let favorites = window
        .update(&mut cx, |shell, _, cx| {
            shell.runtime().read(cx).model().search().favorites_only()
        })
        .unwrap();
    assert!(
        favorites,
        "the stale list took the press meant for favourites"
    );
}
