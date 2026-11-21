//! The TypeScript gallery specification (epic E09) ported to the client crate: the
//! cases of `gridGeometry.test.ts`, `createVirtualGrid.test.tsx`,
//! `tileSizeService.test.ts`, `GalleryStatusFooter.test.tsx` and the thumbnail rules
//! of `ThumbnailImage.test.tsx` that describe product behaviour and are not already
//! pinned by the unit tests next to the modules. Each test names the TS case it
//! ports. The cases about WebKit's physical window (`MAX_GRID_HEIGHT`, the moving
//! base) have no counterpart: the GPUI grid keeps one logical `f64` offset (ADR 0042).

use std::time::Duration;

use media_tagger_client::{
    gallery::{
        anchor_for_resize, grid_layout, layout::result_count, pool_rows, row_range, row_top,
        to_library_range, GridEffect, GridGeometry, GridInput, GridLayout, LibraryRange,
        ScrollEdge, ThumbnailDemand, TileRequest, TileThumbnail, VirtualGrid, GRID_GAP,
        OVERSCAN_ROWS,
    },
    i18n::{
        format_count, interpolate::interpolate, plural_category, I18n, Language, Locales,
        PluralOperands,
    },
    model::{Model, RequestToken},
    runtime::{
        clamp_tile_size, next_tile_size_by_wheel, TILE_SIZE_MAX, TILE_SIZE_MIN, TILE_SIZE_STEP,
    },
    thumbs::{ThumbnailEvent, ThumbnailInput, ThumbnailKey, ThumbnailMode},
};

// ---------------------------------------------------------------------------
// gridGeometry.test.ts
// ---------------------------------------------------------------------------

fn layout(count: usize, width: f64, tile_min: f64) -> GridLayout {
    grid_layout(width, tile_min, GRID_GAP, count)
}

/// "stretches tiles to the width and keeps one column for narrow widths"
#[test]
fn tiles_stretch_to_the_width_and_a_narrow_width_keeps_one_column() {
    let wide = layout(10, 1_000.0, 188.0);
    assert_eq!((wide.columns, wide.size), (5, 190.0));
    let narrow = layout(10, 100.0, 188.0);
    assert_eq!((narrow.columns, narrow.size), (1, 188.0));
    let tall = layout(1_000_000, 588.0, 340.0);
    assert_eq!((tall.columns, tall.size, tall.rows), (1, 588.0, 1_000_000));
    assert_eq!(tall.content_height, 1_000_000.0 * 600.0 - 12.0);
}

/// "keeps a partial last row and no content when empty"
#[test]
fn a_partial_last_row_counts_and_an_empty_grid_has_no_content() {
    let partial = layout(12, 988.0, 188.0);
    assert_eq!((partial.rows, partial.content_height), (3, 588.0));
    let empty = layout(0, 988.0, 188.0);
    assert_eq!((empty.rows, empty.content_height), (0, 0.0));
}

/// "clips overscan at the start and keeps the margin above the grid at row zero"
#[test]
fn overscan_is_clipped_at_the_start_and_the_margin_maps_to_row_zero() {
    let grid = layout(10_000, 988.0, 188.0);
    let above = row_range(&grid, -80.0, 600.0, OVERSCAN_ROWS).expect("rows");
    assert_eq!(
        (
            above.first,
            above.last,
            above.visible_first,
            above.visible_last
        ),
        (0, 6, 0, 2)
    );
    // A viewport ending exactly on a row boundary does not touch the next row.
    let exact = row_range(&grid, 0.0, 1_000.0, OVERSCAN_ROWS).expect("rows");
    assert_eq!((exact.visible_first, exact.visible_last), (0, 4));
    let one_more = row_range(&grid, 0.0, 1_001.0, OVERSCAN_ROWS).expect("rows");
    assert_eq!((one_more.visible_first, one_more.visible_last), (0, 5));
}

/// "clamps the final partial row and an offset beyond the end at the millionth slot"
#[test]
fn the_final_partial_row_is_the_last_visible_index_too() {
    let grid = layout(999_998, 988.0, 188.0);
    let rows = row_range(&grid, grid.content_height - 600.0, 600.0, OVERSCAN_ROWS).expect("rows");
    let range = to_library_range(&grid, &rows);
    assert_eq!(
        (range.end_index, range.visible_end_index),
        (999_997, 999_997)
    );
}

