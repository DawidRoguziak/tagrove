//! The gallery grid (epic E09) on GPUI's test platform (no display, no GPU) with the
//! scripted backend and the deterministic executor: where tiles are drawn, what the
//! query model and the thumbnail pipeline are asked for while scrolling, the bound of
//! the picture cache, and what a tile's activation reports.
//!
//! `frame_time_while_scrolling_a_large_library` is a measurement, not a check: it is
//! ignored by default and prints the time of a frame while scrolling through 100 000
//! results (`docs/development/testing.md`, "GPUI workspace").

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use gpui::{
    point, px, AppContext as _, Bounds, Entity, KeyUpEvent, Keystroke, Modifiers, Pixels,
    ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext, WindowHandle,
};
use media_tagger_client::{
    backend::{fake::Call, BackendError, FakeBackend, ThumbnailImage, ThumbnailReply},
    exec::{Executor, ManualExecutor},
    gallery::{layout::footer, TileRequest},
    i18n::Language,
    library::Viewport,
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput, DEFAULT_TILE_SIZE, TILE_SIZE_STEP},
    theme::ThemeName,
    thumbs::ThumbnailKey,
};
use media_tagger_core::models::{
    AssetQueryPageResult, AssetQuerySectionsResult, AssetSummary, ScanRoot, StartAssetQueryResult,
};
use media_tagger_gpui::{
    profile,
    runtime::{Runtime, RuntimeParts},
    thumbs::{Thumbnails, TilePicture},
    ui::layers,
    views::{
        gallery::{self, GalleryEvent, GalleryView},
        shell::ShellView,
    },
    window::{init_application, open_main_window},
};

const PAGE: usize = 128;
/// An 8 x 8 JPEG: 256 bytes once decoded.
const JPEG: [u8; 285] = [
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x0d, 0x09, 0x0a, 0x0b, 0x0a, 0x08, 0x0d,
    0x0b, 0x0a, 0x0b, 0x0e, 0x0e, 0x0d, 0x0f, 0x13, 0x20, 0x15, 0x13, 0x12, 0x12, 0x13, 0x27, 0x1c,
    0x1e, 0x17, 0x20, 0x2e, 0x29, 0x31, 0x30, 0x2e, 0x29, 0x2d, 0x2c, 0x33, 0x3a, 0x4a, 0x3e, 0x33,
    0x36, 0x46, 0x37, 0x2c, 0x2d, 0x40, 0x57, 0x41, 0x46, 0x4c, 0x4e, 0x52, 0x53, 0x52, 0x32, 0x3e,
    0x5a, 0x61, 0x5a, 0x50, 0x60, 0x4a, 0x51, 0x52, 0x4f, 0xff, 0xdb, 0x00, 0x43, 0x01, 0x0e, 0x0e,
    0x0e, 0x13, 0x11, 0x13, 0x26, 0x15, 0x15, 0x26, 0x4f, 0x35, 0x2d, 0x35, 0x4f, 0x4f, 0x4f, 0x4f,
    0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f,
    0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f,
    0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0x4f, 0xff, 0xc0,
    0x00, 0x11, 0x08, 0x00, 0x08, 0x00, 0x08, 0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11,
    0x01, 0xff, 0xc4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0xff, 0xc4, 0x00, 0x14, 0x10, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xc4, 0x00,
    0x15, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x05, 0x06, 0xff, 0xc4, 0x00, 0x14, 0x11, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xda, 0x00, 0x0c, 0x03, 0x01,
    0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0x32, 0x80, 0x43, 0xff, 0xd9,
];
const PICTURE_BYTES: usize = 8 * 8 * 4;

fn asset(index: usize) -> AssetSummary {
    let id = index as i64 + 1;
    AssetSummary {
        id,
        file_name: format!("asset-{id}.png"),
        preview_path: None,
        kind: match index % 10 {
            3 => "video",
            7 => "gif",
            _ => "image",
        }
        .to_owned(),
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
    page_of(offset, total, asset)
}

fn page_of(offset: usize, total: usize, make: fn(usize) -> AssetSummary) -> Vec<AssetSummary> {
    (offset..(offset + PAGE).min(total)).map(make).collect()
}

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
}

impl Fixture {
    /// A library of `total` synthetic assets in one scan root. Thumbnail requests
    /// are held until a test answers them.
    fn library(total: usize) -> Self {
        Self::library_of(total, asset)
    }

    /// [`Fixture::library`] with the assets `make` builds for each index.
    fn library_of(total: usize, make: fn(usize) -> AssetSummary) -> Self {
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(move |request| {
                let offset = request.anchor_offset.unwrap_or(0) / PAGE * PAGE;
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total,
                    offset,
                    items: page_of(offset, total, make),
                })
            })
            .on_get_asset_query_page(move |_, offset, _| {
                Ok(AssetQueryPageResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total,
                    offset,
                    items: page_of(offset, total, make),
                })
            })
            .on_get_asset_query_sections(move |_| {
                Ok(AssetQuerySectionsResult::Ready {
                    session_id: 1,
                    total,
                    sections: Vec::new(),
                })
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
        }
    }

    /// Answers every thumbnail request at once with the JPEG.
    fn with_thumbnails(self) -> Self {
        self.backend.on_request_thumbnail(|request, responder| {
            responder.respond(ThumbnailReply::Ready(ThumbnailImage {
                bytes: JPEG.to_vec(),
                version: request.version,
            }));
        });
        self
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

    /// The page offsets asked of the backend since the last call.
    fn take_pages(&self) -> Vec<usize> {
        self.backend
            .take_calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::GetAssetQueryPage { offset, .. } => Some(offset),
                _ => None,
            })
            .collect()
    }

    /// The thumbnail requests since the last call: asset, cached only, prefetch.
    fn take_thumbnail_requests(&self) -> Vec<(i64, bool, bool)> {
        self.backend
            .take_calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::RequestThumbnail { request } => {
                    Some((request.asset_id, request.cached_only, request.prefetch))
                }
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

fn tile(index: usize, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    bounds(gallery::tile_selector(index), cx)
}

fn rect(bounds: Bounds<Pixels>) -> (f32, f32, f32, f32) {
    (
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    )
}

fn viewport_bounds(cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.debug_bounds(gallery::VIEWPORT_SELECTOR)
        .expect("the gallery viewport is drawn")
}

/// A wheel step over the middle of the gallery. `delta_y` is a DOM `deltaY`:
/// positive scrolls towards the end.
fn wheel(delta_y: f32, modifiers: Modifiers, cx: &mut VisualTestContext) {
    // Below the header, wherever it ends. No debug selector: a release build, which
    // the measurement runs in, records none.
    let window = cx.update(|window, _| window.viewport_size());
    let position = point(window.width / 2.0, window.height * 0.6);
    cx.simulate_event(ScrollEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.0), px(-delta_y))),
        modifiers,
        ..Default::default()
    });
}

fn published(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> Option<Viewport> {
    runtime(window, cx).read_with(cx, |runtime, _| runtime.model().query().viewport())
}

#[gpui::test]
fn tiles_fill_the_width_in_square_rows_below_the_header(cx: &mut TestAppContext) {
    let fixture = Fixture::library(1_000);
    let (window, mut cx) = open(&fixture, cx);

    let viewport = rect(viewport_bounds(&mut cx));
    assert_eq!((viewport.0, viewport.2), (0.0, 1440.0));
    // 1386 px of tiles: six columns of 221 px with 12 px gaps, 20 px from the left
    // and from the header.
    let first = rect(tile(0, &mut cx).expect("the first tile"));
    assert_eq!(first, (20.0, viewport.1 + 20.0, 221.0, 221.0));
    let sixth = rect(tile(5, &mut cx).expect("the sixth tile"));
    assert_eq!((sixth.0, sixth.1), (20.0 + 5.0 * 233.0, first.1));
    assert_eq!(
        sixth.0 + sixth.2,
        1440.0 - 34.0,
        "clear of the timeline rail"
    );
    let seventh = rect(tile(6, &mut cx).expect("the seventh tile"));
    assert_eq!((seventh.0, seventh.1), (20.0, first.1 + 233.0));

    // Rows in view and four rows of overscan are elements; nothing beyond them.
    let range = gallery_view(&window, &cx)
        .read_with(&cx, |gallery, _| gallery.grid().range())
        .expect("rows are rendered");
    assert_eq!(range.start_index, 0);
    assert_eq!(range.visible_start_index, 0);
    assert!(tile(range.end_index, &mut cx).is_some());
    assert!(tile(range.end_index + 1, &mut cx).is_none());
    assert!(range.end_index < 60, "{range:?}");

    // The query model knows the same range and needs no page beyond the first.
    assert_eq!(
        published(&window, &cx),
        Some(Viewport {
            start_index: range.start_index,
            end_index: range.end_index,
            visible_start_index: range.visible_start_index,
            visible_end_index: range.visible_end_index,
            paused: false,
        })
    );
    assert!(fixture.take_pages().is_empty());
}

#[gpui::test]
fn a_short_result_keeps_the_footer_at_the_bottom_of_the_viewport(cx: &mut TestAppContext) {
    let fixture = Fixture::library(7);
    let (window, mut cx) = open(&fixture, cx);

    let viewport = viewport_bounds(&mut cx);
    let end = cx
        .debug_bounds(gallery::FOOTER_SELECTOR)
        .expect("the status footer");
    assert_eq!(f32::from(end.size.height), footer::HEIGHT);
    assert_eq!(end.bottom(), viewport.bottom());
    assert_eq!(
        (end.left(), end.right()),
        (viewport.left(), viewport.right())
    );
    assert!(tile(6, &mut cx).is_some() && tile(7, &mut cx).is_none());
    assert!(cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).is_none());

    // Nothing to scroll: the wheel leaves every tile where it is.
    let before = tile(0, &mut cx);
    wheel(200.0, Modifiers::none(), &mut cx);
    assert_eq!(tile(0, &mut cx), before);
    assert_eq!(
        gallery_view(&window, &cx).read_with(&cx, |gallery, _| gallery.grid().scroll_top()),
        0.0
    );
}

