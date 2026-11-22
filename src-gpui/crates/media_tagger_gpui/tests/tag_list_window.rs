//! The tag list in the window (epic E13): the launcher opens the dialog with the focus in
//! its filter, tags are chosen with clicks, double clicks and Shift+Enter, Apply submits
//! the search and closes the dialog, and the focus goes back to the launcher. The
//! scripted backend runs on the deterministic executor, timers too.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    point, px, Focusable as _, KeyContext, Keystroke, Modifiers, MouseButton, MouseDownEvent,
    MouseUpEvent, ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext, WindowHandle,
};
use media_tagger_client::{
    backend::{fake::Call, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::RuntimeConfig,
    search::tag_list::{TagSelectionMode, SINGLE_CLICK_DELAY},
    shortcuts::types_a_character,
    theme::ThemeName,
};
use media_tagger_core::models::{StartAssetQueryResult, TagListPage};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui::text_input,
    views::{shell::ShellView, tag_list},
    window::{init_application, open_main_window, open_main_window_started, ShellStart},
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
}

impl Fixture {
    fn new(tags: &[&str]) -> Self {
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
            })
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| Ok(Vec::new()));
        Self {
            backend,
            executor: Arc::new(ManualExecutor::new()),
        }
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

    fn last_query(&self) -> media_tagger_client::backend::AssetQueryRequest {
        self.backend
            .calls()
            .into_iter()
            .rev()
            .find_map(|call| match call {
                Call::StartAssetQuery { request } => Some(request),
                _ => None,
            })
            .expect("a query")
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

/// Runs the background and delivers the next frame, as the platform does after
/// drawing: a dialog takes and gives back the focus one frame later.
fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    for _ in 0..2 {
        loop {
            cx.run_until_parked();
            if fixture.executor.run_all() == 0 {
                break;
            }
        }
        cx.update(|window, cx| window.simulate_next_frame(cx));
    }
    cx.run_until_parked();
}

/// A press and release with the click count of the second click of a double click.
fn double_click(fixture: &Fixture, selector: &'static str, cx: &mut VisualTestContext) {
    let position = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    for click_count in 1..=2 {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::none(),
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::none(),
            click_count,
        });
    }
    settle(fixture, cx);
}

fn click(fixture: &Fixture, selector: &'static str, cx: &mut VisualTestContext) {
    let point = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_click(point, Modifiers::none());
    settle(fixture, cx);
}

fn chip(index: usize) -> &'static str {
    Box::leak(tag_list::chip_selector(index).into_boxed_str())
}

fn is_open(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery
                .search()
                .read(cx)
                .tag_list()
                .read(cx)
                .modal()
                .read(cx)
                .is_open()
        })
        .unwrap()
}

fn mode(
    window: &WindowHandle<ShellView>,
    tag: &str,
    cx: &mut VisualTestContext,
) -> Option<TagSelectionMode> {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let runtime = gallery.search().read(cx).runtime().clone();
            runtime.read(cx).model().tag_list().selections().mode(tag)
        })
        .unwrap()
}

/// Which of the filter and the launcher has the focus.
fn focus(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> (bool, bool) {
    window
        .update(cx, |shell, window, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let list = gallery.search().read(cx).tag_list().read(cx);
            (
                list.input().focus_handle(cx).is_focused(window),
                list.launcher_focus().is_focused(window),
            )
        })
        .unwrap()
}

#[gpui::test]
fn the_launcher_opens_the_list_with_the_focus_in_the_filter(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera", "car", "cat"]);
    let (window, mut cx) = open(&fixture, cx);
    assert!(cx.debug_bounds(tag_list::LIST_SELECTOR).is_none());

    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    assert!(is_open(&window, &mut cx));
    assert!(cx.debug_bounds(tag_list::LIST_SELECTOR).is_some());
    assert!(cx.debug_bounds(chip(2)).is_some());
    assert!(cx.debug_bounds(chip(3)).is_none());
    assert_eq!(focus(&window, &mut cx), (true, false));
    assert!(fixture.backend.calls().iter().any(|call| matches!(
        call,
        Call::ListTags { query, offset: 0, limit: 100 } if query.is_empty()
    )));
}

