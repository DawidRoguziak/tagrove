//! Selection mode in the gallery (epic E11) on GPUI's test platform (no display, no
//! GPU) with the scripted backend and the deterministic executor: the bulk-actions
//! toggle, clicks with and without modifiers, the rectangle with its hold delay and
//! its swallowed click, the long press, Escape and Ctrl+A, the Retry of a range that
//! could not be read, and the start state of the `gallery-selection` scenario.

use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use gpui::{
    point, px, Bounds, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, WindowHandle,
};
use media_tagger_client::{
    backend::{fake::Call, AssetIdRange, BackendError, FakeBackend},
    exec::{Executor, ManualExecutor},
    gallery::{SelectionInput, DRAG_HOLD_DELAY, LONG_PRESS},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    scenario::GALLERY_SELECTION_CLICKS,
    theme::ThemeName,
};
use media_tagger_core::models::{
    AssetQueryPageResult, AssetQuerySectionsResult, AssetSummary, ScanRoot, StartAssetQueryResult,
};
use media_tagger_gpui::{
    profile,
    runtime::{Runtime, RuntimeParts},
    ui::layers::{self, LayerOptions},
    views::{
        gallery::{self, GalleryEvent, GalleryView},
        shell::ShellView,
    },
    window::{init_application, open_main_window, open_main_window_started, ShellStart},
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
    /// While set, reads of ID ranges longer than one ID fail.
    fail_ranges: Arc<AtomicBool>,
}

impl Fixture {
    /// A library of `total` synthetic assets (ID = index + 1) in one scan root.
    fn library(total: usize) -> Self {
        let backend = Arc::new(FakeBackend::new());
        let fail_ranges = Arc::new(AtomicBool::new(false));
        let failing = fail_ranges.clone();
        backend
            .on_start_asset_query(move |request| {
                let offset = request.anchor_offset.unwrap_or(0) / PAGE * PAGE;
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total,
                    offset,
                    items: page(offset, total),
                })
            })
            .on_get_asset_query_page(move |_, offset, _| {
                Ok(AssetQueryPageResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total,
                    offset,
                    items: page(offset, total),
                })
            })
            .on_get_asset_query_sections(move |_| {
                Ok(AssetQuerySectionsResult::Ready {
                    session_id: 1,
                    total,
                    sections: Vec::new(),
                })
            })
            .on_get_asset_query_id_range(move |_, start, end| {
                if start != end && failing.load(Ordering::SeqCst) {
                    return Err(BackendError::rejected("i18n:common.libraryBusy"));
                }
                Ok(AssetIdRange::Ready(
                    (start..=end).map(|index| index as i64 + 1).collect(),
                ))
            })
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| {
                Ok(vec![ScanRoot {
                    path: "/media".into(),
                    auto_scan_on_startup: false,
                }])
            });
        Self {
            backend,
            executor: Arc::new(ManualExecutor::new()),
            fail_ranges,
        }
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

    /// The ID ranges read from the backend since the last call.
    fn take_id_ranges(&self) -> Vec<(usize, usize)> {
        self.backend
            .take_calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::GetAssetQueryIdRange { start, end, .. } => Some((start, end)),
                _ => None,
            })
            .collect()
    }
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

/// Runs the backend calls and lets the replies reach the UI thread.
fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    loop {
        cx.run_until_parked();
        if fixture.executor.run_all() == 0 {
            break;
        }
    }
    cx.run_until_parked();
}

