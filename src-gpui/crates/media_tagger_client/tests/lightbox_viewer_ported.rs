//! The behaviour tests of the image viewer and its keys (epic E16), ported from the
//! Solid suite: `src/features/lightbox/__tests__/imageControls.test.tsx` (zoom, pan,
//! drag, resize and the picture's own size, on `ImageControls`) and
//! `keyboard.test.tsx` (which keys the viewer takes, when, and fullscreen through the
//! window, on `Session`). Each test names the TS case it ports in its doc comment;
//! each row of an `it.each` is its own test.
//!
//! The Solid stage measures a stage of 800 x 600 at the window's origin; so does
//! every viewer here. A wheel turn of the DOM is `ImageControls::wheel` with the same
//! `deltaY`, a key is `Session::key` (the view routes `+`, `=`, `-`, `0`, `f` and
//! the arrows there), a `load` event of the `<img>` is `ImageControls::loaded` and a
//! rerender with another selection is `Session::sync`. What the cases check of the
//! DOM itself (the same `<img>` node, one `keydown` listener, `requestAnimationFrame`
//! coalescing) has no counterpart: the GPUI view draws `ImageControls::image_rect` in
//! every frame and binds its keys once, in its key context.

use std::time::Duration;

use media_tagger_client::lightbox::{
    image_controls::WHEEL_ZOOM_RATE, Dismissal, ImageControls, ImageRect, KeyOutcome,
    NavigationStatus, Pan, SelectedAsset, Session, Shown, StageKind, ViewerKey, Viewport,
    WheelMode, WindowFullscreen,
};
use media_tagger_core::models::AssetSummary;

const MS: fn(u64) -> Duration = Duration::from_millis;

/// The stage of the Solid suite: `clientWidth` 800, `clientHeight` 600 at (0, 0).
const STAGE: Viewport = Viewport {
    left: 0.0,
    top: 0.0,
    width: 800.0,
    height: 600.0,
};

