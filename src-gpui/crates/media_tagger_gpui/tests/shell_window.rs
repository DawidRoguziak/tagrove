//! The main window's shell (epic E06) on GPUI's test platform (no display, no GPU) with
//! the scripted backend and the deterministic executor, and through the in-process
//! headless renderer (skipped without a GPU or software driver).

use std::{path::PathBuf, sync::Arc};

use gpui::{
    div, font, px, Entity, EntityId, HeadlessAppContext, Styled as _, TestAppContext,
    VisualTestContext, WindowDecorations, WindowHandle,
};
use media_tagger_client::{
    backend::{BackendError, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    model::Model as _,
    prefs::{MemoryPrefsStore, Prefs, PrefsStore},
    runtime::{RuntimeConfig, RuntimeInput, View, DEFAULT_TILE_SIZE},
    theme::{ThemeName, TYPOGRAPHY},
    window_chrome::{ResizeDirection, WindowControl},
};
use media_tagger_core::models::{ScanCompletion, ScanSummary, StartAssetQueryResult};
use media_tagger_gpui::{
    profile,
    runtime::{Runtime, RuntimeParts},
    ui::{self, assets::Assets},
    views::{chrome, gallery, settings, shell::ShellView},
    window::{
        init_application, main_window_options, open_main_window, WINDOW_MIN_SIZE, WINDOW_SIZE,
    },
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

fn runtime(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<Runtime> {
    window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open")
}

fn send(window: &WindowHandle<ShellView>, input: RuntimeInput, cx: &mut VisualTestContext) {
    let runtime = runtime(window, cx);
    runtime.update(cx, |runtime, cx| runtime.send(input, cx));
    cx.run_until_parked();
}

fn gallery_id(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Option<EntityId> {
    window
        .read_with(cx, |shell, _| {
            shell.gallery().map(|gallery| gallery.entity_id())
        })
        .unwrap()
}

#[gpui::test]
fn each_profile_opens_a_window_with_its_own_title_and_client_decorations(cx: &mut TestAppContext) {
    for (profile, title, identifier) in [
        (
            profile::DEV,
            "Tagrove GPUI Dev",
            "com.example.mediatagger.gpui.dev",
        ),
        (
            profile::E2E,
            "Tagrove GPUI E2E",
            "com.example.mediatagger.gpui.e2e",
        ),
    ] {
        let options = cx.update(|cx| main_window_options(profile, cx));

        let titlebar = options.titlebar.expect("the window has a title");
        assert_eq!(titlebar.title.as_deref(), Some(title));
        assert_eq!(options.app_id.as_deref(), Some(identifier));
        assert_eq!(options.window_decorations, Some(WindowDecorations::Client));
        let min_size = options.window_min_size.expect("minimum size");
        assert_eq!(
            (f32::from(min_size.width), f32::from(min_size.height)),
            WINDOW_MIN_SIZE
        );
    }
}

#[gpui::test]
fn the_shell_starts_once_and_shows_the_empty_gallery(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (_window, mut cx) = open(&fixture, ThemeName::Dark, cx);
    settle(&fixture, &mut cx);

    assert_eq!(fixture.calls("start_asset_query"), 1);
    assert_eq!(fixture.calls("scan_startup_roots"), 1);
    assert_eq!(fixture.calls("list_scan_roots"), 1);
    let viewport = cx.update(|window, _| window.viewport_size());
    assert_eq!(
        (f32::from(viewport.width), f32::from(viewport.height)),
        WINDOW_SIZE
    );
    for selector in [
        gallery::HEADER_SELECTOR,
        gallery::SEARCH_SLOT_SELECTOR,
        gallery::COUNT_SELECTOR,
        gallery::EMPTY_STATE_SELECTOR,
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector} is drawn");
    }
    for control in WindowControl::ALL {
        assert!(cx.debug_bounds(chrome::control_selector(control)).is_some());
    }
    for direction in ResizeDirection::ALL {
        let grip = chrome::grip_selector(direction);
        assert!(cx.debug_bounds(Box::leak(grip.into_boxed_str())).is_some());
    }
    // The header sits above the empty state, which is centred horizontally.
    let header = cx.debug_bounds(gallery::HEADER_SELECTOR).unwrap();
    let empty = cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).unwrap();
    assert!(empty.top() >= header.bottom());
    let off_center = f32::from(empty.center().x) - WINDOW_SIZE.0 / 2.0;
    assert!(off_center.abs() <= 1.0, "{empty:?}");
    // The start preferences are stored, as the Solid frontend stores them at boot.
    assert_eq!(
        fixture.prefs.load(),
        Prefs::default()
            .with_theme(ThemeName::Dark)
            .with_language(Language::En)
    );
}

#[gpui::test]
fn models_survive_a_view_switch_and_views_are_made_anew(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open(&fixture, ThemeName::Dark, cx);
    settle(&fixture, &mut cx);
    let runtime_before = runtime(&window, &cx).entity_id();
    let gallery_before = gallery_id(&window, &cx).expect("the gallery is shown");
    send(&window, RuntimeInput::SetTileSize(260.0), &mut cx);

    send(&window, RuntimeInput::OpenSettings, &mut cx);
    assert_eq!(
        window.read_with(&cx, |shell, _| shell.view()).unwrap(),
        View::Settings
    );
    assert_eq!(gallery_id(&window, &cx), None);
    assert!(cx.debug_bounds(settings::HEADER_SELECTOR).is_some());
    assert!(cx.debug_bounds(gallery::HEADER_SELECTOR).is_none());

    send(&window, RuntimeInput::CloseSettings, &mut cx);
    let gallery_after = gallery_id(&window, &cx).expect("the gallery is back");
    assert_ne!(gallery_after, gallery_before, "the view was made anew");
    assert_eq!(runtime(&window, &cx).entity_id(), runtime_before);
    let (tile_size, slider) = window
        .read_with(&cx, |shell, cx| {
            let gallery = shell.gallery().unwrap().read(cx);
            (
                shell.runtime().read(cx).model().tile_size(),
                gallery.tile_size_slider().read(cx).value(),
            )
        })
        .unwrap();
    assert_eq!(tile_size, 260, "the runtime keeps the size");
    assert_eq!(slider, 260.0, "the new slider shows the runtime's size");
    // Switching views asked the backend for nothing.
    assert_eq!(fixture.executor.pending(), 0);
    assert_eq!(fixture.calls("start_asset_query"), 1);
}

#[gpui::test]
fn escape_closes_settings_and_the_focus_returns_to_the_settings_button(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open(&fixture, ThemeName::Dark, cx);
    settle(&fixture, &mut cx);
    let opener = window
        .read_with(&cx, |shell, _| shell.settings_opener().clone())
        .unwrap();
    cx.update(|window, cx| window.focus(&opener, cx));
    send(&window, RuntimeInput::OpenSettings, &mut cx);
    cx.update(|window, cx| {
        let focus = window.focused(cx);
        assert!(focus.is_none_or(|focus| focus != opener));
    });

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    // The layer stack gives the focus back one frame later.
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    cx.run_until_parked();

    assert_eq!(
        window.read_with(&cx, |shell, _| shell.view()).unwrap(),
        View::Gallery
    );
    cx.update(|window, _| assert!(opener.is_focused(window)));
    let layers = cx.update(|_, cx| ui::layers::layer_ids(cx));
    assert!(layers.is_empty(), "the settings layer is released");
}

#[gpui::test]
fn a_failed_startup_scan_does_not_block_the_window(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    fixture
        .backend
        .on_scan_startup_roots(|| Err(BackendError::rejected("synthetic scan failure")));
    let (window, mut cx) = open(&fixture, ThemeName::Dark, cx);
    settle(&fixture, &mut cx);

    assert!(cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).is_some());
    let loading =
        runtime(&window, &cx).read_with(&cx, |runtime, _| runtime.model().maintenance_loading());
    assert!(!loading);
    // Library and roots are refreshed once more after the failure, and the scan is
    // never asked again, however often the views switch.
    send(&window, RuntimeInput::OpenSettings, &mut cx);
    send(&window, RuntimeInput::CloseSettings, &mut cx);
    send(&window, RuntimeInput::Start, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(fixture.calls("scan_startup_roots"), 1);
    assert_eq!(fixture.calls("start_asset_query"), 2);
}

#[gpui::test]
fn the_startup_scan_refreshes_the_count_when_it_found_work(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let mut totals = vec![4, 0];
    fixture.backend.on_start_asset_query(move |_| {
        Ok(StartAssetQueryResult::Ready {
            session_id: 1,
            revision: 1,
            total: totals.pop().unwrap_or(4),
            offset: 0,
            items: Vec::new(),
        })
    });
    fixture.backend.on_scan_startup_roots(|| {
        Ok(Some(ScanSummary {
            completion: ScanCompletion::Complete,
            indexed: 4,
            removed: 0,
            failed: 0,
        }))
    });
    let (window, mut cx) = open(&fixture, ThemeName::Dark, cx);
    settle(&fixture, &mut cx);

    let total = runtime(&window, &cx).read_with(&cx, |runtime, _| runtime.model().query().total());
    assert_eq!(total, 4);
    assert!(
        cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).is_none(),
        "a library with assets shows no empty state"
    );
}

#[gpui::test]
fn theme_and_language_switch_live_and_are_saved(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open(&fixture, ThemeName::Dark, cx);
    settle(&fixture, &mut cx);
    assert_eq!(cx.update(|_, cx| ui::theme::active(cx)), ThemeName::Dark);

    send(&window, RuntimeInput::SetTheme(ThemeName::Light), &mut cx);
    send(&window, RuntimeInput::SetLanguage(Language::Pl), &mut cx);
    assert_eq!(cx.update(|_, cx| ui::theme::active(cx)), ThemeName::Light);
    settle(&fixture, &mut cx);

    assert_eq!(
        fixture.prefs.load(),
        Prefs::default()
            .with_theme(ThemeName::Light)
            .with_language(Language::Pl)
    );
    let label = window
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
    assert_eq!(label, f64::from(DEFAULT_TILE_SIZE));
}

#[gpui::test]
fn the_close_button_closes_the_window(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open(&fixture, ThemeName::Dark, cx);
    settle(&fixture, &mut cx);
    let close = cx
        .debug_bounds(chrome::control_selector(WindowControl::Close))
        .expect("the close button is drawn");

    cx.simulate_click(close.center(), gpui::Modifiers::none());
    cx.run_until_parked();

    assert!(
        window.read_with(&cx, |_, _| ()).is_err(),
        "the window is gone"
    );
}

/// The window buttons are tab stops after the settings button (`UiButton` in Tauri),
/// and Enter presses the focused one: three Tabs from the settings button reach Close.
#[gpui::test]
fn the_window_buttons_are_reached_with_tab_and_pressed_with_enter(cx: &mut TestAppContext) {
    let fixture = Fixture::empty();
    let (window, mut cx) = open(&fixture, ThemeName::Dark, cx);
    settle(&fixture, &mut cx);
    let opener = window
        .read_with(&cx, |shell, _| shell.settings_opener().clone())
        .unwrap();
    cx.update(|window, cx| window.focus(&opener, cx));
    cx.run_until_parked();

    let mut seen = Vec::new();
    for _ in WindowControl::ALL {
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
        let focused = cx
            .update(|window, cx| window.focused(cx))
            .expect("Tab focuses a window button");
        assert!(focused != opener);
        assert!(!seen.contains(&focused), "each Tab reaches a new button");
        seen.push(focused);
    }
    assert!(
        window.read_with(&cx, |_, _| ()).is_ok(),
        "Tab pressed nothing"
    );

    // GPUI's keyboard click fires on the key's release.
    cx.simulate_keystrokes("enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").expect("a key"),
    });
    cx.run_until_parked();

    assert!(
        window.read_with(&cx, |_, _| ()).is_err(),
        "Enter on the focused close button closes the window"
    );
}