fn runtime(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<Runtime> {
    window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open")
}

fn gallery_view(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Entity<GalleryView> {
    window
        .read_with(cx, |shell, _| shell.gallery().cloned())
        .expect("the window is open")
        .expect("the gallery is shown")
}

fn bounds(selector: String, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    // `debug_bounds` keys by `&'static str`; a test leaks the few names it asks for.
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
}

fn tile(index: usize, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    bounds(gallery::tile_selector(index), cx).unwrap_or_else(|| panic!("tile {index} is drawn"))
}

fn marked(index: usize, cx: &mut VisualTestContext) -> bool {
    bounds(gallery::selection_mark_selector(index), cx).is_some()
}

fn marks(indexes: std::ops::Range<usize>, cx: &mut VisualTestContext) -> Vec<usize> {
    indexes.filter(|index| marked(*index, cx)).collect()
}

fn rectangle(cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(gallery::SELECTION_RECTANGLE_SELECTOR)
}

fn selection<R>(
    window: &WindowHandle<ShellView>,
    cx: &VisualTestContext,
    read: impl FnOnce(&media_tagger_client::gallery::Selection) -> R,
) -> R {
    runtime(window, cx).read_with(cx, |runtime, _| read(runtime.model().selection()))
}

fn ids(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Vec<i64> {
    selection(window, cx, |selection| selection.ids().to_vec())
}

/// How many layers the window's layer stack holds.
fn layer_count(cx: &mut VisualTestContext) -> usize {
    cx.update(|_, cx| layers::layer_ids(cx).len())
}

/// Clicks the bulk-actions toggle.
fn toggle_mode(fixture: &Fixture, cx: &mut VisualTestContext) {
    let toggle = cx
        .debug_bounds(gallery::BULK_TOGGLE_ID)
        .expect("the bulk-actions toggle");
    cx.simulate_click(toggle.center(), Modifiers::none());
    settle(fixture, cx);
}

fn events(
    window: &WindowHandle<ShellView>,
    cx: &mut VisualTestContext,
) -> Arc<Mutex<Vec<GalleryEvent>>> {
    let gallery = gallery_view(window, cx);
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&gallery, move |_, event: &GalleryEvent, _| {
            seen.lock().unwrap().push(*event);
        })
        .detach();
    });
    events
}

fn ctrl() -> Modifiers {
    Modifiers {
        control: true,
        ..Modifiers::none()
    }
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Modifiers::none()
    }
}

/// Presses at `from`, holds past the drag delay and moves to `to`, button down.
fn drag(from: Point<Pixels>, to: Point<Pixels>, modifiers: Modifiers, cx: &mut VisualTestContext) {
    cx.simulate_mouse_down(from, MouseButton::Left, modifiers);
    cx.executor().advance_clock(DRAG_HOLD_DELAY);
    cx.simulate_mouse_move(to, MouseButton::Left, modifiers);
    cx.run_until_parked();
}

#[gpui::test]
fn the_toggle_turns_selection_mode_on_and_off_and_a_click_selects_instead_of_opening(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    let opened = events(&window, &mut cx);
    assert!(!selection(&window, &cx, |selection| selection.enabled()));
    assert_eq!(layer_count(&mut cx), 0);

    toggle_mode(&fixture, &mut cx);
    assert!(selection(&window, &cx, |selection| selection.enabled()));
    assert_eq!(layer_count(&mut cx), 1, "selection mode holds a layer");

    let first = tile(0, &mut cx).center();
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1]);
    assert!(marked(0, &mut cx) && !marked(1, &mut cx));
    assert!(
        opened.lock().unwrap().is_empty(),
        "nothing opens in selection mode"
    );

    // The same tile again: unselected.
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty() && !marked(0, &mut cx));

    // Off: the selection goes, the layer goes, and a click opens again.
    cx.simulate_click(first, Modifiers::none());
    toggle_mode(&fixture, &mut cx);
    assert!(!selection(&window, &cx, |selection| selection.enabled()));
    assert!(ids(&window, &cx).is_empty() && !marked(0, &mut cx));
    assert_eq!(layer_count(&mut cx), 0);
    cx.simulate_click(first, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        *opened.lock().unwrap(),
        [GalleryEvent::OpenAsset {
            asset_id: 1,
            index: 0
        }]
    );
}

#[gpui::test]
fn selection_mode_leaves_room_for_the_dock_below_the_end_of_the_result(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    let (_window, mut cx) = open(&fixture, cx);
    let viewport = cx
        .debug_bounds(gallery::VIEWPORT_SELECTOR)
        .expect("the gallery viewport");
    let footer = |cx: &mut VisualTestContext| {
        cx.debug_bounds(gallery::FOOTER_SELECTOR)
            .expect("the status footer")
    };
    assert_eq!(footer(&mut cx).bottom(), viewport.bottom());

    // `.workspace-body--bulk { padding-bottom: 88px }`.
    toggle_mode(&fixture, &mut cx);
    assert_eq!(footer(&mut cx).bottom(), viewport.bottom() - px(88.0));
    toggle_mode(&fixture, &mut cx);
    assert_eq!(footer(&mut cx).bottom(), viewport.bottom());
}