#[gpui::test]
fn the_wheel_scrolls_the_rows_and_asks_for_the_pages_that_come_into_range(cx: &mut TestAppContext) {
    let fixture = Fixture::library(10_000);
    let (window, mut cx) = open(&fixture, cx);
    let top = rect(tile(0, &mut cx).expect("the first tile")).1;
    fixture.take_pages();

    // A slow step: rows move by exactly the wheel's distance and demand goes on.
    wheel(100.0, Modifiers::none(), &mut cx);
    assert_eq!(
        rect(tile(0, &mut cx).expect("still rendered")).1,
        top - 100.0
    );
    assert!(!published(&window, &cx).expect("a viewport").paused);

    // Step by step to the 30th row (index 174 is in the second page), slowly.
    for _ in 0..70 {
        cx.executor().advance_clock(Duration::from_millis(200));
        wheel(100.0, Modifiers::none(), &mut cx);
    }
    settle(&fixture, &mut cx);
    let viewport = published(&window, &cx).expect("a viewport");
    assert!(!viewport.paused);
    assert_eq!(viewport.visible_start_index, 30 * 6);
    assert_eq!(fixture.take_pages(), [PAGE], "the second page, once");
    assert!(tile(0, &mut cx).is_none(), "the first row left the range");
    assert!(tile(viewport.visible_start_index, &mut cx).is_some());
}

#[gpui::test]
fn a_fast_jump_pauses_page_and_thumbnail_demand_until_scrolling_rests(cx: &mut TestAppContext) {
    let fixture = Fixture::library(100_000);
    let (window, mut cx) = open(&fixture, cx);
    fixture.backend.take_calls();
    let focus =
        gallery_view(&window, &cx).read_with(&cx, |gallery, _| gallery.grid_focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));

    // End: one step across the whole result.
    cx.simulate_keystrokes("end");
    settle(&fixture, &mut cx);
    let paused = published(&window, &cx).expect("a viewport");
    assert!(paused.paused);
    assert_eq!(paused.end_index, 99_999);
    assert!(
        fixture.backend.take_calls().is_empty(),
        "no page and no thumbnail while paused"
    );
    // The rows are there as still slots, and the footer ends the result.
    assert!(bounds(gallery::placeholder_selector(99_999), &mut cx).is_some());
    let viewport = viewport_bounds(&mut cx);
    let end = cx
        .debug_bounds(gallery::FOOTER_SELECTOR)
        .expect("the status footer");
    assert_eq!(end.bottom(), viewport.bottom());

    // 150 ms without a step: the same range, now with demand.
    cx.executor().advance_clock(Duration::from_millis(150));
    settle(&fixture, &mut cx);
    assert_eq!(
        published(&window, &cx),
        Some(Viewport {
            paused: false,
            ..paused
        })
    );
    let calls = fixture.backend.take_calls();
    let pages: Vec<_> = calls
        .iter()
        .filter_map(|call| match call {
            Call::GetAssetQueryPage { offset, .. } => Some(*offset),
            _ => None,
        })
        .collect();
    // The range with its overscan spans the last two pages; each is asked for once.
    let expected: Vec<usize> = (paused.start_index / PAGE..=paused.end_index / PAGE)
        .map(|page| page * PAGE)
        .collect();
    assert_eq!(pages, expected);
    assert_eq!(expected.last(), Some(&(99_999 / PAGE * PAGE)));
    assert!(tile(99_999, &mut cx).is_some(), "the last tile is loaded");
    // The tiles in view ask for a render at once, the overscan at prefetch priority.
    let thumbnails: Vec<_> = calls
        .iter()
        .filter_map(|call| match call {
            Call::RequestThumbnail { request } => Some((request.cached_only, request.prefetch)),
            _ => None,
        })
        .collect();
    assert!(!thumbnails.is_empty());
    assert!(thumbnails.iter().all(|(cached_only, _)| !cached_only));
    assert!(thumbnails.iter().any(|(_, prefetch)| *prefetch));
    assert!(thumbnails.iter().any(|(_, prefetch)| !*prefetch));
}

#[gpui::test]
fn tiles_first_shown_during_a_pause_ask_for_cached_thumbnails_only(cx: &mut TestAppContext) {
    let fixture = Fixture::library(1_000);
    let (window, mut cx) = open(&fixture, cx);
    let first = fixture.take_thumbnail_requests();
    assert!(first.iter().all(|(_, cached_only, _)| !cached_only));
    let known: Vec<i64> = first.iter().map(|(asset, _, _)| *asset).collect();

    // Half a viewport in one step is fast scrolling; the rows it brings are in the
    // first page, so their assets are known.
    wheel(600.0, Modifiers::none(), &mut cx);
    settle(&fixture, &mut cx);
    assert!(published(&window, &cx).expect("a viewport").paused);
    let during = fixture.take_thumbnail_requests();
    assert!(!during.is_empty());
    for (asset, cached_only, _) in &during {
        assert!(
            !known.contains(asset),
            "asset {asset} was asked for already"
        );
        assert!(cached_only, "asset {asset} must not start a render");
    }
}

#[gpui::test]
fn a_tile_shows_its_picture_once_decoded_and_videos_and_gifs_a_chip(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);

    // Held requests: no picture yet, the loader is drawn (invisible at first).
    let held = fixture.backend.held_thumbnails();
    assert!(held.iter().any(|request| request.asset_id == 1));
    assert!(bounds(gallery::picture_selector(0), &mut cx).is_none());
    assert!(bounds(gallery::loader_selector(0), &mut cx).is_some());

    // Asset 1 arrives; decoding runs off the UI thread and the tile draws it.
    let index = held
        .iter()
        .position(|request| request.asset_id == 1)
        .expect("asset 1 was asked for");
    fixture.backend.answer_thumbnail(
        index,
        ThumbnailReply::Ready(ThumbnailImage {
            bytes: JPEG.to_vec(),
            version: held[index].version.clone(),
        }),
    );
    settle(&fixture, &mut cx);
    let picture = bounds(gallery::picture_selector(0), &mut cx).expect("the picture");
    let frame = tile(0, &mut cx).expect("the tile");
    assert!(frame.contains(&picture.center()));
    assert!(bounds(gallery::loader_selector(0), &mut cx).is_none());
    assert!(bounds(gallery::picture_selector(1), &mut cx).is_none());
    let thumbnails =
        runtime(&window, &cx).read_with(&cx, |runtime, _| runtime.thumbnails().clone());
    assert_eq!(
        thumbnails.read_with(&cx, |thumbnails, _| thumbnails.cached_bytes()),
        PICTURE_BYTES
    );

    // Index 3 is a video and index 7 a GIF: static thumbnails with a chip.
    for (index, chip) in [(3, true), (7, true), (0, false), (4, false)] {
        assert_eq!(
            bounds(gallery::chip_selector(index), &mut cx).is_some(),
            chip,
            "tile {index}"
        );
    }
    let chip = bounds(gallery::chip_selector(3), &mut cx).expect("the video chip");
    let frame = tile(3, &mut cx).expect("the video tile");
    assert_eq!(
        frame.right() - chip.right(),
        px(5.0),
        "4 px inside the border"
    );
    assert_eq!(frame.bottom() - chip.bottom(), px(5.0));
}

