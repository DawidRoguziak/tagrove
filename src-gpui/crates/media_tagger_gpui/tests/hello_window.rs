//! The placeholder window in GPUI's test platform (no display, no GPU) and through the
//! in-process headless renderer (no display; needs a GPU or a software Vulkan/GL
//! driver, and skips without one).

use std::{path::PathBuf, sync::Arc};

use gpui::{HeadlessAppContext, TestAppContext, VisualTestContext};
use media_tagger_client::{hello::HelloScreen, theme::ThemeName};
use media_tagger_gpui::{
    profile,
    views::hello::HELLO_TEXT_SELECTOR,
    window::{main_window_options, open_main_window, WINDOW_MIN_SIZE, WINDOW_SIZE},
};

fn dark_screen() -> HelloScreen {
    HelloScreen::load(ThemeName::Dark).expect("shared sources are complete")
}

#[gpui::test]
fn main_window_centers_the_shared_text(cx: &mut TestAppContext) {
    let screen = dark_screen();
    let window = cx
        .update(|cx| open_main_window(profile::DEV, &screen, cx))
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    assert_eq!(
        window
            .read_with(&cx, |view, _| view.text().to_owned())
            .unwrap(),
        "Loading..."
    );

    let viewport = cx.update(|window, _| window.viewport_size());
    let text = cx
        .debug_bounds(HELLO_TEXT_SELECTOR)
        .expect("the text element is drawn");
    let off_center_x = f32::from(text.center().x) - f32::from(viewport.width) / 2.0;
    let off_center_y = f32::from(text.center().y) - f32::from(viewport.height) / 2.0;
    assert!(
        off_center_x.abs() <= 1.0 && off_center_y.abs() <= 1.0,
        "text bounds {text:?} are not centered in {viewport:?}"
    );
}

#[gpui::test]
fn each_profile_opens_under_its_own_title_and_application_id(cx: &mut TestAppContext) {
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
        let min_size = options.window_min_size.expect("minimum size");
        assert_eq!(
            (f32::from(min_size.width), f32::from(min_size.height)),
            WINDOW_MIN_SIZE
        );
    }
}

/// Where the headless test leaves its PNG when `MEDIATAGGER_GPUI_SCREENSHOT_DIR` is set.
fn screenshot_target() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("MEDIATAGGER_GPUI_SCREENSHOT_DIR")?);
    Some(dir.join("hello-headless.png"))
}

/// Path A of the E00 screenshot spike: render the real view in-process, without any
/// display server, and read the pixels back.
#[test]
fn hello_window_renders_through_the_headless_renderer() {
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
    let screen = dark_screen();
    let background = [
        screen.background.r,
        screen.background.g,
        screen.background.b,
        0xff,
    ];

    let window = cx
        .update(|cx| open_main_window(profile::E2E, &screen, cx))
        .expect("open the main window");
    cx.run_until_parked();
    let scale = cx
        .update_window(window.into(), |_, window, _| window.scale_factor())
        .expect("the window is open");
    let image = cx
        .capture_screenshot(window.into())
        .expect("capture the window");

    assert_eq!(
        (image.width() as f32, image.height() as f32),
        (WINDOW_SIZE.0 * scale, WINDOW_SIZE.1 * scale),
        "the capture covers the whole window at scale {scale}"
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
            "corner ({x}, {y}) shows the theme background"
        );
    }
    let text_pixels = image.pixels().filter(|pixel| pixel.0 != background).count();
    assert!(
        text_pixels > 50,
        "the text is drawn over the background ({text_pixels} pixels differ)"
    );

    if let Some(target) = screenshot_target() {
        image.save(&target).expect("write the screenshot");
        eprintln!("screenshot: {}", target.display());
    }
}