#[gpui::test]
fn ctrl_click_toggles_and_shift_click_reads_the_range_from_the_anchor(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    fixture.backend.take_calls();

    let at = |index: usize, cx: &mut VisualTestContext| tile(index, cx).center();
    let position = at(1, &mut cx);
    cx.simulate_click(position, Modifiers::none());
    let position = at(0, &mut cx);
    cx.simulate_click(position, ctrl());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [2, 1]);
    assert!(fixture.take_id_ranges().is_empty(), "clicks read nothing");

    // Shift replaces the selection with the range from the anchor (the Ctrl-click).
    let position = at(2, &mut cx);
    cx.simulate_click(position, shift());
    cx.run_until_parked();
    assert!(selection(&window, &cx, |selection| selection.busy()));
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1, 2, 3]);
    assert_eq!(fixture.take_id_ranges(), [(0, 2)]);
    assert_eq!(marks(0..12, &mut cx), [0, 1, 2]);

    // Ctrl+Shift adds the range from the same anchor.
    let position = at(7, &mut cx);
    cx.simulate_click(
        position,
        Modifiers {
            control: true,
            shift: true,
            ..Modifiers::none()
        },
    );
    settle(&fixture, &mut cx);
    assert_eq!(selection(&window, &cx, |selection| selection.count()), 8);
    assert_eq!(marks(0..12, &mut cx), (0..8).collect::<Vec<_>>());
}