#[gpui::test]
fn bytes_that_are_no_picture_leave_the_tile_empty(cx: &mut TestAppContext) {
    let fixture = Fixture::library(6);
    fixture.backend.on_request_thumbnail(|request, responder| {
        responder.respond(ThumbnailReply::Ready(ThumbnailImage {
            bytes: b"not a jpeg".to_vec(),
            version: request.version,
        }));
    });
    let (window, mut cx) = open(&fixture, cx);

    assert!(tile(0, &mut cx).is_some());
    assert!(bounds(gallery::picture_selector(0), &mut cx).is_none());
    assert!(bounds(gallery::loader_selector(0), &mut cx).is_none());
    let thumbnails =
        runtime(&window, &cx).read_with(&cx, |runtime, _| runtime.thumbnails().clone());
    assert_eq!(
        thumbnails.read_with(&cx, |thumbnails, _| (
            thumbnails.cached_pictures(),
            thumbnails.pending()
        )),
        (0, 0)
    );
}

#[gpui::test]
fn activating_a_tile_asks_for_the_lightbox(cx: &mut TestAppContext) {
    let fixture = Fixture::library(40);
    let (window, mut cx) = open(&fixture, cx);
    let gallery = gallery_view(&window, &cx);
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&gallery, move |_, event: &GalleryEvent, _| {
            seen.lock().unwrap().push(*event);
        })
        .detach();
    });

    let position = tile(8, &mut cx).expect("the ninth tile").center();
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(
        *events.lock().unwrap(),
        [GalleryEvent::OpenAsset {
            asset_id: 9,
            index: 8
        }]
    );
}

#[gpui::test]
fn ctrl_and_the_wheel_change_the_tile_size_instead_of_scrolling(cx: &mut TestAppContext) {
    let fixture = Fixture::library(1_000);
    let (window, mut cx) = open(&fixture, cx);
    let tile_size = |cx: &VisualTestContext| {
        runtime(&window, cx).read_with(cx, |runtime, _| runtime.model().tile_size())
    };
    let scroll_top = |cx: &VisualTestContext| {
        gallery_view(&window, cx).read_with(cx, |gallery, _| gallery.grid().scroll_top())
    };

    // Wheel down: one step smaller.
    wheel(100.0, Modifiers::control(), &mut cx);
    assert_eq!(tile_size(&cx), DEFAULT_TILE_SIZE - TILE_SIZE_STEP);
    assert_eq!(scroll_top(&cx), 0.0);
    wheel(-100.0, Modifiers::control(), &mut cx);
    wheel(-100.0, Modifiers::control(), &mut cx);
    assert_eq!(tile_size(&cx), DEFAULT_TILE_SIZE + TILE_SIZE_STEP);

    // The grid follows: 196 px tiles still fit six times, 13 px larger each.
    let first = rect(tile(0, &mut cx).expect("the first tile"));
    assert_eq!((first.2, first.3), (221.0, 221.0));
    let send = RuntimeInput::SetTileSize(340.0);
    runtime(&window, &cx).update(&mut cx, |runtime, cx| runtime.send(send, cx));
    cx.run_until_parked();
    let first = rect(tile(0, &mut cx).expect("the first tile"));
    assert_eq!((first.2, first.3), (454.0, 454.0), "three columns");
    assert!(tile(3, &mut cx).is_some_and(|fourth| rect(fourth).0 == 20.0));
}

#[gpui::test]
fn a_tile_size_change_keeps_the_first_visible_asset_at_the_top(cx: &mut TestAppContext) {
    let fixture = Fixture::library(10_000);
    let (window, mut cx) = open(&fixture, cx);
    for _ in 0..10 {
        cx.executor().advance_clock(Duration::from_millis(200));
        wheel(253.0, Modifiers::none(), &mut cx);
    }
    settle(&fixture, &mut cx);
    let gallery = gallery_view(&window, &cx);
    let first_visible = gallery
        .read_with(&cx, |gallery, _| gallery.grid().range())
        .expect("rows")
        .visible_start_index;
    assert_eq!(first_visible, 60, "the 11th row of six");

    runtime(&window, &cx).update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::SetTileSize(140.0), cx)
    });
    settle(&fixture, &mut cx);

    // Nine columns: index 60 is in the row that starts with 54, now at the top.
    let (range, row_y) = gallery.read_with(&cx, |gallery, _| {
        (
            gallery.grid().range().expect("rows"),
            gallery.grid().row_y(6),
        )
    });
    assert_eq!(range.visible_start_index, 54);
    assert_eq!(row_y, 0.0);
    let viewport = viewport_bounds(&mut cx);
    assert_eq!(
        tile(54, &mut cx).expect("the row's first tile").top(),
        viewport.top()
    );
}

#[gpui::test]
fn keys_scroll_by_lines_pages_and_to_the_ends(cx: &mut TestAppContext) {
    let fixture = Fixture::library(10_000);
    let (window, mut cx) = open(&fixture, cx);
    let gallery = gallery_view(&window, &cx);
    let scroll_top =
        |cx: &VisualTestContext| gallery.read_with(cx, |gallery, _| gallery.grid().scroll_top());
    // Keys reach the gallery once the focus is inside it.
    let focus = gallery.read_with(&cx, |gallery, _| gallery.grid_focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));

    cx.simulate_keystrokes("down");
    assert_eq!(scroll_top(&cx), 40.0);
    cx.simulate_keystrokes("up");
    assert_eq!(scroll_top(&cx), 0.0);

    let page = f32::from(viewport_bounds(&mut cx).size.height) * 0.875;
    cx.simulate_keystrokes("pagedown");
    assert_eq!(scroll_top(&cx), f64::from(page));
    cx.simulate_keystrokes("pageup");
    assert_eq!(scroll_top(&cx), 0.0);

    cx.simulate_keystrokes("end");
    let (top, max) = gallery.read_with(&cx, |gallery, _| {
        let grid = gallery.grid();
        (grid.offset(), grid.max_offset())
    });
    assert_eq!(top, max);
    cx.simulate_keystrokes("home");
    assert_eq!(scroll_top(&cx), 0.0);
}

#[gpui::test]
fn a_failed_load_shows_a_banner_whose_retry_starts_the_query_again(cx: &mut TestAppContext) {
    let fixture = Fixture::library(1_000);
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = attempts.clone();
    fixture.backend.on_start_asset_query(move |_| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(BackendError::rejected("the database is busy"))
        } else {
            Ok(StartAssetQueryResult::Ready {
                session_id: 2,
                revision: 1,
                total: 1_000,
                offset: 0,
                items: page(0, 1_000),
            })
        }
    });
    let (window, mut cx) = open(&fixture, cx);

    // Nothing to list: the banner sits above the empty state.
    let banner = cx
        .debug_bounds(gallery::LOAD_ERROR_SELECTOR)
        .expect("the load error banner");
    let viewport = viewport_bounds(&mut cx);
    assert_eq!(
        (f32::from(banner.left()), banner.top() - viewport.top()),
        (20.0, px(20.0))
    );
    assert_eq!(f32::from(banner.right()), 1440.0 - 34.0);
    assert!(cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).is_some());
    assert!(tile(0, &mut cx).is_none());

    let retry = cx
        .debug_bounds(gallery::LOAD_RETRY_ID)
        .expect("the retry button")
        .center();
    cx.simulate_click(retry, Modifiers::none());
    settle(&fixture, &mut cx);

    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_none());
    assert!(tile(0, &mut cx).is_some());
    assert!(published(&window, &cx).is_some());
}

/// Retry is a button of the window's tab order (a native `<button>` in
/// `GalleryGrid.tsx`): Tab reaches it, Enter and Space press it, and the focus it held
/// stays in the gallery when the banner leaves.
#[gpui::test]
fn the_retry_button_is_reached_with_tab_and_pressed_with_enter_or_space(cx: &mut TestAppContext) {
    let fixture = Fixture::library(1_000);
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = attempts.clone();
    fixture.backend.on_start_asset_query(move |_| {
        if counted.fetch_add(1, Ordering::SeqCst) < 2 {
            Err(BackendError::rejected("the database is busy"))
        } else {
            Ok(StartAssetQueryResult::Ready {
                session_id: 2,
                revision: 1,
                total: 1_000,
                offset: 0,
                items: page(0, 1_000),
            })
        }
    });
    let (window, mut cx) = open(&fixture, cx);
    let gallery = gallery_view(&window, &cx);
    let grid_focus = gallery.read_with(&cx, |gallery, _| gallery.grid_focus().clone());
    assert!(cx.debug_bounds(gallery::LOAD_RETRY_ID).is_some());
    assert_eq!(attempts.load(Ordering::SeqCst), 1);

    // GPUI's keyboard click fires on the key's release.
    let press = |key: &'static str, cx: &mut VisualTestContext| {
        cx.simulate_keystrokes(key);
        cx.simulate_event(KeyUpEvent {
            keystroke: Keystroke::parse(key).expect("a key"),
        });
    };
    // Retry is the only tab stop inside the gallery's viewport while nothing is listed.
    let on_retry = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            window.focused(cx).is_some_and(|focused| {
                focused != grid_focus && grid_focus.contains(&focused, window)
            })
        })
    };

    // From the window root, as after the window opened: Tab walks the header's
    // controls and arrives at Retry before it wraps.
    cx.update(layers::focus_root);
    cx.run_until_parked();
    let mut tabs = 0;
    while !on_retry(&mut cx) {
        tabs += 1;
        assert!(tabs <= 32, "Tab never reaches Retry");
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();
    }

    // Enter starts the query again; it fails again and the banner is back.
    press("enter", &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert!(cx.debug_bounds(gallery::LOAD_RETRY_ID).is_some());
    assert!(
        cx.update(|window, _| grid_focus.is_focused(window)),
        "the focus of the button that left is on the grid"
    );

    // One Tab from the grid is Retry again, and Space presses it as Enter does.
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert!(on_retry(&mut cx));
    press("space", &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_none());
    assert!(tile(0, &mut cx).is_some());

    // The focus is still inside the gallery: its keys scroll.
    cx.simulate_keystrokes("down");
    assert_eq!(
        gallery.read_with(&cx, |gallery, _| gallery.grid().scroll_top()),
        40.0
    );
}

