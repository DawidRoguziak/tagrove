//! The search header in the window (epic E12): typing opens the suggestion list, keys
//! and the pointer pick a suggestion, Enter and the controls submit, Ctrl+K focuses the
//! field, Escape closes the list, a refused submit shows its message. The scripted
//! backend runs on the deterministic executor, so the suggestion searches do too.

use std::sync::Arc;

use gpui::{
    Focusable as _, HeadlessAppContext, Modifiers, TestAppContext, VisualTestContext, WindowHandle,
};
use media_tagger_client::{
    backend::{fake::Call, FakeBackend, MediaKind, MetaFilter},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::RuntimeConfig,
    scenario::ScenarioSearch,
    search::SearchFilterValidationError,
    theme::ThemeName,
};
use media_tagger_core::models::{StartAssetQueryResult, TagListPage};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui::assets::Assets,
    views::{search, shell::ShellView},
    window::{
        init_application, open_main_window, open_main_window_started, ShellStart, WINDOW_SIZE,
    },
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

    /// The query requests the backend got, oldest first.
    fn queries(&self) -> Vec<media_tagger_client::backend::AssetQueryRequest> {
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
    // Focus events reach their listeners only in an active window.
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

fn text(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) -> String {
    window
        .update(cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).input().read(cx).text().to_owned()
        })
        .unwrap()
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

fn options(cx: &mut VisualTestContext) -> usize {
    (0..10)
        .take_while(|index| {
            let selector: &'static str =
                Box::leak(search::option_selector(*index).into_boxed_str());
            cx.debug_bounds(selector).is_some()
        })
        .count()
}

fn type_text(fixture: &Fixture, text: &str, cx: &mut VisualTestContext) {
    cx.simulate_input(text);
    settle(fixture, cx);
}

fn key(fixture: &Fixture, keystroke: &str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(keystroke);
    settle(fixture, cx);
}

#[gpui::test]
fn ctrl_k_focuses_the_field_and_typing_opens_the_suggestions(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera", "car", "cat", "dog"]);
    let (window, mut cx) = open(&fixture, cx);
    assert!(!field_focused(&window, &mut cx));

    key(&fixture, "ctrl-k", &mut cx);
    assert!(field_focused(&window, &mut cx));
    assert!(cx.debug_bounds(search::LIST_SELECTOR).is_none());

    type_text(&fixture, "ca", &mut cx);
    assert!(cx.debug_bounds(search::LIST_SELECTOR).is_some());
    assert_eq!(options(&mut cx), 3);
    assert_eq!(fixture.queries().len(), 1, "typing applies nothing");

    key(&fixture, "escape", &mut cx);
    assert!(cx.debug_bounds(search::LIST_SELECTOR).is_none());
    assert!(
        field_focused(&window, &mut cx),
        "Escape keeps the focus in the field"
    );
    assert_eq!(text(&window, &mut cx), "ca");
}

#[gpui::test]
fn arrows_and_enter_pick_a_suggestion_and_a_second_enter_submits(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera", "car", "cat"]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    type_text(&fixture, "dog -ca", &mut cx);
    assert_eq!(options(&mut cx), 3);

    key(&fixture, "down", &mut cx);
    key(&fixture, "enter", &mut cx);
    assert_eq!(text(&window, &mut cx), "dog -car");
    assert!(cx.debug_bounds(search::LIST_SELECTOR).is_none());
    assert_eq!(fixture.queries().len(), 1);

    key(&fixture, "enter", &mut cx);
    let query = fixture.queries().pop().expect("a query");
    assert_eq!(fixture.queries().len(), 2);
    assert_eq!(
        (query.tags_and, query.tags_not),
        (vec!["dog".to_owned()], vec!["car".to_owned()])
    );
}

#[gpui::test]
fn a_press_on_a_suggestion_picks_it_and_keeps_the_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera", "car", "cat"]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    type_text(&fixture, "ca", &mut cx);
    let third = cx
        .debug_bounds(Box::leak(search::option_selector(2).into_boxed_str()))
        .expect("the third suggestion")
        .center();
    cx.simulate_click(third, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(text(&window, &mut cx), "cat");
    assert!(field_focused(&window, &mut cx));
}