#[gpui::test]
fn a_held_press_that_moves_draws_a_rectangle_and_its_release_selects_what_it_covers(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let gallery = gallery_view(&window, &cx);
    let (first, eighth) = (tile(0, &mut cx), tile(7, &mut cx));

    // Moved at once: no rectangle, and the release is a click on the pressed tile.
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(eighth.center(), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(rectangle(&mut cx).is_none());
    assert!(!gallery.read_with(&cx, |gallery, _| gallery.gesture().dragging()));
    cx.simulate_mouse_up(eighth.center(), MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());
    assert!(!selection(&window, &cx, |selection| selection.busy()));

    // Held for the delay, then moved from the first tile to the eighth (second row,
    // second column): the rectangle and the tiles under it show at once.
    drag(first.center(), eighth.center(), Modifiers::none(), &mut cx);
    assert!(gallery.read_with(&cx, |gallery, _| gallery.gesture().dragging()));
    let drawn = rectangle(&mut cx).expect("the selection rectangle");
    assert_eq!(drawn.origin, first.center());
    assert_eq!(drawn.bottom_right(), eighth.center());
    assert_eq!(marks(0..12, &mut cx), [0, 1, 6, 7]);
    assert!(
        ids(&window, &cx).is_empty(),
        "nothing is selected before the release"
    );
    assert!(selection(&window, &cx, |selection| selection.busy()));

    cx.simulate_mouse_up(eighth.center(), MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(rectangle(&mut cx).is_none());
    assert_eq!(ids(&window, &cx), [1, 2, 7, 8]);
    assert_eq!(marks(0..12, &mut cx), [0, 1, 6, 7]);

    // A rectangle that ends on the pressed tile: the click of that release does not
    // toggle the tile.
    let inside = first.center() + point(px(30.0), px(30.0));
    drag(first.center(), inside, Modifiers::none(), &mut cx);
    cx.simulate_mouse_up(inside, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(
        ids(&window, &cx),
        [1],
        "replaced by the rectangle, not toggled off"
    );

    // With Ctrl held at the press the rectangle adds.
    drag(
        eighth.center(),
        eighth.center() + point(px(20.0), px(20.0)),
        ctrl(),
        &mut cx,
    );
    assert_eq!(marks(0..12, &mut cx), [0, 7]);
    cx.simulate_mouse_up(
        eighth.center() + point(px(20.0), px(20.0)),
        MouseButton::Left,
        ctrl(),
    );
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1, 8]);
}

#[gpui::test]
fn a_rectangle_started_in_the_gap_selects_and_a_press_outside_selection_mode_draws_none(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    let (first, second) = (tile(0, &mut cx), tile(1, &mut cx));
    // From the gap between the first two tiles into the second one.
    let gap = point((first.right() + second.left()) / 2.0, first.center().y);
    let inside = second.center() + point(px(0.0), px(10.0));

    drag(gap, inside, Modifiers::none(), &mut cx);
    assert!(rectangle(&mut cx).is_none(), "selection mode is off");
    cx.simulate_mouse_up(inside, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    toggle_mode(&fixture, &mut cx);
    drag(gap, inside, Modifiers::none(), &mut cx);
    assert!(rectangle(&mut cx).is_some());
    assert_eq!(marks(0..12, &mut cx), [1]);
    cx.simulate_mouse_up(inside, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [2]);

    // A rectangle without a height covers nothing: it replaces the selection with
    // nothing.
    drag(gap, second.center(), Modifiers::none(), &mut cx);
    cx.simulate_mouse_up(second.center(), MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());
}

#[gpui::test]
fn dragging_near_the_bottom_edge_scrolls_the_gallery_and_the_rectangle_grows_with_it(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(10_000);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let gallery = gallery_view(&window, &cx);
    let viewport = cx
        .debug_bounds(gallery::VIEWPORT_SELECTOR)
        .expect("the gallery viewport");
    let first = tile(0, &mut cx);
    let scroll_top =
        |cx: &VisualTestContext| gallery.read_with(cx, |gallery, _| gallery.grid().scroll_top());

    // To ten pixels above the bottom edge, in the second column.
    let near_bottom = point(tile(1, &mut cx).center().x, viewport.bottom() - px(10.0));
    drag(first.center(), near_bottom, Modifiers::none(), &mut cx);
    assert_eq!(scroll_top(&cx), 0.0);
    let before = gallery.read_with(&cx, |gallery, _| {
        gallery
            .gesture()
            .preview()
            .map(|preview| preview.ranges.len())
    });

    // Frames scroll while the pointer rests in the edge zone.
    for _ in 0..30 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    let scrolled = scroll_top(&cx);
    // 900 px/s scaled by how deep the pointer is in the 48 px zone, for 30 frames.
    let expected = 900.0 * (38.0 / 48.0) * 0.016 * 30.0;
    assert!(
        (scrolled - expected).abs() < 1.0,
        "{scrolled} vs {expected}"
    );
    let after = gallery.read_with(&cx, |gallery, _| {
        gallery
            .gesture()
            .preview()
            .map(|preview| preview.ranges.len())
    });
    assert!(
        after > before,
        "{before:?} -> {after:?}: more rows are under the rectangle"
    );
    // The pressed point scrolled out of view with its tile; the rectangle is clipped
    // to the viewport.
    let drawn = rectangle(&mut cx).expect("the selection rectangle");
    assert_eq!(drawn.top(), viewport.top());
    assert_eq!(drawn.bottom(), near_bottom.y);

    cx.simulate_mouse_up(near_bottom, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    let rows = after.expect("a preview");
    assert_eq!(
        selection(&window, &cx, |selection| selection.count()),
        rows * 2
    );
    // No frame scrolls after the release.
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    assert_eq!(scroll_top(&cx), scrolled);
}

#[gpui::test]
fn a_long_press_opens_the_tile_and_leaves_the_selection_alone(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let opened = events(&window, &mut cx);
    let (first, third) = (tile(0, &mut cx).center(), tile(2, &mut cx).center());
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);

    cx.simulate_mouse_down(third, MouseButton::Left, Modifiers::none());
    cx.executor()
        .advance_clock(LONG_PRESS - Duration::from_millis(1));
    cx.run_until_parked();
    assert!(opened.lock().unwrap().is_empty());
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(
        *opened.lock().unwrap(),
        [GalleryEvent::OpenAsset {
            asset_id: 3,
            index: 2
        }]
    );
    // The release and its click change nothing.
    cx.simulate_mouse_up(third, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1]);
    assert!(!selection(&window, &cx, |selection| selection.busy()));

    // The shell opened the lightbox on the pressed tile (epic E16); it is closed
    // again before the grid is used.
    let runtime = runtime(&window, &cx);
    assert_eq!(
        runtime.read_with(&cx, |runtime, _| runtime
            .model()
            .lightbox()
            .selected()
            .map(|selected| selected.id())),
        Some(3)
    );
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(
            media_tagger_client::runtime::RuntimeInput::lightbox(
                media_tagger_client::lightbox::LightboxCommand::Close,
            ),
            cx,
        )
    });
    settle(&fixture, &mut cx);
    // The closed lightbox blocks the pointer until it has faded out.
    cx.executor().advance_clock(Duration::from_millis(100));
    settle(&fixture, &mut cx);
    assert_eq!(
        ids(&window, &cx),
        [1],
        "closing the lightbox selects nothing"
    );

    // A press with a modifier never opens.
    cx.simulate_mouse_down(third, MouseButton::Left, shift());
    cx.executor().advance_clock(LONG_PRESS * 2);
    cx.run_until_parked();
    assert_eq!(opened.lock().unwrap().len(), 1);
    cx.simulate_mouse_up(third, MouseButton::Left, shift());
    settle(&fixture, &mut cx);
    assert_eq!(
        ids(&window, &cx),
        [1, 2, 3],
        "the Shift-click selected the range"
    );
}

