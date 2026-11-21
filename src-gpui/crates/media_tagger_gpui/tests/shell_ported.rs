//! The main window's shell (epic E06) ported from the Solid specification
//! (`src/app/__tests__/shell.test.tsx`, `WindowControls.test.tsx`, `bootstrap.test.tsx`)
//! and the acceptance criteria a view has to prove: clicks, keys and focus on GPUI's
//! test platform, with the scripted backend on the deterministic executor.

use std::sync::Arc;

use gpui::{
    Focusable as _, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    WindowHandle,
};
use media_tagger_client::{
    backend::FakeBackend,
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    prefs::{MemoryPrefsStore, Prefs, PrefsStore},
    runtime::{RuntimeConfig, RuntimeInput, View, DEFAULT_TILE_SIZE, TILE_SIZE_STEP},
    theme::ThemeName,
    window_chrome::ResizeDirection,
};
use media_tagger_core::models::StartAssetQueryResult;
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui,
    views::{chrome, gallery, settings, shell::ShellView},
    window::{init_application, open_main_window, WINDOW_SIZE},
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    prefs: Arc<MemoryPrefsStore>,
}

impl Fixture {
    /// An empty library: no assets, no roots, no startup work.
    fn empty() -> Self {
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
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| Ok(Vec::new()));
        Self {
            backend,
            executor: Arc::new(ManualExecutor::new()),
            prefs: Arc::new(MemoryPrefsStore::default()),
        }
    }

    fn parts(&self, theme: ThemeName) -> RuntimeParts {
        RuntimeParts {
            config: RuntimeConfig {
                theme,
                language: Language::En,
            },
            backend: self.backend.clone(),
            prefs: self.prefs.clone(),
            executor: self.executor.clone() as Arc<dyn Executor>,
        }
    }

    fn calls(&self, method: &str) -> usize {
        self.backend
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }
}

fn open(
    fixture: &Fixture,
    theme: ThemeName,
    cx: &mut TestAppContext,
) -> (WindowHandle<ShellView>, VisualTestContext) {
    let window = cx
        .update(|cx| {
            init_application(theme, cx)?;
            open_main_window(profile::E2E, fixture.parts(theme), cx)
        })
        .expect("open the main window");
    let cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    (window, cx)
}

/// Runs the backend calls and lets the replies reach the UI thread.
fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    loop {
        cx.run_until_parked();
        if fixture.executor.run_all() == 0 {
            break;
        }
    }
    cx.run_until_parked();
}

fn open_settled(
    fixture: &Fixture,
    cx: &mut TestAppContext,
) -> (WindowHandle<ShellView>, VisualTestContext) {
    let (window, mut cx) = open(fixture, ThemeName::Dark, cx);
    settle(fixture, &mut cx);
    (window, cx)
}