/// Where the headless test leaves its PNGs when `MEDIATAGGER_GPUI_SCREENSHOT_DIR` is set.
fn screenshot_target(name: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("MEDIATAGGER_GPUI_SCREENSHOT_DIR")?);
    Some(dir.join(name))
}

fn renderer_available() -> bool {
    match gpui_platform::current_headless_renderer() {
        Ok(_) => true,
        Err(error) => {
            if std::env::var_os("MEDIATAGGER_GPUI_REQUIRE_RENDERER").is_some() {
                panic!("no headless renderer: {error:#}");
            }
            eprintln!("skipped: no GPU adapter for the headless renderer ({error:#})");
            false
        }
    }
}

/// The empty shell in both themes: the header in the chrome colour, the gallery in the
/// application background, and the same picture twice.
#[test]
fn the_empty_shell_renders_through_the_headless_renderer() {
    if !renderer_available() {
        return;
    }
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), || {
        gpui_platform::current_headless_renderer()
    });
    for (theme, name) in [
        (ThemeName::Dark, "shell-empty-headless.png"),
        (ThemeName::Light, "shell-empty-light-headless.png"),
    ] {
        let mut captures = Vec::new();
        for _ in 0..2 {
            let fixture = Fixture::empty();
            let window = cx
                .update(|cx| {
                    init_application(theme, cx)?;
                    open_main_window(profile::E2E, fixture.parts(theme), cx)
                })
                .expect("open the main window");
            loop {
                cx.run_until_parked();
                if fixture.executor.run_all() == 0 {
                    break;
                }
            }
            cx.run_until_parked();
            captures.push(
                cx.capture_screenshot(window.into())
                    .expect("capture the window"),
            );
        }
        let image = &captures[0];
        assert!(
            image.as_raw() == captures[1].as_raw(),
            "{name}: two captures of the same state differ"
        );
        let colors = theme.tokens().colors;
        let scale = image.width() as f32 / WINDOW_SIZE.0;
        let pixel = |x: f32, y: f32| image.get_pixel((x * scale) as u32, (y * scale) as u32).0;
        let chrome = colors.window_chrome_bg;
        let background = colors.bg_1;
        assert_eq!(
            pixel(600.0, 20.0),
            [chrome.r, chrome.g, chrome.b, 0xff],
            "{name}: the header"
        );
        assert_eq!(
            pixel(20.0, WINDOW_SIZE.1 - 20.0),
            [background.r, background.g, background.b, 0xff],
            "{name}: the gallery background"
        );
        if let Some(target) = screenshot_target(name) {
            image.save(&target).expect("write the screenshot");
            eprintln!("screenshot: {}", target.display());
        }
    }
}

