//! The presence of a modal: it stays mounted while it fades out, and it fades only its
//! opacity (`createModalPresence` with `animateModal`).
//!
//! Time is passed in as the duration since any fixed origin, so the model runs on a
//! fake clock in tests.

use std::time::Duration;

use crate::theme::{CubicBezier, MOTION};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Fading in.
    Opening,
    Open,
    /// Fading out; still drawn, no longer interactive.
    Closing,
    /// Not drawn.
    Hidden,
}

/// The durations of the fade: `--modal-entrance` and `--modal-exit` when set (the
/// lightbox backdrop), else `--motion-entrance` and `--motion-exit`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModalMotion {
    pub entrance: Duration,
    pub exit: Duration,
    pub ease: CubicBezier,
}

impl ModalMotion {
    /// Every modal except the lightbox.
    pub const DEFAULT: Self = Self {
        entrance: MOTION.entrance,
        exit: MOTION.exit,
        ease: MOTION.ease,
    };
    /// `[data-lightbox-backdrop]`.
    pub const LIGHTBOX: Self = Self {
        entrance: MOTION.lightbox_entrance,
        exit: MOTION.lightbox_exit,
        ease: MOTION.ease,
    };
}

impl Default for ModalMotion {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Transition {
    open: bool,
    generation: u64,
    /// Something was shown before this transition (a close of a never-opened modal
    /// does not fade).
    was_present: bool,
    started: Duration,
    /// The opacity the fade starts from: where an interrupted fade stood.
    from: f32,
}

/// The phase of a modal. [`Self::set_open`] starts a transition and returns the
/// generation and duration of the timer to schedule; [`Self::finish`] with that
/// generation ends it, and a finish of an older transition is ignored.
#[derive(Debug, Clone, PartialEq)]
pub struct ModalPresence {
    transition: Transition,
    finished: Option<u64>,
    reduced_motion: bool,
    motion: ModalMotion,
}

impl ModalPresence {
    /// A modal created `open` fades in; one created closed is hidden.
    pub fn new(open: bool, reduced_motion: bool, motion: ModalMotion, now: Duration) -> Self {
        Self {
            transition: Transition {
                open,
                generation: 1,
                was_present: false,
                started: now,
                from: 0.0,
            },
            finished: None,
            reduced_motion,
            motion,
        }
    }

    pub fn is_open(&self) -> bool {
        self.transition.open
    }

    pub fn generation(&self) -> u64 {
        self.transition.generation
    }

    pub fn phase(&self) -> Phase {
        let current = &self.transition;
        if self.reduced_motion || self.finished == Some(current.generation) {
            return if current.open {
                Phase::Open
            } else {
                Phase::Hidden
            };
        }
        match (current.open, current.was_present) {
            (true, _) => Phase::Opening,
            (false, true) => Phase::Closing,
            (false, false) => Phase::Hidden,
        }
    }

    /// The modal is drawn: open, or fading in or out.
    pub fn is_present(&self) -> bool {
        self.phase() != Phase::Hidden
    }

    /// The fade the current phase runs, if any.
    pub fn duration(&self) -> Option<Duration> {
        match self.phase() {
            Phase::Opening => Some(self.motion.entrance),
            Phase::Closing => Some(self.motion.exit),
            Phase::Open | Phase::Hidden => None,
        }
    }

    /// Opens or closes. Returns the timer to schedule, `(generation, duration)`, when a
    /// fade starts; nothing when `open` did not change or motion is reduced.
    pub fn set_open(&mut self, open: bool, now: Duration) -> Option<(u64, Duration)> {
        if self.transition.open == open {
            return None;
        }
        let from = self.opacity(now);
        self.transition = Transition {
            open,
            generation: self.transition.generation + 1,
            was_present: true,
            started: now,
            from,
        };
        self.timer()
    }

    /// The timer of the current phase, `(generation, duration)`, if it fades.
    pub fn timer(&self) -> Option<(u64, Duration)> {
        self.duration()
            .map(|duration| (self.transition.generation, duration))
    }

    /// Ends the fade of `generation`. Returns whether it was the current one.
    pub fn finish(&mut self, generation: u64) -> bool {
        if generation != self.transition.generation {
            return false;
        }
        self.finished = Some(generation);
        true
    }

    /// `prefers-reduced-motion`: phases end at once, including a fade in flight.
    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.reduced_motion = reduced;
    }

