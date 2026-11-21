//! The library models (`src/library` of the Solid frontend).
//!
//! - [`query`]: [`AssetQuery`], the port of `createAssetQuery` together with the
//!   viewport and filter handling of `createLibrary`: one backend query session at a
//!   time, read in pages of 128 summaries into a sparse cache of at most 24 pages.
//! - [`page_cache`]: that cache, as a structure without I/O.
//! - [`filters`]: the applied search filters a query is started with.
//! - [`query_impact`]: whether a metadata write has to restart the query
//!   (`library/metadata/queryImpact.ts`).
//!
//! `total` is the number of results of the session and the count a virtual grid lays
//! out; the cache holds a bounded part of them. Nothing here grows with `total`: the
//! cache is bounded by its page limit, and positions, ID ranges and timeline sections
//! are read from the backend session when they are needed
//! (`docs/subsystems/library-query-and-gallery.md`, "GPUI port: the query model").

pub mod filters;
pub mod page_cache;
pub mod query;
pub mod query_impact;

pub use filters::LibraryFilters;
pub use page_cache::{AssetPatch, PageCache, SummaryPatch, MAX_CACHED_PAGES};
pub use query::{
    run_query_effect, AssetQuery, KeepAlive, NavigationSnapshot, QueryConfig, QueryEffect,
    QueryError, QueryEvent, QueryInput, QueryRunner, Ticket, Viewport, ID_RANGE_CHUNK,
    MAX_QUERY_PAGE_SIZE, STALE_REFRESH_DELAY, STALE_REFRESH_MIN_INTERVAL,
};
pub use query_impact::{favorite_mutation_requires_refresh, tag_mutation_requires_refresh};
