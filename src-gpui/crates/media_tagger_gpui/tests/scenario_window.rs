//! The window a comparison scenario starts in (`media_tagger_gpui --scenario <id>`,
//! epic E01): the start state the E2E profile accepts becomes the main window, in the
//! scenario's theme and in English, as the Tauri E2E specs start; and the same scenario
//! always lays out and draws the same, so a rerun of `compare:ui` gives the same capture.
//! GPUI's test platform needs no display; the pixel case uses the in-process headless
//! renderer and skips without a GPU or software driver, like `hello_window`.

use std::sync::Arc;

use gpui::{Bounds, HeadlessAppContext, Pixels, Size, TestAppContext, VisualTestContext};
use media_tagger_client::{
    hello::{HelloScreen, BACKGROUND_TOKEN, HELLO_TEXT_KEY},
    i18n::Catalog,
    scenario::SCENARIOS,
    theme::{color_token, ThemeName},
};
use media_tagger_gpui::{
    profile,
    scenario::start_state,
    views::hello::HELLO_TEXT_SELECTOR,
    window::{open_main_window, WINDOW_SIZE},
};

/// What `run` would show for `--scenario <id>` in the E2E build.
fn scenario_screen(id: &str) -> HelloScreen {
    let start = start_state(profile::E2E, Some(id)).expect("the E2E profile accepts it");
    HelloScreen::load(start.theme).expect("shared sources are complete")
}

/// Opens the main window of the E2E profile on `screen` and returns its text, its
/// viewport and where the text is drawn.
fn open(cx: &mut TestAppContext, screen: &HelloScreen) -> (String, Size<Pixels>, Bounds<Pixels>) {
    let window = cx
        .update(|cx| open_main_window(profile::E2E, screen, cx))
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    let text = window
        .read_with(&cx, |view, _| view.text().to_owned())
        .expect("the window is open");
    let viewport = cx.update(|window, _| window.viewport_size());
    let bounds = cx
        .debug_bounds(HELLO_TEXT_SELECTOR)
        .expect("the text element is drawn");
    (text, viewport, bounds)
}

#[test]
fn every_scenario_starts_in_english_on_the_background_of_its_theme() {
    let english = Catalog::english().expect("the English catalogue");
    let loading = english.text(HELLO_TEXT_KEY).expect("the placeholder text");

    for scenario in SCENARIOS {
        let screen = scenario_screen(scenario.id);

        assert_eq!(screen.text, loading, "{}", scenario.id);
        assert_eq!(
            Some(screen.background),
            color_token(scenario.theme, BACKGROUND_TOKEN),
            "{}",
            scenario.id
        );
    }
    assert_ne!(
        scenario_screen("shell-empty").background,
        scenario_screen("shell-empty-light").background,
        "the light scenario must not draw the dark background"
    );
    assert_eq!(
        scenario_screen("shell-empty"),
        HelloScreen::load(ThemeName::Dark).unwrap(),
        "shell-empty is the default dark start"
    );
}

#[gpui::test]
fn reopening_a_scenario_gives_the_same_layout(cx: &mut TestAppContext) {
    for scenario in SCENARIOS {
        let screen = scenario_screen(scenario.id);

        let first = open(cx, &screen);
        let second = open(cx, &screen);

        assert_eq!(first, second, "{}", scenario.id);
        assert_eq!(
            (f32::from(first.1.width), f32::from(first.1.height)),
            WINDOW_SIZE,
            "{}: the capture size of compare:ui",
            scenario.id
        );
    }
}

#[gpui::test]
fn the_light_scenario_lays_out_like_the_dark_one(cx: &mut TestAppContext) {
    let dark = open(cx, &scenario_screen("shell-empty"));
    let light = open(cx, &scenario_screen("shell-empty-light"));

    assert_eq!(dark, light);
}

/// The two captures of one scenario are identical pixel for pixel, and every corner shows
/// the scenario's own background.
#[test]
fn scenario_captures_repeat_and_show_their_theme() {
    if let Err(error) = gpui_platform::current_headless_renderer() {
        if std::env::var_os("MEDIATAGGER_GPUI_REQUIRE_RENDERER").is_some() {
            panic!("no headless renderer: {error:#}");
        }
        eprintln!("skipped: no GPU adapter for the headless renderer ({error:#})");
        return;
    }
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(()), || {
        gpui_platform::current_headless_renderer()
    });

    for scenario in SCENARIOS {
        let screen = scenario_screen(scenario.id);
        let background = [
            screen.background.r,
            screen.background.g,
            screen.background.b,
            0xff,
        ];
        let mut captures = Vec::new();
        for _ in 0..2 {
            let window = cx
                .update(|cx| open_main_window(profile::E2E, &screen, cx))
                .expect("open the main window");
            cx.run_until_parked();
            captures.push(
                cx.capture_screenshot(window.into())
                    .expect("capture the window"),
            );
        }

        let image = &captures[0];
        assert_eq!(
            image.dimensions(),
            captures[1].dimensions(),
            "{}",
            scenario.id
        );
        assert!(
            image.as_raw() == captures[1].as_raw(),
            "{}: two captures of the same scenario differ",
            scenario.id
        );
        for (x, y) in [
            (0, 0),
            (image.width() - 1, 0),
            (0, image.height() - 1),
            (image.width() - 1, image.height() - 1),
        ] {
            assert_eq!(
                image.get_pixel(x, y).0,
                background,
                "{}: corner ({x}, {y})",
                scenario.id
            );
        }
    }
}
