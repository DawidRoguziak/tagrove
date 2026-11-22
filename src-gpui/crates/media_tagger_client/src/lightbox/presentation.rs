//! What the open lightbox decides about its own presentation
//! (`createLightboxPresentation.ts`): whether the side panel is open, whether the
//! floating controls are shown, and the "copied" confirmation of the group name.
//!
//! These three machines belong to the lightbox view and go with it, as the Solid
//! owners go with the mounted modal; nothing here outlives a close. They have no
//! timers of their own: time is passed in as the duration since any fixed origin, so
//! they run on a fake clock in tests, and each one names the moment its state changes
//! next ([`Sidebar::deadline`], [`Activity::deadline`], [`GroupClipboard::deadline`])
//! for the view to draw again then.
//!
//! # Deadlines
//!
//! `deadline(now)` is on the clock of `now` and is always later than `now`: once the
//! moment has come, the state it stood for has changed (`occupied`, `visible` and
//! `confirmed` take the same `now`) and there is no deadline until a call starts a
//! new one. A view therefore draws, asks every machine for its deadline with the time
//! of that frame, and sets one timer for the earliest; a timer that fires late or
//! early finds either nothing or the same future moment, never a moment that has
//! passed, so re-arming from it cannot spin.

use std::time::Duration;

use crate::search::text::js_trim;

/// The controls fade after this long without activity.
pub const ACTIVITY_TIMEOUT: Duration = Duration::from_millis(3000);
/// The side panel keeps its column for this long while it slides out.
pub const SIDEBAR_EXIT: Duration = Duration::from_millis(200);
/// How long the copy button shows that the group name was copied.
pub const COPY_CONFIRMATION: Duration = Duration::from_millis(1600);
/// `(max-width: 767px)`: up to this window width the side panel is a drawer that
/// starts closed.
pub const NARROW_MAX_WIDTH: f32 = 767.0;