#[gpui::test]
fn clicks_choose_tags_and_apply_searches_closes_and_restores_the_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera", "car", "cat"]);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);

    click(&fixture, chip(0), &mut cx);
    assert_eq!(mode(&window, "camera", &mut cx), None, "a click waits");
    fixture.executor.advance_and_run(SINGLE_CLICK_DELAY);
    settle(&fixture, &mut cx);
    assert_eq!(
        mode(&window, "camera", &mut cx),
        Some(TagSelectionMode::Include)
    );

    double_click(&fixture, chip(2), &mut cx);
    fixture.executor.advance_and_run(SINGLE_CLICK_DELAY);
    settle(&fixture, &mut cx);
    assert_eq!(
        mode(&window, "cat", &mut cx),
        Some(TagSelectionMode::Exclude)
    );

    click(&fixture, tag_list::APPLY_ID, &mut cx);
    let query = fixture.last_query();
    assert_eq!(
        (query.tags_and, query.tags_not),
        (vec!["camera".to_owned()], vec!["cat".to_owned()])
    );
    assert!(!is_open(&window, &mut cx));
    settle(&fixture, &mut cx);
    assert_eq!(focus(&window, &mut cx), (false, true));
    let filter = window
        .update(&mut cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).input().read(cx).text().to_owned()
        })
        .unwrap();
    assert_eq!(filter, "camera -cat");
}

#[gpui::test]
fn escape_closes_the_list_and_the_launcher_gets_the_focus_back(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera"]);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    cx.simulate_input("cam");
    settle(&fixture, &mut cx);
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(!is_open(&window, &mut cx));
    assert_eq!(focus(&window, &mut cx), (false, true));

    // Opened again, the filter is empty.
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    let text = window
        .update(&mut cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let list = gallery.search().read(cx).tag_list().read(cx);
            list.input().read(cx).text().to_owned()
        })
        .unwrap();
    assert_eq!(text, "");
}

/// The tag buttons' keys follow the rule of `text_input`: each binding is in the
/// buttons' context, and a key that types a character (Shift+Space) is bound with
/// `outside_text_input`, so it would not fire with a text field on the focus path.
#[test]
fn the_tag_button_keys_stay_out_of_text_fields() {
    let chip = KeyContext::parse(tag_list::CHIP_KEY_CONTEXT).unwrap();
    let field = KeyContext::parse(text_input::KEY_CONTEXT).unwrap();
    let bindings = tag_list::key_bindings();
    for key in ["shift-enter", "shift-space"] {
        let typed = [Keystroke::parse(key).unwrap()];
        let binding = bindings
            .iter()
            .find(|binding| binding.match_keystrokes(&typed) == Some(false))
            .unwrap_or_else(|| panic!("no binding for {key}"));
        let predicate = binding.predicate().expect("a context");
        // `depth_of` is how dispatch matches a binding against the focus path.
        assert!(
            predicate.depth_of(std::slice::from_ref(&chip)).is_some(),
            "{key} on a tag button"
        );
        assert_eq!(
            predicate.depth_of(&[chip.clone(), field.clone()]).is_some(),
            !types_a_character(key),
            "{key} with a text field on the focus path"
        );
    }
    assert_eq!(bindings.len(), 2);
}

#[gpui::test]
fn shift_enter_on_a_tag_excludes_it_and_keeps_its_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["cherry"]);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    cx.simulate_keystrokes("tab");
    settle(&fixture, &mut cx);
    cx.simulate_keystrokes("shift-enter");
    settle(&fixture, &mut cx);
    assert_eq!(
        mode(&window, "cherry", &mut cx),
        Some(TagSelectionMode::Exclude)
    );
    let focused = window
        .update(&mut cx, |shell, window, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let list = gallery.search().read(cx).tag_list().read(cx);
            list.chip_focus("cherry")
                .is_some_and(|handle| handle.is_focused(window))
        })
        .unwrap();
    assert!(focused);
    cx.simulate_keystrokes("shift-space");
    settle(&fixture, &mut cx);
    assert_eq!(
        mode(&window, "cherry", &mut cx),
        Some(TagSelectionMode::Include)
    );
}

