//! The position bar and the month timeline beside the gallery grid (epic E10) on
//! GPUI's test platform with the scripted backend: where they are drawn, what a press,
//! a drag and the keys do to the grid, and when the timeline is hidden.

use std::{sync::Arc, time::Duration};

use gpui::{
    point, px, Bounds, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    PlatformInput, Point, TestAppContext, VisualTestContext, WindowHandle,
};
use media_tagger_client::{
    backend::FakeBackend,
    exec::{Executor, ManualExecutor},
    i18n::Language,
    library::QueryInput,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    theme::ThemeName,
};
use media_tagger_core::models::{
    AssetQueryPageResult, AssetQuerySection, AssetQuerySectionsResult, AssetSummary, ScanRoot,
    StartAssetQueryResult,
};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    ui::layers,
    views::{
        gallery::{self, GalleryView},
        shell::ShellView,
    },
    window::{init_application, open_main_window},
};

const PAGE: usize = 128;

fn asset(index: usize) -> AssetSummary {
    let id = index as i64 + 1;
    AssetSummary {
        id,
        file_name: format!("asset-{id}.png"),
        preview_path: None,
        kind: "image".to_owned(),
        modified_at: 0,
        width: None,
        height: None,
        duration_ms: None,
        thumb_version: format!("{id:016x}"),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

fn page(offset: usize, total: usize) -> Vec<AssetSummary> {
    (offset..(offset + PAGE).min(total)).map(asset).collect()
}

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
}

impl Fixture {
    fn library(total: usize, sections: Vec<AssetQuerySection>) -> Self {
        let backend = Arc::new(FakeBackend::new());
        let fixture = Self {
            backend,
            executor: Arc::new(ManualExecutor::new()),
        };
        fixture.serve(1, total, sections);
        fixture
            .backend
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| {
                Ok(vec![ScanRoot {
                    path: "/media".into(),
                    auto_scan_on_startup: false,
                }])
            });
        fixture
    }

    /// The backend answers the next queries with session `session`.
    fn serve(&self, session: u64, total: usize, sections: Vec<AssetQuerySection>) {
        self.backend
            .on_start_asset_query(move |request| {
                let offset = request.anchor_offset.unwrap_or(0) / PAGE * PAGE;
                Ok(StartAssetQueryResult::Ready {
                    session_id: session,
                    revision: session as i64,
                    total,
                    offset,
                    items: page(offset, total),
                })
            })
            .on_get_asset_query_page(move |_, offset, _| {
                Ok(AssetQueryPageResult::Ready {
                    session_id: session,
                    revision: session as i64,
                    total,
                    offset,
                    items: page(offset, total),
                })
            })
            .on_get_asset_query_sections(move |_| {
                Ok(AssetQuerySectionsResult::Ready {
                    session_id: session,
                    total,
                    sections: sections.clone(),
                })
            });
    }

    fn parts(&self) -> RuntimeParts {
        RuntimeParts {
            config: RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            },
            backend: self.backend.clone(),
            prefs: Arc::new(MemoryPrefsStore::default()),
            executor: self.executor.clone() as Arc<dyn Executor>,
        }
    }
}

fn section(t: u32, start: usize, count: usize) -> AssetQuerySection {
    AssetQuerySection { t, start, count }
}

/// 100 000 results over three months and a no-date tail; March 2024 split in two.
fn months() -> Vec<AssetQuerySection> {
    vec![
        section(202403, 0, 5_000),
        section(202403, 5_000, 45_000),
        section(202402, 50_000, 25_000),
        section(202312, 75_000, 24_990),
        section(0, 99_990, 10),
    ]
}

fn open(
    fixture: &Fixture,
    cx: &mut TestAppContext,
) -> (WindowHandle<ShellView>, VisualTestContext) {
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window(profile::E2E, fixture.parts(), cx)
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    settle(fixture, &mut cx);
    (window, cx)
}

fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    loop {
        cx.run_until_parked();
        if fixture.executor.run_all() == 0 {
            break;
        }
    }
    cx.run_until_parked();
}

