//! The TypeScript specification of the position bar and the month timeline (epic E10)
//! ported to the client crate: the cases of `timeline.test.ts`,
//! `GalleryScrollbar.test.tsx` and `GalleryTimelineScrubber.test.tsx` that describe
//! product behaviour, at the layer of the pure rules and the `TimelineScrubber` model.
//! Each test names the TS case it ports. What only the view can show (focus, the
//! window-wide drag listeners, the frame callback) is in
//! `media_tagger_gpui/tests/gallery_position.rs`.

use media_tagger_client::{
    gallery::{
        format_timeline_month, index_at_fraction, month_at, scrollbar_key_target,
        scrollbar_value_now, timeline_key_target, timeline_months, ScrollEdge, ScrollbarDrag,
        ScrollbarKey, ScrollbarMetrics, ScrollbarTarget, TimelineEffect, TimelineInput,
        TimelineKey, TimelineMonth, TimelineScrubber,
    },
    i18n::{I18n, Language},
    model::Model,
};
use media_tagger_core::models::AssetQuerySection;

fn section(t: u32, start: usize, count: usize) -> AssetQuerySection {
    AssetQuerySection { t, start, count }
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{actual} is not {expected}"
    );
}

// ---------------------------------------------------------------------------
// timeline.test.ts
// ---------------------------------------------------------------------------