#[gpui::test]
fn leaving_the_gallery_gives_up_its_viewport_and_its_thumbnail_requests(cx: &mut TestAppContext) {
    let fixture = Fixture::library(1_000);
    let (window, mut cx) = open(&fixture, cx);
    let runtime = runtime(&window, &cx);
    let thumbnails = runtime.read_with(&cx, |runtime, _| runtime.thumbnails().clone());
    assert!(published(&window, &cx).is_some());
    assert!(thumbnails.read_with(&cx, |thumbnails, _| thumbnails.tiles_in_range()) > 0);

    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::OpenSettings, cx)
    });
    settle(&fixture, &mut cx);

    assert_eq!(published(&window, &cx), None);
    assert_eq!(
        thumbnails.read_with(&cx, |thumbnails, _| thumbnails.tiles_in_range()),
        0
    );

    // Back in the gallery, the grid starts at the top and asks again.
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::CloseSettings, cx)
    });
    settle(&fixture, &mut cx);
    assert_eq!(
        published(&window, &cx).map(|viewport| viewport.visible_start_index),
        Some(0)
    );
    assert!(tile(0, &mut cx).is_some());
}

/// The tiles of a window of `count` assets starting at `first`, all in view.
fn window_of_tiles(first: i64, count: i64) -> Vec<TileRequest> {
    (first..first + count)
        .map(|id| TileRequest {
            key: ThumbnailKey::new(id, format!("{id:016x}")),
            visible: true,
        })
        .collect()
}

#[gpui::test]
fn the_picture_cache_stays_within_its_budget_however_far_the_grid_scrolls(cx: &mut TestAppContext) {
    let fixture = Fixture::library(0).with_thumbnails();
    let executor = fixture.executor.clone();
    // Room for 50 pictures; 20 tiles are in range at a time.
    let budget = 50 * PICTURE_BYTES;
    let thumbnails = cx.new(|cx| {
        Thumbnails::with_budget(
            fixture.backend.clone(),
            executor.clone() as Arc<dyn Executor>,
            budget,
            cx,
        )
    });
    let mut most = 0;

    for step in 0..200 {
        let first = 1 + step * 10;
        thumbnails.update(cx, |thumbnails, _| {
            thumbnails.sync(window_of_tiles(first, 20), false)
        });
        loop {
            cx.run_until_parked();
            if executor.run_all() == 0 {
                break;
            }
        }
        cx.run_until_parked();
        let (bytes, pictures) = thumbnails.read_with(cx, |thumbnails, _| {
            (thumbnails.cached_bytes(), thumbnails.cached_pictures())
        });
        assert!(bytes <= budget, "{bytes} bytes after step {step}");
        assert_eq!(bytes, pictures * PICTURE_BYTES);
        most = most.max(pictures);
    }

    // 2 010 pictures went through; the cache holds its budget and no more.
    assert_eq!(most, 50);
    thumbnails.read_with(cx, |thumbnails, _| {
        assert_eq!(thumbnails.budget(), budget);
        assert_eq!(thumbnails.cached_pictures(), 50);
        assert_eq!(thumbnails.pending(), 0);
        // Every dropped picture waits to have its texture released, once.
        assert_eq!(thumbnails.retired_pictures(), 2_010 - 50);
        // The tiles in range have theirs; an early one is gone.
        for id in [1_991, 2_010] {
            let key = ThumbnailKey::new(id, format!("{id:016x}"));
            assert!(
                matches!(thumbnails.picture(&key), TilePicture::Ready(_)),
                "{id}"
            );
        }
        let early = ThumbnailKey::new(1, format!("{:016x}", 1));
        assert!(matches!(thumbnails.picture(&early), TilePicture::Empty));
    });
}

#[gpui::test]
fn scrolling_on_keeps_every_picture_while_the_default_budget_lasts(cx: &mut TestAppContext) {
    let fixture = Fixture::library(1_000).with_thumbnails();
    let (window, mut cx) = open(&fixture, cx);
    let thumbnails =
        runtime(&window, &cx).read_with(&cx, |runtime, _| runtime.thumbnails().clone());
    let in_range = thumbnails.read_with(&cx, |thumbnails, _| thumbnails.tiles_in_range());
    assert!(in_range > 0);
    assert_eq!(
        thumbnails.read_with(&cx, |thumbnails, _| thumbnails.cached_pictures()),
        in_range,
        "every tile in range has its picture"
    );
    assert!(bounds(gallery::picture_selector(0), &mut cx).is_some());

    // The default budget is far from reached: nothing is dropped by scrolling on.
    for _ in 0..20 {
        cx.executor().advance_clock(Duration::from_millis(200));
        wheel(233.0, Modifiers::none(), &mut cx);
    }
    settle(&fixture, &mut cx);
    thumbnails.read_with(&cx, |thumbnails, _| {
        assert!(thumbnails.cached_pictures() > in_range);
        assert_eq!(thumbnails.retired_pictures(), 0);
        assert!(thumbnails.cached_bytes() <= thumbnails.budget());
    });
}

/// Prints the time of a frame while scrolling through 100 000 results: the work of
/// one wheel step (grid, viewport, thumbnail demand) plus building, laying out and
/// painting the frame on the test platform. Run it with
/// `cargo test --manifest-path src-gpui/Cargo.toml --locked --release -p media_tagger_gpui
/// --test gallery_grid frame_time -- --ignored --nocapture`.
#[gpui::test]
#[ignore = "a measurement; run it on its own with --ignored --nocapture"]
fn frame_time_while_scrolling_a_large_library(cx: &mut TestAppContext) {
    const TOTAL: usize = 100_000;
    const FRAMES: usize = 600;
    let fixture = Fixture::library(TOTAL).with_thumbnails();
    let (window, mut cx) = open(&fixture, cx);
    for (name, tile_size, step) in [
        ("default tiles (188 px)", 188.0, 48.0_f32),
        ("smallest tiles (140 px)", 140.0, 48.0),
        ("smallest tiles, fast (900 px a frame)", 140.0, 900.0),
    ] {
        runtime(&window, &cx).update(&mut cx, |runtime, cx| {
            runtime.send(RuntimeInput::SetTileSize(tile_size), cx)
        });
        let focus =
            gallery_view(&window, &cx).read_with(&cx, |gallery, _| gallery.grid_focus().clone());
        cx.update(|window, cx| window.focus(&focus, cx));
        cx.simulate_keystrokes("home");
        cx.executor().advance_clock(Duration::from_millis(200));
        settle(&fixture, &mut cx);

        let mut frames = Vec::with_capacity(FRAMES);
        for _ in 0..FRAMES {
            cx.executor().advance_clock(Duration::from_millis(16));
            let started = Instant::now();
            wheel(step, Modifiers::none(), &mut cx);
            fixture.executor.run_all();
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            frames.push(started.elapsed());
        }
        frames.sort_unstable();
        let total: Duration = frames.iter().sum();
        let tiles = gallery_view(&window, &cx)
            .read_with(&cx, |gallery, _| gallery.grid().range())
            .map(|range| range.end_index - range.start_index + 1)
            .unwrap_or_default();
        println!(
            "gallery scroll, {TOTAL} results, {name}: {tiles} tiles rendered, \
             mean {:.2} ms, median {:.2} ms, p95 {:.2} ms, max {:.2} ms over {FRAMES} frames",
            total.as_secs_f64() * 1_000.0 / FRAMES as f64,
            frames[FRAMES / 2].as_secs_f64() * 1_000.0,
            frames[FRAMES * 95 / 100].as_secs_f64() * 1_000.0,
            frames[FRAMES - 1].as_secs_f64() * 1_000.0,
        );
    }
}