#[gpui::test]
fn the_comparison_scenario_starts_with_the_list_open_and_two_tags_chosen(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera", "car", "cat"]);
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window_started(
                profile::E2E,
                fixture.parts(),
                ShellStart {
                    tag_list: Some(media_tagger_client::scenario::TAG_LIST_MODAL),
                    ..ShellStart::default()
                },
                cx,
            )
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    settle(&fixture, &mut cx);
    assert!(is_open(&window, &mut cx));
    assert!(cx.debug_bounds(chip(2)).is_some());
    assert_eq!(
        mode(&window, "camera", &mut cx),
        Some(TagSelectionMode::Include)
    );
    assert_eq!(
        mode(&window, "cat", &mut cx),
        Some(TagSelectionMode::Exclude)
    );
    assert_eq!(mode(&window, "car", &mut cx), None);
    // The Tauri steps end with a double click on cat, which takes the focus from the
    // filter; a mouse focus draws no ring.
    assert_eq!(focus(&window, &mut cx), (false, false));
    let cat = window
        .update(&mut cx, |shell, window, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let list = gallery.search().read(cx).tag_list().read(cx);
            list.chip_focus("cat")
                .is_some_and(|handle| handle.is_focused(window))
        })
        .unwrap();
    assert!(cat, "the button of cat has the focus");
    assert!(!cx.update(|window, _| window.last_input_was_keyboard()));
}

// Behaviour tests added from the specification (TagListModal.test.tsx and the
// acceptance criteria of E13) beyond the implementer's.

impl Fixture {
    /// A library of `tags` whose `list_tags` filters by the query and answers at most
    /// `page` tags from the offset, as the core does with its limit.
    fn paged(tags: Vec<String>, page: usize) -> Self {
        let fixture = Self::new(&[]);
        fixture.backend.on_list_tags(move |query, offset, limit| {
            let needle = query.trim().to_lowercase();
            let matching: Vec<&String> = tags
                .iter()
                .filter(|tag| tag.to_lowercase().contains(&needle))
                .collect();
            let start = usize::try_from(offset).unwrap_or(0).min(matching.len());
            let count = page.min(usize::try_from(limit).unwrap_or(0));
            Ok(TagListPage {
                items: matching
                    .iter()
                    .skip(start)
                    .take(count)
                    .map(|tag| (*tag).clone())
                    .collect(),
                total: matching.len() as i64,
            })
        });
        fixture
    }

    /// The offsets of the `list_tags` calls of the tag list (the library's own
    /// vocabulary load asks with a limit other than 100).
    fn page_offsets(&self) -> Vec<i64> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::ListTags {
                    offset, limit: 100, ..
                } => Some(offset),
                _ => None,
            })
            .collect()
    }
}

fn model_applying(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let runtime = gallery.search().read(cx).runtime().clone();
            runtime.read(cx).model().tag_list().applying()
        })
        .unwrap()
}

fn shown(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> Vec<String> {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let runtime = gallery.search().read(cx).runtime().clone();
            runtime.read(cx).model().tag_list().page().items.clone()
        })
        .unwrap()
}

fn filter_disabled(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> bool {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let list = gallery.search().read(cx).tag_list().read(cx);
            list.input().read(cx).is_disabled()
        })
        .unwrap()
}

fn list_scroll_top(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> f32 {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let list = gallery.search().read(cx).tag_list().read(cx);
            -f32::from(list.scroll_handle().offset().y)
        })
        .unwrap()
}

/// A click whose work stays queued on the executor: only the UI thread runs.
fn click_without_backend(selector: &'static str, cx: &mut VisualTestContext) {
    let point = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_click(point, Modifiers::none());
    cx.run_until_parked();
}

/// Acceptance "focus returns to the trigger after closing the modal" for the Cancel
/// button and the close button of the header (`onClose` of both).
#[gpui::test]
fn cancel_and_the_close_button_give_the_launcher_the_focus_back(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera", "cat"]);
    let (window, mut cx) = open(&fixture, cx);
    for closer in [tag_list::CANCEL_ID, tag_list::CLOSE_ID] {
        click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
        assert!(is_open(&window, &mut cx), "{closer}");
        assert_eq!(focus(&window, &mut cx), (true, false), "{closer}");
        double_click(&fixture, chip(1), &mut cx);
        click(&fixture, closer, &mut cx);
        assert!(!is_open(&window, &mut cx), "{closer}");
        // Gone once the dialog has faded out.
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        settle(&fixture, &mut cx);
        assert!(
            cx.debug_bounds(tag_list::LIST_SELECTOR).is_none(),
            "{closer}"
        );
        assert_eq!(focus(&window, &mut cx), (false, true), "{closer}");
        assert_eq!(
            mode(&window, "cat", &mut cx),
            None,
            "closing forgets the choices ({closer})"
        );
    }
    assert!(
        fixture
            .backend
            .calls()
            .iter()
            .all(|call| !matches!(call, Call::StartAssetQuery { request } if !request.tags_not.is_empty())),
        "closing searches nothing"
    );
}