fn view(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> View {
    window.read_with(cx, |shell, _| shell.view()).unwrap()
}

fn send(window: &WindowHandle<ShellView>, input: RuntimeInput, cx: &mut VisualTestContext) {
    let runtime = window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .unwrap();
    runtime.update(cx, |runtime, cx| runtime.send(input, cx));
    cx.run_until_parked();
}

fn center(selector: &'static str, cx: &mut VisualTestContext) -> Point<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"))
        .center()
}

fn click(selector: &'static str, cx: &mut VisualTestContext) {
    let position = center(selector, cx);
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}

/// Escape, then the frame in which the layer stack hands the focus back.
fn escape(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    cx.run_until_parked();
}

fn highlights_scan(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> bool {
    window
        .read_with(cx, |shell, cx| {
            shell
                .settings()
                .expect("the settings page is shown")
                .read(cx)
                .highlights_scan(cx)
        })
        .unwrap()
}

fn grips_drawn(cx: &mut VisualTestContext) -> usize {
    ResizeDirection::ALL
        .into_iter()
        .filter(|direction| {
            let selector: &'static str =
                Box::leak(chrome::grip_selector(*direction).into_boxed_str());
            cx.debug_bounds(selector).is_some()
        })
        .count()
}

/// shell.test.tsx "highlights scanning only for the first-folder route and clears it
/// on Back", through the buttons a user clicks.
#[gpui::test]
fn add_first_folder_highlights_scanning_until_back(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open_settled(&fixture, cx);

    click(gallery::ADD_FIRST_FOLDER_ID, &mut cx);
    assert_eq!(view(&window, &cx), View::Settings);
    assert!(highlights_scan(&window, &cx));

    click(settings::BACK_ID, &mut cx);
    assert_eq!(view(&window, &cx), View::Gallery);
    click(gallery::OPEN_SETTINGS_ID, &mut cx);
    assert_eq!(view(&window, &cx), View::Settings);
    assert!(!highlights_scan(&window, &cx));
}

/// shell.test.tsx "switches views, translates live, persists appearance and returns
/// with Escape": the change of theme and language is live in the window, Escape goes
/// back to the gallery, whose new view speaks the new language, and reopening the
/// settings page keeps both. (The controls that make the change are the settings
/// content of E17; the runtime command is what they send.)
#[gpui::test]
fn appearance_changes_are_live_and_survive_escape_and_reopening(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open_settled(&fixture, cx);

    click(gallery::OPEN_SETTINGS_ID, &mut cx);
    assert!(cx.debug_bounds(settings::HEADING_SELECTOR).is_some());
    send(&window, RuntimeInput::SetTheme(ThemeName::Light), &mut cx);
    assert_eq!(cx.update(|_, cx| ui::theme::active(cx)), ThemeName::Light);
    send(&window, RuntimeInput::SetLanguage(Language::Pl), &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(
        fixture.prefs.load(),
        Prefs::default()
            .with_theme(ThemeName::Light)
            .with_language(Language::Pl)
    );

    escape(&mut cx);
    assert_eq!(view(&window, &cx), View::Gallery);
    let label = window
        .read_with(&cx, |shell, cx| {
            shell
                .gallery()
                .unwrap()
                .read(cx)
                .tile_size_slider()
                .read(cx)
                .label()
                .to_string()
        })
        .unwrap();
    assert_eq!(label, I18n::new(Language::Pl).t("controls.tileSize"));
    assert_ne!(label, I18n::new(Language::En).t("controls.tileSize"));

    click(gallery::OPEN_SETTINGS_ID, &mut cx);
    let (theme, language) = window
        .read_with(&cx, |shell, cx| {
            let runtime = shell.runtime().read(cx).model();
            (runtime.theme(), runtime.language())
        })
        .unwrap();
    assert_eq!((theme, language), (ThemeName::Light, Language::Pl));
    assert_eq!(cx.update(|_, cx| ui::theme::active(cx)), ThemeName::Light);
}

/// shell.test.tsx "restores focus to the settings opener after Escape (bulk mode
/// false)": opened by a click, the focus moved on to Back, Escape returns to the
/// gallery with the focus on the settings button.
#[gpui::test]
fn escape_from_back_returns_the_focus_to_the_settings_button(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open_settled(&fixture, cx);
    let opener = window
        .read_with(&cx, |shell, _| shell.settings_opener().clone())
        .unwrap();

    click(gallery::OPEN_SETTINGS_ID, &mut cx);
    assert_eq!(view(&window, &cx), View::Settings);
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let focused = window
            .focused(cx)
            .expect("Tab focuses a control of the page");
        assert!(focused != opener, "the opener is not on the settings page");
    });

    escape(&mut cx);

    assert_eq!(view(&window, &cx), View::Gallery);
    cx.update(|window, _| assert!(opener.is_focused(window), "focus is back on the opener"));
    assert!(cx.update(|_, cx| ui::layers::layer_ids(cx)).is_empty());
}

/// shell.test.tsx "applies theme before readiness" and acceptance "the startup scan
/// does not block the window": before the backend has answered anything the window is
/// drawn in its theme, and while the startup scan still runs the views switch and the
/// gallery is shown; the scan is asked once and finishes afterwards.
#[gpui::test]
fn the_window_is_drawn_and_usable_before_the_startup_scan_answers(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open(&fixture, ThemeName::Light, cx);

    assert_eq!(cx.update(|_, cx| ui::theme::active(cx)), ThemeName::Light);
    assert_eq!(
        fixture.executor.pending(),
        4,
        "library, scan, roots, preferences"
    );
    assert!(cx.debug_bounds(gallery::HEADER_SELECTOR).is_some());
    assert!(cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).is_some());

    // Everything but the scan answers (the scan is the second job).
    assert!(fixture.executor.run_nth(0));
    assert!(fixture.executor.run_nth(1));
    assert!(fixture.executor.run_nth(1));
    cx.run_until_parked();
    let loading = |cx: &mut VisualTestContext| {
        window
            .read_with(cx, |shell, cx| {
                shell.runtime().read(cx).model().maintenance_loading()
            })
            .unwrap()
    };
    assert!(loading(&mut cx));

    click(gallery::OPEN_SETTINGS_ID, &mut cx);
    assert_eq!(view(&window, &cx), View::Settings);
    escape(&mut cx);
    assert_eq!(view(&window, &cx), View::Gallery);
    assert!(cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).is_some());
    assert_eq!(fixture.calls("scan_startup_roots"), 0);

    settle(&fixture, &mut cx);
    assert!(!loading(&mut cx));
    assert_eq!(fixture.calls("scan_startup_roots"), 1);
}