/// `portrait` of the Solid suite: 1200 x 6000, fitted into the stage as 120 x 600.
fn summary(id: i64, kind: &str, width: i64, height: i64) -> AssetSummary {
    AssetSummary {
        id,
        file_name: format!("{id}.png"),
        preview_path: None,
        kind: kind.into(),
        modified_at: 1,
        width: Some(width),
        height: Some(height),
        duration_ms: None,
        thumb_version: "0123456789abcdef".into(),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

fn shown_of(summary: AssetSummary, tags: &[&str]) -> Shown {
    let tags: Vec<String> = tags.iter().map(|tag| (*tag).to_owned()).collect();
    Shown {
        selected: SelectedAsset {
            path: Some(format!("/synthetic/{}", summary.file_name)),
            summary,
            size_bytes: Some(10),
            tags: tags.clone(),
        },
        selected_index: Some(0),
        navigation: NavigationStatus::Ready,
        tags,
        tag_saving: false,
        tag_failed: false,
        tag_details_loading: false,
        tag_details_failed: false,
        tag_editing_disabled: false,
        asset_details_failed: false,
        favorite_pending: false,
        favorite_failed: false,
        group_pending: false,
        group_failed: false,
        group_key: String::new(),
        group_order: String::new(),
        group_valid: true,
    }
}

fn portrait(kind: &str) -> Shown {
    shown_of(summary(10, kind, 1200, 6000), &["cat"])
}

/// `asset(id)` of the Solid fixtures with the kind the case gives it.
fn asset(id: i64, kind: &str) -> Shown {
    shown_of(summary(id, kind, 100, 80), &["cat"])
}

/// A mounted lightbox showing `shown` on the Solid stage, with the picture on it.
fn session_with(shown: &Shown) -> Session {
    let mut session = Session::new(false, false, false, MS(0));
    session.sync(shown, MS(0));
    session.controls_mut().set_viewport(STAGE);
    session.controls_mut().set_mounted(true);
    session
}

/// A viewer of `portrait` on the Solid stage.
fn viewer() -> ImageControls {
    let mut controls = ImageControls::new();
    controls.select(10, true, Some(1200), Some(6000), MS(0));
    controls.set_viewport(STAGE);
    controls.set_mounted(true);
    controls
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// `zoomToTen`: ten 1.25x key steps, then a wheel turn at the stage's centre to
/// exactly 10x.
fn zoom_to_ten_with(session: &mut Session, kind: StageKind) {
    for _ in 0..10 {
        assert_eq!(
            session.key(ViewerKey::ZoomIn, kind, true, false),
            KeyOutcome::Handled
        );
    }
    let delta = -(10.0 / 1.25f64.powi(10)).ln() / WHEEL_ZOOM_RATE;
    assert!(session
        .controls_mut()
        .wheel(delta, WheelMode::Pixels, 400.0, 300.0, MS(0)));
    assert!(close(session.controls().zoom(), 10.0));
}

fn zoom_to_ten(controls: &mut ImageControls) {
    for _ in 0..10 {
        controls.zoom_in();
    }
    let delta = -(10.0 / 1.25f64.powi(10)).ln() / WHEEL_ZOOM_RATE;
    assert!(controls.wheel(delta, WheelMode::Pixels, 400.0, 300.0, MS(0)));
    assert!(close(controls.zoom(), 10.0));
}

/// The `width` and `height` of the Solid `<img>` style: the fitted size at zoom 1.
fn fitted_size(controls: &ImageControls) -> (f64, f64) {
    let fitted = controls.fitted();
    (fitted.width, fitted.height)
}

/// imageControls.test.tsx, "sizes a tall %s from the original and keeps zoom across
/// rerenders", shared by the rows `image` and `gif`.
fn sizes_a_tall_picture_and_keeps_zoom_across_rerenders(kind_name: &str) {
    let shown = portrait(kind_name);
    let kind = shown.kind();
    assert!(!kind.is_video());
    let mut session = session_with(&shown);
    zoom_to_ten_with(&mut session, kind);

    let controls = session.controls();
    assert_eq!(fitted_size(controls), (120.0, 600.0));
    let rect = controls.image_rect().expect("the picture has a size");
    assert!(close(rect.width, 1200.0) && close(rect.height, 6000.0));
    assert_eq!(controls.pan(), Pan::ZERO, "translate(0px, 0px)");
    // Centred: the picture overflows the stage symmetrically.
    assert!(close(rect.left, (800.0 - rect.width) / 2.0));
    assert!(close(rect.top, (600.0 - rect.height) / 2.0));

    // A rerender with the same asset (a tag write) keeps the zoom.
    session.sync(
        &shown_of(summary(10, kind_name, 1200, 6000), &["edited"]),
        MS(10),
    );
    assert!(close(session.controls().zoom(), 10.0));
    assert_eq!(fitted_size(session.controls()), (120.0, 600.0));

    assert_eq!(
        session.key(ViewerKey::ResetZoom, kind, true, false),
        KeyOutcome::Handled
    );
    assert_eq!(session.controls().zoom(), 1.0);
    assert_eq!(session.controls().pan(), Pan::ZERO);
}

/// imageControls.test.tsx, "sizes a tall %s from the original and keeps zoom across
/// rerenders", row `image`.
#[test]
fn sizes_a_tall_image_from_the_original_and_keeps_zoom_across_rerenders() {
    sizes_a_tall_picture_and_keeps_zoom_across_rerenders("image");
}

/// imageControls.test.tsx, "sizes a tall %s from the original and keeps zoom across
/// rerenders", row `gif`.
#[test]
fn sizes_a_tall_gif_from_the_original_and_keeps_zoom_across_rerenders() {
    sizes_a_tall_picture_and_keeps_zoom_across_rerenders("gif");
}

/// imageControls.test.tsx, "preserves the wheel anchor and clamps drag and resize pan
/// to the zoomed bounds".
#[test]
fn preserves_the_wheel_anchor_and_clamps_drag_and_resize_pan_to_the_zoomed_bounds() {
    let mut controls = viewer();
    zoom_to_ten(&mut controls);
    // 1.2x at (450, 400): 50 right of and 100 below the centre.
    assert!(controls.wheel(
        -(1.2f64).ln() / WHEEL_ZOOM_RATE,
        WheelMode::Pixels,
        450.0,
        400.0,
        MS(0)
    ));
    assert!(close(controls.zoom(), 12.0));
    assert!(close(controls.pan().x, -10.0), "{:?}", controls.pan());
    assert!(close(controls.pan().y, -20.0), "{:?}", controls.pan());

    // A drag far past every edge stops at the bounds: 120 x 600 at 12x is 1440 x 7200.
    assert!(controls.pointer_down(true, 400.0, 300.0, MS(100)));
    assert!(controls.pointer_move(10000.0, 10000.0));
    controls.pointer_up(MS(200));
    assert_eq!(controls.pan(), Pan::new(320.0, 3300.0));

    // A stage of 1200 x 400 fits the picture as 80 x 400: no overflow across, 2200
    // down.
    controls.set_viewport(Viewport {
        width: 1200.0,
        height: 400.0,
        ..STAGE
    });
    assert_eq!(fitted_size(&controls).1, 400.0);
    assert_eq!(controls.pan(), Pan::new(0.0, 2200.0));
}

/// imageControls.test.tsx, "uses natural dimensions after load and resets on
/// navigation".
#[test]
fn uses_natural_dimensions_after_load_and_resets_on_navigation() {
    let mut session = session_with(&portrait("image"));
    session.controls_mut().loaded(2400.0, 6000.0);
    assert_eq!(fitted_size(session.controls()).0, 240.0);
    zoom_to_ten_with(&mut session, StageKind::Image);

    session.sync(
        &shown_of(summary(11, "image", 1600, 1200), &["cat"]),
        MS(10),
    );
    let controls = session.controls();
    assert_eq!(fitted_size(controls), (800.0, 600.0));
    assert_eq!(controls.zoom(), 1.0);
    assert_eq!(controls.pan(), Pan::ZERO);
    assert_eq!(
        controls.image_rect(),
        Some(ImageRect {
            left: 0.0,
            top: 0.0,
            width: 800.0,
            height: 600.0
        })
    );
}

/// imageControls.test.tsx, "coalesces zoom writes into one frame and cancels pending
/// work on unmount": the zoom half. Three key steps make 1.25^3; a viewer that went
/// with its view takes no picture size any more. (The coalescing into one animation
/// frame is the DOM writer's: the GPUI view reads the zoom when it draws.)
#[test]
fn three_zoom_steps_make_one_zoom_and_a_disposed_viewer_takes_nothing() {
    let mut session = session_with(&portrait("image"));
    for _ in 0..3 {
        session.key(ViewerKey::ZoomIn, StageKind::Image, true, false);
    }
    assert_eq!(session.controls().zoom(), 1.25f64.powi(3));
    session.key(ViewerKey::ZoomIn, StageKind::Image, true, false);
    let controls = session.controls_mut();
    let fitted = controls.fitted();
    controls.dispose(MS(10));
    controls.loaded(2000.0, 100.0);
    assert_eq!(controls.fitted(), fitted);
}

/// imageControls.test.tsx, "ignores a detached image load, same asset=%s", shared by
/// both rows. The GPUI counterpart of a load event of a retired `<img>` is a late
/// decode, which `Originals` drops by its generation (the adapter's
/// `tests/lightbox_ported.rs`); what is left for the viewer is that the current
/// picture's size and the zoom built on it are what the next wheel turn uses.
fn ignores_a_detached_load(same_asset: bool) {
    let mut session = session_with(&portrait("image"));
    let next_id = if same_asset { 10 } else { 11 };
    let mut next = portrait("image");
    next.selected.summary.id = next_id;
    next.selected.path = Some("/synthetic/next.png".into());
    session.sync(&next, MS(10));
    session.controls_mut().loaded(1600.0, 1200.0);
    zoom_to_ten_with(&mut session, StageKind::Image);
    let before = (
        session.controls().fitted(),
        session.controls().zoom(),
        session.controls().pan(),
    );
    // The retired picture's size is never reported to the viewer: nothing changed.
    assert_eq!(
        (
            session.controls().fitted(),
            session.controls().zoom(),
            session.controls().pan()
        ),
        before
    );
    assert!(session.controls_mut().wheel(
        -(1.2f64).ln() / WHEEL_ZOOM_RATE,
        WheelMode::Pixels,
        450.0,
        400.0,
        MS(20)
    ));
    let controls = session.controls();
    assert_eq!(fitted_size(controls).0, 800.0);
    assert!(close(controls.zoom(), 12.0));
    assert!(close(controls.pan().x, -10.0) && close(controls.pan().y, -20.0));
}

/// imageControls.test.tsx, "ignores a detached image load, same asset=%s", row
/// `false`.
#[test]
fn ignores_a_detached_image_load_of_another_asset() {
    ignores_a_detached_load(false);
}

/// imageControls.test.tsx, "ignores a detached image load, same asset=%s", row
/// `true`.
#[test]
fn ignores_a_detached_image_load_of_the_same_asset() {
    ignores_a_detached_load(true);
}

/// imageControls.test.tsx, "ignores loads for an %s owner", row `inactive`.
#[test]
fn ignores_loads_for_an_inactive_owner() {
    let mut controls = viewer();
    let fitted = controls.fitted();
    let zoom = controls.zoom();
    controls.set_active(false, MS(10));
    controls.loaded(2000.0, 100.0);
    assert_eq!((controls.fitted(), controls.zoom()), (fitted, zoom));
    // The lightbox closed logically: the session ends the viewer with it.
    let mut session = session_with(&portrait("image"));
    let fitted = session.controls().fitted();
    session.deactivate(false, MS(10));
    session.controls_mut().loaded(2000.0, 100.0);
    assert_eq!(session.controls().fitted(), fitted);
}

/// imageControls.test.tsx, "ignores loads for an %s owner", row `disposed`.
#[test]
fn ignores_loads_for_a_disposed_owner() {
    let mut controls = viewer();
    let fitted = controls.fitted();
    controls.dispose(MS(10));
    controls.loaded(2000.0, 100.0);
    assert_eq!(controls.fitted(), fitted);
    assert_eq!(controls.zoom(), 1.0);
}

/// keyboard.test.tsx, "handles the first %s after idle and a synchronous activity
/// update", shared by both rows. The view reveals the controls on every key that
/// reaches the dialog (`Session::input`, from its capturing key listener) before the
/// key's action runs.
fn handles_the_first_arrow_after_idle(key: ViewerKey, expected: KeyOutcome) {
    let mut session = session_with(&asset(1, "image"));
    let mut now = MS(0);
    for _ in 1..=3 {
        now += MS(3001);
        assert!(!session.controls_visible(now), "hidden after three seconds");
        session.input(now);
        assert!(session.controls_visible(now), "revealed before navigating");
        assert_eq!(session.key(key, StageKind::Image, true, false), expected);
        assert!(session.controls_visible(now));
    }
}

/// keyboard.test.tsx, "handles the first %s after idle and a synchronous activity
/// update", row `ArrowLeft`.
#[test]
fn handles_the_first_arrow_left_after_idle() {
    handles_the_first_arrow_after_idle(ViewerKey::Previous, KeyOutcome::Previous);
}

/// keyboard.test.tsx, "handles the first %s after idle and a synchronous activity
/// update", row `ArrowRight`.
#[test]
fn handles_the_first_arrow_right_after_idle() {
    handles_the_first_arrow_after_idle(ViewerKey::Next, KeyOutcome::Next);
}

/// keyboard.test.tsx, "reads current callbacks and media kind without replacing the
/// keyboard listener": the kind half. An image takes the arrows and the zoom keys; once
/// a video is selected the zoom keys are not taken (they go on to the next binding)
/// while the arrows and `f` still are. The video's own fullscreen is its player's
/// (epic E21); the GPUI placeholder takes `f` without an effect.
#[test]
fn reads_the_current_media_kind_for_every_key() {
    let mut session = session_with(&asset(1, "image"));
    for (key, outcome) in [
        (ViewerKey::Previous, KeyOutcome::Previous),
        (ViewerKey::Next, KeyOutcome::Next),
        (ViewerKey::ResetZoom, KeyOutcome::Handled),
        (ViewerKey::ZoomIn, KeyOutcome::Handled),
        (ViewerKey::ZoomOut, KeyOutcome::Handled),
    ] {
        assert_eq!(
            session.key(key, StageKind::Image, true, false),
            outcome,
            "{key:?}"
        );
    }
    let video = asset(2, "video");
    session.sync(&video, MS(10));
    let kind = video.kind();
    assert_eq!(kind, StageKind::Video);
    for key in [ViewerKey::ResetZoom, ViewerKey::ZoomIn, ViewerKey::ZoomOut] {
        assert_eq!(
            session.key(key, kind, true, false),
            KeyOutcome::Ignored,
            "{key:?}"
        );
    }
    assert_eq!(
        session.key(ViewerKey::Next, kind, true, false),
        KeyOutcome::Next
    );
    assert_eq!(
        session.key(ViewerKey::ToggleFullscreen, kind, true, false),
        KeyOutcome::Handled
    );
    assert!(!session.is_fullscreen());
}

/// keyboard.test.tsx, "stops shortcuts when disabled, closed or disposed and reopens
/// with one listener": the keys are off under another layer and under the delete
/// confirmation (`keyboardEnabled`), and on again after; a logically closed lightbox
/// takes none; a new opening takes them again. (One listener per opening is the
/// view's: `tests/lightbox_ported.rs` of the adapter.)
#[test]
fn stops_shortcuts_when_disabled_closed_or_disposed_and_reopens() {
    let mut session = session_with(&asset(1, "image"));
    let image = StageKind::Image;
    assert_eq!(
        session.key(ViewerKey::Next, image, true, false),
        KeyOutcome::Next
    );
    // Disabled: another layer on top, then the confirmation.
    assert_eq!(
        session.key(ViewerKey::Next, image, false, false),
        KeyOutcome::Ignored
    );
    assert_eq!(
        session.key(ViewerKey::Next, image, true, false),
        KeyOutcome::Next
    );
    session.open_delete_confirm(MS(1));
    assert!(!session.keyboard_enabled(true));
    assert_eq!(
        session.key(ViewerKey::Next, image, true, false),
        KeyOutcome::Ignored
    );
    session.close_delete_confirm(MS(2));
    assert_eq!(
        session.key(ViewerKey::Next, image, true, false),
        KeyOutcome::Next
    );
    // Closed.
    session.deactivate(false, MS(3));
    assert!(!session.keyboard_enabled(true));
    for key in [
        ViewerKey::Next,
        ViewerKey::ZoomIn,
        ViewerKey::ToggleFullscreen,
    ] {
        assert_eq!(session.key(key, image, true, false), KeyOutcome::Ignored);
    }
    // Reopened: a new session.
    let mut reopened = session_with(&asset(1, "image"));
    assert_eq!(
        reopened.key(ViewerKey::Next, image, true, false),
        KeyOutcome::Next
    );
    reopened.deactivate(false, MS(4));
    assert_eq!(
        reopened.key(ViewerKey::Next, image, true, false),
        KeyOutcome::Ignored
    );
}

/// keyboard.test.tsx, "exits the image DOM fullscreen when navigation reaches a video
/// and routes F and Escape to the video": the image half. `F` makes the window
/// fullscreen; a selected video takes it back once, and Escape is then the lightbox's
/// close again. (`F` and Escape of the video's own fullscreen belong to its player,
/// epic E21.)
#[test]
fn leaves_the_image_fullscreen_when_navigation_reaches_a_video() {
    let mut session = session_with(&asset(1, "image"));
    assert_eq!(
        session.key(ViewerKey::ToggleFullscreen, StageKind::Image, true, false),
        KeyOutcome::ToggleWindowFullscreen
    );
    assert!(
        !session.sync_fullscreen(StageKind::Image, true),
        "the window answered"
    );
    assert!(session.is_fullscreen());

    let video = asset(2, "video");
    session.sync(&video, MS(10));
    assert!(
        session.sync_fullscreen(StageKind::Video, true),
        "the window is taken out of fullscreen once"
    );
    assert!(!session.is_fullscreen());
    assert!(!session.sync_fullscreen(StageKind::Video, false));
    assert_eq!(
        session.key(ViewerKey::ToggleFullscreen, StageKind::Video, true, false),
        KeyOutcome::Handled
    );
    assert!(!session.is_fullscreen());
    // Not fullscreen: Escape is left to the lightbox's close.
    assert_eq!(session.escape(false, MS(20)), Dismissal::Close);
}

/// keyboard.test.tsx, "keeps another element's fullscreen untouched and exits the
/// shell's when the lightbox is deactivated".
#[test]
fn keeps_another_fullscreen_untouched_and_exits_its_own_when_deactivated() {
    // The window was fullscreen before (another element's): the lightbox does not
    // take it back.
    let mut session = session_with(&asset(1, "image"));
    assert_eq!(
        session.toggle_fullscreen(StageKind::Image, true),
        KeyOutcome::Handled,
        "a fullscreen window is not toggled"
    );
    assert!(!session.deactivate(true, MS(10)));
    // A window the lightbox made fullscreen is taken back with it.
    let mut session = session_with(&asset(1, "image"));
    assert_eq!(
        session.toggle_fullscreen(StageKind::Image, false),
        KeyOutcome::ToggleWindowFullscreen
    );
    session.sync_fullscreen(StageKind::Image, true);
    assert!(session.deactivate(true, MS(10)));
    assert!(!session.is_fullscreen());
    // Not fullscreen at all: nothing to take back.
    let mut session = session_with(&asset(1, "image"));
    assert!(!session.deactivate(false, MS(10)));
}

/// keyboard.test.tsx, "drops a shell fullscreen request that resolves after the
/// selection moved to a video": the session takes the request back once, and the
/// window's late answer neither turns the lightbox's fullscreen on again nor makes
/// the session ask again. The window itself (`WindowFullscreen`, which the view asks
/// whenever the session says so) is toggled once on the way in and, as `dom.exit` is
/// called in the Solid case, once when it finishes entering: never while its answer
/// is still due.
#[test]
fn drops_a_fullscreen_request_that_resolves_after_the_selection_moved_to_a_video() {
    let mut session = session_with(&asset(1, "image"));
    let mut window = WindowFullscreen::new();
    assert_eq!(
        session.toggle_fullscreen(StageKind::Image, false),
        KeyOutcome::ToggleWindowFullscreen
    );
    assert!(window.request(session.is_fullscreen(), false, MS(0)));
    session.sync(&asset(2, "video"), MS(10));
    // The window has not answered yet.
    assert!(
        session.sync_fullscreen(StageKind::Video, false),
        "taken back once"
    );
    assert!(!session.is_fullscreen());
    assert!(
        !window.request(session.is_fullscreen(), false, MS(10)),
        "the window is not toggled again before it answers"
    );
    // The window finishes entering fullscreen only now, then leaves it again.
    assert!(
        window.window_changed(true, MS(20)),
        "taken out once it is fullscreen"
    );
    assert!(!session.sync_fullscreen(StageKind::Video, true));
    assert!(!session.is_fullscreen());
    assert!(!window.window_changed(false, MS(30)));
    assert!(!session.sync_fullscreen(StageKind::Video, false));
    assert!(!session.is_fullscreen());
}