/// Whether a window of `width` logical pixels has the narrow layout.
pub fn is_narrow(width: f32) -> bool {
    width <= NARROW_MAX_WIDTH
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Retained {
    No,
    /// The panel is open.
    Yes,
    /// The panel closed and slides out until then.
    Until(Duration),
}

/// The side panel (`createLightboxSidebar`): open by default in a wide window and
/// closed in a narrow one, until the user chooses; a choice lasts until the lightbox
/// closes. While it is locked (the delete confirmation is open, a deletion runs) it
/// is open and cannot be hidden.
#[derive(Debug, Clone, PartialEq)]
pub struct Sidebar {
    narrow: bool,
    choice: Option<bool>,
    locked: bool,
    active: bool,
    reduced_motion: bool,
    retained: Retained,
}

impl Sidebar {
    /// The panel of a lightbox that just opened in a window that is `narrow` or not.
    pub fn new(narrow: bool, reduced_motion: bool) -> Self {
        let mut sidebar = Self {
            narrow,
            choice: None,
            locked: false,
            active: true,
            reduced_motion,
            retained: Retained::No,
        };
        if sidebar.open() {
            sidebar.retained = Retained::Yes;
        }
        sidebar
    }

    /// `narrow`: the panel is a drawer over the media.
    pub fn narrow(&self) -> bool {
        self.narrow
    }

    /// `open`.
    pub fn open(&self) -> bool {
        self.locked || self.choice.unwrap_or(!self.narrow)
    }

    /// `occupied`: the panel is open or still slides out; its column stays.
    pub fn occupied(&self, now: Duration) -> bool {
        self.open()
            || match self.retained {
                Retained::Yes => true,
                Retained::Until(until) => now < until,
                Retained::No => false,
            }
    }

    /// When [`occupied`](Self::occupied) ends by itself: the end of the slide of a
    /// panel that closed. `None` while the panel is open, and from that moment on.
    pub fn deadline(&self, now: Duration) -> Option<Duration> {
        match self.retained {
            Retained::Until(until) if !self.open() && now < until => Some(until),
            _ => None,
        }
    }

    /// Applies a change and starts or ends the retention of the column.
    fn change(&mut self, now: Duration, change: impl FnOnce(&mut Self)) {
        let before = self.open();
        change(self);
        let after = self.open();
        if after {
            self.retained = Retained::Yes;
        } else if before {
            let exit = if self.reduced_motion {
                Duration::ZERO
            } else {
                SIDEBAR_EXIT
            };
            self.retained = Retained::Until(now + exit);
        }
    }

    /// The window crossed the breakpoint. Without a choice the panel follows it.
    pub fn set_narrow(&mut self, narrow: bool, now: Duration) {
        self.change(now, |sidebar| sidebar.narrow = narrow);
    }

    pub fn set_locked(&mut self, locked: bool, now: Duration) {
        self.change(now, |sidebar| sidebar.locked = locked);
    }

    pub fn set_reduced_motion(&mut self, reduced_motion: bool) {
        self.reduced_motion = reduced_motion;
    }

    /// The lightbox closed (`false`) or opened: a close forgets the choice.
    pub fn set_active(&mut self, active: bool, now: Duration) {
        self.change(now, |sidebar| {
            sidebar.active = active;
            if !active {
                sidebar.choice = None;
            }
        });
    }

    /// `show`.
    pub fn show(&mut self, now: Duration) {
        self.change(now, |sidebar| sidebar.choice = Some(true));
    }

    /// `hide`: nothing while the panel is locked.
    pub fn hide(&mut self, now: Duration) {
        if !self.locked {
            self.change(now, |sidebar| sidebar.choice = Some(false));
        }
    }
}

/// The floating controls (`createLightboxActivity`): shown when the lightbox opens
/// and for [`ACTIVITY_TIMEOUT`] after every pointer movement, press, wheel, scroll or
/// key; always shown on a touch device. Focus alone does not show them.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    active: bool,
    visible: bool,
    touch: bool,
    hide_at: Option<Duration>,
    pointer: Option<(f32, f32)>,
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

impl Activity {
    /// The controls of a lightbox that is not active yet: shown, no countdown.
    pub fn new() -> Self {
        Self {
            active: false,
            visible: true,
            touch: false,
            hide_at: None,
            pointer: None,
        }
    }

    /// A countdown that ran out hides the controls.
    fn settle(&mut self, now: Duration) {
        if self.hide_at.is_some_and(|at| now >= at) {
            self.visible = false;
            self.hide_at = None;
        }
    }

    /// `visible`.
    pub fn visible(&self, now: Duration) -> bool {
        self.touch || (self.visible && !self.hide_at.is_some_and(|at| now >= at))
    }

    /// When [`visible`](Self::visible) ends unless something happens before. `None`
    /// while the controls are hidden (also from the moment the countdown ran out,
    /// whether or not a later call has taken note of it), while the lightbox is not
    /// active, and on a touch device, where they stay.
    pub fn deadline(&self, now: Duration) -> Option<Duration> {
        self.hide_at.filter(|at| !self.touch && now < *at)
    }

    /// The lightbox opened (`true`) or closed logically. Opening shows the controls
    /// and starts the countdown; closing ends it. `touch_device`:
    /// `(hover: none) and (pointer: coarse)`.
    pub fn set_active(&mut self, active: bool, touch_device: bool, now: Duration) {
        self.settle(now);
        if active == self.active {
            return;
        }
        self.active = active;
        if active {
            self.touch = touch_device;
            self.pointer = None;
            self.reveal(now);
        } else {
            self.hide_at = None;
        }
    }

    /// The device gained or lost its touch-only pointer.
    pub fn set_touch_device(&mut self, touch_device: bool) {
        if self.active {
            self.touch = touch_device;
        }
    }

    /// `reveal`: shows the controls and starts the countdown again. Nothing while the
    /// lightbox is not active.
    pub fn reveal(&mut self, now: Duration) {
        self.settle(now);
        if !self.active {
            return;
        }
        self.visible = true;
        self.hide_at = Some(now + ACTIVITY_TIMEOUT);
    }

    /// The pointer is at `(x, y)`. A repeated event at the same place is no movement.
    pub fn pointer_moved(&mut self, x: f32, y: f32, now: Duration) {
        if !self.active || self.pointer == Some((x, y)) {
            return;
        }
        self.pointer = Some((x, y));
        self.reveal(now);
    }

    /// A pointer press; one made by touch keeps the controls shown from then on.
    pub fn pointer_down(&mut self, touch: bool, now: Duration) {
        if !self.active {
            return;
        }
        if touch {
            self.touch = true;
        }
        self.reveal(now);
    }

    /// A wheel turn, a scroll or a key press. The event keeps its normal meaning.
    pub fn input(&mut self, now: Duration) {
        self.reveal(now);
    }
}

/// A copy of the group name that was started and is not confirmed yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyRequest {
    generation: u64,
    asset_id: i64,
    /// The trimmed group name to put on the clipboard.
    pub text: String,
}

