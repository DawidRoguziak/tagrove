//! The month timeline beside the grid: the pure rules of `timeline.ts` and the
//! state of `GalleryTimelineScrubber.tsx` as a model.
//!
//! Sections come from the query model (`QueryModel::sections`, read once per session;
//! ADR 0007). Split months are merged back into one labelled month. An empty list
//! hides the timeline: the sections are still loading, the read failed or was stale,
//! or the backend has none for this snapshot (ADR 0027). Nothing here ever shows
//! sections of another session, because the query model drops them with the session.
//!
//! A position on the rail is `index / total`, so a million results cost nothing to
//! map; the grid turns the index into a row offset ([`GridInput::JumpToIndex`]).
//! Index order runs from the newest month to the oldest, with the "no date" month
//! (`t = 0`) last.
//!
//! [`GridInput::JumpToIndex`]: super::GridInput::JumpToIndex

use media_tagger_core::models::AssetQuerySection;

use crate::{
    i18n::{format_year_month, Language},
    model::Model,
};

/// Hit width of the rail (`TIMELINE_WIDTH`); the grid reserves it, plus
/// [`TIMELINE_RIGHT`], as its right padding.
pub const TIMELINE_WIDTH: f32 = 10.0;
/// The rail's distance from the right edge (`TIMELINE_RIGHT`), clear of the position
/// bar.
pub const TIMELINE_RIGHT: f32 = 24.0;
/// The rail starts this far below the top of the viewport and ends as far above its
/// bottom (`INSET`).
pub const TIMELINE_INSET: f64 = 4.0;
/// Year ticks closer than this to the tick above are left out (`MIN_TICK_GAP`).
pub const MIN_TICK_GAP: f64 = 14.0;

/// One labelled month: the chunks of a split month merged back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineMonth {
    /// `yyyymm` in UTC; `0` means no date.
    pub t: u32,
    pub start: usize,
    pub count: usize,
}

/// `timelineMonths`: merges adjacent sections of the same month (the backend splits
/// months larger than 5,000 rows).
pub fn timeline_months(sections: &[AssetQuerySection]) -> Vec<TimelineMonth> {
    let mut months: Vec<TimelineMonth> = Vec::new();
    for section in sections {
        match months.last_mut() {
            Some(last) if last.t == section.t && last.start + last.count == section.start => {
                last.count += section.count;
            }
            _ => months.push(TimelineMonth {
                t: section.t,
                start: section.start,
                count: section.count,
            }),
        }
    }
    months
}

/// `monthAt`: the month that holds `index` (binary search; months are in index
/// order). `0` when there are no months.
pub fn month_at(months: &[TimelineMonth], index: usize) -> usize {
    // The last month that starts at or before `index`, or the first.
    months
        .partition_point(|month| month.start <= index)
        .saturating_sub(1)
}

/// `indexAtFraction`: the global index at `fraction` of the rail.
pub fn index_at_fraction(fraction: f64, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let fraction = if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
    };
    ((fraction * total as f64).floor() as usize).min(total - 1)
}

/// The year of a `yyyymm` month.
pub const fn year_of(t: u32) -> u32 {
    t / 100
}

/// The keys of a focused timeline (`TimelineKey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimelineKey {
    /// Arrow down: the next (older) month.
    Down,
    /// Arrow up: the start of the current month, or the month before.
    Up,
    /// Page down: the first month of the next (older) year.
    PageDown,
    /// Page up: the start of the current year, or of the year before.
    PageUp,
    Home,
    End,
}