#[gpui::test]
fn escape_clears_the_selection_and_ctrl_a_selects_every_result(cx: &mut TestAppContext) {
    let fixture = Fixture::library(50_000);
    let (window, mut cx) = open(&fixture, cx);
    let focus =
        gallery_view(&window, &cx).read_with(&cx, |gallery, _| gallery.grid_focus().clone());

    // Outside selection mode Ctrl+A selects nothing.
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.simulate_keystrokes("ctrl-a");
    settle(&fixture, &mut cx);
    assert_eq!(selection(&window, &cx, |selection| selection.count()), 0);

    toggle_mode(&fixture, &mut cx);
    fixture.backend.take_calls();
    cx.simulate_keystrokes("ctrl-a");
    cx.run_until_parked();
    // One range, no ID read; the keepalive reads one ID of the session.
    assert_eq!(
        selection(&window, &cx, |selection| selection.count()),
        50_000
    );
    assert!(selection(&window, &cx, |selection| selection.ranged()));
    settle(&fixture, &mut cx);
    assert_eq!(fixture.take_id_ranges(), [(0, 0)]);
    assert!(marked(0, &mut cx) && marked(11, &mut cx));

    // Ctrl-click inside the range excludes one tile.
    let second = tile(1, &mut cx).center();
    cx.simulate_click(second, ctrl());
    settle(&fixture, &mut cx);
    assert_eq!(
        selection(&window, &cx, |selection| selection.count()),
        49_999
    );
    assert!(marked(0, &mut cx) && !marked(1, &mut cx));

    // Escape clears the selection and keeps selection mode.
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(selection(&window, &cx, |selection| selection.count()), 0);
    assert!(selection(&window, &cx, |selection| selection.enabled()));
    assert!(!marked(0, &mut cx));
}

#[gpui::test]
fn retry_repeats_a_range_that_could_not_be_read(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let (first, third) = (tile(0, &mut cx).center(), tile(2, &mut cx).center());
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);

    fixture.fail_ranges.store(true, Ordering::SeqCst);
    cx.simulate_click(third, shift());
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_some());
    assert_eq!(ids(&window, &cx), [1], "the selection from before stays");

    // A press on Retry is a press on a control: it starts no gesture, so the failed
    // range is still known when the click arrives.
    fixture.fail_ranges.store(false, Ordering::SeqCst);
    let retry = cx
        .debug_bounds(gallery::LOAD_RETRY_ID)
        .expect("the Retry button");
    cx.simulate_click(retry.center(), Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_none());
    assert_eq!(ids(&window, &cx), [1, 2, 3]);
}

#[gpui::test]
fn selection_mode_and_the_selection_survive_a_visit_to_the_settings_page(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let first = tile(0, &mut cx).center();
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);

    let runtime = runtime(&window, &cx);
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::OpenSettings, cx)
    });
    settle(&fixture, &mut cx);
    assert_eq!(layer_count(&mut cx), 1, "only the settings page's layer");
    // Escape closes the settings page and leaves the selection alone.
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1]);
    assert!(marked(0, &mut cx));
    assert_eq!(layer_count(&mut cx), 1, "the new gallery view's layer");

    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());
    assert!(selection(&window, &cx, |selection| selection.enabled()));
}

#[gpui::test]
fn the_selection_scenario_starts_with_its_three_tiles_selected(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window_started(
                profile::E2E,
                fixture.parts(),
                ShellStart {
                    selection: &GALLERY_SELECTION_CLICKS,
                    ..ShellStart::default()
                },
                cx,
            )
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    settle(&fixture, &mut cx);

    assert!(selection(&window, &cx, |selection| selection.enabled()));
    assert_eq!(ids(&window, &cx), [1, 2, 3]);
    assert_eq!(marks(0..7, &mut cx), [0, 1, 2]);
    assert!(!selection(&window, &cx, |selection| selection.busy()));

    // The script ran once: a later change of the selection stays.
    let runtime = runtime(&window, &cx);
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::Selection(SelectionInput::Clear), cx);
        runtime.send(RuntimeInput::RefreshLibrary, cx);
    });
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());
}

// The createGridHandlers.test.tsx cases that need a window: keys, layers, mouse
// buttons, controls, window activation and size, the view's lifetime. The gesture
// logic of the other cases is ported in media_tagger_client/tests/gallery_selection.rs.

