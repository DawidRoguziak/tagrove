//! The `kit` board (epic E05): every shared control in both themes. GPUI's test
//! platform checks the layout; the in-process headless renderer draws it (skipped without
//! a GPU or software driver) and leaves `kit-headless.png` and `kit-dialog-headless.png`
//! (the board with its small dialog open) in `MEDIATAGGER_GPUI_SCREENSHOT_DIR` when that
//! is set, for review against `src/styles.css`.

use std::{path::PathBuf, sync::Arc};

use gpui::{
    div, prelude::*, px, size, Context, HeadlessAppContext, TestAppContext, VisualTestContext,
    Window,
};
use media_tagger_client::{
    controls::ButtonVariant,
    icons::IconName,
    theme::{Rgba, ThemeName},
};
use media_tagger_gpui::{
    profile,
    ui::{assets::Assets, button::Button, theme::app_root},
    window::{init_application, open_kit_window, WINDOW_SIZE},
};

#[gpui::test]
fn the_board_opens_in_the_window_size(cx: &mut TestAppContext) {
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_kit_window(profile::E2E, cx)
        })
        .expect("open the kit");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    let viewport = cx.update(|window, _| window.viewport_size());
    assert_eq!(
        (f32::from(viewport.width), f32::from(viewport.height)),
        WINDOW_SIZE
    );
}

/// Whether the headless renderer is available. Without one the test is skipped, unless
/// `MEDIATAGGER_GPUI_REQUIRE_RENDERER` is set, which turns the skip into a failure.
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

#[test]
fn a_dialog_of_the_board_draws_over_the_whole_window() {
    if !renderer_available() {
        return;
    }
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), || {
        gpui_platform::current_headless_renderer()
    });
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_kit_window(profile::E2E, cx)
        })
        .expect("open the kit");
    cx.run_until_parked();
    window
        .update(&mut cx, |kit, window, cx| kit.open_dialog(1, window, cx))
        .expect("the window is open");
    // Past the 150 ms entrance.
    cx.advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    let image = cx
        .capture_screenshot(window.into())
        .expect("capture the window");
    if let Some(target) = screenshot_target("kit-dialog-headless.png") {
        image.save(&target).expect("write the screenshot");
        eprintln!("screenshot: {}", target.display());
    }

    let light = ThemeName::Light.tokens().colors;
    let scale = image.width() as f32 / WINDOW_SIZE.0;
    // Inside the small dialog's left padding, centred in the window.
    let x = ((WINDOW_SIZE.0 - 460.0) / 2.0 + 10.0) * scale;
    let y = WINDOW_SIZE.1 / 2.0 * scale;
    let surface = light.surface_solid;
    assert_eq!(
        image.get_pixel(x as u32, y as u32).0,
        [surface.r, surface.g, surface.b, 0xff],
        "the dialog surface"
    );
    // The backdrop dims the light column's background.
    let corner = image.get_pixel(image.width() - 2, image.height() - 2).0;
    let background = light.bg_1;
    assert_ne!(corner, [background.r, background.g, background.b, 0xff]);
}

fn screenshot_target(name: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("MEDIATAGGER_GPUI_SCREENSHOT_DIR")?);
    Some(dir.join(name))
}

#[test]
fn the_board_renders_through_the_headless_renderer() {
    if !renderer_available() {
        return;
    }
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), || {
        gpui_platform::current_headless_renderer()
    });
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_kit_window(profile::E2E, cx)
        })
        .expect("open the kit");
    cx.run_until_parked();
    let image = cx
        .capture_screenshot(window.into())
        .expect("capture the window");

    // The left half is the dark theme's background, the right half the light one's.
    let dark = ThemeName::Dark.tokens().colors.bg_1;
    let light = ThemeName::Light.tokens().colors.bg_1;
    let at = |x: u32, y: u32| image.get_pixel(x, y).0;
    assert_eq!(at(2, image.height() - 2), [dark.r, dark.g, dark.b, 0xff]);
    assert_eq!(
        at(image.width() - 2, image.height() - 2),
        [light.r, light.g, light.b, 0xff]
    );

    if let Some(target) = screenshot_target("kit-headless.png") {
        image.save(&target).expect("write the screenshot");
        eprintln!("screenshot: {}", target.display());
    }
}

/// Two light-theme buttons whose fill lets the background through: an active icon
/// button (`bg-primary/12`) at (20, 20) and a disabled primary button
/// (`disabled:opacity-50`) at (20, 80).
struct SeeThrough;

impl Render for SeeThrough {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        app_root(div(), ThemeName::Light)
            .relative()
            .child(
                div().absolute().left(px(20.)).top(px(20.)).child(
                    Button::icon("bulk", IconName::BulkActions, "Bulk")
                        .active(true)
                        .theme(ThemeName::Light),
                ),
            )
            .child(
                div().absolute().left(px(20.)).top(px(80.)).child(
                    Button::new("confirm", "Confirm")
                        .variant(ButtonVariant::Primary)
                        .disabled(true)
                        .theme(ThemeName::Light),
                ),
            )
    }
}

/// CSS draws an outer `box-shadow` only outside the border box; GPUI draws it beneath
/// the whole element. Inside a translucent button the colour must be its fill over the
/// background alone, as in the browser, without the `--shadow-control` showing through.
#[test]
fn a_translucent_button_shows_the_background_and_not_its_shadow() {
    if !renderer_available() {
        return;
    }
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), || {
        gpui_platform::current_headless_renderer()
    });
    cx.update(|cx| init_application(ThemeName::Light, cx))
        .expect("initialise");
    let window = cx
        .open_window(size(px(200.), px(140.)), |_, cx| cx.new(|_| SeeThrough))
        .expect("open the window");
    cx.run_until_parked();
    let scale = cx
        .update_window(window.into(), |_, window, _| window.scale_factor())
        .expect("the window is open");
    let image = cx
        .capture_screenshot(window.into())
        .expect("capture the window");
    let at = |x: f32, y: f32| image.get_pixel((x * scale) as u32, (y * scale) as u32).0;
    let colors = ThemeName::Light.tokens().colors;
    // `fill` at `alpha` (0..=1) over the background.
    let over = |fill: Rgba, alpha: f32| {
        let mix = |top: u8, bottom: u8| {
            (f32::from(bottom) + (f32::from(top) - f32::from(bottom)) * alpha).round() as i32
        };
        let background = colors.bg_1;
        [
            mix(fill.r, background.r),
            mix(fill.g, background.g),
            mix(fill.b, background.b),
        ]
    };
    let close = |pixel: [u8; 4], expected: [i32; 3], what: &str| {
        for (channel, want) in pixel.iter().zip(expected) {
            assert!(
                (i32::from(*channel) - want).abs() <= 2,
                "{what}: {pixel:?}, expected {expected:?}"
            );
        }
    };
    // Inside the left border, clear of the icon and of the label.
    close(
        at(25., 38.),
        over(colors.primary, 0.12),
        "the active icon button",
    );
    close(
        at(25., 96.),
        over(colors.primary, 0.5),
        "the disabled primary button",
    );
}