/// Acceptance "views lose only their local state": the tile size set with the slider's
/// keys lives in the runtime, so the gallery made anew after the settings page shows it.
#[gpui::test]
fn the_tile_size_slider_drives_the_runtime_and_survives_a_view_switch(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open_settled(&fixture, cx);
    let slider = window
        .read_with(&cx, |shell, cx| {
            shell
                .gallery()
                .unwrap()
                .read(cx)
                .tile_size_slider()
                .read(cx)
                .focus_handle(cx)
        })
        .unwrap();
    cx.update(|window, cx| window.focus(&slider, cx));
    cx.run_until_parked();

    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    let expected = DEFAULT_TILE_SIZE + TILE_SIZE_STEP;
    let size = |window: &WindowHandle<ShellView>, cx: &VisualTestContext| {
        window
            .read_with(cx, |shell, cx| shell.runtime().read(cx).model().tile_size())
            .unwrap()
    };
    assert_eq!(size(&window, &cx), expected);

    click(gallery::OPEN_SETTINGS_ID, &mut cx);
    click(settings::BACK_ID, &mut cx);
    let value = window
        .read_with(&cx, |shell, cx| {
            shell
                .gallery()
                .unwrap()
                .read(cx)
                .tile_size_slider()
                .read(cx)
                .value()
        })
        .unwrap();
    assert_eq!(value, f64::from(expected));
    assert_eq!(size(&window, &cx), expected);
}

/// WindowControls.test.tsx "follows externally changed maximized/fullscreen state",
/// fullscreen half (GPUI's test window is never maximised): a fullscreen window has no
/// resize grips, and they come back when it leaves fullscreen.
#[gpui::test]
fn fullscreen_removes_the_resize_grips_until_it_ends(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (_window, mut cx) = open_settled(&fixture, cx);
    let size = gpui::size(gpui::px(WINDOW_SIZE.0), gpui::px(WINDOW_SIZE.1));
    assert_eq!(grips_drawn(&mut cx), 8);

    cx.update(|window, _| window.toggle_fullscreen());
    cx.simulate_resize(size);
    cx.run_until_parked();
    assert_eq!(grips_drawn(&mut cx), 0);

    cx.update(|window, _| window.toggle_fullscreen());
    cx.simulate_resize(size);
    cx.run_until_parked();
    assert_eq!(grips_drawn(&mut cx), 8);
}