fn gallery_view(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<GalleryView> {
    window
        .read_with(cx, |shell, _| shell.gallery().cloned())
        .expect("the window is open")
        .expect("the gallery is shown")
}

fn read<T>(
    window: &WindowHandle<ShellView>,
    cx: &VisualTestContext,
    f: impl FnOnce(&GalleryView) -> T,
) -> T {
    gallery_view(window, cx).read_with(cx, |gallery, _| f(gallery))
}

fn bounds(selector: &'static str, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(selector)
}

fn viewport(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    bounds(gallery::VIEWPORT_SELECTOR, cx).expect("the gallery viewport")
}

fn f(value: Pixels) -> f32 {
    f32::from(value)
}

/// Runs the callbacks of the next frame (the timeline applies a drag target per frame).
fn next_frame(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
}

#[gpui::test]
fn the_bar_spans_the_viewport_at_the_right_edge_and_only_when_it_scrolls(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, Vec::new());
    let (_, mut cx) = open(&fixture, cx);

    let viewport = viewport(&mut cx);
    let bar = bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).expect("the position bar");
    assert_eq!(f(bar.right()), f(viewport.right()) - 4.0);
    assert_eq!(f(bar.size.width), 10.0);
    assert_eq!(f(bar.top()), f(viewport.top()) + 4.0);
    assert_eq!(f(bar.size.height), f(viewport.size.height) - 8.0);
    let thumb = bounds(gallery::SCROLLBAR_THUMB_SELECTOR, &mut cx).expect("the thumb");
    assert_eq!((f(thumb.top()), f(thumb.size.height)), (f(bar.top()), 32.0));
    // Without sections there is no timeline.
    assert!(bounds(gallery::TIMELINE_SELECTOR, &mut cx).is_none());
}

#[gpui::test]
fn a_short_result_has_no_bar(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7, vec![section(202403, 0, 4), section(202402, 4, 3)]);
    let (_, mut cx) = open(&fixture, cx);
    assert!(bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).is_none());
    // Two months: the timeline is there even when nothing scrolls, as in Solid.
    assert!(bounds(gallery::TIMELINE_SELECTOR, &mut cx).is_some());
}

