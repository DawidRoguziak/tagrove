//! The gallery grid without a GUI: what `src/features/gallery` decides in the Solid
//! frontend, as pure functions and state machines the GPUI adapter draws.
//!
//! - [`geometry`]: the square-tile grid (`gridGeometry.ts`): columns and tile size for
//!   a width, the rows of a scroll offset, the indexes of those rows, the offset that
//!   keeps the first visible asset when the layout changes.
//! - [`virtual_grid`]: the scroll state of the grid (`createVirtualGrid.ts`): one
//!   logical offset in `f64`, the rendered rows, the fast-scroll pause and the
//!   [`Viewport`](crate::library::Viewport) the query model is told.
//! - [`thumbnail_demand`]: which thumbnail each tile in range asks for and in which
//!   mode (`GalleryTile.tsx`), as inputs of the
//!   [`ThumbnailPipeline`](crate::thumbs::ThumbnailPipeline).
//! - [`picture_cache`]: the bounded cache of decoded pictures.
//! - [`layout`]: the measures and texts of the tile, the status footer and the load
//!   error banner.
//! - [`scrollbar`]: the position bar beside the grid (`GalleryScrollbar.tsx`): track,
//!   thumb, press, drag and keys.
//! - [`timeline`]: the month timeline (`timeline.ts`, `GalleryTimelineScrubber.tsx`):
//!   months from the session's sections, keys, ticks and the rail's state.
//! - [`selection`]: the bulk selection (`createSelection.ts`): ranges of the query
//!   session plus included and excluded IDs, the query and identity epochs, the
//!   session keepalive, and the spec a bulk write goes through.
//! - [`selection_geometry`]: which tiles a rectangle covers
//!   (`selectionGeometry.ts`), from global indexes only.
//! - [`selection_gesture`]: the pointer gestures of selection mode
//!   (`createGridHandlers.ts`): rectangle, long press, click suppression.
//!
//! Offsets are logical pixels relative to the top of the grid, as in the Solid
//! frontend, and may be negative while the margin above the grid is in view. They
//! are `f64`: a million results in one column are 600 million pixels, far beyond
//! what `f32` (GPUI's `Pixels`) holds exactly. The adapter converts only the
//! positions of the few rows in view, which are small numbers. The physical window of
//! the Solid grid (`MAX_GRID_HEIGHT`, the moving base) exists for WebKit's 2^24 px
//! limit and has no counterpart here (ADR 0042).

pub mod geometry;
pub mod layout;
pub mod picture_cache;
pub mod scrollbar;
pub mod selection;
pub mod selection_geometry;
pub mod selection_gesture;
pub mod thumbnail_demand;
pub mod timeline;
pub mod virtual_grid;

pub use geometry::{
    anchor_for_resize, grid_layout, index_to_row, pool_rows, row_range, row_top, to_library_range,
    GridLayout, LibraryRange, RowRange, GRID_GAP, OVERSCAN_ROWS,
};
pub use picture_cache::{PictureCache, PICTURE_CACHE_BUDGET};
pub use scrollbar::{
    scrollbar_key_target, scrollbar_value_now, scrollbar_visible, ScrollbarDrag, ScrollbarKey,
    ScrollbarMetrics, ScrollbarTarget,
};
pub use selection::{
    merge_ranges, ranges_contain, IndexRange, Interaction, QuerySnapshot, Selection,
    SelectionCapture, SelectionEffect, SelectionError, SelectionInput, SelectionNotice,
    SelectionWrite, MATERIALIZE_LIMIT, SELECTION_TICKET_BASE, SESSION_KEEPALIVE,
};
pub use selection_geometry::{
    contains_index, edge_scroll_speed, intersected_ranges, rectangle_between, tile_index_at, Point,
    SelectionGrid, SelectionRectangle,
};
pub use selection_gesture::{
    GestureEffect, GestureFrame, GestureInput, PressedTile, Preview, SelectionGesture,
    DRAG_HOLD_DELAY, LONG_PRESS, MOVE_TOLERANCE,
};
pub use thumbnail_demand::{ThumbnailDemand, TileRequest, TileThumbnail};
pub use timeline::{
    format_timeline_month, index_at_fraction, month_at, timeline_key_target, timeline_months,
    timeline_ticks, TimelineEffect, TimelineInput, TimelineKey, TimelineLabel, TimelineMonth,
    TimelineScrubber, TimelineTick,
};
pub use virtual_grid::{
    GridEffect, GridGeometry, GridInput, ScrollEdge, VirtualGrid, IDLE_DELAY, MOTION_WINDOW,
};

/// Reads a Solid gallery source; tests check the classes copied from it are still
/// there.
#[cfg(test)]
pub(crate) fn gallery_source(file: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/features/gallery")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}