/// "sizes the row pool for any viewport alignment"
#[test]
fn the_row_pool_holds_every_rendered_range_at_any_alignment() {
    let grid = layout(10_000, 988.0, 188.0);
    assert_eq!(pool_rows(&grid, 1_000.0, OVERSCAN_ROWS), 5 + 1 + 8);
    assert_eq!(
        pool_rows(&layout(12, 988.0, 188.0), 1_000.0, OVERSCAN_ROWS),
        3
    );
    let pool = pool_rows(&grid, 1_000.0, OVERSCAN_ROWS);
    for offset in (0..400).step_by(37) {
        let rows =
            row_range(&grid, 20_000.0 + offset as f64, 1_000.0, OVERSCAN_ROWS).expect("rows");
        assert!(rows.len() <= pool, "offset {offset}: {} rows", rows.len());
    }
}

/// "keeps the first visible global index when columns or tile size change"
#[test]
fn a_resize_keeps_the_first_visible_index_for_columns_and_tile_size() {
    let before = layout(10_000, 988.0, 188.0);
    let after = layout(10_000, 588.0, 188.0);
    assert_eq!(after.columns, 3);
    // Row 10 at five columns starts with index 50, which is row 16 at three columns.
    assert_eq!(
        anchor_for_resize(&before, &after, 2_000.0),
        row_top(&after, 16)
    );
    let larger = layout(10_000, 988.0, 340.0);
    assert_eq!(
        anchor_for_resize(&before, &larger, 2_000.0),
        row_top(&larger, 25)
    );
}

/// "keeps unchanged layouts and the margin above the grid"
#[test]
fn an_unchanged_layout_and_an_offset_in_the_margin_keep_the_offset() {
    let grid = layout(10_000, 988.0, 188.0);
    // A new count alone changes neither columns nor tile size.
    assert_eq!(
        anchor_for_resize(&grid, &layout(20_000, 988.0, 188.0), 2_345.0),
        2_345.0
    );
    // An offset inside the margin above the grid stays where it is.
    assert_eq!(
        anchor_for_resize(&grid, &layout(10_000, 588.0, 188.0), -40.0),
        -40.0
    );
}

// ---------------------------------------------------------------------------
// createVirtualGrid.test.tsx
// ---------------------------------------------------------------------------

/// The TS mount: a 988 x 600 viewport, 80 px above the grid, a 136 px footer.
const GEOMETRY: GridGeometry = GridGeometry {
    width: 988.0,
    viewport: 600.0,
    lead: 80.0,
    trail: 136.0,
};

fn laid_out(count: usize, geometry: GridGeometry, tile_size: f64) -> VirtualGrid {
    let mut grid = VirtualGrid::new();
    assert!(grid
        .handle(GridInput::Layout {
            geometry,
            count,
            tile_size,
        })
        .is_empty());
    grid
}

fn grid(count: usize) -> VirtualGrid {
    laid_out(count, GEOMETRY, 188.0)
}

fn relayout(grid: &mut VirtualGrid, geometry: GridGeometry, count: usize, tile_size: f64) {
    let effects = grid.handle(GridInput::Layout {
        geometry,
        count,
        tile_size,
    });
    assert!(effects.is_empty(), "a layout starts no timer: {effects:?}");
}

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

/// Moves to the DOM `scrollTop` `top` (80 px of margin above the grid) at `at`.
fn scroll_to(grid: &mut VirtualGrid, top: f64, at: Duration) -> Vec<GridEffect> {
    grid.handle(GridInput::ScrollTo {
        offset: top - GEOMETRY.lead,
        at,
    })
}

fn idle_token(effects: &[GridEffect]) -> RequestToken {
    match effects {
        [GridEffect::IdleTimer { token, .. }] => *token,
        other => panic!("expected one idle timer, got {other:?}"),
    }
}

fn range(grid: &VirtualGrid) -> LibraryRange {
    grid.range().expect("rows are rendered")
}