/// TagListModal "keeps the modal open when applying the search fails", in the window:
/// while Apply runs, Escape and Cancel do not close the list and the filter is
/// disabled; a failed query leaves the list open with its choices and the controls
/// enabled again.
#[gpui::test]
fn while_apply_runs_the_list_stays_and_a_failed_search_keeps_the_choices(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["Alpha"]);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    double_click(&fixture, chip(0), &mut cx);
    assert_eq!(
        mode(&window, "Alpha", &mut cx),
        Some(TagSelectionMode::Exclude)
    );
    fixture.backend.on_start_asset_query(|_| {
        Err(media_tagger_client::backend::BackendError::rejected(
            "save failed",
        ))
    });

    click_without_backend(tag_list::APPLY_ID, &mut cx);
    assert!(model_applying(&window, &mut cx));
    assert!(filter_disabled(&window, &mut cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        is_open(&window, &mut cx),
        "Escape is refused while applying"
    );
    click_without_backend(tag_list::CANCEL_ID, &mut cx);
    assert!(
        is_open(&window, &mut cx),
        "Cancel is refused while applying"
    );

    settle(&fixture, &mut cx);
    assert!(is_open(&window, &mut cx));
    assert!(!model_applying(&window, &mut cx));
    assert!(!filter_disabled(&window, &mut cx));
    assert_eq!(
        mode(&window, "Alpha", &mut cx),
        Some(TagSelectionMode::Exclude)
    );
    assert!(cx.debug_bounds(chip(0)).is_some());

    click(&fixture, tag_list::CANCEL_ID, &mut cx);
    assert!(!is_open(&window, &mut cx));
    assert_eq!(focus(&window, &mut cx), (false, true));
}

/// TagListModal "applies literal constructor/__proto__ through keyboard selection":
/// Tab reaches the tags in order, Shift+Enter excludes each, and Apply searches for
/// both literal names.
#[gpui::test]
fn shift_enter_excludes_literal_names_and_apply_searches_for_them(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["__proto__", "constructor"]);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    for _ in 0..2 {
        cx.simulate_keystrokes("tab");
        settle(&fixture, &mut cx);
        cx.simulate_keystrokes("shift-enter");
        settle(&fixture, &mut cx);
    }
    for name in ["__proto__", "constructor"] {
        assert_eq!(
            mode(&window, name, &mut cx),
            Some(TagSelectionMode::Exclude),
            "{name}"
        );
    }
    click(&fixture, tag_list::APPLY_ID, &mut cx);
    assert!(!is_open(&window, &mut cx));
    let mut excluded = fixture.last_query().tags_not;
    excluded.sort();
    assert_eq!(excluded, ["__proto__", "constructor"]);
    let filter = window
        .update(&mut cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).input().read(cx).text().to_owned()
        })
        .unwrap();
    assert_eq!(filter, "-__proto__ -constructor");
}

/// createTagList's automatic pagination in the window: a list whose content fits loads
/// its next pages on its own, from the measurement the view reports after layout, until
/// every tag is shown.
#[gpui::test]
fn a_list_that_fits_loads_its_next_pages_on_its_own(cx: &mut TestAppContext) {
    let tags: Vec<String> = (1..=9).map(|index| format!("tag-{index}")).collect();
    let fixture = Fixture::paged(tags.clone(), 3);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    for _ in 0..6 {
        settle(&fixture, &mut cx);
    }
    assert_eq!(fixture.page_offsets(), [0, 3, 6]);
    assert_eq!(shown(&window, &mut cx), tags);
    assert!(cx.debug_bounds(chip(8)).is_some());
    assert!(cx.debug_bounds(tag_list::LOADING_MORE_SELECTOR).is_none());
}