#[gpui::test]
fn the_controls_submit_and_clear_resets_the_header(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&[]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    type_text(&fixture, "tags:2", &mut cx);
    let submit = cx.debug_bounds(search::SUBMIT_ID).expect("Search").center();
    cx.simulate_click(submit, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(
        fixture.queries().pop().expect("a query").meta_filter,
        Some(MetaFilter::HasNoTags { tag_count: 2 })
    );

    let favorites = cx
        .debug_bounds(search::FAVORITES_ID)
        .expect("favourites")
        .center();
    cx.simulate_click(favorites, Modifiers::none());
    settle(&fixture, &mut cx);
    let query = fixture.queries().pop().expect("a query");
    assert!(query.favorites_only);
    assert_eq!(
        query.meta_filter,
        Some(MetaFilter::HasNoTags { tag_count: 2 })
    );

    window
        .update(&mut cx, |shell, _, cx| {
            let gallery = shell.gallery().expect("the gallery").read(cx);
            gallery.search().read(cx).kind_control().clone()
        })
        .unwrap()
        .update(&mut cx, |control, cx| {
            cx.emit(media_tagger_gpui::ui::segmented::SegmentedEvent::Change(
                "video".into(),
            ));
            let _ = control;
        });
    settle(&fixture, &mut cx);
    assert_eq!(
        fixture.queries().pop().expect("a query").kind,
        Some(MediaKind::Video)
    );

    let clear = cx.debug_bounds(search::CLEAR_ID).expect("Clear").center();
    cx.simulate_click(clear, Modifiers::none());
    settle(&fixture, &mut cx);
    let query = fixture.queries().pop().expect("a query");
    assert_eq!(
        (query.kind, query.favorites_only, query.meta_filter),
        (None, false, None)
    );
    assert_eq!(text(&window, &mut cx), "");
    assert!(
        field_focused(&window, &mut cx),
        "Clear gives the focus back to the field"
    );
}

#[gpui::test]
fn a_refused_submit_shows_its_message_until_the_draft_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&[]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    type_text(&fixture, "gN:", &mut cx);
    key(&fixture, "enter", &mut cx);
    assert!(cx.debug_bounds(search::VALIDATION_SELECTOR).is_some());
    assert_eq!(fixture.queries().len(), 1);
    let error = window
        .update(&mut cx, |shell, _, cx| {
            shell.runtime().read(cx).model().search().validation_error()
        })
        .unwrap();
    assert_eq!(
        error,
        Some(SearchFilterValidationError::GroupNameMissingValue)
    );
    type_text(&fixture, "x", &mut cx);
    assert!(cx.debug_bounds(search::VALIDATION_SELECTOR).is_none());
}

#[gpui::test]
fn operators_and_group_names_open_no_list_and_enter_submits_them(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["tags", "tagstone", "trip-2026"]);
    let (window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    type_text(&fixture, "gN:Trip 2026", &mut cx);
    assert!(cx.debug_bounds(search::LIST_SELECTOR).is_none());
    key(&fixture, "enter", &mut cx);
    assert_eq!(text(&window, &mut cx), "gN:Trip 2026");
    assert_eq!(
        fixture.queries().pop().expect("a query").meta_filter,
        Some(MetaFilter::GroupName {
            group_name: "trip 2026".into()
        })
    );
}

/// The `search-suggestions` state through the headless renderer: the list below the
/// field, its first option in the active colours. Skipped without a GPU or software
/// driver; `MEDIATAGGER_GPUI_SCREENSHOT_DIR` keeps the picture.
#[test]
fn the_suggestion_list_renders_below_the_field() {
    if gpui_platform::current_headless_renderer().is_err() {
        if std::env::var_os("MEDIATAGGER_GPUI_REQUIRE_RENDERER").is_some() {
            panic!("no headless renderer");
        }
        eprintln!("skipped: no GPU adapter for the headless renderer");
        return;
    }
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), || {
        gpui_platform::current_headless_renderer()
    });
    let fixture = Fixture::new(&["camera", "car", "cat", "dog"]);
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window_started(
                profile::E2E,
                fixture.parts(),
                ShellStart {
                    search: Some(ScenarioSearch {
                        input: "ca",
                        kind: None,
                        focus: true,
                    }),
                    ..ShellStart::default()
                },
                cx,
            )
        })
        .expect("open the main window");
    cx.update(|cx| {
        window
            .update(cx, |_, window, _| window.activate_window())
            .ok();
    });
    for _ in 0..4 {
        loop {
            cx.run_until_parked();
            if fixture.executor.run_all() == 0 {
                break;
            }
        }
    }
    let image = cx
        .capture_screenshot(window.into())
        .expect("capture the window");
    if let Some(dir) = std::env::var_os("MEDIATAGGER_GPUI_SCREENSHOT_DIR") {
        let target = std::path::PathBuf::from(dir).join("search-suggestions-headless.png");
        image.save(&target).expect("write the screenshot");
        eprintln!("screenshot: {}", target.display());
    }
    let open = cx.update(|cx| {
        window
            .read_with(cx, |shell, cx| {
                let gallery = shell.gallery().expect("the gallery").read(cx);
                let combobox = gallery.search().read(cx).combobox();
                (combobox.popup(), combobox.items().len())
            })
            .expect("the window is open")
    });
    assert_eq!(open, (true, 3));
    // The first option is active: its fill is the active suggestion colour, just
    // inside the list below the field (the field's frame starts at x 216, the input
    // after the magnifier).
    let active = ThemeName::Dark.tokens().colors.tagator_suggestion_active_bg;
    let scale = image.width() as f32 / WINDOW_SIZE.0;
    let pixel = image
        .get_pixel((600.0 * scale) as u32, (75.0 * scale) as u32)
        .0;
    assert_eq!(pixel, [active.r, active.g, active.b, 0xff]);
}

