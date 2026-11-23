//! The image viewer of the lightbox (`createImageControls.ts`): the zoom and pan of
//! the shown picture, the drag that moves a zoomed picture, and the short pauses
//! after a gesture in which a click neither zooms nor closes the lightbox.
//!
//! [`ImageControls`] belongs to the lightbox view and goes with it. It has no timers
//! and reads no clock: every call that depends on time takes `now`, the duration
//! since any fixed origin (the same clock as [`super::presentation`]). Positions are
//! in the coordinates the stage was measured in ([`Viewport`]).
//!
//! The Solid owner writes the transform to the DOM once per animation frame; the GPUI
//! view draws [`ImageControls::image_rect`] in every frame it renders, so there is
//! nothing to coalesce here. Fullscreen is the window's and has its own small machine,
//! [`Fullscreen`].

use std::time::Duration;

use super::pan_zoom::{
    calculate_fitted_media_size, clamp_pan, clamp_zoom_factor, next_zoom_factor, FittedMedia,
    MediaDimensions, Pan, ZoomDirection,
};

/// A drag that moved further than this, on either axis, is no click.
pub const DRAG_THRESHOLD: f64 = 3.0;
/// After a drag that moved, a click does not zoom for this long.
pub const CLICK_SUPPRESSION_AFTER_DRAG: Duration = Duration::from_millis(120);
/// The backdrop does not close the lightbox for this long after a drag ended, a
/// wheel turn or a zooming click, and after a press that started a drag.
pub const CLOSE_SUPPRESSION_AFTER_DRAG: Duration = Duration::from_millis(240);
pub const CLOSE_SUPPRESSION_AFTER_ZOOM: Duration = Duration::from_millis(360);
pub const CLOSE_SUPPRESSION_AFTER_PRESS: Duration = Duration::from_millis(500);
/// A wheel event counts for at most this many pixels.
pub const WHEEL_DELTA_LIMIT: f64 = 100.0;
/// The zoom factor is multiplied by `exp(-delta * WHEEL_ZOOM_RATE)`.
pub const WHEEL_ZOOM_RATE: f64 = 0.002;
/// A wheel line (`deltaMode` 1) in pixels.
pub const WHEEL_LINE: f64 = 16.0;

/// Where the stage (the media viewport) was laid out.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Viewport {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Viewport {
    pub fn size(&self) -> MediaDimensions {
        MediaDimensions::new(self.width, self.height)
    }

    fn center(&self) -> (f64, f64) {
        (self.left + self.width / 2.0, self.top + self.height / 2.0)
    }
}

/// The unit of a wheel delta (`WheelEvent.deltaMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelMode {
    Pixels,
    Lines,
    Pages,
}

/// Where the picture is drawn, relative to the stage's top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImageRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    x: f64,
    y: f64,
    origin: Pan,
    moved: bool,
}

/// The asset whose dimensions the viewer last started from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Subject {
    id: i64,
    width: Option<i64>,
    height: Option<i64>,
}

/// The viewer's state.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageControls {
    subject: Option<Subject>,
    /// The selected asset is an image or a GIF, not a video.
    image: bool,
    /// The picture is on the stage (its source is known and it did not fail).
    mounted: bool,
    active: bool,
    disposed: bool,
    viewport: Option<Viewport>,
    intrinsic: MediaDimensions,
    zoom: f64,
    pan: Pan,
    drag: Option<Drag>,
    suppress_click_until: Duration,
    suppress_close_until: Duration,
}

impl Default for ImageControls {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageControls {
    /// The viewer of a lightbox that just opened: nothing measured, nothing selected.
    pub fn new() -> Self {
        Self {
            subject: None,
            image: true,
            mounted: false,
            active: true,
            disposed: false,
            viewport: None,
            intrinsic: MediaDimensions::default(),
            zoom: 1.0,
            pan: Pan::ZERO,
            drag: None,
            suppress_click_until: Duration::ZERO,
            suppress_close_until: Duration::ZERO,
        }
    }

    fn size(&self) -> MediaDimensions {
        self.viewport
            .map(|viewport| viewport.size())
            .unwrap_or_default()
    }

    /// `mediaDisplaySize`: the picture fitted into the stage at zoom 1.
    pub fn fitted(&self) -> FittedMedia {
        calculate_fitted_media_size(self.intrinsic, self.size())
    }

    /// `zoomFactor`.
    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn pan(&self) -> Pan {
        self.pan
    }