/// `timelineKeyTarget`: the index a key moves to from `position`, or `None` when it
/// moves nowhere. Moving up first returns to the start of the current month or year
/// when the position is inside it.
pub fn timeline_key_target(
    key: TimelineKey,
    months: &[TimelineMonth],
    position: usize,
    total: usize,
) -> Option<usize> {
    if months.is_empty() || total == 0 {
        return None;
    }
    let current = month_at(months, position);
    let year = |month: usize| year_of(months[month].t);
    let year_start = |month: usize| {
        let mut first = month;
        while first > 0 && year(first - 1) == year(month) {
            first -= 1;
        }
        first
    };
    match key {
        TimelineKey::Home => Some(0),
        TimelineKey::End => Some(months[months.len() - 1].start),
        TimelineKey::Down => months.get(current + 1).map(|month| month.start),
        TimelineKey::Up => {
            if position > months[current].start {
                Some(months[current].start)
            } else if current > 0 {
                Some(months[current - 1].start)
            } else {
                None
            }
        }
        TimelineKey::PageDown => {
            let mut next = current + 1;
            while next < months.len() && year(next) == year(current) {
                next += 1;
            }
            months.get(next).map(|month| month.start)
        }
        TimelineKey::PageUp => {
            let first = year_start(current);
            if position > months[first].start {
                Some(months[first].start)
            } else if first > 0 {
                Some(months[year_start(first - 1)].start)
            } else {
                None
            }
        }
    }
}

/// `formatTimelineMonth`: the label of a month in `language` (UTC calendar months),
/// or `no_date` (`gallery.timelineNoDate`) for `t = 0`.
pub fn format_timeline_month(t: u32, language: Language, no_date: &str) -> String {
    if t == 0 {
        return no_date.to_owned();
    }
    format_year_month(language, year_of(t), t % 100)
}

/// The height of the rail in a viewport `viewport` high.
pub fn track_height(viewport: f64) -> f64 {
    (viewport - 2.0 * TIMELINE_INSET).max(0.0)
}

/// A year boundary on the rail: `y` pixels below the rail's top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineTick {
    pub year: u32,
    pub y: f64,
}

/// The year boundaries of `months` on a rail `track` high, thinned so their labels
/// never overlap. The "no date" month has none.
pub fn timeline_ticks(months: &[TimelineMonth], total: usize, track: f64) -> Vec<TimelineTick> {
    let mut ticks = Vec::new();
    let mut last_y = f64::NEG_INFINITY;
    for (index, month) in months.iter().enumerate() {
        if month.t == 0 {
            continue;
        }
        if index > 0 && year_of(months[index - 1].t) == year_of(month.t) {
            continue;
        }
        let y = fraction_of(month.start, total) * track;
        if y - last_y < MIN_TICK_GAP {
            continue;
        }
        last_y = y;
        ticks.push(TimelineTick {
            year: year_of(month.t),
            y,
        });
    }
    ticks
}

/// Where `index` sits on the rail, 0..=1.
fn fraction_of(index: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        index as f64 / total as f64
    }
}