/// The TypeScript gallery specification (epic E09) ported to the view: the cases of
/// `GalleryGrid.test.tsx`, `ThumbnailImage.test.tsx` and `TileSizeSlider` that need the
/// drawn grid, the query model and the thumbnails together. Each test names the TS
/// case it ports; the pure rules are ported in the client crate
/// (`media_tagger_client/tests/gallery_ported.rs`).
mod ported {
    use std::{collections::BTreeSet, sync::atomic::AtomicBool};

    use gpui::Focusable as _;
    use media_tagger_client::{
        backend::ThumbnailRequest,
        gallery::{layout::error_banner, pool_rows, VirtualGrid, OVERSCAN_ROWS},
        library::{QueryInput, MAX_CACHED_PAGES},
        runtime::TILE_SIZE_MAX,
        thumbs::{RECOVERY_DELAY, RETRY_DELAYS},
    };

    use super::*;

    const REST: Duration = Duration::from_millis(150);

    fn grid_state<R>(
        window: &WindowHandle<ShellView>,
        cx: &VisualTestContext,
        read: impl FnOnce(&VirtualGrid) -> R,
    ) -> R {
        gallery_view(window, cx).read_with(cx, |gallery, _| read(gallery.grid()))
    }

    /// Scrolls with one wheel step until the row of `index` is at the top.
    fn scroll_to_index(window: &WindowHandle<ShellView>, index: usize, cx: &mut VisualTestContext) {
        let delta = grid_state(window, cx, |grid| {
            grid.offset_of_index(index) - grid.offset()
        });
        wheel(delta as f32, Modifiers::none(), cx);
    }

    /// Lets the fast-scroll pause end and the backend answer.
    fn rest(fixture: &Fixture, cx: &mut VisualTestContext) {
        cx.executor().advance_clock(REST);
        settle(fixture, cx);
    }

    /// The page offsets and the thumbnail requests (asset, cached only, prefetch) of
    /// `calls`.
    fn split(calls: Vec<Call>) -> (Vec<usize>, Vec<(i64, bool, bool)>) {
        let mut pages = Vec::new();
        let mut thumbnails = Vec::new();
        for call in calls {
            match call {
                Call::GetAssetQueryPage { offset, .. } => pages.push(offset),
                Call::RequestThumbnail { request } => {
                    thumbnails.push((request.asset_id, request.cached_only, request.prefetch))
                }
                _ => {}
            }
        }
        (pages, thumbnails)
    }

    /// The (cached only, prefetch) flags of the requests for `asset_id`.
    fn requests_for(requests: &[(i64, bool, bool)], asset_id: i64) -> Vec<(bool, bool)> {
        requests
            .iter()
            .filter(|(asset, _, _)| *asset == asset_id)
            .map(|(_, cached_only, prefetch)| (*cached_only, *prefetch))
            .collect()
    }

    fn ready(request: &ThumbnailRequest) -> ThumbnailReply {
        ThumbnailReply::Ready(ThumbnailImage {
            bytes: JPEG.to_vec(),
            version: request.version.clone(),
        })
    }

    /// Answers the oldest held request for `asset_id` that matches `cached_only`.
    fn answer(
        fixture: &Fixture,
        asset_id: i64,
        cached_only: bool,
        reply: impl FnOnce(&ThumbnailRequest) -> ThumbnailReply,
    ) {
        let held = fixture.backend.held_thumbnails();
        let index = held
            .iter()
            .position(|request| request.asset_id == asset_id && request.cached_only == cached_only)
            .unwrap_or_else(|| panic!("no held request for asset {asset_id}"));
        fixture.backend.answer_thumbnail(index, reply(&held[index]));
    }

    fn send(window: &WindowHandle<ShellView>, input: RuntimeInput, cx: &mut VisualTestContext) {
        runtime(window, cx).update(cx, |runtime, cx| runtime.send(input, cx));
    }

    /// "requests overscan thumbnails at prefetch priority and visible ones without it"
    #[gpui::test]
    fn visible_tiles_ask_at_once_and_every_overscan_tile_at_prefetch_priority(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::library(1_000);
        let (window, _cx) = open(&fixture, cx);
        let viewport = published(&window, &_cx).expect("a viewport");
        assert!(
            viewport.end_index > viewport.visible_end_index,
            "overscan rows are rendered"
        );

        let mut requests = fixture.take_thumbnail_requests();
        requests.sort_unstable();
        let expected: Vec<_> = (viewport.start_index..=viewport.end_index)
            .map(|index| {
                let visible =
                    (viewport.visible_start_index..=viewport.visible_end_index).contains(&index);
                (index as i64 + 1, false, !visible)
            })
            .collect();
        assert_eq!(requests, expected);
    }

    /// "drops p=1 once when an overscan tile scrolls into view before its thumbnail
    /// loaded"
    #[gpui::test]
    fn a_pending_overscan_tile_is_promoted_once_when_it_scrolls_into_view(cx: &mut TestAppContext) {
        let fixture = Fixture::library(1_000);
        let (window, mut cx) = open(&fixture, cx);
        let viewport = published(&window, &cx).expect("a viewport");
        let pending = viewport.visible_end_index + 1;
        let loaded = pending + 1;
        let (stride, height) = grid_state(&window, &cx, |grid| {
            (grid.layout().stride, grid.geometry().viewport)
        });
        assert!(stride < height / 2.0, "one row is a slow step");

        answer(&fixture, loaded as i64 + 1, false, ready);
        settle(&fixture, &mut cx);
        assert!(bounds(gallery::picture_selector(loaded), &mut cx).is_some());
        fixture.backend.take_calls();

        // One row down: the first overscan row comes into view.
        cx.executor().advance_clock(Duration::from_millis(200));
        wheel(stride as f32, Modifiers::none(), &mut cx);
        settle(&fixture, &mut cx);
        let moved = published(&window, &cx).expect("a viewport");
        assert!(!moved.paused);
        assert!(moved.visible_start_index <= pending && moved.visible_end_index >= loaded);
        let requests = fixture.take_thumbnail_requests();
        // The visible request moves the queued prefetch render ahead of every prefetch.
        assert_eq!(
            requests_for(&requests, pending as i64 + 1),
            [(false, false)]
        );
        // A loaded picture is never asked for again.
        assert_eq!(requests_for(&requests, loaded as i64 + 1), []);

        // One way only: back in the overscan, nothing more is asked.
        cx.executor().advance_clock(Duration::from_millis(200));
        wheel(-(stride as f32), Modifiers::none(), &mut cx);
        settle(&fixture, &mut cx);
        assert!(
            published(&window, &cx)
                .expect("a viewport")
                .visible_end_index
                < pending
        );
        let requests = fixture.take_thumbnail_requests();
        assert_eq!(requests_for(&requests, pending as i64 + 1), []);
        assert_eq!(requests_for(&requests, loaded as i64 + 1), []);
    }

    /// "keeps p=1 on a pending tile that reaches the view during a fast-scroll pause
    /// until it ends"
    #[gpui::test]
    fn a_pending_prefetch_that_reaches_the_view_during_a_pause_waits_for_its_end(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::library(1_000);
        let (window, mut cx) = open(&fixture, cx);
        let pending = published(&window, &cx)
            .expect("a viewport")
            .visible_end_index
            + 1;
        let (stride, height) = grid_state(&window, &cx, |grid| {
            (grid.layout().stride, grid.geometry().viewport)
        });
        assert!(height / 2.0 > stride);
        fixture.backend.take_calls();

        // Half a viewport in one step starts the pause; the overscan row is in view.
        wheel((height / 2.0) as f32, Modifiers::none(), &mut cx);
        settle(&fixture, &mut cx);
        let moved = published(&window, &cx).expect("a viewport");
        assert!(moved.paused);
        assert!(moved.visible_start_index <= pending && pending <= moved.visible_end_index);
        let during = fixture.take_thumbnail_requests();
        assert_eq!(requests_for(&during, pending as i64 + 1), []);

        rest(&fixture, &mut cx);
        assert!(!published(&window, &cx).expect("a viewport").paused);
        let after = fixture.take_thumbnail_requests();
        assert_eq!(requests_for(&after, pending as i64 + 1), [(false, false)]);
    }