    /// The size of the picture as last reported: the asset's stored dimensions, then
    /// the decoded picture's.
    pub fn intrinsic(&self) -> MediaDimensions {
        self.intrinsic
    }

    /// The stage as last measured.
    pub fn viewport(&self) -> Option<Viewport> {
        self.viewport
    }

    /// `isZoomed`.
    pub fn is_zoomed(&self) -> bool {
        self.zoom > 1.0
    }

    /// `isDragging`.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn is_image(&self) -> bool {
        self.image
    }

    /// Where the picture is drawn in the stage: its fitted size times the zoom,
    /// centred and moved by the pan. `None` while a size is unknown.
    pub fn image_rect(&self) -> Option<ImageRect> {
        let fitted = self.fitted();
        let size = self.size();
        if fitted.width <= 0.0 || fitted.height <= 0.0 {
            return None;
        }
        let width = fitted.width * self.zoom;
        let height = fitted.height * self.zoom;
        Some(ImageRect {
            left: (size.width - width) / 2.0 + self.pan.x,
            top: (size.height - height) / 2.0 + self.pan.y,
            width,
            height,
        })
    }

    /// The selected asset. Another asset, or other stored dimensions of the same one,
    /// start over: a drag ends, the zoom and the pan are reset and the size is the
    /// stored one until the picture reports its own. A republished selection with the
    /// same dimensions (a tag or favorite write) keeps zoom and pan.
    pub fn select(
        &mut self,
        id: i64,
        image: bool,
        width: Option<i64>,
        height: Option<i64>,
        now: Duration,
    ) {
        self.image = image;
        let subject = Subject { id, width, height };
        if self.subject == Some(subject) {
            return;
        }
        self.subject = Some(subject);
        self.finish_drag(now);
        self.reset();
        self.suppress_click_until = Duration::ZERO;
        self.intrinsic = MediaDimensions::new(
            width.unwrap_or_default() as f64,
            height.unwrap_or_default() as f64,
        );
    }

    /// Whether the picture is on the stage. Without it nothing zooms.
    pub fn set_mounted(&mut self, mounted: bool) {
        self.mounted = mounted;
    }

    /// The lightbox closed logically (`false`): a drag ends. The owner leaves
    /// fullscreen ([`Fullscreen::leave`]).
    pub fn set_active(&mut self, active: bool, now: Duration) {
        self.active = active;
        if !active {
            self.finish_drag(now);
        }
    }