/// "bounds every initial and remount range with %i slots and null parent refs"
#[test]
fn every_initial_and_remount_range_is_bounded() {
    for count in [10_000, 26_000] {
        for pass in 0..2 {
            // Before the first measurement there is nothing to ask for.
            let mut grid = VirtualGrid::new();
            assert_eq!(grid.range(), None, "{count}, pass {pass}");
            assert_eq!(grid.viewport(), None);
            relayout(&mut grid, GEOMETRY, count, 188.0);
            // The first width measurement keeps the margin above the grid in view.
            assert_eq!(grid.scroll_top(), 0.0);
            assert_eq!(grid.layout().columns, 5);
            let first = range(&grid);
            assert!(first.end_index > 0);
            assert!(first.end_index - first.start_index + 1 < 70, "{first:?}");

            scroll_to(&mut grid, 20_080.0, ms(0));
            let deep = range(&grid);
            assert!(deep.start_index > 450, "{deep:?}");
            assert!(deep.end_index - deep.start_index + 1 < 70, "{deep:?}");
        }
    }
}

/// "publishes demand only when the row range changes"
#[test]
fn the_published_range_changes_only_when_the_rows_change() {
    let mut grid = grid(10_000);
    // Mid-row, so the bottom edge does not sit on a row boundary.
    let token = idle_token(&scroll_to(&mut grid, 20_180.0, ms(0)));
    grid.handle(GridInput::IdleElapsed(token));
    let settled = grid.viewport().expect("a viewport");
    assert!(!settled.paused);

    let mut changes = 0;
    let mut last = settled;
    for step in 1..20 {
        let effects = scroll_to(
            &mut grid,
            20_180.0 + step as f64 * 5.0,
            ms(1_000 + step * 20),
        );
        assert!(effects.is_empty(), "slow steps start no pause");
        let now = grid.viewport().expect("a viewport");
        if now != last {
            changes += 1;
            last = now;
        }
    }
    assert_eq!(changes, 0, "steps inside one row publish nothing new");

    scroll_to(&mut grid, 20_280.0, ms(1_400));
    let moved = grid.viewport().expect("a viewport");
    assert_ne!(moved, settled);
    assert_eq!(moved.visible_start_index, 505);
    assert!(!moved.paused);
}

/// "keeps layout getters quiet while only the rendered rows change"
#[test]
fn scrolling_changes_the_rows_and_never_the_layout() {
    let mut grid = grid(10_000);
    let layout = *grid.layout();
    let mut at = 0;
    for top in (1_000..=20_000).step_by(700) {
        at += 200;
        scroll_to(&mut grid, top as f64, ms(at));
        assert_eq!(*grid.layout(), layout);
    }
    assert!(grid.rows().expect("rows").first > 80);

    relayout(
        &mut grid,
        GridGeometry {
            width: 588.0,
            ..GEOMETRY
        },
        10_000,
        188.0,
    );
    assert_ne!(*grid.layout(), layout);
}

/// "anchors resizing on the first visible global index and respects the external margin"
#[test]
fn resizing_anchors_on_the_first_visible_index_below_the_margin() {
    let mut grid = grid(10_000);
    scroll_to(&mut grid, 2_080.0, ms(0));
    let narrow = GridGeometry {
        width: 588.0,
        ..GEOMETRY
    };

    relayout(&mut grid, narrow, 10_000, 188.0);
    assert_eq!(grid.layout().columns, 3);
    assert_eq!(grid.scroll_top(), 3_280.0);
    assert_eq!(range(&grid).visible_start_index, 48);

    // Index 48 is row 48 at 588 px with a single 340 px-minimum column.
    relayout(&mut grid, narrow, 10_000, 340.0);
    assert_eq!(grid.layout().columns, 1);
    assert_eq!(range(&grid).visible_start_index, 48);

    relayout(
        &mut grid,
        GridGeometry {
            viewport: 450.0,
            ..narrow
        },
        10_000,
        340.0,
    );
    assert_eq!(range(&grid).visible_start_index, 48);
}