fn focus_grid(window: &WindowHandle<ShellView>, cx: &mut VisualTestContext) {
    let focus = gallery_view(window, cx).read_with(cx, |gallery, _| gallery.grid_focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));
}

/// The window loses the focus (`blur`). The test platform opens windows inactive, so
/// the window is activated first.
fn activate(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.is_window_active()));
}

fn blur(cx: &mut VisualTestContext) {
    cx.deactivate_window();
    cx.run_until_parked();
    assert!(!cx.update(|window, _| window.is_window_active()));
}

fn count(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> usize {
    selection(window, cx, |selection| selection.count())
}

/// "selects all results with Ctrl+A in selection mode, but not from text fields or when
/// disabled": Meta+A selects too; Ctrl+Shift+A, Ctrl+Alt+A and a plain A do not, and
/// neither does Ctrl+A while another layer lies above the gallery.
#[gpui::test]
fn meta_a_selects_every_result_and_other_chords_or_a_layer_above_do_not(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    focus_grid(&window, &mut cx);

    for keys in ["ctrl-shift-a", "ctrl-alt-a", "a"] {
        cx.simulate_keystrokes(keys);
        settle(&fixture, &mut cx);
        assert_eq!(count(&window, &cx), 0, "{keys}");
    }
    cx.simulate_keystrokes("cmd-a");
    settle(&fixture, &mut cx);
    assert_eq!(count(&window, &cx), 40);
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(count(&window, &cx), 0);

    let modal = cx.update(|window, cx| {
        let container = cx.focus_handle();
        layers::register(LayerOptions::modal(container, |_, _| {}), window, cx)
    });
    cx.run_until_parked();
    focus_grid(&window, &mut cx);
    cx.simulate_keystrokes("ctrl-a");
    settle(&fixture, &mut cx);
    assert_eq!(
        count(&window, &cx),
        0,
        "a layer above the gallery keeps Ctrl+A"
    );
    cx.update(|window, cx| layers::release(modal, None, window, cx));
    cx.run_until_parked();
    focus_grid(&window, &mut cx);
    cx.simulate_keystrokes("ctrl-a");
    settle(&fixture, &mut cx);
    assert_eq!(count(&window, &cx), 40);
}

/// "lets an open or locked modal own Escape before clearing the gallery".
#[gpui::test]
fn an_open_or_locked_modal_owns_escape_before_the_gallery_clears(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let first = tile(0, &mut cx).center();
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1]);

    let dismissed = Rc::new(Cell::new(0));
    let modal = {
        let dismissed = dismissed.clone();
        cx.update(|window, cx| {
            let container = cx.focus_handle();
            layers::register(
                LayerOptions::modal(container, move |_, _| dismissed.set(dismissed.get() + 1)),
                window,
                cx,
            )
        })
    };
    cx.run_until_parked();
    focus_grid(&window, &mut cx);
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(dismissed.get(), 1);
    assert_eq!(ids(&window, &cx), [1]);

    // Locked: nobody takes Escape.
    cx.update(|_, cx| layers::set_close_on_escape(modal, false, cx));
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(dismissed.get(), 1);
    assert_eq!(ids(&window, &cx), [1]);

    cx.update(|window, cx| layers::release(modal, None, window, cx));
    cx.run_until_parked();
    focus_grid(&window, &mut cx);
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());
    assert!(selection(&window, &cx, |selection| selection.enabled()));
}

/// "cancels on blur" while dragging: window blur ends a held rectangle, and its
/// release selects nothing.
#[gpui::test]
fn window_blur_ends_a_held_rectangle(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    activate(&mut cx);
    let gallery = gallery_view(&window, &cx);
    let (first, eighth) = (tile(0, &mut cx).center(), tile(7, &mut cx).center());

    drag(first, eighth, Modifiers::none(), &mut cx);
    assert!(rectangle(&mut cx).is_some());
    blur(&mut cx);
    assert!(rectangle(&mut cx).is_none());
    assert!(!gallery.read_with(&cx, |gallery, _| gallery.gesture().held()));

    cx.simulate_mouse_up(eighth, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());
    assert!(!selection(&window, &cx, |selection| selection.busy()));
}

/// "keeps a released selection read through window blur".
#[gpui::test]
fn window_blur_keeps_a_released_rectangle_read(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    activate(&mut cx);
    let (first, eighth) = (tile(0, &mut cx).center(), tile(7, &mut cx).center());

    drag(first, eighth, Modifiers::none(), &mut cx);
    cx.simulate_mouse_up(eighth, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(selection(&window, &cx, |selection| selection.busy()));
    blur(&mut cx);
    assert!(selection(&window, &cx, |selection| selection.busy()));

    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1, 2, 7, 8]);
    assert!(!selection(&window, &cx, |selection| selection.busy()));
}