/// TagListModal "loads the next page on scroll and merges it without duplicates" in the
/// window: a list taller than the dialog loads nothing on its own; scrolling it to its
/// end asks for the page at the end of the shown tags; a new filter starts over at the
/// top of the list.
#[gpui::test]
fn scrolling_to_the_end_loads_the_next_page_and_a_new_filter_scrolls_back(cx: &mut TestAppContext) {
    let tags: Vec<String> = (0..250)
        .map(|index| format!("synthetic-tag-number-{index:03}-with-a-longer-name"))
        .collect();
    let fixture = Fixture::paged(tags, 100);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    assert_eq!(
        fixture.page_offsets(),
        [0],
        "a list taller than the dialog waits for a scroll"
    );
    assert_eq!(shown(&window, &mut cx).len(), 100);

    let list = cx
        .debug_bounds(tag_list::LIST_SELECTOR)
        .expect("the list is drawn");
    cx.simulate_event(ScrollEvent {
        position: list.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-20_000.0))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    assert!(list_scroll_top(&window, &mut cx) > 0.0);
    assert_eq!(fixture.page_offsets(), [0, 100]);
    let items = shown(&window, &mut cx);
    assert_eq!(items.len(), 200);
    assert_eq!(
        label_builds(&window, &mut cx),
        200,
        "a next page labels only its own tags"
    );
    let mut unique = items.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 200);

    cx.simulate_input("number-1");
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    assert_eq!(fixture.page_offsets().last(), Some(&0));
    assert_eq!(
        list_scroll_top(&window, &mut cx),
        0.0,
        "the list starts over at the top"
    );
    assert!(shown(&window, &mut cx)
        .iter()
        .all(|tag| tag.contains("number-1")));
}

fn label_builds(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> u64 {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).tag_list().read(cx).label_builds()
        })
        .unwrap()
}

fn model_scroll_top(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> Option<f32> {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let runtime = gallery.search().read(cx).runtime().clone();
            runtime
                .read(cx)
                .model()
                .tag_list()
                .metrics()
                .map(|metrics| metrics.scroll_top)
        })
        .unwrap()
}

/// Review finding r1-perf-1: the buttons' states and labels are worked out once per
/// page and again only for the tag whose choice changed; a scroll in the middle of a
/// long list works out none and reports nothing to the runtime (no second render).
#[gpui::test]
fn a_scroll_in_the_middle_of_the_list_works_out_no_labels_and_reports_nothing(
    cx: &mut TestAppContext,
) {
    let tags: Vec<String> = (0..100)
        .map(|index| format!("synthetic-tag-number-{index:03}-with-a-longer-name"))
        .collect();
    let fixture = Fixture::paged(tags, 100);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    assert_eq!(
        label_builds(&window, &mut cx),
        100,
        "once per tag of the page"
    );
    assert_eq!(model_scroll_top(&window, &mut cx), Some(0.0));

    click(&fixture, chip(0), &mut cx);
    fixture.executor.advance_and_run(SINGLE_CLICK_DELAY);
    settle(&fixture, &mut cx);
    assert_eq!(
        mode(
            &window,
            "synthetic-tag-number-000-with-a-longer-name",
            &mut cx
        ),
        Some(TagSelectionMode::Include)
    );
    assert_eq!(
        label_builds(&window, &mut cx),
        101,
        "a choice relabels only its own tag"
    );

    let list = cx
        .debug_bounds(tag_list::LIST_SELECTOR)
        .expect("the list is drawn");
    cx.simulate_event(ScrollEvent {
        position: list.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-40.0))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
    settle(&fixture, &mut cx);
    assert!(list_scroll_top(&window, &mut cx) > 0.0);
    assert_eq!(fixture.page_offsets(), [0], "far from the end");
    assert_eq!(
        model_scroll_top(&window, &mut cx),
        Some(0.0),
        "a scroll in the middle goes no further than the view"
    );
    assert_eq!(
        label_builds(&window, &mut cx),
        101,
        "scrolling relabels nothing"
    );
}

fn focused_chip_index(
    window: &WindowHandle<ShellView>,
    tags: &[String],
    cx: &mut VisualTestContext,
) -> Option<usize> {
    window
        .update(cx, |shell, window, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let list = gallery.search().read(cx).tag_list().read(cx);
            tags.iter().position(|tag| {
                list.chip_focus(tag)
                    .is_some_and(|handle| handle.is_focused(window))
            })
        })
        .unwrap()
}