/// "keeps the margin above the grid visible when the first measurement changes columns"
#[test]
fn the_margin_stays_in_view_when_the_first_measurements_change_columns() {
    let mut grid = laid_out(10_000, GEOMETRY, 140.0);
    assert_eq!(grid.layout().columns, 6);
    relayout(
        &mut grid,
        GridGeometry {
            width: 588.0,
            ..GEOMETRY
        },
        10_000,
        140.0,
    );
    assert_eq!(grid.layout().columns, 3);
    assert_eq!(grid.scroll_top(), 0.0);
    let range = range(&grid);
    assert_eq!((range.start_index, range.visible_start_index), (0, 0));
}

/// "keeps the scroll position when only the margin above the grid changes"
#[test]
fn a_taller_margin_moves_the_grid_under_an_unchanged_scroll_position() {
    let mut grid = grid(10_000);
    scroll_to(&mut grid, 2_080.0, ms(0));
    assert_eq!(range(&grid).visible_start_index, 50);

    relayout(
        &mut grid,
        GridGeometry {
            lead: 128.0,
            ..GEOMETRY
        },
        10_000,
        188.0,
    );
    assert_eq!(grid.scroll_top(), 2_080.0);
    // The grid moved down 48 px under an unchanged scroll offset.
    assert_eq!(range(&grid).visible_start_index, 45);
}

/// "retains a partial last row and publishes no slots when empty"
#[test]
fn a_partial_last_row_is_in_range_and_an_empty_result_has_none() {
    let mut grid = grid(12);
    assert_eq!(
        grid.range(),
        Some(LibraryRange {
            start_index: 0,
            end_index: 11,
            visible_start_index: 0,
            visible_end_index: 11,
        })
    );
    assert_eq!(grid.layout().content_height, 588.0);

    relayout(&mut grid, GEOMETRY, 0, 188.0);
    assert_eq!(grid.range(), None);
    assert_eq!(grid.viewport(), None);
}

/// "reaches the millionth slot through a bounded native window": End and Home reach
/// both ends of a million results (the window itself is WebKit's).
#[test]
fn end_and_home_reach_both_ends_of_a_million_results() {
    let mut grid = grid(1_000_000);

    grid.handle(GridInput::ScrollToEdge {
        edge: ScrollEdge::End,
        at: ms(0),
    });
    let end = range(&grid);
    assert_eq!((end.end_index, end.visible_end_index), (999_999, 999_999));
    assert_eq!(grid.offset(), grid.max_offset());

    grid.handle(GridInput::ScrollToEdge {
        edge: ScrollEdge::Start,
        at: ms(1_000),
    });
    assert_eq!(grid.scroll_top(), 0.0);
    let start = range(&grid);
    assert_eq!((start.start_index, start.visible_start_index), (0, 0));
}

/// "moves the window just before native scrolling reaches its edge, keeping the
/// logical row": the logical row of a deep offset, without a window to move.
#[test]
fn a_deep_logical_offset_maps_to_its_row_at_a_million_results() {
    let mut grid = grid(1_000_000);
    scroll_to(&mut grid, 8_000_080.0, ms(0));
    assert_eq!(range(&grid).visible_start_index, 200_000);

    let logical = 16_000_000.0 - 1_700.0;
    grid.handle(GridInput::ScrollTo {
        offset: logical,
        at: ms(1_000),
    });
    assert_eq!(grid.offset(), logical);
    assert_eq!(
        range(&grid).visible_start_index,
        (logical / 200.0).floor() as usize * 5
    );
}

/// "keeps subpixel edge-scroll steps logical despite rounded physical offsets"
#[test]
fn subpixel_steps_add_up_deep_in_a_million_results() {
    let mut grid = grid(1_000_000);
    scroll_to(&mut grid, 5_000_080.0, ms(0));
    for step in 0..10 {
        grid.handle(GridInput::ScrollBy {
            delta: 0.4,
            at: ms(1_000 + step * 200),
        });
    }
    assert!(
        (grid.offset() - 5_000_004.0).abs() < 1e-6,
        "{}",
        grid.offset()
    );
    assert!((grid.scroll_top() - 5_000_084.0).abs() < 1e-6);
}