/// "cancels on resize" while dragging: a new window size ends a held rectangle.
#[gpui::test]
fn a_window_resize_ends_a_held_rectangle(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let (first, eighth) = (tile(0, &mut cx).center(), tile(7, &mut cx).center());

    drag(first, eighth, Modifiers::none(), &mut cx);
    assert!(rectangle(&mut cx).is_some());
    cx.simulate_resize(gpui::size(px(700.0), px(600.0)));
    cx.run_until_parked();
    assert!(rectangle(&mut cx).is_none());

    cx.simulate_mouse_up(eighth, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());
}

/// "keeps a released rectangle read through layout changes while removing its old
/// preview": a resize after the release removes the rectangle and keeps its read.
#[gpui::test]
fn a_window_resize_keeps_a_released_rectangle_read_and_removes_its_preview(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let (first, eighth) = (tile(0, &mut cx).center(), tile(7, &mut cx).center());

    drag(first, eighth, Modifiers::none(), &mut cx);
    cx.simulate_mouse_up(eighth, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    assert!(
        rectangle(&mut cx).is_some(),
        "shown until the model took the ranges"
    );
    cx.simulate_resize(gpui::size(px(700.0), px(600.0)));
    cx.run_until_parked();
    assert!(rectangle(&mut cx).is_none());
    assert!(selection(&window, &cx, |selection| selection.busy()));

    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1, 2, 7, 8]);
}

/// "cancels on query" and "cancels on disabled" while dragging.
#[gpui::test]
fn a_query_refresh_or_selection_mode_turned_off_ends_a_held_rectangle(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let runtime = runtime(&window, &cx);
    let (first, eighth) = (tile(0, &mut cx).center(), tile(7, &mut cx).center());

    drag(first, eighth, Modifiers::none(), &mut cx);
    assert!(rectangle(&mut cx).is_some());
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::RefreshLibrary, cx)
    });
    settle(&fixture, &mut cx);
    assert!(rectangle(&mut cx).is_none(), "the query changed");
    cx.simulate_mouse_up(eighth, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());

    drag(first, eighth, Modifiers::none(), &mut cx);
    assert!(rectangle(&mut cx).is_some());
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::Selection(SelectionInput::ToggleMode), cx)
    });
    cx.run_until_parked();
    assert!(rectangle(&mut cx).is_none(), "selection mode is off");
    cx.simulate_mouse_up(eighth, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(ids(&window, &cx).is_empty());
    assert!(!selection(&window, &cx, |selection| selection.enabled()));
}

/// "ignores controls, scrollbar, secondary pointers and normal-mode dragging": a
/// secondary-button drag draws no rectangle and selects nothing; a drag that starts on
/// the scrollbar draws none and keeps the selection.
#[gpui::test]
fn a_secondary_button_or_a_press_on_the_scrollbar_starts_no_rectangle(cx: &mut TestAppContext) {
    let fixture = Fixture::library(10_000);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let gallery = gallery_view(&window, &cx);
    let (first, eighth) = (tile(0, &mut cx).center(), tile(7, &mut cx).center());

    cx.simulate_mouse_down(first, MouseButton::Right, Modifiers::none());
    cx.executor().advance_clock(DRAG_HOLD_DELAY);
    cx.simulate_mouse_move(eighth, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
    assert!(rectangle(&mut cx).is_none());
    assert!(!gallery.read_with(&cx, |gallery, _| gallery.gesture().held()));
    cx.simulate_mouse_up(eighth, MouseButton::Right, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(
        ids(&window, &cx).is_empty(),
        "a secondary click selects nothing"
    );

    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1]);
    let thumb = cx
        .debug_bounds(gallery::SCROLLBAR_THUMB_SELECTOR)
        .expect("the scrollbar thumb");
    let below = thumb.center() + point(px(0.0), px(60.0));
    drag(thumb.center(), below, Modifiers::none(), &mut cx);
    assert!(rectangle(&mut cx).is_none());
    assert!(!gallery.read_with(&cx, |gallery, _| gallery.gesture().held()));
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(
        ids(&window, &cx),
        [1],
        "the scrollbar does not touch the selection"
    );
}