    pub fn motion(&self) -> ModalMotion {
        self.motion
    }

    /// The opacity of the overlay at `now`.
    pub fn opacity(&self, now: Duration) -> f32 {
        let Some(duration) = self.duration() else {
            return if self.phase() == Phase::Open {
                1.0
            } else {
                0.0
            };
        };
        let current = &self.transition;
        let target = if current.open { 1.0 } else { 0.0 };
        let progress = if duration.is_zero() {
            1.0
        } else {
            now.saturating_sub(current.started).as_secs_f32() / duration.as_secs_f32()
        };
        let eased = self.motion.ease.sample(progress);
        current.from + (target - current.from) * eased
    }

    /// The fade has run its time at `now` (the timer may still be pending).
    pub fn is_settled(&self, now: Duration) -> bool {
        self.duration()
            .is_none_or(|duration| now.saturating_sub(self.transition.started) >= duration)
    }
}

/// Keeps the values a modal showed while it fades out (`createModalSnapshot`): an open
/// modal shows the newest value, a closing one the last value it showed open.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModalSnapshot<T> {
    value: Option<T>,
}

impl<T> ModalSnapshot<T> {
    pub fn new() -> Self {
        Self { value: None }
    }

    /// The value to show: `current` when `open` or before anything was shown, else
    /// the kept one.
    pub fn update(&mut self, open: bool, current: impl FnOnce() -> T) -> &T {
        if open || self.value.is_none() {
            self.value = Some(current());
        }
        self.value.as_ref().expect("set above")
    }
}

/// `size` of `UiModal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ModalSize {
    Small,
    #[default]
    Medium,
    Large,
    XLarge,
}

/// The breakpoint `sm` of Tailwind.
const SM: f32 = 640.0;
const REM: f32 = 16.0;

impl ModalSize {
    /// The width of the dialog in a viewport `viewport_width` wide:
    /// `w-[90vw] max-w-[clamp(min, share * vw, max)]` (`96vw` for `XLarge`).
    pub fn width(self, viewport_width: f32) -> f32 {
        let vw = viewport_width / 100.0;
        let (width, min_rem, share, max_rem) = match self {
            Self::Small => (90.0, 20.0, 88.0, 28.75),
            Self::Medium => (90.0, 24.0, 90.0, 32.5),
            Self::Large => (90.0, 28.0, 92.0, 68.0),
            Self::XLarge => (96.0, 34.0, 96.0, 88.0),
        };
        let max = (share * vw).clamp(min_rem * REM, max_rem * REM);
        (width * vw).min(max)
    }

    /// The overlay's padding around the dialog: `p-3 sm:p-5`.
    pub fn overlay_padding(viewport_width: f32) -> f32 {
        if viewport_width >= SM {
            20.0
        } else {
            12.0
        }
    }