/// "scrolls a global index to the top and restores native mode when the result
/// shrinks"
#[test]
fn a_global_index_scrolls_to_the_top_and_a_shrinking_result_clamps_the_range() {
    let mut grid = grid(1_000_000);
    grid.handle(GridInput::JumpToIndex(777_777));
    assert_eq!(range(&grid).visible_start_index, 777_775);

    relayout(&mut grid, GEOMETRY, 100, 188.0);
    assert_eq!(grid.layout().content_height, 20.0 * 200.0 - 12.0);
    assert_eq!(range(&grid).end_index, 99);
    assert!(grid.offset() <= grid.max_offset());
}

/// "pauses on a half-viewport jump and resumes the same range after 150ms without
/// movement"
#[test]
fn a_repeated_notification_of_the_same_position_does_not_extend_the_pause() {
    let mut grid = grid(10_000);
    let token = idle_token(&scroll_to(&mut grid, 380.0, ms(0)));
    assert!(grid.paused());
    let latest = grid.viewport().expect("a viewport");
    assert!(latest.paused);

    // The same position reported again is no step: no new timer.
    assert!(scroll_to(&mut grid, 380.0, ms(149)).is_empty());
    assert!(grid.paused());

    grid.handle(GridInput::IdleElapsed(token));
    assert!(!grid.paused());
    assert_eq!(
        grid.viewport().expect("a viewport"),
        media_tagger_client::library::Viewport {
            paused: false,
            ..latest
        }
    );
}

/// "keeps slow scrolling active and counts distance across direction changes in a
/// rolling 100ms"
#[test]
fn distance_adds_up_across_direction_changes_within_a_rolling_window() {
    let mut grid = grid(10_000);
    let mut at = 0;
    for top in (100..=500).step_by(100) {
        assert!(scroll_to(&mut grid, top as f64, ms(at)).is_empty());
        assert!(!grid.paused(), "{top}");
        at += 101;
    }

    // 200 px three times within 60 ms: a whole viewport.
    scroll_to(&mut grid, 700.0, ms(at));
    scroll_to(&mut grid, 500.0, ms(at + 30));
    let first = idle_token(&scroll_to(&mut grid, 700.0, ms(at + 60)));
    assert!(grid.paused());

    // A small step 100 ms later keeps the pause and restarts its timer.
    let second = idle_token(&scroll_to(&mut grid, 690.0, ms(at + 160)));
    grid.handle(GridInput::IdleElapsed(first));
    assert!(grid.paused(), "the timer of the earlier step is obsolete");
    grid.handle(GridInput::IdleElapsed(second));
    assert!(!grid.paused());
}

/// "excludes geometry anchors from motion and clears paused demand and timers on
/// disposal" (the disposal half is the view's: `gallery_grid.rs`)
#[test]
fn layout_anchors_are_no_motion_but_the_next_large_step_pauses() {
    let mut grid = grid(1_000_000);
    let token = idle_token(&scroll_to(&mut grid, 2_000_000.0, ms(0)));
    grid.handle(GridInput::IdleElapsed(token));
    assert!(!grid.paused());

    let narrow = GridGeometry {
        width: 588.0,
        ..GEOMETRY
    };
    relayout(&mut grid, narrow, 1_000_000, 188.0);
    relayout(&mut grid, narrow, 1_000_000, 340.0);
    relayout(
        &mut grid,
        GridGeometry {
            viewport: 450.0,
            ..narrow
        },
        1_000_000,
        340.0,
    );
    assert!(!grid.paused());

    // Half of the 450 px viewport in one step.
    idle_token(&grid.handle(GridInput::ScrollBy {
        delta: 225.0,
        at: ms(500),
    }));
    assert!(grid.paused());
}

/// "holds demand for a timeline drag until release or 150ms at rest, and maps
/// indexes to rows"
#[test]
fn a_held_drag_pauses_until_release_or_rest_and_indexes_map_to_rows() {
    let mut grid = grid(1_000_000);
    let columns = grid.layout().columns;
    let stride = grid.layout().stride;
    assert_eq!(grid.offset_of_index(0), 0.0);
    assert_eq!(grid.offset_of_index(columns * 7 + 1), 7.0 * stride);

    // Slow, small moves alone would not pause; a held drag does.
    let first = idle_token(&grid.handle(GridInput::HoldDemand));
    grid.handle(GridInput::JumpToIndex(columns * 3));
    assert!(grid.paused());
    assert!(grid.viewport().expect("a viewport").paused);

    // Holding again restarts the rest timer.
    let second = idle_token(&grid.handle(GridInput::HoldDemand));
    grid.handle(GridInput::IdleElapsed(first));
    assert!(grid.paused());
    // At rest for 150 ms the pause ends even while the pointer is still down.
    grid.handle(GridInput::IdleElapsed(second));
    assert!(!grid.paused());

    grid.handle(GridInput::HoldDemand);
    assert!(grid.paused());
    grid.handle(GridInput::ResumeDemand);
    assert!(!grid.paused());
    assert!(!grid.viewport().expect("a viewport").paused);
}