/// The tag list by keyboard alone, as the browser gives the Solid list: Tab to a button
/// below the list's edge scrolls it into view, and Page Down, End, Home and the arrows
/// on a tag scroll the list; reaching its end loads the next page.
#[gpui::test]
fn the_keyboard_reaches_every_tag_by_focus_and_scroll_keys(cx: &mut TestAppContext) {
    let tags: Vec<String> = (0..250)
        .map(|index| format!("synthetic-tag-number-{index:03}-with-a-longer-name"))
        .collect();
    let fixture = Fixture::paged(tags.clone(), 100);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    assert_eq!(fixture.page_offsets(), [0]);

    let mut scrolled = false;
    for step in 0..40 {
        cx.simulate_keystrokes("tab");
        settle(&fixture, &mut cx);
        let index = focused_chip_index(&window, &tags, &mut cx).expect("a tag has the focus");
        assert_eq!(index, step, "Tab reaches the tags in order");
        let list = cx
            .debug_bounds(tag_list::LIST_SELECTOR)
            .expect("the list is drawn");
        let chip = cx.debug_bounds(chip(index)).expect("the tag is drawn");
        assert!(
            chip.top() >= list.top() && chip.bottom() <= list.bottom(),
            "tag {index} is in view"
        );
        scrolled |= list_scroll_top(&window, &mut cx) > 0.0;
    }
    assert!(scrolled, "the focus moved past the first screen of tags");
    let focused = focused_chip_index(&window, &tags, &mut cx);

    let before = list_scroll_top(&window, &mut cx);
    cx.simulate_keystrokes("pagedown");
    settle(&fixture, &mut cx);
    let paged = list_scroll_top(&window, &mut cx);
    assert!(paged > before, "Page Down scrolls the list");
    cx.simulate_keystrokes("up");
    settle(&fixture, &mut cx);
    assert!(list_scroll_top(&window, &mut cx) < paged, "Up scrolls back");
    assert_eq!(
        focused_chip_index(&window, &tags, &mut cx),
        focused,
        "the keys move the list, not the focus"
    );

    cx.simulate_keystrokes("end");
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    assert_eq!(
        fixture.page_offsets(),
        [0, 100],
        "the end of the list loads the next page"
    );
    cx.simulate_keystrokes("home");
    settle(&fixture, &mut cx);
    assert_eq!(list_scroll_top(&window, &mut cx), 0.0);
    assert!(is_open(&window, &mut cx));
}

/// Synthetic tag names of mixed lengths, as a real vocabulary has.
fn synthetic_vocabulary(count: usize) -> Vec<String> {
    const WORDS: [&str; 8] = [
        "sky",
        "portrait",
        "landscape-evening",
        "macro",
        "black-and-white-film",
        "street",
        "architecture-detail",
        "pet",
    ];
    (0..count)
        .map(|index| format!("{}-{index:05}", WORDS[index % WORDS.len()]))
        .collect()
}

/// The bounds of the list of tags, from its scroll state: a release build, which the
/// measurement runs in, records no debug selector.
fn list_bounds(
    window: &WindowHandle<ShellView>,
    cx: &mut VisualTestContext,
) -> gpui::Bounds<gpui::Pixels> {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let list = gallery.search().read(cx).tag_list().read(cx);
            list.scroll_handle().bounds()
        })
        .unwrap()
}

/// Opens the tag list as the launcher does, without its debug selector.
fn open_list(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            let runtime = gallery.search().read(cx).runtime().clone();
            runtime.update(cx, |runtime, cx| {
                runtime.send(
                    media_tagger_client::runtime::RuntimeInput::TagList(
                        media_tagger_client::search::tag_list::TagListInput::Open,
                    ),
                    cx,
                )
            });
        })
        .unwrap();
}

/// Scrolls the list of tags by `delta` pixels (a negative delta scrolls down).
fn wheel_list(window: &WindowHandle<ShellView>, delta: f32, cx: &mut VisualTestContext) {
    let list = list_bounds(window, cx);
    assert!(list.size.height > px(0.0), "the list is drawn");
    cx.simulate_event(ScrollEvent {
        position: list.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
        modifiers: Modifiers::none(),
        ..Default::default()
    });
}

/// Mean, p95 and max of `frames`, in milliseconds.
fn frame_stats(frames: &mut [Duration]) -> (f64, f64, f64) {
    frames.sort_unstable();
    let count = frames.len();
    let total: Duration = frames.iter().sum();
    (
        total.as_secs_f64() * 1_000.0 / count as f64,
        frames[count * 95 / 100].as_secs_f64() * 1_000.0,
        frames[count - 1].as_secs_f64() * 1_000.0,
    )
}