/// The copy button of the group name (`createGroupClipboard`): it confirms a copy for
/// [`COPY_CONFIRMATION`], unless the group name or the asset changed meanwhile.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupClipboard {
    key: String,
    asset_id: i64,
    generation: u64,
    confirmed_until: Option<Duration>,
    disposed: bool,
}

impl GroupClipboard {
    /// The button for the group name `key` of asset `asset_id`.
    pub fn new(key: impl Into<String>, asset_id: i64) -> Self {
        Self {
            key: key.into(),
            asset_id,
            generation: 0,
            confirmed_until: None,
            disposed: false,
        }
    }

    /// The group name or the asset the button belongs to; a change ends the
    /// confirmation and makes a copy that is on its way obsolete.
    pub fn sync(&mut self, key: &str, asset_id: i64) {
        if self.key == key && self.asset_id == asset_id {
            return;
        }
        self.key = key.to_owned();
        self.asset_id = asset_id;
        self.generation += 1;
        self.confirmed_until = None;
    }

    /// `canCopy`: there is a group name to copy.
    pub fn can_copy(&self) -> bool {
        !js_trim(&self.key).is_empty()
    }

    /// `confirmed`.
    pub fn confirmed(&self, now: Duration) -> bool {
        self.confirmed_until.is_some_and(|until| now < until)
    }

    /// When [`confirmed`](Self::confirmed) ends. `None` without a confirmation, and
    /// from that moment on.
    pub fn deadline(&self, now: Duration) -> Option<Duration> {
        self.confirmed_until.filter(|until| now < *until)
    }

