//! Bulk actions on the selection (`src/features/bulk` of the Solid frontend): the
//! model behind the floating dock (ADR 0035), the records of the selected assets, the
//! tag and group services, and what the dock and its order board decide without a GUI.
//!
//! - [`tag_merge`]: `bulkTagMergeService.ts`: one normalized tag of a draft, and tag
//!   lists merged in first-seen order.
//! - [`group_order`]: `bulkGroupOrderService.ts` and `parseMediaGroupOrder.ts`: the
//!   group key, the group a selection shares, its saved order, a drop and a typed
//!   order.
//! - [`selected_summaries`]: `createSelectedSummaries.ts`: the summaries of the
//!   selected assets, read in batches of 256 with four reads in flight, with the
//!   canonical values of later writes laid over older answers.
//! - [`model`]: [`Bulk`], the port of `createBulk.ts`: drafts, tag loading for a single
//!   asset, and the tag, favorite and group writes of ID and range selections.
//! - [`dock`]: [`Dock`], the local state of `BulkDock.tsx` and `GroupOrderModal.tsx`:
//!   the tab, the tag draft and its focus, the inline order and the larger view.
//! - [`board`]: the geometry and the drag of `GroupOrderBoard.tsx`.
//! - [`layout`]: the dock's measures from `src/styles.css` and the Tailwind classes of
//!   the three components.
//!
//! [`Bulk`] lives in the application runtime next to the library and the selection and
//! survives the gallery view; [`Dock`] belongs to the view and goes with it, as the
//! Solid component's signals do. Contract:
//! `docs/subsystems/search-tags-and-media-groups.md` ("GPUI port: bulk dock and group
//! order").

pub mod board;
pub mod dock;
pub mod group_order;
pub mod layout;
pub mod model;
pub mod selected_summaries;
pub mod tag_merge;

pub use board::{BoardDrag, BoardKey, BoardLayout, DragEffect, InsertionLine};
pub use dock::{Dock, DockEffect, DockTab, OrderContext, TabKey};
pub use group_order::{
    build_bulk_media_group_assignments, derive_bulk_group_selection_state, normalize_group_key,
    parse_media_group_order, reorder_asset_ids_by_drop, unique_ordered_asset_ids,
    BulkGroupSelectionState, GroupOrderDraft,
};
pub use model::{
    Bulk, BulkCommand, BulkEffect, BulkEnv, BulkEvent, BulkInput, BulkView, GroupSaveResult,
    PartialResult, TagMode, BULK_TICKET_BASE,
};
pub use selected_summaries::{SelectedSummaries, SummaryLoad, SUMMARY_BATCH, SUMMARY_READS};
pub use tag_merge::{merge_tag_lists, normalize_bulk_tag, normalize_bulk_tag_list};