    /// The stage was measured. A pan that no longer fits is pulled back.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        if !self.active {
            return;
        }
        self.viewport = Some(viewport);
        self.reclamp();
    }

    /// `onLoad`: the decoded picture's own size replaces the stored one.
    pub fn loaded(&mut self, width: f64, height: f64) {
        if self.disposed || !self.active || !self.image {
            return;
        }
        self.intrinsic = MediaDimensions::new(width, height);
        self.reclamp();
    }

    fn reclamp(&mut self) {
        self.pan = clamp_pan(self.pan, self.zoom, self.fitted().dimensions(), self.size());
    }

    /// `resetZoom`, the `0` key.
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.pan = Pan::ZERO;
    }

    /// The `+` and `=` keys: one step in, around the centre of the stage.
    pub fn zoom_in(&mut self) {
        self.update_zoom(next_zoom_factor(self.zoom, ZoomDirection::In), None);
    }

    /// The `-` key.
    pub fn zoom_out(&mut self) {
        self.update_zoom(next_zoom_factor(self.zoom, ZoomDirection::Out), None);
    }

    /// Sets the zoom factor so that the point of the picture under `anchor` (the
    /// centre of the stage without one) stays where it is, as far as the pan's bounds
    /// allow.
    fn update_zoom(&mut self, next: f64, anchor: Option<(f64, f64)>) {
        let Some(viewport) = self.viewport else {
            return;
        };
        if !self.mounted || !self.image {
            return;
        }
        let display = self.fitted();
        let size = viewport.size();
        if display.width <= 0.0 || display.height <= 0.0 || size.width <= 0.0 || size.height <= 0.0
        {
            return;
        }
        let value = clamp_zoom_factor(next);
        let center = viewport.center();
        let (anchor_x, anchor_y) = anchor.unwrap_or(center);
        let x = anchor_x - center.0;
        let y = anchor_y - center.1;
        self.pan = clamp_pan(
            Pan {
                x: x - ((x - self.pan.x) / self.zoom) * value,
                y: y - ((y - self.pan.y) / self.zoom) * value,
            },
            value,
            display.dimensions(),
            size,
        );
        self.zoom = value;
    }

    /// A wheel turn over the stage with the DOM's `deltaY` (down is positive), anchored
    /// at the pointer. Returns whether the viewer took it; a video's stage and a turn
    /// without a vertical part do not.
    pub fn wheel(&mut self, delta_y: f64, mode: WheelMode, x: f64, y: f64, now: Duration) -> bool {
        if !self.active || !self.image || delta_y == 0.0 || delta_y.is_nan() {
            return false;
        }
        self.suppress_close_until = now + CLOSE_SUPPRESSION_AFTER_ZOOM;
        let multiplier = match mode {
            WheelMode::Pixels => 1.0,
            WheelMode::Lines => WHEEL_LINE,
            WheelMode::Pages => self.size().height.max(1.0),
        };
        let delta = (delta_y * multiplier).clamp(-WHEEL_DELTA_LIMIT, WHEEL_DELTA_LIMIT);
        self.update_zoom(self.zoom * (-delta * WHEEL_ZOOM_RATE).exp(), Some((x, y)));
        true
    }

    /// `onClick` of the picture: one step in at the pointer, unless a drag that moved
    /// has just ended. Returns whether the click zoomed.
    pub fn click(&mut self, x: f64, y: f64, now: Duration) -> bool {
        if !self.image || now < self.suppress_click_until {
            return false;
        }
        self.suppress_close_until = now + CLOSE_SUPPRESSION_AFTER_ZOOM;
        self.update_zoom(next_zoom_factor(self.zoom, ZoomDirection::In), Some((x, y)));
        true
    }

    /// `onPointerDown` on the stage: the primary button on a zoomed picture starts a
    /// drag. Returns whether it did.
    pub fn pointer_down(&mut self, primary: bool, x: f64, y: f64, now: Duration) -> bool {
        if !self.active || !self.image || !primary || self.zoom <= 1.0 {
            return false;
        }
        self.suppress_close_until = now + CLOSE_SUPPRESSION_AFTER_PRESS;
        self.drag = Some(Drag {
            x,
            y,
            origin: self.pan,
            moved: false,
        });
        true
    }

    /// The pointer moved while a drag is held. Returns whether a drag took it.
    pub fn pointer_move(&mut self, x: f64, y: f64) -> bool {
        let Some(drag) = &mut self.drag else {
            return false;
        };
        let (dx, dy) = (x - drag.x, y - drag.y);
        if dx.abs() > DRAG_THRESHOLD || dy.abs() > DRAG_THRESHOLD {
            drag.moved = true;
        }
        let next = Pan::new(drag.origin.x + dx, drag.origin.y + dy);
        self.pan = clamp_pan(next, self.zoom, self.fitted().dimensions(), self.size());
        true
    }

    /// The button was released, the pointer was cancelled or the window lost its
    /// focus: the drag ends.
    pub fn pointer_up(&mut self, now: Duration) {
        self.finish_drag(now);
    }

    fn finish_drag(&mut self, now: Duration) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        self.suppress_close_until = now + CLOSE_SUPPRESSION_AFTER_DRAG;
        if drag.moved {
            self.suppress_click_until = now + CLICK_SUPPRESSION_AFTER_DRAG;
        }
    }

    /// `tryClose`: whether a press on the backdrop may close the lightbox now.
    pub fn may_close(&self, now: Duration) -> bool {
        self.drag.is_none() && now >= self.suppress_close_until
    }

    /// The view is going away.
    pub fn dispose(&mut self, now: Duration) {
        self.disposed = true;
        self.finish_drag(now);
    }
}

/// Fullscreen of the lightbox "through the window": the Solid shell asks the browser
/// for fullscreen of its element; the GPUI view asks the window manager for a
/// fullscreen window and lays the shell out over all of it.
///
/// The window answers later, so the machine keeps what it asked for: whether it was
/// the one that made the window fullscreen (then it also takes it back), and whether
/// the window has been seen fullscreen since (then a window that is no longer
/// fullscreen was taken out by the user, and the lightbox follows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fullscreen {
    on: bool,
    toggled_window: bool,
    seen: bool,
}

impl Fullscreen {
    pub fn new() -> Self {
        Self::default()
    }