    /// "holds failed pages until explicit retry and replaces error placeholders on
    /// success"
    #[gpui::test]
    fn a_failed_page_waits_for_retry_and_retry_replaces_its_placeholders(cx: &mut TestAppContext) {
        const TOTAL: usize = 10_000;
        let fixture = Fixture::library(TOTAL);
        let (window, mut cx) = open(&fixture, cx);
        let failing = Arc::new(AtomicBool::new(true));
        let fail = failing.clone();
        fixture
            .backend
            .on_get_asset_query_page(move |_, offset, _| {
                if fail.load(Ordering::SeqCst) {
                    Err(BackendError::rejected("the database is busy"))
                } else {
                    Ok(AssetQueryPageResult::Ready {
                        session_id: 1,
                        revision: 1,
                        total: TOTAL,
                        offset,
                        items: page(offset, TOTAL),
                    })
                }
            });
        fixture.backend.take_calls();

        scroll_to_index(&window, 1_000, &mut cx);
        rest(&fixture, &mut cx);
        assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_some());
        assert!(tile(1_000, &mut cx).is_none());
        assert!(bounds(gallery::placeholder_selector(1_000), &mut cx).is_some());
        assert!(!fixture.take_pages().is_empty());

        // Nothing asks for the failed pages again on its own.
        cx.executor().advance_clock(Duration::from_secs(5));
        fixture.executor.advance(Duration::from_secs(5));
        settle(&fixture, &mut cx);
        assert_eq!(fixture.take_pages(), Vec::<usize>::new());

