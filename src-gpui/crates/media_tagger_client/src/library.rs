//! The library models (`src/library` of the Solid frontend).
//!
//! - [`library_model`]: [`Library`], the port of `createLibrary` and
//!   `createMutations`: the one model that owns everything below, admits metadata
//!   writes to the queue and publishes from the backend's answers.
//! - [`query`]: [`AssetQuery`], the port of `createAssetQuery` together with the
//!   viewport and filter handling of `createLibrary`: one backend query session at a
//!   time, read in pages of 128 summaries into a sparse cache of at most 24 pages.
//! - [`page_cache`]: that cache, as a structure without I/O.
//! - [`filters`]: the applied search filters a query is started with.
//! - [`query_impact`]: whether a metadata write has to restart the query
//!   (`library/metadata/queryImpact.ts`).
//! - [`tag_state`]: [`AssetTagState`], the port of `createAssetTagState`: the
//!   complete tag list per asset, the per-asset write queue, identity epochs and the
//!   maintenance barrier (ADR 0001).
//! - [`details`]: [`DetailsCache`], the port of `createDetailsCache`: the 256 most
//!   recently used `AssetDetails`.
//! - [`mutations`]: the metadata writes as data, and which canonical values a
//!   backend answer acknowledges.
//! - [`vocabulary`]: [`KnownTags`], the port of `createKnownTags`: the known tags of
//!   the library, read in pages of 200.
//! - [`tags`]: tag normalization on top of the core's rule.
//!
//! `total` is the number of results of the session and the count a virtual grid lays
//! out; the cache holds a bounded part of them. Nothing here grows with `total`: the
//! page cache is bounded by its page limit, the details cache and the tag records by
//! their own limits, and positions, ID ranges and timeline sections are read from the
//! backend session when they are needed
//! (`docs/subsystems/library-query-and-gallery.md`, "GPUI port: the query model";
//! `docs/subsystems/search-tags-and-media-groups.md`, "GPUI port: metadata,
//! mutations and the vocabulary").

pub mod details;
pub mod filters;
pub mod library_model;
pub mod mutations;
pub mod page_cache;
pub mod query;
pub mod query_impact;
pub mod tag_state;
pub mod tags;
pub mod vocabulary;

pub use details::{DetailsCache, DetailsResult, MAX_CACHED_DETAILS};
pub use filters::LibraryFilters;
pub use library_model::{
    run_library_effect, Library, LibraryEffect, LibraryEvent, LibraryInput, LibraryReset,
    LibraryRunner, RefreshError,
};
pub use mutations::{
    MetadataCommit, MetadataPatch, Mutation, MutationError, MutationOutcome, TagEdit, WriteReply,
    WriteRequest,
};
pub use page_cache::{AssetPatch, PageCache, SummaryPatch, MAX_CACHED_PAGES};
pub use query::{
    run_query_effect, AssetQuery, KeepAlive, NavigationSnapshot, QueryConfig, QueryEffect,
    QueryError, QueryEvent, QueryInput, QueryRunner, Ticket, Viewport, ID_RANGE_CHUNK,
    MAX_QUERY_PAGE_SIZE, STALE_REFRESH_DELAY, STALE_REFRESH_MIN_INTERVAL,
};
pub use query_impact::{favorite_mutation_requires_refresh, tag_mutation_requires_refresh};
pub use tag_state::{
    Admission, AssetTagState, AssetTags, BarrierId, TagStateError, TagToken, TurnId,
    MAX_INACTIVE_RECORDS,
};
pub use tags::{is_valid_tag, normalize_tags};
pub use vocabulary::{
    run_vocabulary_effect, KnownTags, VocabularyEffect, VocabularyEvent, VocabularyInput,
    VocabularyResult, VocabularyRunner, VOCABULARY_PAGE_SIZE,
};