#[gpui::test]
fn a_track_press_centres_the_thumb_and_a_drag_follows_the_pointer(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, Vec::new());
    let (window, mut cx) = open(&fixture, cx);
    let bar = bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).expect("the position bar");
    let (min, max) = read(&window, &cx, |g| {
        (g.grid().min_offset(), g.grid().max_offset())
    });
    let travel = f64::from(f(bar.size.height)) - 32.0;
    let x = bar.center().x;

    // The thumb's centre goes to the pointer: half the travel below its top.
    let press = point(x, bar.top() + px(16.0 + travel as f32 / 2.0));
    cx.simulate_mouse_down(press, MouseButton::Left, Modifiers::none());
    let half = min + (max - min) / 2.0;
    let offset = read(&window, &cx, |g| g.grid().offset());
    assert!((offset - half).abs() < 1.0, "{offset} vs {half}");
    // The press leaves the focus where it was: neither the bar nor the grid (whose
    // own press handler focuses it) takes it.
    let (bar_focus, grid_focus) = read(&window, &cx, |g| {
        (g.scrollbar_focus().clone(), g.grid_focus().clone())
    });
    assert!(!cx.update(|window, _| bar_focus.is_focused(window)));
    assert!(!cx.update(|window, _| grid_focus.is_focused(window)));

    // A tenth of the travel further down moves a tenth of the range.
    cx.simulate_mouse_move(
        point(x, press.y + px(travel as f32 / 10.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    let offset = read(&window, &cx, |g| g.grid().offset());
    let expected = min + (max - min) * 0.6;
    assert!((offset - expected).abs() < 1.0, "{offset} vs {expected}");
    // Far past the end: the end, and the drag goes on outside the bar.
    cx.simulate_mouse_move(
        point(x - px(300.0), px(5_000.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(read(&window, &cx, |g| g.grid().offset()), max);
    cx.simulate_mouse_up(point(x, px(5_000.0)), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(x, bar.top()), None, Modifiers::none());
    assert_eq!(read(&window, &cx, |g| g.grid().offset()), max);
}

#[gpui::test]
fn the_focused_bar_scrolls_by_row_page_and_to_the_ends(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, Vec::new());
    let (window, mut cx) = open(&fixture, cx);
    let focus = read(&window, &cx, |g| g.scrollbar_focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let (stride, viewport) = read(&window, &cx, |g| {
        (g.grid().layout().stride, g.grid().geometry().viewport)
    });
    let start = read(&window, &cx, |g| g.grid().offset());

    cx.simulate_keystrokes("down");
    assert_eq!(read(&window, &cx, |g| g.grid().offset()), start + stride);
    cx.simulate_keystrokes("up");
    assert_eq!(read(&window, &cx, |g| g.grid().offset()), start);
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.simulate_keystrokes("pagedown");
    assert_eq!(read(&window, &cx, |g| g.grid().offset()), start + viewport);
    cx.simulate_keystrokes("end");
    assert_eq!(
        read(&window, &cx, |g| g.grid().offset()),
        read(&window, &cx, |g| g.grid().max_offset())
    );
    cx.simulate_keystrokes("home");
    assert_eq!(
        read(&window, &cx, |g| g.grid().offset()),
        read(&window, &cx, |g| g.grid().min_offset())
    );
}

#[gpui::test]
fn the_timeline_sits_beside_the_bar_and_steps_months_with_the_keys(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);

    let viewport = viewport(&mut cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    assert_eq!(f(rail.right()), f(viewport.right()) - 24.0);
    assert_eq!(f(rail.size.width), 10.0);
    assert_eq!(f(rail.size.height), f(viewport.size.height) - 8.0);
    let (now, max) = read(&window, &cx, |g| {
        (g.timeline().value_now(), g.timeline().value_max())
    });
    assert_eq!((now, max), (1, 4));
    assert!(bounds(gallery::TIMELINE_LABEL_SELECTOR, &mut cx).is_none());

    let focus = read(&window, &cx, |g| g.timeline_focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.run_until_parked();
    // Focused: the current month is labelled beside the rail.
    assert!(bounds(gallery::TIMELINE_LABEL_SELECTOR, &mut cx).is_some());

    cx.simulate_keystrokes("down");
    let first = |cx: &VisualTestContext| {
        read(&window, cx, |g| {
            g.grid().range().expect("rows").visible_start_index
        })
    };
    // The row that holds the month's first item comes to the top (six columns).
    let columns = read(&window, &cx, |g| g.grid().layout().columns);
    let row_of = |index: usize| index / columns * columns;
    assert_eq!(first(&cx), row_of(50_000));
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 2);
    cx.simulate_keystrokes("pagedown");
    assert_eq!(first(&cx), row_of(75_000));
    cx.simulate_keystrokes("home");
    assert_eq!(first(&cx), 0);
    // End: the no-date month starts near the end, where the scroll clamps; the
    // timeline still reports the month it moved to.
    cx.simulate_keystrokes("end");
    assert!(first(&cx) < 99_990);
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 4);
    cx.simulate_keystrokes("up");
    assert_eq!(first(&cx), row_of(75_000));
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 3);
}

#[gpui::test]
fn a_timeline_drag_jumps_once_per_frame_and_holds_demand_until_release(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    let x = rail.center().x;
    let at = |fraction: f32| point(x, rail.top() + rail.size.height * fraction);

    cx.simulate_mouse_down(at(0.3), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(0.45), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(0.8), MouseButton::Left, Modifiers::none());
    // Demand is held and nothing jumped before the frame.
    assert!(read(&window, &cx, |g| g.grid().paused()));
    assert!(read(&window, &cx, |g| g.timeline().dragging()));
    assert_eq!(
        read(&window, &cx, |g| g
            .grid()
            .range()
            .expect("rows")
            .visible_start_index),
        0
    );
    // The press focused the rail.
    let focus = read(&window, &cx, |g| g.timeline_focus().clone());
    assert!(cx.update(|window, _| focus.is_focused(window)));

    next_frame(&mut cx);
    let first = read(&window, &cx, |g| {
        g.grid().range().expect("rows").visible_start_index
    });
    // 80 % of 100 000 lies in December 2023; the grid shows its row at the top.
    assert!((79_990..=80_000).contains(&first), "{first}");
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 3);
    assert!(bounds(gallery::TIMELINE_LABEL_SELECTOR, &mut cx).is_some());

    cx.simulate_mouse_up(at(0.8), MouseButton::Left, Modifiers::none());
    assert!(!read(&window, &cx, |g| g.timeline().dragging()));
    assert!(!read(&window, &cx, |g| g.grid().paused()));
}

#[gpui::test]
fn sections_unavailable_for_the_snapshot_hide_the_timeline(cx: &mut TestAppContext) {
    // ADR 0027: `ready` with no sections while the total is not zero.
    let fixture = Fixture::library(100_000, Vec::new());
    let (window, mut cx) = open(&fixture, cx);
    assert!(bounds(gallery::TIMELINE_SELECTOR, &mut cx).is_none());
    assert!(!read(&window, &cx, |g| g.timeline().visible()));
}

#[gpui::test]
fn a_result_of_one_month_has_no_timeline(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, vec![section(202403, 0, 100_000)]);
    let (_, mut cx) = open(&fixture, cx);
    assert!(bounds(gallery::TIMELINE_SELECTOR, &mut cx).is_none());
    assert!(bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).is_some());
}