/// Newest first, as the backend orders them; 2024-03 was split into two chunks.
fn spec_sections() -> Vec<AssetQuerySection> {
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
const SPEC_TOTAL: usize = 5215;

fn spec_months() -> Vec<TimelineMonth> {
    timeline_months(&spec_sections())
}

/// "merges split months back into one labelled month"
#[test]
fn split_months_merge_back_into_one_labelled_month() {
    let rows: Vec<_> = spec_months()
        .iter()
        .map(|month| (month.t, month.start, month.count))
        .collect();
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
}

/// "finds the month of an index and maps track fractions to indexes"
#[test]
fn finds_the_month_of_an_index_and_maps_track_fractions_to_indexes() {
    let months = spec_months();
    assert_eq!(month_at(&months, 0), 0);
    assert_eq!(month_at(&months, 5099), 0);
    assert_eq!(month_at(&months, 5100), 1);
    assert_eq!(month_at(&months, 5214), 5);
    assert_eq!(index_at_fraction(-1.0, SPEC_TOTAL), 0);
    assert_eq!(index_at_fraction(0.5, SPEC_TOTAL), 2607);
    assert_eq!(index_at_fraction(1.0, SPEC_TOTAL), SPEC_TOTAL - 1);
    assert_eq!(index_at_fraction(0.5, 0), 0);
    // A million-row index is exact without materializing anything.
    assert_eq!(index_at_fraction(0.999_999, 1_000_000), 999_999);
}

/// "steps months with arrows, years with PageUp/PageDown and ends with Home/End"
#[test]
fn steps_months_with_arrows_years_with_pages_and_ends_with_home_and_end() {
    let months = spec_months();
    let key = |key, from| timeline_key_target(key, &months, from, SPEC_TOTAL);
    assert_eq!(key(TimelineKey::Down, 0), Some(5100));
    assert_eq!(key(TimelineKey::Down, 5210), None);
    // Inside a month, up returns to its start first.
    assert_eq!(key(TimelineKey::Up, 5120), Some(5100));
    assert_eq!(key(TimelineKey::Up, 5100), Some(0));
    assert_eq!(key(TimelineKey::Up, 0), None);
    assert_eq!(key(TimelineKey::PageDown, 0), Some(5150));
    assert_eq!(key(TimelineKey::PageDown, 5150), Some(5200));
    assert_eq!(key(TimelineKey::PageDown, 5200), Some(5210));
    assert_eq!(key(TimelineKey::PageUp, 5175), Some(5150));
    assert_eq!(key(TimelineKey::PageUp, 5150), Some(0));
    assert_eq!(key(TimelineKey::PageUp, 5210), Some(5200));
    assert_eq!(key(TimelineKey::Home, 5000), Some(0));
    assert_eq!(key(TimelineKey::End, 0), Some(5210));
    assert_eq!(timeline_key_target(TimelineKey::End, &[], 0, 0), None);
}

/// "formats UTC months in the active language and labels missing dates"
#[test]
fn formats_months_in_the_active_language_and_labels_missing_dates() {
    assert_eq!(
        format_timeline_month(202403, Language::En, "No date"),
        "March 2024"
    );
    assert_eq!(
        format_timeline_month(202401, Language::De, "Ohne Datum"),
        "Januar 2024"
    );
    assert_eq!(
        format_timeline_month(0, Language::Pl, "Bez daty"),
        "Bez daty"
    );
    // The no-date text is `gallery.timelineNoDate` of the language.
    let no_date = I18n::new(Language::Pl).t("gallery.timelineNoDate");
    assert_eq!(format_timeline_month(0, Language::Pl, &no_date), "Bez daty");
    let no_date = I18n::new(Language::En).t("gallery.timelineNoDate");
    assert_eq!(format_timeline_month(0, Language::En, &no_date), "No date");
}

// ---------------------------------------------------------------------------
// GalleryScrollbar.test.tsx
// ---------------------------------------------------------------------------
// A 600 px viewport over 1,000,000 px: the track is 592 px tall, the thumb its 32 px
// minimum, travel 560 px; 25,000 items, five columns of 200 px rows.

const BAR_VIEWPORT: f64 = 600.0;
const BAR_MAX: f64 = 1_000_000.0;
const BAR_COUNT: usize = 25_000;

fn bar_at(offset: f64) -> ScrollbarMetrics {
    ScrollbarMetrics::new(BAR_VIEWPORT, offset, 0.0, BAR_MAX)
}

fn first_visible(offset: f64) -> usize {
    (offset / 200.0).floor() as usize * 5
}

/// "exposes an accessible global position and moves one thumb transform"
#[test]
fn exposes_the_global_position_and_moves_the_thumb() {
    let top = bar_at(0.0);
    assert_eq!((top.track, top.thumb, top.travel), (592.0, 32.0, 560.0));
    assert_eq!(top.thumb_top(), 0.0);
    assert_eq!(scrollbar_value_now(first_visible(0.0), BAR_COUNT), 1);
    let half = bar_at(500_000.0);
    assert_eq!(half.thumb_top(), 280.0);
    assert_eq!(
        scrollbar_value_now(first_visible(500_000.0), BAR_COUNT),
        12_501
    );
}

/// "jumps to a track press and keeps dragging from the jumped position"
#[test]
fn a_track_press_jumps_and_the_drag_goes_on_from_there() {
    let bar = bar_at(0.0);
    // The thumb's centre goes to the pointer: (296 - 16) / 560 = half the range.
    let (target, start_fraction) = bar.press(296.0);
    assert_eq!(target, Some(500_000.0));
    assert_eq!(start_fraction, 0.5);
    // The drag measures from the press, on the window's axis (the track starts at 4).
    let drag = ScrollbarDrag {
        start_y: 4.0 + 296.0,
        start_fraction,
    };
    let jumped = bar_at(500_000.0);
    close(drag.offset_at(&jumped, 4.0 + 296.0 + 56.0), 600_000.0);
}

/// "drags the thumb relative to its grab point and clamps to the ends"
#[test]
fn a_thumb_drag_is_relative_to_its_grab_point_and_clamps_to_the_ends() {
    let bar = bar_at(0.0);
    // A press on the thumb does not scroll.
    let (target, start_fraction) = bar.press(10.0);
    assert_eq!(target, None);
    assert_eq!(start_fraction, 0.0);
    let drag = ScrollbarDrag {
        start_y: 4.0 + 10.0,
        start_fraction,
    };
    assert_eq!(drag.offset_at(&bar, 4.0 + 10.0 + 2_000.0), BAR_MAX);
    assert_eq!(drag.offset_at(&bar, -2_000.0), 0.0);
    // Half the travel below the grab point is half the range.
    close(drag.offset_at(&bar, 4.0 + 10.0 + 280.0), 500_000.0);
    // Grabbed in the middle of a thumb further down, the thumb keeps its grip.
    let lower = bar_at(500_000.0);
    let (target, start_fraction) = lower.press(280.0 + 16.0);
    assert_eq!((target, start_fraction), (None, 0.5));
}

/// "scrolls by row, page and to the ends from the keyboard"
#[test]
fn keys_scroll_by_row_page_and_to_the_ends() {
    let key = |key| scrollbar_key_target(key, 10_000.0, 200.0, BAR_VIEWPORT);
    assert_eq!(key(ScrollbarKey::Down), ScrollbarTarget::Offset(10_200.0));
    assert_eq!(key(ScrollbarKey::Up), ScrollbarTarget::Offset(9_800.0));
    assert_eq!(
        key(ScrollbarKey::PageDown),
        ScrollbarTarget::Offset(10_600.0)
    );
    assert_eq!(key(ScrollbarKey::PageUp), ScrollbarTarget::Offset(9_400.0));
    assert_eq!(
        key(ScrollbarKey::Home),
        ScrollbarTarget::Edge(ScrollEdge::Start)
    );
    assert_eq!(
        key(ScrollbarKey::End),
        ScrollbarTarget::Edge(ScrollEdge::End)
    );
}

// ---------------------------------------------------------------------------
// GalleryTimelineScrubber.test.tsx
// ---------------------------------------------------------------------------
// One million results over three months and a no-date tail, on a 608 px viewport: a
// 600 px rail.

const TOTAL: usize = 1_000_000;

fn million() -> Vec<AssetQuerySection> {
    vec![
        section(202403, 0, 5000),
        section(202403, 5000, 495_000),
        section(202402, 500_000, 250_000),
        section(202312, 750_000, 249_990),
        section(0, 999_990, 10),
    ]
}

fn view(first_visible: usize) -> TimelineInput {
    TimelineInput::View {
        total: TOTAL,
        viewport: 608.0,
        first_visible,
    }
}

fn scrubber() -> TimelineScrubber {
    let mut scrubber = TimelineScrubber::new();
    scrubber.handle(TimelineInput::Sections(million()));
    scrubber.handle(view(0));
    scrubber
}

fn text(scrubber: &TimelineScrubber) -> String {
    scrubber.month_text(scrubber.current(), Language::En, "No date")
}

fn label_text(scrubber: &TimelineScrubber, focused: bool) -> Option<String> {
    scrubber
        .label(focused)
        .map(|label| scrubber.month_text(label.month, Language::En, "No date"))
}

/// What the grid does with a jump, as the Solid spec's `onJump`: the index comes to
/// the top unless the end of the result clamps the scroll at `clamp_from`.
fn apply(scrubber: &mut TimelineScrubber, effects: &[TimelineEffect], clamp_from: usize) {
    for effect in effects {
        if let TimelineEffect::Jump(index) = *effect {
            let first_visible = index.min(clamp_from);
            scrubber.handle(TimelineInput::Jumped { first_visible });
            scrubber.handle(view(first_visible));
        }
    }
}

/// "exposes the current month as an accessible vertical slider"
#[test]
fn exposes_the_current_month_as_a_slider_value() {
    let mut scrubber = scrubber();
    assert!(scrubber.visible());
    assert_eq!(scrubber.track(), 600.0);
    assert_eq!(scrubber.value_max(), 4);
    assert_eq!(scrubber.value_now(), 1);
    assert_eq!(text(&scrubber), "March 2024");
    scrubber.handle(view(999_995));
    assert_eq!(scrubber.value_now(), 4);
    assert_eq!(text(&scrubber), "No date");
}

/// "jumps by month, year and end with the keyboard"
#[test]
fn jumps_by_month_year_and_end_with_the_keyboard() {
    let mut scrubber = scrubber();
    let key = |scrubber: &mut TimelineScrubber, key| {
        let effects = scrubber.handle(TimelineInput::Key(key));
        apply(scrubber, &effects, usize::MAX);
        effects
    };
    assert_eq!(
        key(&mut scrubber, TimelineKey::Down),
        [TimelineEffect::Jump(500_000)]
    );
    assert_eq!(
        key(&mut scrubber, TimelineKey::PageDown),
        [TimelineEffect::Jump(750_000)]
    );
    assert_eq!(
        key(&mut scrubber, TimelineKey::End),
        [TimelineEffect::Jump(999_990)]
    );
    assert_eq!(
        key(&mut scrubber, TimelineKey::Home),
        [TimelineEffect::Jump(0)]
    );
    // Keys never hold demand.
    assert!(!scrubber.dragging());
}

/// "drags once per frame with demand held, and releases demand on pointer up"
#[test]
fn drags_once_per_frame_with_demand_held_and_releases_it_on_release() {
    let mut scrubber = scrubber();
    let mut effects = scrubber.handle(TimelineInput::Press { y: 300.0 });
    effects.extend(scrubber.handle(TimelineInput::PointerMove { y: 450.0 }));
    effects.extend(scrubber.handle(TimelineInput::PointerMove { y: 480.0 }));
    // Three pointer events, one frame: demand is held at each, nothing jumps yet.
    let holds = effects
        .iter()
        .filter(|effect| **effect == TimelineEffect::HoldDemand)
        .count();
    assert_eq!(holds, 3);
    let frames = effects
        .iter()
        .filter(|effect| **effect == TimelineEffect::RequestFrame)
        .count();
    assert_eq!(frames, 1);
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, TimelineEffect::Jump(_))));

    // The frame applies only the latest target.
    let jumped = scrubber.handle(TimelineInput::Frame);
    assert_eq!(jumped, [TimelineEffect::Jump(800_000)]);
    apply(&mut scrubber, &jumped, usize::MAX);
    assert_eq!(
        label_text(&scrubber, false).as_deref(),
        Some("December 2023")
    );
    assert!(scrubber.dragging());

    assert_eq!(
        scrubber.handle(TimelineInput::Release),
        [TimelineEffect::ResumeDemand]
    );
    assert!(!scrubber.dragging());
    assert_eq!(label_text(&scrubber, false), None);
    // A second release (the capture lost after the pointer went up) resumes nothing.
    assert!(scrubber.handle(TimelineInput::Release).is_empty());

    // Hovering only previews the month under the pointer.
    assert!(scrubber
        .handle(TimelineInput::PointerMove { y: 60.0 })
        .is_empty());
    assert_eq!(label_text(&scrubber, false).as_deref(), Some("March 2024"));
    assert!(scrubber.handle(TimelineInput::Frame).is_empty());
    // Leaving the rail hides the preview; the focused rail labels its own month.
    scrubber.handle(TimelineInput::PointerLeave);
    assert_eq!(label_text(&scrubber, false), None);
    assert_eq!(
        label_text(&scrubber, true).as_deref(),
        Some("December 2023")
    );
}

