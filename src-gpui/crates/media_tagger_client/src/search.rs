//! The search header (`src/features/search` of the Solid frontend): the grammar of the
//! search field, the draft and applied filters, and tag suggestions.
//!
//! - [`grammar`], [`tokens`], [`text`]: `parseSearchFilter` and the token syntax of
//!   `src/utils/media.ts` and `src/utils/searchTokens.ts`, with JavaScript's notion of
//!   whitespace;
//! - [`filters`], [`model`]: `filterService.ts` and `createSearch.ts`: the draft the
//!   header edits, the applied descriptor, and the one submit every command goes
//!   through ([`Search`], owned by the application runtime);
//! - [`active_token`]: the token under the caret, the tags already used, and how a
//!   picked suggestion replaces the token;
//! - [`matcher`]: the fuzzy match that replaces the Fuse.js worker (plan decision D13);
//! - [`suggestions`]: the application's one suggestion client (`SuggestionClient`):
//!   one search at a time on the executor, the latest request per editor, obsolete
//!   answers dropped;
//! - [`combobox`]: one suggesting text field (`SearchTagator` with
//!   `createSuggestions`): when it asks for suggestions, which it shows, the keys of
//!   its list, picking and Retry;
//! - [`layout`]: the measures of the search field, its tools and the list.
//!
//! See `docs/subsystems/search-tags-and-media-groups.md`, "GPUI port: search".

pub mod active_token;
pub mod combobox;
pub mod filters;
pub mod grammar;
pub mod layout;
pub mod matcher;
pub mod model;
pub mod suggestions;
pub mod text;
pub mod tokens;

pub use active_token::{
    apply_suggestion_to_value, collect_used_tags, normalize_tag_token, read_active_token,
    ActiveToken,
};
pub use combobox::{Combobox, ComboboxConfig, ComboboxEffect, ComboboxKey};
pub use filters::{
    build_filter_descriptor, media_kind_label_key, media_kind_value, meta_filter_key,
    parse_media_kind_value, FilterDescriptor, SearchFilters, MEDIA_KIND_OPTIONS,
};
pub use grammar::{
    parse_filter_tags, parse_search_filter, read_group_name_filter_value, FilterMode,
    ParsedSearchFilter, SearchFilterValidationError, MAX_TAG_COUNT,
};
pub use matcher::{build_tag_suggestions, Cancel, TagIndex, TagSuggestion};
pub use model::{Search, SearchCommand};
pub use suggestions::{
    run_suggestion_effect, EditorId, SuggestionClient, SuggestionContext, SuggestionEffect,
    SuggestionEvent, SuggestionFailure, SuggestionInput, SuggestionRequest, SuggestionRunner,
};
pub use tokens::{read_tag_token, serialize_tag_token, TagToken};