    /// The dialog's padding: `p-5 sm:p-6`.
    pub fn content_padding(viewport_width: f32) -> f32 {
        if viewport_width >= SM {
            24.0
        } else {
            20.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn a_closed_modal_is_hidden_and_opens_with_a_fade() {
        let mut presence = ModalPresence::new(false, false, ModalMotion::DEFAULT, MS(0));
        assert_eq!(presence.phase(), Phase::Hidden);
        assert!(!presence.is_present());

        let timer = presence.set_open(true, MS(10));
        assert_eq!(timer, Some((2, MS(150))));
        assert_eq!(presence.phase(), Phase::Opening);
        assert_eq!(presence.opacity(MS(10)), 0.0);
        assert!(presence.opacity(MS(85)) > 0.5);
        assert!(presence.finish(2));
        assert_eq!(presence.phase(), Phase::Open);
        assert_eq!(presence.opacity(MS(500)), 1.0);
    }

    /// Port of "retains visible values for 100 ms".
    #[test]
    fn closing_stays_present_for_the_exit_duration() {
        let mut presence = ModalPresence::new(true, false, ModalMotion::DEFAULT, MS(0));
        presence.finish(presence.generation());
        let (generation, duration) = presence.set_open(false, MS(150)).expect("a fade");
        assert_eq!(duration, MS(100));
        assert_eq!(presence.phase(), Phase::Closing);
        assert!(presence.is_present());
        assert_eq!(presence.opacity(MS(150)), 1.0);
        assert!(!presence.is_settled(MS(249)));
        assert!(presence.is_settled(MS(250)));
        assert!(presence.finish(generation));
        assert!(!presence.is_present());
    }

    /// Port of "ignores stale animation completion on rapid reopen".
    #[test]
    fn a_stale_finish_does_not_end_a_newer_fade() {
        let mut presence = ModalPresence::new(true, false, ModalMotion::DEFAULT, MS(0));
        let entrance = presence.generation();
        let (exit, _) = presence.set_open(false, MS(25)).expect("exit");
        let from = presence.opacity(MS(50));
        assert!(from > 0.0 && from < 1.0);
        presence.set_open(true, MS(50)).expect("reopen");

        assert!(!presence.finish(entrance));
        assert!(!presence.finish(exit));
        assert_eq!(presence.phase(), Phase::Opening);
        assert!(
            (presence.opacity(MS(50)) - from).abs() < 1e-6,
            "the reopen fades from where the exit stood"
        );
        assert!(presence.finish(presence.generation()));
        assert_eq!(presence.phase(), Phase::Open);
    }

    /// Port of "honors reduced motion and live changes".
    #[test]
    fn reduced_motion_ends_phases_at_once() {
        let mut presence = ModalPresence::new(true, true, ModalMotion::DEFAULT, MS(0));
        assert_eq!(presence.phase(), Phase::Open);
        assert_eq!(presence.set_open(false, MS(1)), None);
        assert!(!presence.is_present());

        presence.set_reduced_motion(false);
        presence.set_open(true, MS(2));
        presence.set_open(false, MS(3));
        assert!(presence.is_present());
        presence.set_reduced_motion(true);
        assert!(!presence.is_present(), "a fade in flight ends");
    }

    #[test]
    fn setting_the_same_state_starts_nothing() {
        let mut presence = ModalPresence::new(true, false, ModalMotion::LIGHTBOX, MS(0));
        assert_eq!(presence.timer(), Some((1, MS(75))));
        assert_eq!(presence.set_open(true, MS(5)), None);
        assert_eq!(presence.generation(), 1);
        presence.finish(1);
        assert_eq!(presence.set_open(false, MS(9)).map(|t| t.1), Some(MS(50)));
    }

    #[test]
    fn the_snapshot_keeps_the_last_open_value_while_closed() {
        let mut snapshot = ModalSnapshot::new();
        assert_eq!(snapshot.update(false, || "initial"), &"initial");
        assert_eq!(snapshot.update(true, || "draft"), &"draft");
        assert_eq!(snapshot.update(false, || ""), &"draft");
        assert_eq!(snapshot.update(true, || "new"), &"new");
    }

    #[test]
    fn sizes_follow_the_clamped_widths_of_ui_modal() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../src/ui/overlays/UiModal.tsx"),
        )
        .expect("read UiModal.tsx");
        for class in [
            r#"small: "w-[90vw] max-w-[clamp(20rem,88vw,28.75rem)]""#,
            r#"medium: "w-[90vw] max-w-[clamp(24rem,90vw,32.5rem)]""#,
            r#"large: "w-[90vw] max-w-[clamp(28rem,92vw,68rem)]""#,
            r#"xlarge: "w-[96vw] max-w-[clamp(34rem,96vw,88rem)]""#,
            "fixed inset-0 z-[60] grid place-items-center bg-neutral/58 p-3 sm:p-5",
            "rounded-[var(--radius-panel)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-modal)] sm:p-6",
        ] {
            assert!(source.contains(class), "UiModal.tsx lost: {class}");
        }
        assert_eq!(ModalSize::Small.width(1440.0), 460.0);
        assert_eq!(ModalSize::Medium.width(1440.0), 520.0);
        assert_eq!(ModalSize::Large.width(1440.0), 1088.0);
        assert!((ModalSize::XLarge.width(1440.0) - 1382.4).abs() < 1e-3);
        assert_eq!(
            ModalSize::Small.width(300.0),
            270.0,
            "90vw below the minimum"
        );
        assert_eq!(ModalSize::default(), ModalSize::Medium);
        assert_eq!(ModalSize::overlay_padding(1000.0), 20.0);
        assert_eq!(ModalSize::overlay_padding(500.0), 12.0);
        assert_eq!(ModalSize::content_padding(1000.0), 24.0);
    }
}