// ---------------------------------------------------------------------------
// Ported from GalleryScrollbar.test.tsx and GalleryTimelineScrubber.test.tsx (the
// view's part of each case; the rules are in media_tagger_client's
// tests/gallery_position_ported.rs), and the acceptance criteria of epic E10.
// ---------------------------------------------------------------------------

fn offset(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> f64 {
    read(window, cx, |g| g.grid().offset())
}

fn first_visible(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> usize {
    read(window, cx, |g| {
        g.grid().range().expect("rows").visible_start_index
    })
}

/// "drags the thumb relative to its grab point and clamps to the ends"
#[gpui::test]
fn a_press_on_the_thumb_does_not_scroll_and_its_drag_keeps_the_grab_point(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, Vec::new());
    let (window, mut cx) = open(&fixture, cx);
    let bar = bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).expect("the position bar");
    let thumb = bounds(gallery::SCROLLBAR_THUMB_SELECTOR, &mut cx).expect("the thumb");
    let (min, max) = read(&window, &cx, |g| {
        (g.grid().min_offset(), g.grid().max_offset())
    });
    let start = offset(&window, &cx);
    let travel = f64::from(f(bar.size.height)) - 32.0;

    // Below the thumb's centre, but on it: nothing moves.
    let grab = point(thumb.center().x, thumb.top() + px(20.0));
    cx.simulate_mouse_down(grab, MouseButton::Left, Modifiers::none());
    assert_eq!(offset(&window, &cx), start);

    // A quarter of the travel down moves a quarter of the range from where it was.
    cx.simulate_mouse_move(
        point(grab.x, grab.y + px(travel as f32 / 4.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    let expected = start + (max - min) / 4.0;
    let moved = offset(&window, &cx);
    assert!((moved - expected).abs() < 1.0, "{moved} vs {expected}");
    // Far above the bar: the start.
    cx.simulate_mouse_move(
        point(grab.x, px(-2_000.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(offset(&window, &cx), min);
    cx.simulate_mouse_up(
        point(grab.x, px(-2_000.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
}

/// "scrolls by row, page and to the ends from the keyboard": Page Up, which the
/// first bar case leaves out, scrolls back by one viewport (not the grid's 7/8).
#[gpui::test]
fn the_focused_bar_pages_up_by_one_viewport(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, Vec::new());
    let (window, mut cx) = open(&fixture, cx);
    let focus = read(&window, &cx, |g| g.scrollbar_focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    let viewport = read(&window, &cx, |g| g.grid().geometry().viewport);

    cx.simulate_keystrokes("pagedown");
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.simulate_keystrokes("pagedown");
    let before = offset(&window, &cx);
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.simulate_keystrokes("pageup");
    assert_eq!(offset(&window, &cx), before - viewport);
    assert!(cx.update(|window, _| focus.is_focused(window)));
}

/// Both are tab stops of the gallery, the bar first, then the rail (the Solid DOM
/// order: `GalleryScrollbar`, then `GalleryTimelineScrubber`).
#[gpui::test]
fn tab_reaches_the_bar_and_then_the_timeline(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let (bar, rail) = read(&window, &cx, |g| {
        (g.scrollbar_focus().clone(), g.timeline_focus().clone())
    });
    cx.update(layers::focus_root);
    cx.run_until_parked();
    let mut tabs = 0;
    while !cx.update(|window, _| bar.is_focused(window)) {
        tabs += 1;
        assert!(tabs <= 32, "Tab never reaches the position bar");
        assert!(
            !cx.update(|window, _| rail.is_focused(window)),
            "the timeline comes after the bar"
        );
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
    }
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert!(cx.update(|window, _| rail.is_focused(window)));
}

/// "takes keyboard focus on a mouse press even though pointerdown is
/// default-prevented": the keys that follow a click move the timeline.
#[gpui::test]
fn keys_after_a_click_move_the_timeline(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    let rail_focus = read(&window, &cx, |g| g.timeline_focus().clone());
    let columns = read(&window, &cx, |g| g.grid().layout().columns);
    let row_of = |index: usize| index / columns * columns;

    let top = point(rail.center().x, rail.top() + px(1.0));
    cx.simulate_mouse_down(top, MouseButton::Left, Modifiers::none());
    next_frame(&mut cx);
    cx.simulate_mouse_up(top, MouseButton::Left, Modifiers::none());
    assert!(cx.update(|window, _| rail_focus.is_focused(window)));
    cx.simulate_keystrokes("down");
    assert_eq!(first_visible(&window, &cx), row_of(50_000));
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 2);
}

/// The end of the same case: "A press the rail ignores does not steal focus" (a
/// right-button press while another element has the focus).
#[gpui::test]
fn a_right_press_on_the_timeline_keeps_the_focus_where_it_was(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    let (rail_focus, grid_focus) = read(&window, &cx, |g| {
        (g.timeline_focus().clone(), g.grid_focus().clone())
    });
    let start = offset(&window, &cx);

    cx.update(|window, cx| window.focus(&grid_focus, cx));
    cx.run_until_parked();
    let centre = rail.center();
    cx.simulate_mouse_down(centre, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(centre, MouseButton::Right, Modifiers::none());
    next_frame(&mut cx);
    assert!(!read(&window, &cx, |g| g.timeline().dragging()));
    assert_eq!(offset(&window, &cx), start);
    assert!(
        !cx.update(|window, _| rail_focus.is_focused(window)),
        "a right press took the focus to the timeline"
    );
    assert!(cx.update(|window, _| grid_focus.is_focused(window)));
}

/// "steps months with arrows, years with PageUp/PageDown" in the view: Up first
/// returns to the start of the current month, Page Up to the start of the current
/// year; a modified arrow (Ctrl) moves nothing and holds no demand.
#[gpui::test]
fn the_focused_timeline_returns_to_the_month_and_year_start_first(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    let columns = read(&window, &cx, |g| g.grid().layout().columns);
    let row_of = |index: usize| index / columns * columns;
    let at = |fraction: f32| point(rail.center().x, rail.top() + rail.size.height * fraction);
    let click = |fraction: f32, cx: &mut VisualTestContext| {
        cx.simulate_mouse_down(at(fraction), MouseButton::Left, Modifiers::none());
        next_frame(cx);
        cx.simulate_mouse_up(at(fraction), MouseButton::Left, Modifiers::none());
    };

    // 60 % lies inside February 2024.
    click(0.6, &mut cx);
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 2);
    cx.simulate_keystrokes("up");
    assert_eq!(first_visible(&window, &cx), row_of(50_000));
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 2);
    cx.simulate_keystrokes("up");
    assert_eq!(first_visible(&window, &cx), 0);

    // 90 % lies inside December 2023, the only month of 2023.
    click(0.9, &mut cx);
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 3);
    cx.simulate_keystrokes("pageup");
    assert_eq!(first_visible(&window, &cx), row_of(75_000));
    cx.simulate_keystrokes("pageup");
    assert_eq!(first_visible(&window, &cx), 0);

    let before = offset(&window, &cx);
    cx.simulate_keystrokes("ctrl-down");
    assert_eq!(offset(&window, &cx), before);
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 1);
    assert!(!read(&window, &cx, |g| g.grid().paused()));
}

/// "Hovering only previews the month under the pointer" (the end of the drag case):
/// the label follows the pointer, nothing jumps, and it goes when the pointer leaves.
#[gpui::test]
fn hovering_the_timeline_previews_a_month_without_jumping(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    assert!(bounds(gallery::TIMELINE_LABEL_SELECTOR, &mut cx).is_none());
    let start = offset(&window, &cx);

    let over = point(rail.center().x, rail.top() + rail.size.height * 0.8);
    cx.simulate_mouse_move(over, None, Modifiers::none());
    next_frame(&mut cx);
    let label = bounds(gallery::TIMELINE_LABEL_SELECTOR, &mut cx).expect("the month label");
    // The label is centred on the pointer, to the left of the rail (`right-5`).
    assert!((f(label.center().y) - f(over.y)).abs() <= 1.0);
    assert_eq!(f(label.right()), f(rail.right()) - 20.0);
    // One `text-xs` line (16 px), `py-1` and a 1 px border: 26 px high.
    assert_eq!(f(label.size.height), 26.0);
    assert_eq!(offset(&window, &cx), start);
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 1);
    assert!(!read(&window, &cx, |g| g.grid().paused()));

    cx.simulate_mouse_move(
        point(rail.left() - px(200.0), over.y),
        None,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(bounds(gallery::TIMELINE_LABEL_SELECTOR, &mut cx).is_none());
}

/// A drag goes on outside the rail and ends with a release anywhere (the Solid rail's
/// pointer capture): demand resumes and later moves jump nowhere.
#[gpui::test]
fn a_timeline_drag_follows_the_pointer_outside_the_rail(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    let x = rail.center().x;

    cx.simulate_mouse_down(
        point(x, rail.top() + rail.size.height * 0.1),
        MouseButton::Left,
        Modifiers::none(),
    );
    // Far below and to the left of the rail: the end of the result.
    let outside = point(x - px(400.0), rail.bottom() + px(500.0));
    cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::none());
    next_frame(&mut cx);
    assert_eq!(read(&window, &cx, |g| g.timeline().value_now()), 4);
    assert!(read(&window, &cx, |g| g.grid().paused()));

    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    assert!(!read(&window, &cx, |g| g.timeline().dragging()));
    assert!(!read(&window, &cx, |g| g.grid().paused()));
    let settled = offset(&window, &cx);
    cx.simulate_mouse_move(
        point(x, rail.top() + rail.size.height * 0.1),
        MouseButton::Left,
        Modifiers::none(),
    );
    next_frame(&mut cx);
    assert_eq!(offset(&window, &cx), settled);
}

/// Parity of `gallery-default` as both apps draw it: the control seed is a handful of
/// files from one month that do not fill the viewport, and the Solid gallery then shows
/// neither the custom bar (no window mode) nor the rail (`months().length > 1`).
#[gpui::test]
fn a_few_files_of_one_month_show_neither_bar_nor_timeline(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7, vec![section(202403, 0, 7)]);
    let (_, mut cx) = open(&fixture, cx);
    assert!(bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).is_none());
    assert!(bounds(gallery::TIMELINE_SELECTOR, &mut cx).is_none());
}

/// A new query session while the rail or the bar is gone (a scan or a tag write
/// refreshed the library): the drag on it ends, as the Solid parts' cleanup does on
/// unmount, instead of waiting for a release the hidden element can no longer get.
fn restart(
    fixture: &Fixture,
    window: &WindowHandle<ShellView>,
    session: u64,
    total: usize,
    sections: Vec<AssetQuerySection>,
    cx: &mut VisualTestContext,
) {
    fixture.serve(session, total, sections);
    window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open")
        .update(cx, |runtime, cx| {
            runtime.send(RuntimeInput::Query(QueryInput::Refresh), cx)
        });
    settle(fixture, cx);
}

#[gpui::test]
fn a_rail_that_hides_during_a_drag_ends_it_and_comes_back_fresh(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    let x = rail.center().x;
    let at = |fraction: f32| point(x, rail.top() + rail.size.height * fraction);
    cx.simulate_mouse_down(at(0.3), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(0.5), MouseButton::Left, Modifiers::none());
    next_frame(&mut cx);
    assert!(read(&window, &cx, |g| g.timeline().dragging()));
    assert!(read(&window, &cx, |g| g.grid().paused()));

    // The new session has no sections for its snapshot: the rail goes away mid-drag.
    restart(&fixture, &window, 2, 100_000, Vec::new(), &mut cx);
    assert!(bounds(gallery::TIMELINE_SELECTOR, &mut cx).is_none());
    assert!(!read(&window, &cx, |g| g.timeline().dragging()));
    assert!(!read(&window, &cx, |g| g.grid().paused()));
    // The release lands while the rail is hidden.
    cx.simulate_mouse_up(at(0.5), MouseButton::Left, Modifiers::none());

    // Back with its months: no drag and no stale label, and a left drag elsewhere in
    // the window (over the tiles, level with the rail's top tenth) moves nothing.
    restart(&fixture, &window, 3, 100_000, months(), &mut cx);
    assert!(bounds(gallery::TIMELINE_SELECTOR, &mut cx).is_some());
    assert!(!read(&window, &cx, |g| g.timeline().dragging()));
    assert!(bounds(gallery::TIMELINE_LABEL_SELECTOR, &mut cx).is_none());
    let start = offset(&window, &cx);
    let viewport = viewport(&mut cx);
    cx.simulate_mouse_move(
        point(viewport.center().x, at(0.1).y),
        MouseButton::Left,
        Modifiers::none(),
    );
    next_frame(&mut cx);
    assert_eq!(offset(&window, &cx), start);
    assert!(!read(&window, &cx, |g| g.timeline().dragging()));
}

#[gpui::test]
fn a_bar_that_hides_during_a_drag_forgets_it(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, Vec::new());
    let (window, mut cx) = open(&fixture, cx);
    let thumb = bounds(gallery::SCROLLBAR_THUMB_SELECTOR, &mut cx).expect("the thumb");
    cx.simulate_mouse_down(thumb.center(), MouseButton::Left, Modifiers::none());

    // An empty result: the bar goes away and the release lands nowhere.
    restart(&fixture, &window, 2, 0, Vec::new(), &mut cx);
    assert!(bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).is_none());
    cx.simulate_mouse_up(thumb.center(), MouseButton::Left, Modifiers::none());

    // The bar is back: a left drag elsewhere does not move the grid.
    restart(&fixture, &window, 3, 100_000, Vec::new(), &mut cx);
    assert!(bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).is_some());
    let start = offset(&window, &cx);
    let viewport = viewport(&mut cx);
    cx.simulate_mouse_move(
        point(viewport.center().x, viewport.bottom() - px(10.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(offset(&window, &cx), start);
}

/// A left press and its release handled before the next frame is drawn, as a
/// synthetic click (or one shorter than a refresh interval) reaches the window: one
/// batch of platform events dispatched against the frame drawn before the press.
fn click_within_one_frame(position: Point<Pixels>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position,
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                position,
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: 1,
            }),
            cx,
        );
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_bar_click_released_before_the_next_frame_ends_its_drag(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, Vec::new());
    let (window, mut cx) = open(&fixture, cx);
    let bar = bounds(gallery::SCROLLBAR_SELECTOR, &mut cx).expect("the position bar");
    let start = offset(&window, &cx);
    click_within_one_frame(
        point(bar.center().x, bar.top() + bar.size.height * 0.9),
        &mut cx,
    );
    next_frame(&mut cx);
    let clicked = offset(&window, &cx);
    assert!(clicked > start, "the press scrolls: {clicked} vs {start}");

    // A later left drag elsewhere in the window does not move the grid.
    let viewport = viewport(&mut cx);
    cx.simulate_mouse_move(
        point(viewport.center().x, viewport.top() + px(10.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    next_frame(&mut cx);
    assert_eq!(offset(&window, &cx), clicked);
}

#[gpui::test]
fn a_timeline_click_released_before_the_next_frame_ends_its_drag(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000, months());
    let (window, mut cx) = open(&fixture, cx);
    let rail = bounds(gallery::TIMELINE_SELECTOR, &mut cx).expect("the timeline");
    click_within_one_frame(
        point(rail.center().x, rail.top() + rail.size.height * 0.6),
        &mut cx,
    );
    next_frame(&mut cx);
    assert!(!read(&window, &cx, |g| g.timeline().dragging()));
    assert!(!read(&window, &cx, |g| g.grid().paused()));
    let clicked = offset(&window, &cx);

    // A later left drag elsewhere in the window neither jumps nor holds demand.
    let viewport = viewport(&mut cx);
    cx.simulate_mouse_move(
        point(viewport.center().x, rail.top() + rail.size.height * 0.1),
        MouseButton::Left,
        Modifiers::none(),
    );
    next_frame(&mut cx);
    assert_eq!(offset(&window, &cx), clicked);
    assert!(!read(&window, &cx, |g| g.timeline().dragging()));
    assert!(!read(&window, &cx, |g| g.grid().paused()));
}