/// The `search-suggestions` start state in a window that never becomes active, as on
/// the comparison harness's display without a window manager: the scripted focus
/// still opens the list (WebKit sends `focusin` there too), and the frame keeps its
/// normal border until the window is active (`focus-within` needs an active window).
#[gpui::test]
fn a_scenario_focus_opens_the_list_in_an_inactive_window(cx: &mut TestAppContext) {
    let fixture = Fixture::new(&["camera", "car", "cat", "dog"]);
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window_started(
                profile::E2E,
                fixture.parts(),
                ShellStart {
                    search: Some(ScenarioSearch {
                        input: "ca",
                        kind: None,
                        focus: true,
                    }),
                    ..ShellStart::default()
                },
                cx,
            )
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    settle(&fixture, &mut cx);
    assert!(!cx.update(|window, _| window.is_window_active()));

    assert!(field_focused(&window, &mut cx));
    assert!(cx.debug_bounds(search::LIST_SELECTOR).is_some());
    assert_eq!(options(&mut cx), 3);
    let highlighted = |window: &WindowHandle<ShellView>, cx: &mut VisualTestContext| {
        window
            .update(cx, |shell, window, cx| {
                let gallery = shell.gallery().expect("the gallery").read(cx);
                gallery.search().read(cx).frame_highlighted(window, cx)
            })
            .unwrap()
    };
    assert!(!highlighted(&window, &mut cx));

    cx.update(|window, _| window.activate_window());
    settle(&fixture, &mut cx);
    assert!(highlighted(&window, &mut cx));
    assert_eq!(options(&mut cx), 3, "the activation keeps the list");
}

/// The input's box (and the list hanging below it) starts where the Tauri one does:
/// after the 20 pixel magnifier, 37 pixels into the field.
#[gpui::test]
fn the_input_starts_after_the_magnifier_as_in_the_tauri_header(cx: &mut TestAppContext) {
    // Measured in the Tauri capture: frame at x 216, input box and list at x 253.
    assert_eq!(media_tagger_client::search::layout::field::ICON_SIZE, 20.0);
    let fixture = Fixture::new(&["camera", "car", "cat"]);
    let (_window, mut cx) = open(&fixture, cx);
    key(&fixture, "ctrl-k", &mut cx);
    type_text(&fixture, "ca", &mut cx);

    let field = cx.debug_bounds(search::FIELD_SELECTOR).expect("the field");
    let input = cx.debug_bounds(search::INPUT_SELECTOR).expect("the input");
    let list = cx.debug_bounds(search::LIST_SELECTOR).expect("the list");
    // The Ctrl K hint is 16 pixels high (line height `normal`, not the root's 1.5).
    let hint = cx.debug_bounds(search::HINT_SELECTOR).expect("the hint");
    assert_eq!(f32::from(hint.size.height), 16.0);
    let offset = f32::from(input.origin.x - field.origin.x);
    assert_eq!(offset, 37.0);
    assert_eq!(
        offset,
        media_tagger_client::search::layout::field::input_left()
    );
    assert_eq!(list.origin.x, input.origin.x);
    assert_eq!(list.size.width, input.size.width);
}

/// The divider before the tag list's button is drawn: a one pixel line in the strong
/// border colour, 20 pixels high, in the middle of the tools row. Skipped without a
/// GPU or software driver.
#[test]
fn the_tools_divider_is_drawn() {
    if gpui_platform::current_headless_renderer().is_err() {
        if std::env::var_os("MEDIATAGGER_GPUI_REQUIRE_RENDERER").is_some() {
            panic!("no headless renderer");
        }
        eprintln!("skipped: no GPU adapter for the headless renderer");
        return;
    }
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), || {
        gpui_platform::current_headless_renderer()
    });
    let fixture = Fixture::new(&["camera"]);
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window(profile::E2E, fixture.parts(), cx)
        })
        .expect("open the main window");
    for _ in 0..4 {
        loop {
            cx.run_until_parked();
            if fixture.executor.run_all() == 0 {
                break;
            }
        }
    }
    let image = cx
        .capture_screenshot(window.into())
        .expect("capture the window");
    let scale = (image.width() as f32 / WINDOW_SIZE.0).round() as u32;
    let strong = ThemeName::Dark.tokens().colors.border_strong;
    let strong = [strong.r, strong.g, strong.b, 0xff];
    // The tools row runs from y 53 to 96; the favourites button ends at x 277 and
    // the divider follows a gap of 8 and its margin of 4 (x 289 or 290 with the
    // segmented control's text width). In device pixels the divider must fill its
    // whole logical pixel: a left border on a box that narrow is drawn on at most
    // its left half (and not at all at scale 1, as in the comparison capture).
    let columns: Vec<u32> = (280 * scale..300 * scale)
        .filter(|x| (66 * scale..83 * scale).all(|y| image.get_pixel(*x, y).0 == strong))
        .collect();
    assert_eq!(
        columns.len() as u32,
        scale,
        "one logical divider column, filled: {columns:?}"
    );
    let above = image.get_pixel(columns[0], 62 * scale).0;
    assert_ne!(
        above, strong,
        "the divider is 20 pixels high, not the row's height"
    );
}