/// WindowControls.test.tsx "a secondary press on a resize edge starts nothing", and
/// the drag region's rule that a press on a header control is no window drag. GPUI's
/// test window panics on a window move or zoom, so these gestures passing is the proof
/// that none was asked for.
#[gpui::test]
fn secondary_presses_and_presses_on_header_controls_never_move_the_window(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open_settled(&fixture, cx);
    let header = center(gallery::SEARCH_SLOT_SELECTOR, &mut cx);
    let away = Point::new(header.x + gpui::px(40.), header.y);

    // A secondary press and drag on the header and on a grip.
    let grip = Box::leak(chrome::grip_selector(ResizeDirection::SouthEast).into_boxed_str());
    for position in [header, center(grip, &mut cx)] {
        cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_move(away, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(away, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
    }

    // A primary press on the settings button, then a drag with the button held.
    let button = center(gallery::OPEN_SETTINGS_ID, &mut cx);
    cx.simulate_mouse_down(button, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(header, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(header, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(view(&window, &cx), View::Gallery);
}

/// The drag region (`data-tauri-drag-region`, D14): a primary press on the header and a
/// move with the button held hand the window to the window manager. GPUI's test window
/// answers a move with `unimplemented!`, which is how the request is seen here.
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn a_primary_drag_on_the_header_starts_a_window_move(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (_window, mut cx) = open_settled(&fixture, cx);
    let header = center(gallery::SEARCH_SLOT_SELECTOR, &mut cx);

    cx.simulate_mouse_down(header, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        Point::new(header.x + gpui::px(30.), header.y),
        MouseButton::Left,
        Modifiers::none(),
    );
}

/// The drag region: a double click on the header maximises or restores, as a native
/// title bar does. GPUI's test window answers a zoom with `unimplemented!`.
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn a_double_click_on_the_header_toggles_maximize(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (_window, mut cx) = open_settled(&fixture, cx);
    let header = center(gallery::SEARCH_SLOT_SELECTOR, &mut cx);

    cx.simulate_event(gpui::MouseDownEvent {
        position: header,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
}

/// WindowControls.test.tsx "runs native actions": the minimise button asks the window
/// to minimise (GPUI's test window answers with `unimplemented!`).
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn the_minimize_button_minimizes_the_window(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (_window, mut cx) = open_settled(&fixture, cx);

    click(
        chrome::control_selector(media_tagger_client::window_chrome::WindowControl::Minimize),
        &mut cx,
    );
}

/// WindowControls.test.tsx "runs native actions": the maximise button asks the window
/// to maximise (GPUI's test window answers with `unimplemented!`).
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn the_maximize_button_maximizes_the_window(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (_window, mut cx) = open_settled(&fixture, cx);

    click(
        chrome::control_selector(media_tagger_client::window_chrome::WindowControl::ToggleMaximize),
        &mut cx,
    );
}

/// shell.test.tsx "suppresses context menus": a secondary click anywhere in the window
/// opens no menu or layer and changes no view.
#[gpui::test]
fn a_secondary_click_opens_no_menu(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open_settled(&fixture, cx);

    for selector in [
        gallery::EMPTY_STATE_SELECTOR,
        gallery::COUNT_SELECTOR,
        gallery::HEADER_SELECTOR,
    ] {
        let position = center(selector, &mut cx);
        cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.update(|_, cx| ui::layers::layer_ids(cx)).is_empty(),
            "{selector}"
        );
        assert_eq!(view(&window, &cx), View::Gallery);
    }
}

/// bootstrap.test.tsx "renders the bootstrap": the window opens on the gallery with the
/// identity, the settings button and the count in its header.
#[gpui::test]
fn the_window_opens_on_the_gallery_with_its_header(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open_settled(&fixture, cx);

    assert_eq!(view(&window, &cx), View::Gallery);
    let header = cx.debug_bounds(gallery::HEADER_SELECTOR).unwrap();
    for selector in [gallery::OPEN_SETTINGS_ID, gallery::COUNT_SELECTOR] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(
            header.contains(&bounds.origin) && header.contains(&bounds.bottom_right()),
            "{selector} lies in the header"
        );
    }
    let settings_button = cx.debug_bounds(gallery::OPEN_SETTINGS_ID).unwrap();
    let close = cx
        .debug_bounds(chrome::control_selector(
            media_tagger_client::window_chrome::WindowControl::Close,
        ))
        .unwrap();
    assert!(
        settings_button.right() <= close.left(),
        "the window buttons close the header row"
    );
}