    /// `isFullscreen`.
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Enters fullscreen. Returns whether the window has to be toggled; a window that
    /// is fullscreen already stays as it is, and stays so afterwards.
    pub fn enter(&mut self, window_fullscreen: bool) -> bool {
        if self.on {
            return false;
        }
        self.on = true;
        self.toggled_window = !window_fullscreen;
        self.seen = window_fullscreen;
        self.toggled_window
    }

    /// Leaves fullscreen. Returns whether the window has to be toggled back.
    pub fn leave(&mut self, window_fullscreen: bool) -> bool {
        if !self.on {
            return false;
        }
        self.on = false;
        // A window that was asked and has not answered yet is taken back as well;
        // `WindowFullscreen` waits for its answer before it asks.
        let toggle = self.toggled_window && (window_fullscreen || !self.seen);
        self.toggled_window = false;
        self.seen = false;
        toggle
    }

    /// The window's fullscreen state as it is now (`fullscreenchange`).
    pub fn window_changed(&mut self, window_fullscreen: bool) {
        if !self.on || !self.toggled_window {
            return;
        }
        if window_fullscreen {
            self.seen = true;
        } else if self.seen {
            *self = Self::default();
        }
    }
}

/// How long the window's answer to a request is waited for. A window manager answers
/// within a frame or the length of its own animation; a request that is older than
/// this was not granted (a compositor rule, a window manager without fullscreen) and
/// is forgotten.
pub const WINDOW_ANSWER_TIMEOUT: Duration = Duration::from_secs(3);

/// What the lightbox has asked of the window, for a window that can only be toggled
/// and answers later.
///
/// A second toggle before the answer does not mean the same everywhere: an X11 window
/// manager toggles the state it has, so the second request takes the first back; a
/// Wayland window repeats the request it made, since it still reads its old state.
/// So a request in flight is never followed by another: what is wanted by then is
/// kept, and the window is toggled once more when it has answered and something else
/// is wanted. The view keeps one of these for the window's lifetime, so an answer
/// that comes after the lightbox closed is still acted on.
///
/// A window that never grants a request does not keep it in flight for ever: after
/// [`WINDOW_ANSWER_TIMEOUT`] the request is forgotten. Otherwise a window the user
/// makes fullscreen much later would look like the late answer to a request the
/// lightbox has left behind, and would be toggled straight back out. Within that
/// time the two cannot be told apart, and the window is toggled back. A request that
/// was forgotten can be made again; a window that answers one later than that keeps
/// the state it was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowFullscreen {
    /// The state the window was asked for and has not reached yet, and when it was
    /// asked.
    asked: Option<(bool, Duration)>,
    /// The state the lightbox wants once it has.
    wanted: bool,
}

impl WindowFullscreen {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets a request the window has not answered in time.
    fn expire(&mut self, now: Duration) {
        if self
            .asked
            .is_some_and(|(_, at)| now.saturating_sub(at) >= WINDOW_ANSWER_TIMEOUT)
        {
            self.asked = None;
        }
    }

    /// The lightbox wants the window `fullscreen` or not, and the window is
    /// `window_fullscreen` now. Returns whether the window has to be toggled now.
    pub fn request(&mut self, fullscreen: bool, window_fullscreen: bool, now: Duration) -> bool {
        self.expire(now);
        if self
            .asked
            .is_some_and(|(asked, _)| asked == window_fullscreen)
        {
            self.asked = None;
        }
        self.wanted = fullscreen;
        if self.asked.is_some() || fullscreen == window_fullscreen {
            return false;
        }
        self.asked = Some((fullscreen, now));
        true
    }