#[test]
fn the_runtime_model_is_the_client_model() {
    // The adapter adds no state machine of its own: the runtime entity drives the
    // client's AppRuntime, which tests in media_tagger_client cover.
    let mut model = media_tagger_client::runtime::AppRuntime::new(RuntimeConfig {
        theme: ThemeName::Dark,
        language: Language::En,
    });
    assert!(model.handle(RuntimeInput::OpenSettings).is_empty());
    assert_eq!(model.view(), View::Settings);
}

/// Every text of the window draws in the stylesheet's fonts, which `init_application`
/// registers at startup: the shell's root names the sans family of the typography
/// tokens (Inter), and both bundled families resolve to a face of their own with that
/// face's metrics, never to GPUI's fallback stack. Without the registration "JetBrains
/// Mono" (not a usual system font) falls back to a proportional sans.
#[test]
fn the_window_text_uses_the_bundled_fonts() {
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::new(text_system);
    cx.update(|cx| init_application(ThemeName::Dark, cx))
        .expect("prepare the application");

    let mut root = ui::theme::app_root(div(), ThemeName::Dark);
    assert_eq!(
        root.style()
            .text
            .font_family
            .as_ref()
            .map(|family| family.as_ref()),
        Some(TYPOGRAPHY.sans[0]),
        "the shell's root text style"
    );
    assert_eq!(
        (ui::fonts::SANS, ui::fonts::MONO),
        (TYPOGRAPHY.sans[0], TYPOGRAPHY.mono[0])
    );

    let text_system = cx.text_system().clone();
    // Advance widths in font units at a size of one em: Inter.ttf has 2048 units per
    // em ('m' 1794, '0' 1292), JetBrainsMono.ttf 1000 (every glyph 600).
    for (family, units_per_em, advances) in [
        (ui::fonts::SANS, 2048.0, [('m', 1794.0), ('0', 1292.0)]),
        (ui::fonts::MONO, 1000.0, [('m', 600.0), ('0', 600.0)]),
    ] {
        let font_id = text_system.resolve_font(&font(family));
        assert_eq!(
            text_system
                .get_font_for_id(font_id)
                .map(|resolved| resolved.family),
            Some(family.into()),
            "{family} resolves to a face of its own"
        );
        for (ch, units) in advances {
            let advance = text_system
                .advance(font_id, px(units_per_em), ch)
                .expect("the glyph exists")
                .width;
            assert!(
                (f32::from(advance) - units).abs() < 0.01,
                "{family} '{ch}': advance {advance:?}, expected {units} units"
            );
        }
    }
}