/// Docs ("Bulk selection"): clicking gallery gaps preserves the selection and the
/// anchor.
#[gpui::test]
fn a_click_in_a_gap_keeps_the_selection_and_the_anchor(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let (first, second, third) = (tile(0, &mut cx), tile(1, &mut cx), tile(2, &mut cx));
    cx.simulate_click(first.center(), Modifiers::none());
    settle(&fixture, &mut cx);

    let gap = point((first.right() + second.left()) / 2.0, first.center().y);
    cx.simulate_click(gap, Modifiers::none());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1]);
    assert_eq!(
        selection(&window, &cx, |selection| selection.anchor()),
        Some(0)
    );

    cx.simulate_click(third.center(), shift());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [1, 2, 3]);
}

/// "cancels a failed selection retry on explicit click": a new click on a tile drops
/// the error and its Retry.
#[gpui::test]
fn a_new_click_drops_a_failed_range_and_its_retry(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let (first, third, sixth) = (
        tile(0, &mut cx).center(),
        tile(2, &mut cx).center(),
        tile(5, &mut cx).center(),
    );
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);
    fixture.fail_ranges.store(true, Ordering::SeqCst);
    cx.simulate_click(third, shift());
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_some());

    fixture.fail_ranges.store(false, Ordering::SeqCst);
    cx.simulate_click(sixth, Modifiers::none());
    settle(&fixture, &mut cx);
    assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_none());
    assert!(cx.debug_bounds(gallery::LOAD_RETRY_ID).is_none());
    assert_eq!(
        selection(&window, &cx, |selection| selection.error().cloned()),
        None
    );
    assert_eq!(ids(&window, &cx), [1, 6]);
}

/// "cancels a failed selection retry on explicit unmount": the gallery goes away for
/// the settings page; back in the gallery the failed range and its Retry are gone
/// (`createGridHandlers`' cleanup sends `rectangle-cancel`, which drops them).
#[gpui::test]
fn leaving_the_gallery_drops_a_failed_range_and_its_retry(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let (first, third) = (tile(0, &mut cx).center(), tile(2, &mut cx).center());
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);
    fixture.fail_ranges.store(true, Ordering::SeqCst);
    cx.simulate_click(third, shift());
    settle(&fixture, &mut cx);
    assert!(selection(&window, &cx, |selection| selection
        .error()
        .is_some()));
    fixture.fail_ranges.store(false, Ordering::SeqCst);

    let runtime = runtime(&window, &cx);
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::OpenSettings, cx)
    });
    settle(&fixture, &mut cx);
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::CloseSettings, cx)
    });
    settle(&fixture, &mut cx);

    assert_eq!(
        selection(&window, &cx, |selection| selection.error().cloned()),
        None,
        "the failed range went with the gallery that showed it"
    );
    assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_none());
    assert_eq!(ids(&window, &cx), [1]);
}

/// Docs ("Bulk selection"): unmount invalidates older results. A Shift range whose
/// read is still on its way when the gallery goes away for the settings page is not
/// applied afterwards.
#[gpui::test]
fn leaving_the_gallery_drops_a_range_read_on_its_way(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let (first, third) = (tile(0, &mut cx).center(), tile(2, &mut cx).center());
    cx.simulate_click(first, Modifiers::none());
    settle(&fixture, &mut cx);
    cx.simulate_click(third, shift());
    cx.run_until_parked();
    assert!(selection(&window, &cx, |selection| selection.busy()));

    let runtime = runtime(&window, &cx);
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::OpenSettings, cx)
    });
    cx.run_until_parked();
    settle(&fixture, &mut cx);
    assert!(!selection(&window, &cx, |selection| selection.busy()));
    assert_eq!(ids(&window, &cx), [1]);
}

/// "does not open after the pointer moved or with a modifier held", with Ctrl: the
/// held Ctrl-press never opens, and its release toggles the tile.
#[gpui::test]
fn a_long_ctrl_press_never_opens_and_its_release_toggles_the_tile(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    toggle_mode(&fixture, &mut cx);
    let opened = events(&window, &mut cx);
    let third = tile(2, &mut cx).center();

    cx.simulate_mouse_down(third, MouseButton::Left, ctrl());
    cx.executor().advance_clock(LONG_PRESS * 2);
    cx.run_until_parked();
    assert!(opened.lock().unwrap().is_empty());
    cx.simulate_mouse_up(third, MouseButton::Left, ctrl());
    settle(&fixture, &mut cx);
    assert_eq!(ids(&window, &cx), [3]);
    assert!(opened.lock().unwrap().is_empty());
}