    /// The window's fullscreen state as it is now. Returns whether the window has to
    /// be toggled: it reached what it was asked for in time, and by now the other
    /// state is wanted.
    pub fn window_changed(&mut self, window_fullscreen: bool, now: Duration) -> bool {
        self.expire(now);
        if !self
            .asked
            .is_some_and(|(asked, _)| asked == window_fullscreen)
        {
            return false;
        }
        self.asked = None;
        if self.wanted == window_fullscreen {
            return false;
        }
        self.asked = Some((self.wanted, now));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const STAGE: Viewport = Viewport {
        left: 100.0,
        top: 50.0,
        width: 800.0,
        height: 600.0,
    };

    /// A viewer showing a 1600 x 600 picture, which fits as 800 x 300.
    fn viewer() -> ImageControls {
        let mut controls = ImageControls::new();
        controls.select(1, true, Some(1600), Some(600), MS(0));
        controls.set_viewport(STAGE);
        controls.set_mounted(true);
        controls
    }

    #[test]
    fn keys_zoom_around_the_centre_and_reset() {
        let mut controls = viewer();
        assert_eq!(
            controls.image_rect(),
            Some(ImageRect {
                left: 0.0,
                top: 150.0,
                width: 800.0,
                height: 300.0
            })
        );
        controls.zoom_in();
        controls.zoom_in();
        assert_eq!(controls.zoom(), 1.5625);
        assert_eq!(controls.pan(), Pan::ZERO);
        assert!(controls.is_zoomed());
        controls.zoom_out();
        assert_eq!(controls.zoom(), 1.25);
        controls.reset();
        assert_eq!((controls.zoom(), controls.pan()), (1.0, Pan::ZERO));
        assert!(!controls.is_zoomed());
    }

    #[test]
    fn the_wheel_zooms_at_the_pointer_and_clamps_its_delta() {
        let mut controls = viewer();
        // 200 px right of the centre; a large turn counts as 100.
        assert!(controls.wheel(-480.0, WheelMode::Pixels, 700.0, 350.0, MS(0)));
        let zoom = (100.0f64 * WHEEL_ZOOM_RATE).exp();
        assert!((controls.zoom() - zoom).abs() < 1e-12);
        // The point under the pointer stays: x - (x / 1) * zoom.
        assert!((controls.pan().x - (200.0 - 200.0 * zoom)).abs() < 1e-9);
        assert_eq!(
            controls.pan().y,
            0.0,
            "the picture does not overflow downwards"
        );
        // Lines are 16 px each; down zooms out, never below 1.
        assert!(controls.wheel(3.0, WheelMode::Lines, 500.0, 350.0, MS(0)));
        assert!(controls.zoom() < zoom);
        for _ in 0..10 {
            controls.wheel(100.0, WheelMode::Pixels, 500.0, 350.0, MS(0));
        }
        assert_eq!((controls.zoom(), controls.pan()), (1.0, Pan::ZERO));
        assert!(!controls.wheel(0.0, WheelMode::Pixels, 500.0, 350.0, MS(0)));
    }

    #[test]
    fn a_drag_moves_a_zoomed_picture_within_its_bounds() {
        let mut controls = viewer();
        assert!(
            !controls.pointer_down(true, 500.0, 350.0, MS(0)),
            "not zoomed"
        );
        for _ in 0..4 {
            controls.zoom_in();
        }
        let zoom = controls.zoom();
        assert!(
            !controls.pointer_down(false, 500.0, 350.0, MS(0)),
            "secondary"
        );
        assert!(controls.pointer_down(true, 500.0, 350.0, MS(0)));
        assert!(controls.is_dragging());
        assert!(controls.pointer_move(560.0, 390.0));
        // The picture is 800 x 300 times the zoom: it overflows the stage both ways.
        assert_eq!(controls.pan(), Pan::new(60.0, 40.0));
        assert!(controls.pointer_move(5000.0, 350.0));
        assert_eq!(controls.pan().x, (800.0 * zoom - 800.0) / 2.0);
        controls.pointer_up(MS(1000));
        assert!(!controls.is_dragging());
        assert!(!controls.pointer_move(0.0, 0.0));
    }

    #[test]
    fn a_click_right_after_a_drag_does_not_zoom_and_the_backdrop_waits() {
        let mut controls = viewer();
        controls.zoom_in();
        assert!(controls.pointer_down(true, 500.0, 350.0, MS(1000)));
        assert!(!controls.may_close(MS(1100)), "a drag is held");
        controls.pointer_move(520.0, 350.0);
        controls.pointer_up(MS(2000));
        assert!(!controls.click(500.0, 350.0, MS(2119)));
        assert!(!controls.may_close(MS(2239)));
        assert!(controls.may_close(MS(2240)));
        assert!(controls.click(500.0, 350.0, MS(2120)));
        assert!(!controls.may_close(MS(2479)));
        assert!(controls.may_close(MS(2480)));
        // A press that did not move suppresses no click.
        assert!(controls.pointer_down(true, 500.0, 350.0, MS(3000)));
        controls.pointer_up(MS(3010));
        assert!(controls.click(500.0, 350.0, MS(3011)));
    }

    #[test]
    fn another_asset_starts_over_and_a_republished_one_keeps_the_view() {
        let mut controls = viewer();
        controls.zoom_in();
        controls.select(1, true, Some(1600), Some(600), MS(0));
        assert_eq!(controls.zoom(), 1.25);
        controls.select(2, true, Some(400), Some(400), MS(0));
        assert_eq!((controls.zoom(), controls.pan()), (1.0, Pan::ZERO));
        assert_eq!(
            controls.fitted().dimensions(),
            MediaDimensions::new(600.0, 600.0)
        );
        // The decoded picture's own size wins over the stored one.
        controls.loaded(1200.0, 600.0);
        assert_eq!(
            controls.fitted().dimensions(),
            MediaDimensions::new(800.0, 400.0)
        );
    }

    #[test]
    fn a_smaller_stage_pulls_the_pan_back() {
        // A square picture fits as 600 x 600 whatever the stage's width.
        let mut controls = viewer();
        controls.select(5, true, Some(600), Some(600), MS(0));
        for _ in 0..4 {
            controls.zoom_in();
        }
        controls.pointer_down(true, 500.0, 350.0, MS(0));
        controls.pointer_move(5000.0, 350.0);
        controls.pointer_up(MS(0));
        let zoom = controls.zoom();
        assert_eq!(controls.pan().x, (600.0 * zoom - 800.0) / 2.0);
        controls.set_viewport(Viewport {
            width: 1400.0,
            ..STAGE
        });
        assert_eq!(controls.pan().x, (600.0 * zoom - 1400.0) / 2.0);
    }

    #[test]
    fn nothing_zooms_without_a_picture_on_the_stage_or_for_a_video() {
        let mut controls = viewer();
        controls.set_mounted(false);
        controls.zoom_in();
        assert_eq!(controls.zoom(), 1.0);
        controls.set_mounted(true);
        controls.select(3, false, Some(160), Some(90), MS(0));
        controls.zoom_in();
        assert_eq!(controls.zoom(), 1.0);
        assert!(!controls.wheel(-100.0, WheelMode::Pixels, 500.0, 350.0, MS(0)));
        assert!(!controls.click(500.0, 350.0, MS(0)));
        assert!(!controls.pointer_down(true, 500.0, 350.0, MS(0)));
    }

    #[test]
    fn an_inactive_viewer_ends_its_drag_and_ignores_a_late_load() {
        let mut controls = viewer();
        controls.zoom_in();
        controls.pointer_down(true, 500.0, 350.0, MS(0));
        controls.set_active(false, MS(10));
        assert!(!controls.is_dragging());
        let fitted = controls.fitted();
        controls.loaded(10.0, 10.0);
        assert_eq!(controls.fitted(), fitted);
        controls.set_active(true, MS(20));
        controls.dispose(MS(30));
        controls.loaded(10.0, 10.0);
        assert_eq!(controls.fitted(), fitted);
    }

    #[test]
    fn fullscreen_takes_back_only_what_it_asked_of_the_window() {
        let mut fullscreen = Fullscreen::new();
        assert!(fullscreen.enter(false), "the window is toggled");
        assert!(fullscreen.is_on());
        assert!(!fullscreen.enter(false), "already on");
        fullscreen.window_changed(true);
        assert!(fullscreen.leave(true));
        assert!(!fullscreen.is_on());
        assert!(!fullscreen.leave(false));

        // A window that was fullscreen before stays fullscreen.
        assert!(!fullscreen.enter(true));
        assert!(!fullscreen.leave(true));

        // The user took the window out of fullscreen: the lightbox follows.
        assert!(fullscreen.enter(false));
        fullscreen.window_changed(false);
        assert!(fullscreen.is_on(), "the window has not answered yet");
        fullscreen.window_changed(true);
        fullscreen.window_changed(false);
        assert!(!fullscreen.is_on());

        // Left before the window answered: the request is taken back.
        assert!(fullscreen.enter(false));
        assert!(fullscreen.leave(false));
    }

    #[test]
    fn a_window_that_answers_at_once_is_toggled_on_every_request() {
        let mut window = WindowFullscreen::new();
        assert!(window.request(true, false, MS(0)));
        assert!(!window.window_changed(true, MS(0)), "it is what was wanted");
        assert!(window.request(false, true, MS(10)));
        assert!(!window.window_changed(false, MS(10)));
        // No answer is reported between two requests (GPUI's test window).
        assert!(window.request(true, false, MS(20)));
        assert!(window.request(false, true, MS(30)));
        assert!(window.request(true, false, MS(40)));
        // What the window already is needs no toggle.
        assert!(!window.request(true, true, MS(50)));
        assert!(!window.request(false, false, MS(60)));
    }

    /// The lightbox left fullscreen before the window answered (a second `f`, Escape,
    /// a selected video or a closed lightbox within the compositor's round trip). A
    /// second toggle would repeat the request on Wayland and leave the window
    /// fullscreen; the window is taken out when it answers instead.
    #[test]
    fn a_request_overtaken_before_the_answer_is_taken_back_when_the_window_answers() {
        let mut window = WindowFullscreen::new();
        assert!(window.request(true, false, MS(0)));
        assert!(
            !window.request(false, false, MS(10)),
            "no second toggle in flight"
        );
        assert!(!window.window_changed(false, MS(20)), "still no answer");
        assert!(
            window.window_changed(true, MS(400)),
            "answered: taken out now"
        );
        assert!(!window.window_changed(true, MS(410)), "asked once");
        assert!(!window.window_changed(false, MS(500)));
        // Settled: the next request is an ordinary one.
        assert!(window.request(true, false, MS(600)));
    }

    /// A held `f`: on, off and on again before the first answer ask the window once.
    #[test]
    fn requests_that_cancel_out_before_the_answer_ask_the_window_once() {
        let mut window = WindowFullscreen::new();
        assert!(window.request(true, false, MS(0)));
        assert!(!window.request(false, false, MS(10)));
        assert!(!window.request(true, false, MS(20)));
        assert!(
            !window.window_changed(true, MS(100)),
            "fullscreen is wanted again"
        );
        // And the other way round: leaving, then wanting it back before the answer.
        assert!(window.request(false, true, MS(200)));
        assert!(!window.request(true, true, MS(210)));
        assert!(window.window_changed(false, MS(300)), "back to fullscreen");
        assert!(!window.window_changed(true, MS(400)));
    }

    /// The window manager never grants the request (a compositor rule, no fullscreen
    /// support) and the lightbox has left fullscreen. A window the user makes
    /// fullscreen themselves long after is not the answer to that request: it stays
    /// fullscreen.
    #[test]
    fn a_request_the_window_never_grants_is_forgotten_and_a_later_fullscreen_stays() {
        let timeout = WINDOW_ANSWER_TIMEOUT;
        let mut window = WindowFullscreen::new();
        assert!(window.request(true, false, MS(0)));
        assert!(!window.request(false, false, MS(500)), "in flight");
        assert!(!window.window_changed(false, MS(600)), "no answer");
        assert!(
            !window.window_changed(true, timeout),
            "the user's own fullscreen is left alone"
        );
        assert!(!window.window_changed(true, timeout + MS(1)));
        assert!(!window.window_changed(false, timeout + MS(2)));
        // And the lightbox can ask again.
        assert!(window.request(true, false, timeout + MS(3)));
    }

    /// Until the request is that old, a window that turns fullscreen is taken to be
    /// its answer, and the request that follows it starts its own wait.
    #[test]
    fn an_answer_within_the_wait_is_acted_on_and_the_next_request_waits_anew() {
        let timeout = WINDOW_ANSWER_TIMEOUT;
        let last = timeout - MS(1);
        let mut window = WindowFullscreen::new();
        assert!(window.request(true, false, MS(0)));
        assert!(!window.request(false, false, MS(10)));
        assert!(window.window_changed(true, last), "answered in time");
        // The second toggle was asked at `last`: its answer is waited for from then.
        assert!(
            !window.request(true, true, timeout + MS(10)),
            "the way out is in flight"
        );
        assert!(
            window.window_changed(false, last + timeout - MS(1)),
            "out, and fullscreen is wanted again"
        );
    }

    /// A request that was never granted does not block the next one for ever, and
    /// leaving a fullscreen the window never reached asks nothing of it.
    #[test]
    fn a_forgotten_request_can_be_made_again() {
        let timeout = WINDOW_ANSWER_TIMEOUT;
        let mut window = WindowFullscreen::new();
        assert!(window.request(true, false, MS(0)));
        assert!(!window.request(true, false, timeout - MS(1)), "in flight");
        assert!(window.request(true, false, timeout), "asked again");
        assert!(
            !window.request(false, false, timeout * 2),
            "the window is not fullscreen: nothing to take back"
        );
        assert!(!window.window_changed(true, timeout * 3));
    }
}