// ---------------------------------------------------------------------------
// tileSizeService.test.ts
// ---------------------------------------------------------------------------

/// "clamps tile size to allowed range"
#[test]
fn the_tile_size_is_clamped_to_the_slider_range() {
    let min = f64::from(TILE_SIZE_MIN);
    let max = f64::from(TILE_SIZE_MAX);
    assert_eq!(clamp_tile_size(min - 100.0, 188), TILE_SIZE_MIN);
    assert_eq!(clamp_tile_size(max + 100.0, 188), TILE_SIZE_MAX);
    assert_eq!(clamp_tile_size(200.0, 188), 200);
}

/// "adjusts tile size by wheel direction"
#[test]
fn the_wheel_direction_steps_the_tile_size() {
    assert_eq!(next_tile_size_by_wheel(200, -1.0), 200 + TILE_SIZE_STEP);
    assert_eq!(next_tile_size_by_wheel(200, 1.0), 200 - TILE_SIZE_STEP);
    assert_eq!(next_tile_size_by_wheel(200, 0.0), 200);
}

// ---------------------------------------------------------------------------
// GalleryStatusFooter.test.tsx
// ---------------------------------------------------------------------------

/// "shows the end state with the formatted result count"
#[test]
fn the_end_state_shows_the_formatted_count() {
    let english = I18n::new(Language::En);
    assert_eq!(english.t("gallery.endOfResults"), "End of results");
    assert_eq!(result_count(&english, 1_000_000), "1,000,000 items");
    assert_eq!(result_count(&english, 30_187), "30,187 items");
}

/// "formats counts in %s using its plural rules"
#[test]
fn every_language_formats_the_count_with_its_own_plural_rules() {
    for language in Language::ALL {
        let i18n = I18n::new(language);
        let catalog = Locales::embedded().catalog(language);
        for count in [1_i64, 2, 5, 21, 22, 30_187, 1_000_000] {
            let category = plural_category(language, PluralOperands::integer(count));
            let key = format!("gallery.resultCount{}", category.suffix());
            let template = catalog
                .text(&key)
                .unwrap_or_else(|| panic!("{language:?} has no {key}"));
            let formatted = format_count(language, count);
            let expected = interpolate(template, &|name| {
                (name == "formattedCount").then(|| std::borrow::Cow::Owned(formatted.clone()))
            });
            assert_eq!(
                result_count(&i18n, count as usize),
                expected,
                "{language:?}, {count}"
            );
        }
    }
}

/// "updates an already mounted footer when language changes" (the texts; the view
/// reads them from the runtime's language on every frame)
#[test]
fn the_polish_footer_uses_polish_plurals_and_digit_grouping() {
    let polish = I18n::new(Language::Pl);
    assert_eq!(polish.t("gallery.endOfResults"), "Koniec wyników");
    assert_eq!(result_count(&polish, 30_187), "30\u{a0}187 elementów");
    for (count, label) in [
        (1, "1 element"),
        (2, "2 elementy"),
        (5, "5 elementów"),
        (22, "22 elementy"),
    ] {
        assert_eq!(result_count(&polish, count), label);
    }
}

// ---------------------------------------------------------------------------
// ThumbnailImage.test.tsx: what a tile asks for after a failure. The retry timing
// itself (delays, counter, failure reported once) is pinned by the pipeline's tests
// in `thumbs.rs`.
// ---------------------------------------------------------------------------

const VERSION: &str = "0123456789abcdef";

fn key(asset_id: i64) -> ThumbnailKey {
    ThumbnailKey::new(asset_id, VERSION)
}