    /// The locale key of the button's title.
    pub fn title_key(&self, now: Duration) -> &'static str {
        if self.confirmed(now) {
            "lightbox.copyMediaGroup.copied"
        } else if self.can_copy() {
            "lightbox.copyMediaGroup.title"
        } else {
            "lightbox.copyMediaGroup.unavailable"
        }
    }

    /// `copy`, first half: what to write to the clipboard; `None` without a group
    /// name. The view writes [`CopyRequest::text`] and reports the outcome with
    /// [`finish_copy`](Self::finish_copy).
    pub fn begin_copy(&self) -> Option<CopyRequest> {
        (self.can_copy() && !self.disposed).then(|| CopyRequest {
            generation: self.generation,
            asset_id: self.asset_id,
            text: js_trim(&self.key).to_owned(),
        })
    }

    /// `copy`, second half. Returns whether the copy is confirmed: it was written and
    /// the button still shows the same group name of the same asset.
    pub fn finish_copy(&mut self, request: &CopyRequest, written: bool, now: Duration) -> bool {
        if self.disposed || request.generation != self.generation {
            return false;
        }
        if !written {
            self.confirmed_until = None;
            return false;
        }
        if request.text != js_trim(&self.key) || request.asset_id != self.asset_id {
            return false;
        }
        self.confirmed_until = Some(now + COPY_CONFIRMATION);
        true
    }

    /// The view is going away: no later outcome confirms anything.
    pub fn dispose(&mut self) {
        self.disposed = true;
        self.confirmed_until = None;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{is_narrow, Activity, GroupClipboard, Sidebar};

    const fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn the_breakpoint_is_the_css_media_query() {
        assert!(is_narrow(767.0));
        assert!(!is_narrow(768.0));
    }

    #[test]
    fn controls_hide_after_three_seconds_and_only_movement_restarts_the_countdown() {
        let mut activity = Activity::new();
        activity.set_active(true, false, ms(0));
        assert!(activity.visible(ms(2999)));

        activity.pointer_moved(10.0, 20.0, ms(2999));
        assert!(activity.visible(ms(5998)));
        // The same place again is no movement.
        activity.pointer_moved(10.0, 20.0, ms(5998));
        assert!(!activity.visible(ms(5999)));

        activity.pointer_moved(11.0, 20.0, ms(6000));
        assert!(activity.visible(ms(6000)));
    }

    #[test]
    fn closing_ends_the_countdown_and_a_reveal_while_closed_starts_none() {
        let mut activity = Activity::new();
        activity.set_active(true, false, ms(0));
        assert!(!activity.visible(ms(3000)));
        activity.input(ms(3000));
        assert!(activity.visible(ms(3000)));
        assert_eq!(activity.deadline(ms(3000)), Some(ms(6000)));

        activity.set_active(false, false, ms(3100));
        assert_eq!(activity.deadline(ms(3100)), None);
        activity.reveal(ms(3200));
        activity.pointer_down(false, ms(3200));
        assert_eq!(activity.deadline(ms(3200)), None);
    }

    #[test]
    fn touch_keeps_the_controls_shown() {
        let mut activity = Activity::new();
        activity.set_active(true, true, ms(0));
        assert!(activity.visible(ms(10_000)));
        assert_eq!(activity.deadline(ms(0)), None);

        let mut pressed = Activity::new();
        pressed.set_active(true, false, ms(0));
        pressed.pointer_down(true, ms(100));
        assert!(pressed.visible(ms(60_000)));
    }

    #[test]
    fn the_panel_follows_the_breakpoint_until_a_choice_and_keeps_its_column_while_closing() {
        let mut sidebar = Sidebar::new(false, false);
        assert!(sidebar.open());

        sidebar.set_narrow(true, ms(0));
        assert!(!sidebar.open());
        assert!(sidebar.occupied(ms(199)));
        assert_eq!(sidebar.deadline(ms(199)), Some(ms(200)));
        assert!(!sidebar.occupied(ms(200)));

        sidebar.show(ms(300));
        sidebar.set_narrow(false, ms(400));
        sidebar.set_narrow(true, ms(500));
        assert!(sidebar.open(), "an explicit choice survives resizing");

        sidebar.set_locked(true, ms(600));
        sidebar.hide(ms(700));
        assert!(sidebar.open(), "a locked panel cannot be hidden");

        sidebar.set_locked(false, ms(800));
        sidebar.set_active(false, ms(800));
        sidebar.set_active(true, ms(900));
        assert!(!sidebar.open(), "a close forgets the choice");
    }

    #[test]
    fn reduced_motion_gives_the_column_back_at_once() {
        let mut sidebar = Sidebar::new(false, true);
        sidebar.hide(ms(50));
        assert!(!sidebar.occupied(ms(50)));
    }

    #[test]
    fn a_copy_is_confirmed_for_1600_ms_and_not_for_another_group_or_after_disposal() {
        let mut clipboard = GroupClipboard::new(" group ", 1);
        let request = clipboard.begin_copy().expect("a group name to copy");
        assert_eq!(request.text, "group");
        assert!(clipboard.finish_copy(&request, true, ms(0)));
        assert!(clipboard.confirmed(ms(1599)));
        assert!(!clipboard.confirmed(ms(1600)));

        let stale = clipboard.begin_copy().unwrap();
        clipboard.sync("other", 1);
        assert!(!clipboard.finish_copy(&stale, true, ms(2000)));
        assert!(!clipboard.confirmed(ms(2000)));

        let late = clipboard.begin_copy().unwrap();
        clipboard.dispose();
        assert!(!clipboard.finish_copy(&late, true, ms(3000)));
        assert_eq!(clipboard.deadline(ms(3000)), None);

        assert_eq!(GroupClipboard::new("  ", 1).begin_copy(), None);
    }

    /// What a view does with the machines: after each frame it sets a timer for the
    /// deadline it is given and draws again when the timer fires. Returns the frames
    /// drawn before no deadline is left; a deadline that is not later than the frame
    /// it was asked in would fire at once, for ever.
    fn frames_until_quiet(start: Duration, deadline: impl Fn(Duration) -> Option<Duration>) -> u32 {
        let mut now = start;
        for frame in 0..8 {
            match deadline(now) {
                None => return frame,
                Some(next) => {
                    assert!(next > now, "a deadline at {next:?} asked for at {now:?}");
                    now = next;
                }
            }
        }
        panic!("the redraw timer never stops");
    }

    #[test]
    fn the_controls_have_no_deadline_once_the_countdown_ran_out() {
        let mut activity = Activity::new();
        activity.set_active(true, false, ms(0));
        assert_eq!(activity.deadline(ms(0)), Some(ms(3000)));
        assert_eq!(activity.deadline(ms(2999)), Some(ms(3000)));
        // Nothing was called since: the machine has not taken note of the hiding.
        assert!(!activity.visible(ms(3000)));
        assert_eq!(activity.deadline(ms(3000)), None);
        assert_eq!(activity.deadline(ms(60_000)), None);
        assert_eq!(frames_until_quiet(ms(0), |now| activity.deadline(now)), 1);

        // A touch-only pointer that goes away gives the countdown back, if it runs.
        activity.pointer_down(true, ms(4000));
        assert_eq!(activity.deadline(ms(4000)), None);
        activity.set_touch_device(false);
        assert_eq!(activity.deadline(ms(4000)), Some(ms(7000)));
        assert_eq!(activity.deadline(ms(7000)), None);

        // Activity starts a new one.
        activity.input(ms(8000));
        assert_eq!(activity.deadline(ms(8000)), Some(ms(11_000)));
        assert_eq!(
            frames_until_quiet(ms(8000), |now| activity.deadline(now)),
            1
        );
    }

    #[test]
    fn the_panel_has_no_deadline_once_its_column_is_given_back() {
        let mut sidebar = Sidebar::new(false, false);
        assert_eq!(sidebar.deadline(ms(0)), None, "an open panel");

        sidebar.hide(ms(100));
        assert_eq!(sidebar.deadline(ms(100)), Some(ms(300)));
        assert_eq!(sidebar.deadline(ms(299)), Some(ms(300)));
        assert!(!sidebar.occupied(ms(300)));
        assert_eq!(sidebar.deadline(ms(300)), None);
        assert_eq!(sidebar.deadline(ms(60_000)), None);
        assert_eq!(frames_until_quiet(ms(100), |now| sidebar.deadline(now)), 1);

        sidebar.show(ms(400));
        assert_eq!(sidebar.deadline(ms(400)), None, "opened again");

        // With reduced motion the column is given back in the frame of the close.
        let mut still = Sidebar::new(false, true);
        still.hide(ms(50));
        assert_eq!(still.deadline(ms(50)), None);
        assert_eq!(frames_until_quiet(ms(50), |now| still.deadline(now)), 0);
    }

    #[test]
    fn the_copy_button_has_no_deadline_once_the_confirmation_ended() {
        let mut clipboard = GroupClipboard::new("group", 1);
        assert_eq!(clipboard.deadline(ms(0)), None, "nothing was copied");

        let request = clipboard.begin_copy().expect("a group name to copy");
        assert!(clipboard.finish_copy(&request, true, ms(100)));
        assert_eq!(clipboard.deadline(ms(100)), Some(ms(1700)));
        assert_eq!(clipboard.deadline(ms(1699)), Some(ms(1700)));
        assert!(!clipboard.confirmed(ms(1700)));
        assert_eq!(clipboard.deadline(ms(1700)), None);
        assert_eq!(clipboard.deadline(ms(60_000)), None);
        assert_eq!(
            frames_until_quiet(ms(100), |now| clipboard.deadline(now)),
            1
        );

        // Another group name ends the confirmation and its deadline at once.
        let request = clipboard.begin_copy().expect("a group name to copy");
        assert!(clipboard.finish_copy(&request, true, ms(2000)));
        clipboard.sync("other", 1);
        assert_eq!(clipboard.deadline(ms(2000)), None);
    }
}