/// The month label beside the rail: `y` pixels below the rail's top (its vertical
/// centre) and the index of the month in [`TimelineScrubber::months`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineLabel {
    pub y: f64,
    pub month: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimelineInput {
    /// The sections of the current session, as the query model holds them.
    Sections(Vec<AssetQuerySection>),
    /// What the grid shows: the result's total, the viewport's height and the first
    /// visible index.
    View {
        total: usize,
        viewport: f64,
        first_visible: usize,
    },
    /// A key while the rail has the focus.
    Key(TimelineKey),
    /// The grid applied the jump of [`TimelineEffect::Jump`]; `first_visible` is the
    /// first visible index after it, which the end of the result may clamp.
    Jumped { first_visible: usize },
    /// A primary-button press at `y` pixels below the rail's top.
    Press { y: f64 },
    /// The pointer moved to `y` below the rail's top: a drag step while pressed, a
    /// hover otherwise.
    PointerMove { y: f64 },
    /// The pointer left the rail.
    PointerLeave,
    /// The press ended. When the rail goes away during one ([`Sections`] or
    /// [`View`] leave fewer than two months or no results), the scrubber ends the
    /// drag itself, as the Solid rail's cleanup does when it unmounts.
    ///
    /// [`Sections`]: TimelineInput::Sections
    /// [`View`]: TimelineInput::View
    Release,
    /// The frame that [`TimelineEffect::RequestFrame`] asked for.
    Frame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineEffect {
    /// Show global index `index` at the top of the grid, then answer
    /// [`TimelineInput::Jumped`].
    Jump(usize),
    /// Hold page and thumbnail demand ([`GridInput::HoldDemand`]).
    ///
    /// [`GridInput::HoldDemand`]: super::GridInput::HoldDemand
    HoldDemand,
    /// Resume demand ([`GridInput::ResumeDemand`]).
    ///
    /// [`GridInput::ResumeDemand`]: super::GridInput::ResumeDemand
    ResumeDemand,
    /// Answer [`TimelineInput::Frame`] at the next frame: pointer moves arrive faster
    /// than frames, and only the latest target of a frame is applied.
    RequestFrame,
}

/// The state of the timeline rail (`GalleryTimelineScrubber`).
#[derive(Debug, Default)]
pub struct TimelineScrubber {
    sections: Vec<AssetQuerySection>,
    months: Vec<TimelineMonth>,
    total: usize,
    track: f64,
    first_visible: usize,
    /// The index the timeline last moved to. Near the end of the result the scroll
    /// clamps, so the first visible item can still belong to an earlier month; until
    /// the view moves for another reason, the timeline reports (and steps from) its
    /// own target.
    cursor: Option<usize>,
    /// The first visible index right after the timeline's last jump; any other first
    /// visible index hands the position back to the grid.
    settled: Option<usize>,
    dragging: bool,
    /// The latest drag target, applied at the next frame.
    pending: Option<usize>,
    frame_requested: bool,
    hover: Option<TimelineLabel>,
}

impl TimelineScrubber {
    pub fn new() -> Self {
        Self::default()
    }

    /// The sections the months were made from.
    pub fn sections(&self) -> &[AssetQuerySection] {
        &self.sections
    }

    /// The labelled months of the current session.
    pub fn months(&self) -> &[TimelineMonth] {
        &self.months
    }

    /// Whether the rail is drawn: a result that spans at least two months.
    pub fn visible(&self) -> bool {
        self.total > 0 && self.months.len() > 1
    }

    /// The rail's height.
    pub fn track(&self) -> f64 {
        self.track
    }

    /// The index the timeline reports: its own target after a jump, otherwise the
    /// first visible index.
    pub fn position(&self) -> usize {
        self.cursor.unwrap_or(self.first_visible)
    }

    /// The month of [`position`](Self::position).
    pub fn current(&self) -> usize {
        month_at(&self.months, self.position())
    }

    /// `aria-valuenow`: the current month, from 1.
    pub fn value_now(&self) -> usize {
        self.current() + 1
    }

    /// `aria-valuemax`: the number of months.
    pub fn value_max(&self) -> usize {
        self.months.len()
    }

    /// The label of month `month` (an index in [`months`](Self::months)); empty
    /// when there is no such month.
    pub fn month_text(&self, month: usize, language: Language, no_date: &str) -> String {
        self.months
            .get(month)
            .map(|month| format_timeline_month(month.t, language, no_date))
            .unwrap_or_default()
    }

    /// Where the marker of [`position`](Self::position) is, below the rail's top.
    pub fn marker_y(&self) -> f64 {
        fraction_of(self.position(), self.total) * self.track
    }

    /// The year ticks of the rail.
    pub fn ticks(&self) -> Vec<TimelineTick> {
        timeline_ticks(&self.months, self.total, self.track)
    }

    /// The month label: the month under the pointer while it hovers or drags, else
    /// the current month while the rail has the focus.
    pub fn label(&self, focused: bool) -> Option<TimelineLabel> {
        self.hover.or_else(|| {
            focused.then(|| TimelineLabel {
                y: self.marker_y(),
                month: self.current(),
            })
        })
    }

    /// A drag is under way (`data-gallery-timeline-dragging`).
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    /// The index under `y` and the hover label there.
    fn describe(&mut self, y: f64) -> usize {
        let index = index_at_fraction(y / self.track.max(1.0), self.total);
        self.hover = Some(TimelineLabel {
            y: y.clamp(0.0, self.track.max(0.0)),
            month: month_at(&self.months, index),
        });
        index
    }

    fn move_to(&mut self, index: usize) -> Vec<TimelineEffect> {
        self.cursor = Some(index);
        self.settled = None;
        vec![TimelineEffect::Jump(index)]
    }

    /// Once the rail is hidden, nothing of the pointer on it is left: no label, no
    /// pending target, and a drag in progress ends (its release can no longer reach
    /// the rail), so the rail comes back as fresh as a remounted Solid scrubber.
    fn forget_pointer_if_hidden(&mut self) -> Vec<TimelineEffect> {
        if self.visible() {
            return Vec::new();
        }
        self.hover = None;
        self.pending = None;
        // A remounted Solid scrubber starts without a jump target.
        self.cursor = None;
        self.settled = None;
        if std::mem::take(&mut self.dragging) {
            vec![TimelineEffect::ResumeDemand]
        } else {
            Vec::new()
        }
    }

    fn jump_in_frame(&mut self, index: usize) -> Vec<TimelineEffect> {
        self.pending = Some(index);
        let mut effects = vec![TimelineEffect::HoldDemand];
        if !self.frame_requested {
            self.frame_requested = true;
            effects.push(TimelineEffect::RequestFrame);
        }
        effects
    }
}

impl Model for TimelineScrubber {
    type Input = TimelineInput;
    type Effect = TimelineEffect;

    fn handle(&mut self, input: TimelineInput) -> Vec<TimelineEffect> {
        match input {
            TimelineInput::Sections(sections) => {
                if sections != self.sections {
                    self.months = timeline_months(&sections);
                    self.sections = sections;
                }
                self.forget_pointer_if_hidden()
            }
            TimelineInput::View {
                total,
                viewport,
                first_visible,
            } => {
                self.total = total;
                self.track = track_height(viewport);
                if self.first_visible != first_visible {
                    self.first_visible = first_visible;
                    if self.settled.is_some_and(|settled| settled != first_visible) {
                        self.settled = None;
                        self.cursor = None;
                    }
                }
                self.forget_pointer_if_hidden()
            }
            TimelineInput::Key(key) => {
                match timeline_key_target(key, &self.months, self.position(), self.total) {
                    Some(target) => self.move_to(target),
                    None => Vec::new(),
                }
            }
            TimelineInput::Jumped { first_visible } => {
                self.first_visible = first_visible;
                self.settled = Some(first_visible);
                Vec::new()
            }
            TimelineInput::Press { y } => {
                if self.total == 0 {
                    return Vec::new();
                }
                self.dragging = true;
                let index = self.describe(y);
                self.jump_in_frame(index)
            }
            TimelineInput::PointerMove { y } => {
                let index = self.describe(y);
                if self.dragging {
                    self.jump_in_frame(index)
                } else {
                    Vec::new()
                }
            }
            TimelineInput::PointerLeave => {
                if !self.dragging {
                    self.hover = None;
                }
                Vec::new()
            }
            TimelineInput::Release => {
                if !self.dragging {
                    return Vec::new();
                }
                self.dragging = false;
                self.hover = None;
                vec![TimelineEffect::ResumeDemand]
            }
            TimelineInput::Frame => {
                self.frame_requested = false;
                match self.pending.take() {
                    Some(index) => self.move_to(index),
                    None => Vec::new(),
                }
            }
        }
    }
}

/// Looks of the rail (`GalleryTimelineScrubber.tsx`), in CSS pixels.
pub mod visuals {
    use crate::{
        controls::FocusRing,
        theme::{Rgba, Shadow, ThemeTokens, TYPOGRAPHY},
    };

    /// The marker of the current position: `h-1 rounded-full` across the rail.
    pub const MARKER_HEIGHT: f32 = 4.0;
    /// A year tick: `h-px w-2` at the rail's right edge.
    pub const TICK_WIDTH: f32 = 8.0;
    pub const TICK_HEIGHT: f32 = 1.0;
    /// The year beside a tick: `right-3 -top-1.5 font-mono text-[10px] leading-none`,
    /// shown while the pointer is over the rail.
    pub const TICK_LABEL_RIGHT: f32 = 12.0;
    pub const TICK_LABEL_TOP: f32 = -6.0;
    pub const TICK_LABEL_SIZE: f32 = 10.0;
    /// The month label: `right-5` of the rail, centred on its `y`, `px-2 py-1
    /// text-xs` with a 1 px border.
    pub const LABEL_RIGHT: f32 = 20.0;
    pub const LABEL_PADDING_X: f32 = 8.0;
    pub const LABEL_PADDING_Y: f32 = 4.0;
    pub const LABEL_BORDER: f32 = 1.0;

    /// The month label's line: `text-xs`, 12 px on a 16 px line. [`TextStep::line_height`]
    /// is a ratio, so the label draws with this and never with the ratio itself.
    ///
    /// [`TextStep::line_height`]: crate::theme::TextStep::line_height
    pub fn label_line_height() -> f32 {
        TYPOGRAPHY.xs.line_height_px()
    }

    /// The month label's height (26 px): its line, `py-1` and the border. The label is
    /// centred on its `y` (`-translate-y-1/2`), so its top is `y` less half of this.
    pub fn label_height() -> f32 {
        label_line_height() + 2.0 * (LABEL_PADDING_Y + LABEL_BORDER)
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct TimelineVisuals {
        /// `hover:bg-base-content/5`
        pub rail_hover: Rgba,
        /// `bg-primary/70`
        pub marker: Rgba,
        /// `bg-base-content/30`
        pub tick: Rgba,
        /// `text-base-content/60`
        pub tick_label: Rgba,
        /// `border-[var(--border-strong)] bg-[var(--surface-solid)]`
        pub label_border: Rgba,
        pub label_fill: Rgba,
        /// `shadow-[var(--shadow-popover)]`
        pub label_shadow: &'static [Shadow],
        /// `focus-visible:outline-2 focus-visible:outline-primary outline-offset-2`
        pub focus_ring: FocusRing,
    }

    pub fn timeline_visuals(tokens: &ThemeTokens) -> TimelineVisuals {
        let colors = &tokens.colors;
        TimelineVisuals {
            rail_hover: colors.base_content.with_alpha_percent(5),
            marker: colors.primary.with_alpha_percent(70),
            tick: colors.base_content.with_alpha_percent(30),
            tick_label: colors.base_content.with_alpha_percent(60),
            label_border: colors.border_strong,
            label_fill: colors.surface_solid,
            label_shadow: tokens.shadows.popover,
            focus_ring: FocusRing {
                color: colors.primary,
                width: 2.0,
                offset: 2.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use media_tagger_core::models::AssetQuerySection;

    use super::*;
    use crate::gallery::gallery_source;

    fn section(t: u32, start: usize, count: usize) -> AssetQuerySection {
        AssetQuerySection { t, start, count }
    }

    /// Newest first, as the backend orders them; 2024-03 was split into two chunks.
    fn sections() -> Vec<AssetQuerySection> {
        vec![
            section(202403, 0, 5000),
            section(202403, 5000, 100),
            section(202402, 5100, 50),
            section(202312, 5150, 20),
            section(202306, 5170, 30),
            section(202201, 5200, 10),
            section(0, 5210, 5),
        ]
    }
    const TOTAL: usize = 5215;

    #[test]
    fn split_months_merge_back_into_one_labelled_month() {
        let months = timeline_months(&sections());
        let rows: Vec<_> = months.iter().map(|m| (m.t, m.start, m.count)).collect();
        assert_eq!(
            rows,
            [
                (202403, 0, 5100),
                (202402, 5100, 50),
                (202312, 5150, 20),
                (202306, 5170, 30),
                (202201, 5200, 10),
                (0, 5210, 5),
            ]
        );
        assert!(timeline_months(&[]).is_empty());
        assert_eq!(month_at(&[], 7), 0);
    }

    #[test]
    fn keys_step_months_years_and_ends() {
        let months = timeline_months(&sections());
        let key = |key, from| timeline_key_target(key, &months, from, TOTAL);
        assert_eq!(key(TimelineKey::Down, 0), Some(5100));
        assert_eq!(key(TimelineKey::Up, 5120), Some(5100));
        assert_eq!(key(TimelineKey::PageUp, 5175), Some(5150));
        assert_eq!(key(TimelineKey::End, 0), Some(5210));
        assert_eq!(timeline_key_target(TimelineKey::End, &[], 0, 0), None);
    }

    #[test]
    fn ticks_mark_year_starts_and_skip_crowded_ones() {
        let months = timeline_months(&sections());
        // On a 600 px rail 2022 starts 6 px below 2023: its tick is left out.
        let years: Vec<_> = timeline_ticks(&months, TOTAL, 600.0)
            .iter()
            .map(|tick| tick.year)
            .collect();
        assert_eq!(years, [2024, 2023]);
        // Spread evenly, every year gets its tick; the no-date month gets none.
        let even = [
            TimelineMonth {
                t: 202403,
                start: 0,
                count: 10,
            },
            TimelineMonth {
                t: 202402,
                start: 10,
                count: 10,
            },
            TimelineMonth {
                t: 202312,
                start: 20,
                count: 10,
            },
            TimelineMonth {
                t: 0,
                start: 30,
                count: 10,
            },
        ];
        assert_eq!(
            timeline_ticks(&even, 40, 400.0),
            [
                TimelineTick { year: 2024, y: 0.0 },
                TimelineTick {
                    year: 2023,
                    y: 200.0
                },
            ]
        );
    }

    #[test]
    fn the_rail_is_hidden_without_two_months() {
        let mut scrubber = TimelineScrubber::new();
        scrubber.handle(TimelineInput::View {
            total: 10,
            viewport: 608.0,
            first_visible: 0,
        });
        assert!(!scrubber.visible());
        // Sections unavailable for this snapshot (ADR 0027): an empty list.
        scrubber.handle(TimelineInput::Sections(Vec::new()));
        assert!(!scrubber.visible());
        scrubber.handle(TimelineInput::Sections(vec![section(202401, 0, 10)]));
        assert!(!scrubber.visible());
        scrubber.handle(TimelineInput::Sections(vec![
            section(202401, 0, 5),
            section(202312, 5, 5),
        ]));
        assert!(scrubber.visible());
        assert_eq!(scrubber.track(), 600.0);
        scrubber.handle(TimelineInput::Sections(Vec::new()));
        assert!(!scrubber.visible());
    }

    #[test]
    fn a_rail_that_hides_during_a_drag_ends_it_and_comes_back_fresh() {
        let view = |total| TimelineInput::View {
            total,
            viewport: 608.0,
            first_visible: 0,
        };
        let mut scrubber = TimelineScrubber::new();
        scrubber.handle(TimelineInput::Sections(sections()));
        scrubber.handle(view(TOTAL));
        assert!(scrubber.visible());

        // A new session clears the sections in the middle of a drag.
        scrubber.handle(TimelineInput::Press { y: 300.0 });
        assert!(scrubber.dragging());
        assert_eq!(
            scrubber.handle(TimelineInput::Sections(Vec::new())),
            [TimelineEffect::ResumeDemand]
        );
        assert!(!scrubber.dragging());
        assert_eq!(scrubber.label(false), None);
        // The frame the drag asked for has nothing left to apply.
        assert!(scrubber.handle(TimelineInput::Frame).is_empty());
        // The release that follows, and further hidden updates, change nothing.
        assert!(scrubber.handle(TimelineInput::Release).is_empty());
        assert!(scrubber.handle(view(TOTAL)).is_empty());

        // Back with its sections: no drag, no stale label, nothing to resume.
        assert!(scrubber
            .handle(TimelineInput::Sections(sections()))
            .is_empty());
        assert!(scrubber.visible());
        assert!(!scrubber.dragging());
        assert_eq!(scrubber.label(false), None);

        // An empty result hides the rail as well.
        scrubber.handle(TimelineInput::Press { y: 100.0 });
        scrubber.handle(TimelineInput::PointerMove { y: 200.0 });
        assert_eq!(scrubber.handle(view(0)), [TimelineEffect::ResumeDemand]);
        assert!(!scrubber.dragging());
        assert_eq!(scrubber.label(false), None);
    }

    #[test]
    fn a_rail_that_hides_forgets_its_jump_target() {
        let view = |first_visible| TimelineInput::View {
            total: TOTAL,
            viewport: 608.0,
            first_visible,
        };
        let mut scrubber = TimelineScrubber::new();
        scrubber.handle(TimelineInput::Sections(sections()));
        scrubber.handle(view(0));

        // End jumps to the last month, but the grid clamps to an earlier row.
        let target = scrubber.handle(TimelineInput::Key(TimelineKey::End));
        let [TimelineEffect::Jump(index)] = target.as_slice() else {
            panic!("End should jump: {target:?}");
        };
        let clamped = index - 1;
        scrubber.handle(TimelineInput::Jumped {
            first_visible: clamped,
        });
        scrubber.handle(view(clamped));
        assert_eq!(scrubber.position(), *index);

        // A new session hides the rail and brings it back at the same offset.
        scrubber.handle(TimelineInput::Sections(Vec::new()));
        assert!(!scrubber.visible());
        scrubber.handle(view(clamped));
        scrubber.handle(TimelineInput::Sections(sections()));
        assert!(scrubber.visible());
        assert_eq!(scrubber.position(), clamped);
        assert_eq!(scrubber.current(), month_at(&scrubber.months, clamped));
        assert_eq!(scrubber.current() + 1, scrubber.value_max() - 1);
    }

    #[test]
    fn the_solid_rail_still_has_the_classes_copied_here() {
        let source = gallery_source("GalleryTimelineScrubber.tsx");
        // The unmounted rail ends a drag in progress.
        assert!(source.contains("if (pointerId !== null) props.onRelease();"));
        for class in [
            "export const TIMELINE_WIDTH = 10;",
            "const INSET = 4;",
            "const MIN_TICK_GAP = 14;",
            "hover:bg-base-content/5 focus-visible:outline-2 focus-visible:outline-primary",
            "outline-offset-2",
            "absolute right-0 h-px w-2 bg-base-content/30",
            "absolute right-3 -top-1.5 hidden whitespace-nowrap font-mono text-[10px] leading-none text-base-content/60 group-hover:block",
            "absolute inset-x-0 h-1 rounded-full bg-primary/70",
            "absolute right-5 -translate-y-1/2 whitespace-nowrap rounded-[var(--radius-surface)] border border-[var(--border-strong)] bg-[var(--surface-solid)] px-2 py-1 text-xs shadow-[var(--shadow-popover)]",
        ] {
            assert!(source.contains(class), "{class}");
        }
        let grid = gallery_source("GalleryGrid.tsx");
        assert!(grid.contains("const TIMELINE_RIGHT = 24;"));
        assert_eq!(
            TIMELINE_RIGHT + TIMELINE_WIDTH,
            crate::gallery::layout::area::PADDING_RIGHT
        );
    }

    /// `text-xs` is 12 px on a 16 px line; with `py-1` and a 1 px border the month
    /// label is 26 px high. The type scale keeps line heights as ratios (16 / 12), so
    /// using the ratio as pixels drew a label 11.5 px high with its text spilling out.
    #[test]
    fn the_month_label_is_one_text_xs_line_high_plus_padding_and_border() {
        assert_eq!(visuals::label_line_height(), 16.0);
        assert_eq!(visuals::label_height(), 26.0);
    }
}