fn visible(key: ThumbnailKey) -> TileRequest {
    TileRequest { key, visible: true }
}

fn no_pictures(_: &ThumbnailKey) -> bool {
    false
}

fn failed(key: ThumbnailKey) -> ThumbnailEvent {
    ThumbnailEvent::Failed { key }
}

/// "keeps a pending slower retry when an unchanged source is read again"
#[test]
fn a_failed_tile_read_again_leaves_its_slower_retries_alone() {
    let mut demand = ThumbnailDemand::new();
    assert_eq!(
        demand.sync(&[visible(key(1))], false, no_pictures),
        [ThumbnailInput::Request {
            key: key(1),
            mode: ThumbnailMode::Visible
        }]
    );
    assert!(demand.on_event(&failed(key(1))));
    assert_eq!(demand.status(&key(1)), TileThumbnail::Missing);

    // A refreshed row with the same picture: nothing new, nothing cancelled.
    assert!(demand
        .sync(&[visible(key(1))], false, no_pictures)
        .is_empty());
    assert!(demand
        .sync(&[visible(key(1))], false, no_pictures)
        .is_empty());
}

/// "keeps retrying with a capped backoff after the quick retries until the picture
/// loads": a picture that arrives after the failure is shown.
#[test]
fn a_picture_that_arrives_after_the_failure_is_shown() {
    let mut demand = ThumbnailDemand::new();
    demand.sync(&[visible(key(1))], false, no_pictures);
    demand.on_event(&failed(key(1)));
    assert_eq!(demand.status(&key(1)), TileThumbnail::Missing);

    assert!(demand.on_event(&ThumbnailEvent::Ready {
        key: key(1),
        image: media_tagger_client::backend::ThumbnailImage {
            bytes: vec![0xff, 0xd8],
            version: VERSION.into(),
        },
        current: true,
    }));
    assert_eq!(demand.status(&key(1)), TileThumbnail::Loading);
    demand.decoded(&key(1), true);
    assert_eq!(demand.status(&key(1)), TileThumbnail::Idle);
    assert!(demand.sync(&[visible(key(1))], false, |_| true).is_empty());
}

/// "cancels a pending retry on unmount", "requests a source again when it comes back
/// to a recycled element after a failure" and "requests a source again after a final
/// failure without retries"
#[test]
fn a_failed_tile_that_leaves_cancels_its_retries_and_asks_again_when_it_returns() {
    let mut demand = ThumbnailDemand::new();
    demand.sync(&[visible(key(1))], false, no_pictures);
    demand.on_event(&failed(key(1)));

    assert_eq!(
        demand.sync(&[visible(key(2))], false, no_pictures),
        [
            ThumbnailInput::Cancel { key: key(1) },
            ThumbnailInput::Request {
                key: key(2),
                mode: ThumbnailMode::Visible
            }
        ]
    );
    assert!(!demand.wants(&key(1)));

    let back = demand.sync(&[visible(key(1))], false, no_pictures);
    assert_eq!(
        back,
        [
            ThumbnailInput::Cancel { key: key(2) },
            ThumbnailInput::Request {
                key: key(1),
                mode: ThumbnailMode::Visible
            }
        ]
    );
    assert_eq!(demand.status(&key(1)), TileThumbnail::Loading);
}

/// "reveals a retried load and drops a pending retry when the source changes"
#[test]
fn a_new_source_version_drops_the_old_request_and_asks_for_the_new_picture() {
    let mut demand = ThumbnailDemand::new();
    let old = ThumbnailKey::new(1, "0000000000000001");
    let new = ThumbnailKey::new(1, "0000000000000002");
    demand.sync(&[visible(old.clone())], false, no_pictures);
    demand.on_event(&failed(old.clone()));

    assert_eq!(
        demand.sync(&[visible(new.clone())], false, no_pictures),
        [
            ThumbnailInput::Cancel { key: old.clone() },
            ThumbnailInput::Request {
                key: new.clone(),
                mode: ThumbnailMode::Visible
            }
        ]
    );
    assert_eq!(demand.status(&old), TileThumbnail::Idle);
    assert_eq!(demand.status(&new), TileThumbnail::Loading);
}