        failing.store(false, Ordering::SeqCst);
        // The banner leads the scrolled content, as in Tauri's scroller: a thousand
        // results down it is above the viewport, so Retry is sent the way its button
        // sends it (the click itself: `a_failed_load_shows_a_banner_whose_retry_starts_the_query_again`).
        let banner = cx
            .debug_bounds(gallery::LOAD_ERROR_SELECTOR)
            .expect("the banner");
        assert!(banner.bottom() <= viewport_bounds(&mut cx).top());
        send(&window, RuntimeInput::Query(QueryInput::RetryLoad), &mut cx);
        settle(&fixture, &mut cx);
        assert!(tile(1_000, &mut cx).is_some());
        assert!(bounds(gallery::placeholder_selector(1_000), &mut cx).is_none());
        assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_none());
    }

    /// "bounds initial DOM, replaces sparse slots, loads late thumbnail demand and
    /// reloads beyond 12 cached pages"
    #[gpui::test]
    fn a_slot_waits_for_its_page_and_an_evicted_page_is_asked_for_again(cx: &mut TestAppContext) {
        let fixture = Fixture::library(26_000);
        let (window, mut cx) = open(&fixture, cx);
        let rendered = |window: &WindowHandle<ShellView>, cx: &VisualTestContext| {
            let range = grid_state(window, cx, |grid| grid.range()).expect("rows");
            range.end_index - range.start_index + 1
        };
        // TS bounds the DOM at 70 tiles for jsdom's viewport; here the bound is the row
        // pool for this window (rows in view plus overscan on both sides), whatever the
        // total.
        let bound = grid_state(&window, &cx, |grid| {
            let layout = *grid.layout();
            pool_rows(&layout, grid.geometry().viewport, OVERSCAN_ROWS) * layout.columns
        });
        assert!(bound < 100, "{bound}");
        assert!(rendered(&window, &cx) <= bound);
        fixture.backend.take_calls();

        // The page of index 1000 is on its way: the slot is a placeholder.
        scroll_to_index(&window, 1_000, &mut cx);
        cx.executor().advance_clock(REST);
        cx.run_until_parked();
        assert!(bounds(gallery::placeholder_selector(1_000), &mut cx).is_some());
        assert!(tile(1_000, &mut cx).is_none());
        settle(&fixture, &mut cx);
        assert!(tile(1_000, &mut cx).is_some());
        assert!(bounds(gallery::placeholder_selector(1_000), &mut cx).is_none());
        let (pages, thumbnails) = split(fixture.backend.take_calls());
        assert!(pages.contains(&896), "{pages:?}");
        // The late tile asks for its thumbnail once its asset is known.
        assert!(thumbnails.contains(&(1_001, false, false)));

        // More pages than the cache keeps: page 0 is evicted.
        for step in 0..MAX_CACHED_PAGES + 4 {
            let index = 1_536 + step * 256;
            scroll_to_index(&window, index, &mut cx);
            rest(&fixture, &mut cx);
            assert!(tile(index, &mut cx).is_some(), "{index}");
            let cached = runtime(&window, &cx)
                .read_with(&cx, |runtime, _| runtime.model().query().cached_pages());
            assert!(cached <= MAX_CACHED_PAGES, "{cached} pages");
            assert!(rendered(&window, &cx) <= bound, "{index}");
        }
        assert!(runtime(&window, &cx).read_with(&cx, |runtime, _| runtime
            .model()
            .query()
            .asset_at(0)
            .is_none()));

        fixture.backend.take_calls();
        scroll_to_index(&window, 0, &mut cx);
        rest(&fixture, &mut cx);
        assert!(tile(0, &mut cx).is_some());
        assert!(fixture.take_pages().contains(&0));
    }

    /// "keeps GIFs static, updates only their thumbnail DOM and selects the current
    /// slot identity": a tile reports the asset it shows now.
    #[gpui::test]
    fn activating_a_tile_after_scrolling_reports_the_asset_now_at_that_index(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::library(10_000);
        let (window, mut cx) = open(&fixture, cx);
        let gallery = gallery_view(&window, &cx);
        let events = Arc::new(Mutex::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&gallery, move |_, event: &GalleryEvent, _| {
                seen.lock().unwrap().push(*event);
            })
            .detach();
        });
        // The GIF at index 7 asks for its static thumbnail by asset, like any tile.
        assert!(fixture
            .backend
            .held_thumbnails()
            .iter()
            .any(|request| request.asset_id == 8 && !request.cached_only));

        let position = tile(0, &mut cx).expect("the first tile").center();
        cx.simulate_click(position, Modifiers::none());
        scroll_to_index(&window, 1_000, &mut cx);
        rest(&fixture, &mut cx);
        let position = tile(1_000, &mut cx)
            .expect("the tile of index 1000")
            .center();
        cx.simulate_click(position, Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            *events.lock().unwrap(),
            [
                GalleryEvent::OpenAsset {
                    asset_id: 1,
                    index: 0
                },
                GalleryEvent::OpenAsset {
                    asset_id: 1_001,
                    index: 1_000
                }
            ]
        );
    }

    fn grouped(index: usize) -> AssetSummary {
        let mut asset = asset(index);
        if index < 2 {
            asset.media_group_key = Some("group".into());
        }
        asset
    }

    /// "draws media-group backplates per row behind adjacent grouped tiles"
    #[gpui::test]
    fn one_backplate_lies_behind_the_adjacent_tiles_of_a_media_group(cx: &mut TestAppContext) {
        let fixture = Fixture::library_of(1_000, grouped);
        let (_window, mut cx) = open(&fixture, cx);

        let first = rect(tile(0, &mut cx).expect("the first tile"));
        let plate =
            rect(bounds(gallery::group_plate_selector(0), &mut cx).expect("the group's backplate"));
        assert_eq!(
            plate,
            (
                first.0 - 4.0,
                first.1 - 4.0,
                2.0 * 221.0 + 12.0 + 8.0,
                221.0 + 8.0
            )
        );
        for index in 1..60 {
            assert!(
                bounds(gallery::group_plate_selector(index), &mut cx).is_none(),
                "{index}"
            );
        }
    }

    /// "keeps a deep viewport's tiles mounted while a refresh reloads its pages"
    #[gpui::test]
    fn a_refresh_keeps_a_deep_viewports_tiles_until_its_pages_are_back(cx: &mut TestAppContext) {
        const TOTAL: usize = 26_000;
        let fixture = Fixture::library(TOTAL);
        let (window, mut cx) = open(&fixture, cx);
        scroll_to_index(&window, 5_000, &mut cx);
        rest(&fixture, &mut cx);
        assert!(tile(5_000, &mut cx).is_some());

        // The new query's first page does not cover the viewport.
        fixture.backend.on_start_asset_query(|_| {
            Ok(StartAssetQueryResult::Ready {
                session_id: 1,
                revision: 1,
                total: TOTAL,
                offset: 0,
                items: page(0, TOTAL),
            })
        });
        fixture.backend.take_calls();
        send(&window, RuntimeInput::Query(QueryInput::Refresh), &mut cx);
        cx.run_until_parked();
        assert!(tile(5_000, &mut cx).is_some());
        assert!(fixture.executor.run_next(), "the new query starts");
        cx.run_until_parked();
        // Old rows stay as the fallback while the viewport's pages load.
        assert!(tile(5_000, &mut cx).is_some());

        settle(&fixture, &mut cx);
        assert!(tile(5_000, &mut cx).is_some());
        assert!(fixture.take_pages().contains(&(5_000 / PAGE * PAGE)));
        let id = runtime(&window, &cx).read_with(&cx, |runtime, _| {
            runtime
                .model()
                .query()
                .asset_at(5_000)
                .map(|asset| asset.id)
        });
        assert_eq!(id, Some(5_001));
    }

    /// "enters the empty-result state without observing the detached grid or observing
    /// during a delivery": the empty state replaces the grid and the grid comes back.
    #[gpui::test]
    fn an_empty_result_shows_the_empty_state_and_the_next_result_the_grid(cx: &mut TestAppContext) {
        let fixture = Fixture::library(1_000);
        let (window, mut cx) = open(&fixture, cx);
        let total = Arc::new(AtomicUsize::new(0));
        let current = total.clone();
        fixture.backend.on_start_asset_query(move |_| {
            let total = current.load(Ordering::SeqCst);
            Ok(StartAssetQueryResult::Ready {
                session_id: 1,
                revision: 1,
                total,
                offset: 0,
                items: page(0, total),
            })
        });

        send(&window, RuntimeInput::Query(QueryInput::Refresh), &mut cx);
        settle(&fixture, &mut cx);
        assert!(cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).is_some());
        assert!(tile(0, &mut cx).is_none());
        assert!(cx.debug_bounds(gallery::FOOTER_SELECTOR).is_none());
        assert_eq!(published(&window, &cx), None);

        total.store(1_000, Ordering::SeqCst);
        send(&window, RuntimeInput::Query(QueryInput::Refresh), &mut cx);
        settle(&fixture, &mut cx);
        assert!(cx.debug_bounds(gallery::EMPTY_STATE_SELECTOR).is_none());
        assert!(tile(0, &mut cx).is_some());
        assert_eq!(
            published(&window, &cx).map(|viewport| viewport.visible_start_index),
            Some(0)
        );
    }

    /// "lets a recycled tile's late image load or error after a large jump pass
    /// without a stale Show read"
    #[gpui::test]
    fn late_thumbnails_of_tiles_that_left_after_a_jump_change_nothing(cx: &mut TestAppContext) {
        let fixture = Fixture::library(10_000);
        let (window, mut cx) = open(&fixture, cx);
        let held = fixture.backend.held_thumbnails();
        assert!(!held.is_empty());

        // Every rendered row moves to indexes whose page is not loaded.
        scroll_to_index(&window, 5_000, &mut cx);
        settle(&fixture, &mut cx);
        assert!(bounds(gallery::placeholder_selector(5_000), &mut cx).is_some());
        assert!(tile(0, &mut cx).is_none());

        // The backend still answers the requests of the tiles that left.
        for _ in 0..held.len() {
            let Some(request) = fixture.backend.held_thumbnails().first().cloned() else {
                break;
            };
            fixture.backend.answer_thumbnail(0, ready(&request));
        }
        settle(&fixture, &mut cx);
        let thumbnails =
            runtime(&window, &cx).read_with(&cx, |runtime, _| runtime.thumbnails().clone());
        thumbnails.read_with(&cx, |thumbnails, _| {
            assert_eq!(thumbnails.cached_pictures(), 0);
            assert_eq!(thumbnails.cached_bytes(), 0);
            assert_eq!(thumbnails.pending(), 0);
        });

        fixture.backend.take_calls();
        rest(&fixture, &mut cx);
        assert!(tile(5_000, &mut cx).is_some());
        assert!(fixture
            .take_thumbnail_requests()
            .contains(&(5_001, false, false)));
    }

    /// "keeps loaded tiles mounted through a fast jump and suspends only page and
    /// thumbnail demand"
    #[gpui::test]
    fn a_fast_jump_keeps_loaded_tiles_and_a_cached_only_miss_renders_after_the_pause(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::library(1_000);
        let (window, mut cx) = open(&fixture, cx);
        fixture.backend.take_calls();

        wheel(600.0, Modifiers::none(), &mut cx);
        settle(&fixture, &mut cx);
        let viewport = published(&window, &cx).expect("a viewport");
        assert!(viewport.paused);
        // Inside the first page: every slot in range is a tile, none a placeholder.
        assert!(viewport.end_index < PAGE);
        for index in viewport.start_index..=viewport.end_index {
            assert!(tile(index, &mut cx).is_some(), "{index}");
            assert!(bounds(gallery::placeholder_selector(index), &mut cx).is_none());
        }
        let (pages, during) = split(fixture.backend.take_calls());
        assert_eq!(pages, Vec::<usize>::new());
        let (asset, _, _) = *during
            .iter()
            .find(|(_, cached_only, _)| *cached_only)
            .expect("a tile first shown during the pause");

        // The pause ends; the cached-only request still on its way stays.
        rest(&fixture, &mut cx);
        assert_eq!(requests_for(&fixture.take_thumbnail_requests(), asset), []);

        // It misses: the tile asks for a render now.
        answer(&fixture, asset, true, |_| ThumbnailReply::NotFound);
        settle(&fixture, &mut cx);
        let after = requests_for(&fixture.take_thumbnail_requests(), asset);
        assert_eq!(after.len(), 1, "{after:?}");
        assert!(!after[0].0, "a render, not a cached-only request");
    }

    /// "demands the final million-result page and passes the required total to the
    /// idle footer"
    #[gpui::test]
    fn end_at_a_million_results_asks_only_for_the_final_pages(cx: &mut TestAppContext) {
        let fixture = Fixture::library(1_000_000);
        let (window, mut cx) = open(&fixture, cx);
        let focus =
            gallery_view(&window, &cx).read_with(&cx, |gallery, _| gallery.grid_focus().clone());
        cx.update(|window, cx| window.focus(&focus, cx));
        fixture.backend.take_calls();

        cx.simulate_keystrokes("end");
        rest(&fixture, &mut cx);

        let pages = fixture.take_pages();
        assert!(pages.contains(&999_936), "{pages:?}");
        assert!(pages.iter().all(|offset| *offset >= 999_808), "{pages:?}");
        assert!(tile(999_999, &mut cx).is_some());
        let range = grid_state(&window, &cx, |grid| grid.range()).expect("rows");
        assert!(range.end_index - range.start_index + 1 < 70);
        let viewport = viewport_bounds(&mut cx);
        let end = cx
            .debug_bounds(gallery::FOOTER_SELECTOR)
            .expect("the status footer");
        assert_eq!(end.bottom(), viewport.bottom());
    }

    /// "renders static slots without thumbnail or passing-page requests and resumes the
    /// unchanged final range"
    #[gpui::test]
    fn slots_passed_during_a_pause_ask_for_nothing_and_the_final_range_loads_after_it(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::library(26_000);
        let (window, mut cx) = open(&fixture, cx);
        fixture.backend.take_calls();

        for index in [1_000, 2_000, 3_000] {
            scroll_to_index(&window, index, &mut cx);
            settle(&fixture, &mut cx);
            assert!(published(&window, &cx).expect("a viewport").paused);
        }
        assert!(fixture.backend.take_calls().is_empty());
        assert!(bounds(gallery::placeholder_selector(3_000), &mut cx).is_some());

        rest(&fixture, &mut cx);
        let viewport = published(&window, &cx).expect("a viewport");
        assert!(!viewport.paused);
        assert_eq!(viewport.visible_start_index, 3_000);
        let (pages, thumbnails) = split(fixture.backend.take_calls());
        let expected: Vec<usize> = (viewport.start_index / PAGE..=viewport.end_index / PAGE)
            .map(|page| page * PAGE)
            .collect();
        assert_eq!(pages, expected);
        assert!(expected.contains(&2_944));
        // Loaded after the pause: a render, not a cached-only request.
        assert!(tile(3_000, &mut cx).is_some());
        assert!(thumbnails.contains(&(3_001, false, false)));
    }

    /// "keeps retrying with a capped backoff after the quick retries until the picture
    /// loads" and "retries a failed load with backoff on the same element, then reports
    /// failure" (ADR 0024)
    #[gpui::test]
    fn a_thumbnail_that_keeps_failing_shows_once_a_later_retry_succeeds(cx: &mut TestAppContext) {
        let fixture = Fixture::library(6);
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let seen = attempts.clone();
        fixture
            .backend
            .on_request_thumbnail(move |request, responder| {
                if request.asset_id != 1 {
                    responder.respond(ready(&request));
                    return;
                }
                seen.lock().unwrap().push(request.retry);
                if request.retry < 3 {
                    responder.respond(ThumbnailReply::Unavailable);
                } else {
                    responder.respond(ready(&request));
                }
            });
        let (_window, mut cx) = open(&fixture, cx);

        // The first load failed: the tile waits for the quick retry with its loader.
        assert!(bounds(gallery::picture_selector(1), &mut cx).is_some());
        assert!(bounds(gallery::picture_selector(0), &mut cx).is_none());
        assert!(bounds(gallery::loader_selector(0), &mut cx).is_some());
        fixture.executor.advance(RETRY_DELAYS[0]);
        settle(&fixture, &mut cx);
        assert!(bounds(gallery::loader_selector(0), &mut cx).is_some());

        // The quick retries are used up: the tile is empty, slower retries go on.
        fixture.executor.advance(RETRY_DELAYS[1]);
        settle(&fixture, &mut cx);
        assert!(tile(0, &mut cx).is_some());
        assert!(bounds(gallery::loader_selector(0), &mut cx).is_none());
        assert!(bounds(gallery::picture_selector(0), &mut cx).is_none());

        fixture.executor.advance(RECOVERY_DELAY);
        settle(&fixture, &mut cx);
        assert!(bounds(gallery::picture_selector(0), &mut cx).is_some());
        assert_eq!(*attempts.lock().unwrap(), [0, 1, 2, 3]);
    }

    /// "cancels a pending retry on unmount"
    #[gpui::test]
    fn leaving_the_gallery_while_a_thumbnail_waits_for_a_retry_stops_its_retries(
        cx: &mut TestAppContext,
    ) {
        let fixture = Fixture::library(6);
        fixture
            .backend
            .on_request_thumbnail(|_, responder| responder.respond(ThumbnailReply::Unavailable));
        let (window, mut cx) = open(&fixture, cx);
        for delay in RETRY_DELAYS {
            fixture.executor.advance(delay);
            settle(&fixture, &mut cx);
        }
        let retried: BTreeSet<i64> = fixture
            .take_thumbnail_requests()
            .into_iter()
            .map(|(asset, _, _)| asset)
            .collect();
        assert_eq!(retried, (1..=6).collect());

        send(&window, RuntimeInput::OpenSettings, &mut cx);
        settle(&fixture, &mut cx);
        for _ in 0..4 {
            fixture.executor.advance(Duration::from_secs(30));
            settle(&fixture, &mut cx);
        }
        assert_eq!(fixture.take_thumbnail_requests(), []);
    }

    /// `TileSizeSlider` (`AppGalleryView.tsx`): the header's slider sets the tile size
    /// the grid lays out with.
    #[gpui::test]
    fn the_tile_size_slider_sets_the_tile_size_of_the_grid(cx: &mut TestAppContext) {
        let fixture = Fixture::library(1_000);
        let (window, mut cx) = open(&fixture, cx);
        let slider = gallery_view(&window, &cx)
            .read_with(&cx, |gallery, _| gallery.tile_size_slider().clone());
        let handle = slider.read_with(&cx, |slider, cx| slider.focus_handle(cx));
        cx.update(|window, cx| window.focus(&handle, cx));
        let tile_size = |cx: &VisualTestContext| {
            runtime(&window, cx).read_with(cx, |runtime, _| runtime.model().tile_size())
        };

        cx.simulate_keystrokes("right");
        settle(&fixture, &mut cx);
        assert_eq!(tile_size(&cx), DEFAULT_TILE_SIZE + TILE_SIZE_STEP);
        assert_eq!(grid_state(&window, &cx, |grid| grid.scroll_top()), 0.0);

        cx.simulate_keystrokes("end");
        settle(&fixture, &mut cx);
        assert_eq!(tile_size(&cx), TILE_SIZE_MAX);
        let first = rect(tile(0, &mut cx).expect("the first tile"));
        assert_eq!((first.2, first.3), (454.0, 454.0), "three columns");
        assert_eq!(
            grid_state(&window, &cx, |grid| grid.scroll_top()),
            0.0,
            "End on the slider does not scroll the grid"
        );
    }

    /// "publishes a grid margin change from its ResizeObserver without observing in
    /// that delivery", with the shift of "retries the original failed %s selection
    /// after the error banner moves the grid": a failed page puts the banner above the
    /// grid, which moves the tiles down by its height and margin under an unchanged
    /// scroll position; a successful Retry takes it away and the tiles move back.
    #[gpui::test]
    fn the_error_banner_moves_the_grid_down_by_its_height_and_retry_moves_it_back(
        cx: &mut TestAppContext,
    ) {
        const TOTAL: usize = 10_000;
        let fixture = Fixture::library(TOTAL);
        let (window, mut cx) = open(&fixture, cx);
        let failing = Arc::new(AtomicBool::new(true));
        let fail = failing.clone();
        fixture
            .backend
            .on_get_asset_query_page(move |_, offset, _| {
                if fail.load(Ordering::SeqCst) {
                    Err(BackendError::rejected("the database is busy"))
                } else {
                    Ok(AssetQueryPageResult::Ready {
                        session_id: 1,
                        revision: 1,
                        total: TOTAL,
                        offset,
                        items: page(offset, TOTAL),
                    })
                }
            });
        fixture.backend.take_calls();

        // Slowly, half a row at a time, until the second page is in range. Its request
        // waits on the executor, so nothing has failed yet.
        let stride = grid_state(&window, &cx, |grid| grid.layout().stride);
        while grid_state(&window, &cx, |grid| grid.range())
            .expect("rows")
            .end_index
            < PAGE
        {
            cx.executor().advance_clock(Duration::from_millis(200));
            wheel((stride / 2.0) as f32, Modifiers::none(), &mut cx);
            cx.run_until_parked();
        }
        assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_none());
        let index = grid_state(&window, &cx, |grid| grid.range())
            .expect("rows")
            .visible_start_index;
        assert!(index < PAGE, "a tile of the cached first page");
        let before = rect(tile(index, &mut cx).expect("a visible tile"));

        settle(&fixture, &mut cx);
        assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_some());
        let moved = rect(tile(index, &mut cx).expect("still rendered"));
        assert_eq!(
            (moved.0, moved.1, moved.2),
            (before.0, before.1 + error_banner::LEAD, before.2)
        );

        failing.store(false, Ordering::SeqCst);
        send(&window, RuntimeInput::Query(QueryInput::RetryLoad), &mut cx);
        settle(&fixture, &mut cx);
        assert!(cx.debug_bounds(gallery::LOAD_ERROR_SELECTOR).is_none());
        assert_eq!(rect(tile(index, &mut cx).expect("still rendered")), before);
    }

    fn asset_of_a_new_version(index: usize) -> AssetSummary {
        let mut asset = asset(index);
        asset.thumb_version = format!("{:016x}", asset.id + (1 << 32));
        asset
    }

    /// "decodes asynchronously and hides the previous picture until the new source
    /// loads" and "resets readiness per source": a tile whose asset comes back with a
    /// new thumbnail version shows the loader, not the old picture, until the new
    /// picture is decoded.
    #[gpui::test]
    fn a_new_thumbnail_version_hides_the_old_picture_until_the_new_one_is_decoded(
        cx: &mut TestAppContext,
    ) {
        const TOTAL: usize = 20;
        let fixture = Fixture::library(TOTAL);
        let (window, mut cx) = open(&fixture, cx);
        answer(&fixture, 1, false, ready);
        settle(&fixture, &mut cx);
        assert!(bounds(gallery::picture_selector(0), &mut cx).is_some());

        // The thumbnail of every asset changed: the next result names new versions.
        fixture
            .backend
            .on_start_asset_query(|request| {
                let offset = request.anchor_offset.unwrap_or(0) / PAGE * PAGE;
                Ok(StartAssetQueryResult::Ready {
                    session_id: 2,
                    revision: 2,
                    total: TOTAL,
                    offset,
                    items: page_of(offset, TOTAL, asset_of_a_new_version),
                })
            })
            .on_get_asset_query_page(|_, offset, _| {
                Ok(AssetQueryPageResult::Ready {
                    session_id: 2,
                    revision: 2,
                    total: TOTAL,
                    offset,
                    items: page_of(offset, TOTAL, asset_of_a_new_version),
                })
            });
        send(&window, RuntimeInput::Query(QueryInput::Refresh), &mut cx);
        settle(&fixture, &mut cx);
        let version = asset_of_a_new_version(0).thumb_version;
        assert_eq!(
            runtime(&window, &cx).read_with(&cx, |runtime, _| runtime
                .model()
                .query()
                .asset_at(0)
                .map(|asset| asset.thumb_version.clone())),
            Some(version.clone())
        );
        assert!(bounds(gallery::picture_selector(0), &mut cx).is_none());
        assert!(bounds(gallery::loader_selector(0), &mut cx).is_some());
        assert!(fixture
            .backend
            .held_thumbnails()
            .iter()
            .any(|request| request.asset_id == 1 && request.version == version));

        let held = fixture.backend.held_thumbnails();
        let new = held
            .iter()
            .position(|request| request.asset_id == 1 && request.version == version)
            .expect("the request of the new version");
        fixture.backend.answer_thumbnail(new, ready(&held[new]));
        settle(&fixture, &mut cx);
        assert!(bounds(gallery::picture_selector(0), &mut cx).is_some());
        assert!(bounds(gallery::loader_selector(0), &mut cx).is_none());
    }
}