/// The Solid rail ignores a press while the result is empty (`props.total <= 0`).
#[test]
fn a_press_on_an_empty_result_does_nothing() {
    let mut scrubber = TimelineScrubber::new();
    scrubber.handle(TimelineInput::Sections(million()));
    scrubber.handle(TimelineInput::View {
        total: 0,
        viewport: 608.0,
        first_visible: 0,
    });
    assert!(scrubber
        .handle(TimelineInput::Press { y: 300.0 })
        .is_empty());
    assert!(!scrubber.dragging());
    assert!(scrubber
        .handle(TimelineInput::Key(TimelineKey::End))
        .is_empty());
}

/// "reports and steps from its own target when the scroll clamps near the end"
#[test]
fn reports_and_steps_from_its_own_target_when_the_scroll_clamps_near_the_end() {
    // The last row that can reach the top is in December 2023.
    const CLAMP: usize = 999_000;
    let mut scrubber = scrubber();
    let effects = scrubber.handle(TimelineInput::Key(TimelineKey::End));
    assert_eq!(effects, [TimelineEffect::Jump(999_990)]);
    apply(&mut scrubber, &effects, CLAMP);
    assert_eq!(scrubber.value_now(), 4);
    assert_eq!(text(&scrubber), "No date");

    let effects = scrubber.handle(TimelineInput::Key(TimelineKey::Up));
    assert_eq!(effects, [TimelineEffect::Jump(750_000)]);
    apply(&mut scrubber, &effects, CLAMP);
    assert_eq!(text(&scrubber), "December 2023");

    // Scrolling elsewhere hands the label back to the first visible item.
    scrubber.handle(view(100));
    assert_eq!(text(&scrubber), "March 2024");
    assert_eq!(
        scrubber.handle(TimelineInput::Key(TimelineKey::Down)),
        [TimelineEffect::Jump(500_000)]
    );
}

/// ADR 0027: sections of a new session replace the months; an empty list (loading,
/// failed, stale or unavailable) hides the rail rather than keeping the old months.
#[test]
fn new_sections_replace_the_months_and_none_hide_the_rail() {
    let mut scrubber = scrubber();
    assert_eq!(scrubber.months().len(), 4);
    scrubber.handle(TimelineInput::Sections(vec![
        section(202001, 0, 600_000),
        section(201912, 600_000, 400_000),
    ]));
    assert_eq!(scrubber.value_max(), 2);
    assert_eq!(text(&scrubber), "January 2020");
    scrubber.handle(TimelineInput::Sections(Vec::new()));
    assert!(!scrubber.visible());
    assert!(scrubber
        .handle(TimelineInput::Key(TimelineKey::Down))
        .is_empty());
}