/// Review findings r2-perf-1, r4-gpui-3 and r6-gpui-1: prints the time of a frame of
/// the tag list with about 1 000, 5 000 and 20 000 loaded tags, while the wheel scrolls
/// the list and while the pointer moves over its tags. A frame is the event, the work
/// it queues, and building, laying out, prepainting and painting the window on the
/// test platform (no GPU, and text shaped by gpui's `NoopTextSystem`). The pages are
/// loaded as a user loads them: by scrolling to the end of the list. Run it with
/// `cargo test --manifest-path src-gpui/Cargo.toml --locked --release -p media_tagger_gpui
/// --test tag_list_window frame_time -- --ignored --nocapture`.
#[gpui::test]
#[ignore = "a measurement; run it on its own with --ignored --nocapture"]
fn frame_time_while_scrolling_and_hovering_a_long_tag_list(cx: &mut TestAppContext) {
    const FRAMES: usize = 300;
    let fixture = Fixture::paged(synthetic_vocabulary(20_000), 100);
    let (window, mut cx) = open(&fixture, cx);
    open_list(&window, &mut cx);
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    for size in [1_000, 5_000, 20_000] {
        let load_started = Instant::now();
        while shown(&window, &mut cx).len() < size {
            let before = fixture.page_offsets().len();
            wheel_list(&window, -10_000_000.0, &mut cx);
            for _ in 0..3 {
                settle(&fixture, &mut cx);
            }
            assert!(fixture.page_offsets().len() > before, "a page loads");
        }
        let loading = load_started.elapsed();
        let tags = shown(&window, &mut cx).len();
        wheel_list(&window, 10_000_000.0, &mut cx);
        settle(&fixture, &mut cx);
        assert_eq!(list_scroll_top(&window, &mut cx), 0.0);
        let pages = fixture.page_offsets().len();

        // A redraw with nothing changed.
        let mut idle = Vec::with_capacity(FRAMES);
        for _ in 0..FRAMES {
            let started = Instant::now();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            idle.push(started.elapsed());
        }

        // The wheel: 40 px a frame, down for 50 frames and up again, far from the end.
        let mut scroll = Vec::with_capacity(FRAMES);
        for frame in 0..FRAMES {
            let delta = if (frame / 50) % 2 == 0 { -40.0 } else { 40.0 };
            let started = Instant::now();
            wheel_list(&window, delta, &mut cx);
            fixture.executor.run_all();
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            scroll.push(started.elapsed());
        }

        // The pointer over the tags: every frame it lands somewhere else in the list, so
        // a tag's hover starts or ends.
        let list = list_bounds(&window, &mut cx);
        let (width, height) = (f32::from(list.size.width), f32::from(list.size.height));
        let mut hover = Vec::with_capacity(FRAMES);
        for frame in 0..FRAMES {
            let position = point(
                list.left() + px(8.0 + (frame * 37) as f32 % (width - 16.0)),
                list.top() + px(8.0 + (frame * 13) as f32 % (height - 16.0)),
            );
            let started = Instant::now();
            cx.simulate_mouse_move(position, None, Modifiers::none());
            fixture.executor.run_all();
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            hover.push(started.elapsed());
        }
        assert_eq!(
            fixture.page_offsets().len(),
            pages,
            "the frames load nothing"
        );
        let drawn = drawn_chips(&window, &mut cx);

        for (name, frames) in [
            ("redraw", &mut idle),
            ("scroll", &mut scroll),
            ("hover", &mut hover),
        ] {
            let (mean, p95, max) = frame_stats(frames);
            println!(
                "tag list, {tags} tags loaded ({:.1} s to load), {drawn} buttons drawn, {name}: \
                 mean {mean:.2} ms, p95 {p95:.2} ms, max {max:.2} ms over {FRAMES} frames",
                loading.as_secs_f64(),
            );
        }
    }
}

fn drawn_chips(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> usize {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).tag_list().read(cx).drawn_chips()
        })
        .unwrap()
}

/// Review findings r2-perf-1, r4-gpui-3 and r6-gpui-1: a frame builds the buttons of
/// the rows near the visible part of the list only, so their number does not grow with
/// the loaded tags; scrolling draws the rows it reaches, the last tag included.
#[gpui::test]
fn a_frame_draws_the_rows_in_view_whatever_the_number_of_loaded_tags(cx: &mut TestAppContext) {
    let tags = synthetic_vocabulary(1_000);
    let fixture = Fixture::paged(tags.clone(), 100);
    let (window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    assert_eq!(shown(&window, &mut cx).len(), 100);
    let first_page = drawn_chips(&window, &mut cx);
    assert!(
        first_page > 0 && first_page < 100,
        "{first_page} of 100 tags drawn"
    );

    while shown(&window, &mut cx).len() < tags.len() {
        let before = fixture.page_offsets().len();
        wheel_list(&window, -1_000_000.0, &mut cx);
        for _ in 0..3 {
            settle(&fixture, &mut cx);
        }
        assert!(fixture.page_offsets().len() > before, "a page loads");
    }
    wheel_list(&window, -1_000_000.0, &mut cx);
    settle(&fixture, &mut cx);
    assert!(
        cx.debug_bounds(chip(999)).is_some(),
        "the end of the list is drawn"
    );
    assert!(cx.debug_bounds(chip(0)).is_none());
    let at_the_end = drawn_chips(&window, &mut cx);
    assert!(at_the_end < 100, "{at_the_end} of 1000 tags drawn");

    wheel_list(&window, 1_000_000.0, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(list_scroll_top(&window, &mut cx), 0.0);
    assert!(cx.debug_bounds(chip(0)).is_some());
    assert!(cx.debug_bounds(chip(999)).is_none());
    assert_eq!(
        drawn_chips(&window, &mut cx),
        first_page,
        "the top of 1000 loaded tags draws what the top of 100 did"
    );

    // Hovering and clicking a drawn tag still work.
    let point = cx.debug_bounds(chip(1)).expect("tag 1 is drawn").center();
    cx.simulate_mouse_move(point, None, Modifiers::none());
    cx.simulate_click(point, Modifiers::none());
    settle(&fixture, &mut cx);
    fixture.executor.advance_and_run(SINGLE_CLICK_DELAY);
    settle(&fixture, &mut cx);
    assert_eq!(
        mode(&window, &tags[1], &mut cx),
        Some(TagSelectionMode::Include)
    );
}

/// The rows the view packs are the rows flex-wrap makes (`flex flex-wrap gap-1.5`):
/// every drawn button stays inside the list, buttons of a row are one gap apart, a
/// button starts a new row only when it would not fit after the last one, rows are one
/// gap apart, and a tag wider than the list takes a taller row of its own.
#[gpui::test]
fn the_drawn_rows_are_the_rows_of_flex_wrap(cx: &mut TestAppContext) {
    let mut tags = synthetic_vocabulary(60);
    tags[7] = format!("long-{}", "x".repeat(150));
    let fixture = Fixture::paged(tags.clone(), 100);
    let (_window, mut cx) = open(&fixture, cx);
    click(&fixture, tag_list::LAUNCHER_ID, &mut cx);
    for _ in 0..3 {
        settle(&fixture, &mut cx);
    }
    let list = cx
        .debug_bounds(tag_list::LIST_SELECTOR)
        .expect("the list is drawn");
    let padding = px(media_tagger_client::search::tag_list::layout::dialog::LIST_PADDING);
    let gap = px(media_tagger_client::search::tag_list::layout::chip::GAP);
    let (left, right) = (list.left() + padding, list.right() - padding);
    let drawn: Vec<_> = (0..tags.len())
        .map_while(|index| cx.debug_bounds(chip(index)))
        .collect();
    assert!(drawn.len() > 12, "{} tags drawn", drawn.len());
    let one_line = drawn[0].size.height;
    let long = drawn[7];
    assert_eq!(
        (long.left(), long.right()),
        (left, right),
        "as wide as the list"
    );
    assert!(long.size.height > one_line, "a long tag wraps");
    for pair in drawn.windows(2) {
        let (before, after) = (pair[0], pair[1]);
        assert!(
            after.left() >= left && after.right() <= right,
            "inside the list"
        );
        if after.top() == before.top() {
            assert_eq!(after.left(), before.right() + gap, "one gap apart in a row");
        } else {
            assert_eq!(after.left(), left, "a row starts at the left edge");
            assert!(
                before.right() + gap + after.size.width > right,
                "a button wraps only when it does not fit"
            );
            assert_eq!(after.top(), before.bottom() + gap, "rows are one gap apart");
        }
    }
}
