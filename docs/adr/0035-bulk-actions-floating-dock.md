# 0035: Bulk actions in a floating dock, and no startup popular tags

- Status: Accepted
- Date: 2026-11-15
- Areas: frontend, gallery, IPC, startup
- Related: [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md#workspace-presentation), [frontend conventions](../frontend/architecture-and-ui-conventions.md), [IPC contract](../architecture/ipc-contract.md), [system overview](../architecture/system-overview.md)

## Context

Turning on bulk mode added a 360px inspector column beside the gallery (`.workspace-body--bulk`, stacked below the gallery under 1000px). The column narrowed the grid every time bulk mode was on, even when nothing was selected, and group ordering was reachable only through a modal. The inspector also showed a "Most used tags" row: the ten most assigned tags, computed once at startup by `db::list_popular_tags`, kept as `StartupPopularTags` state and read through `get_startup_popular_tags`. The fuzzy autocomplete in the tag input already offers known tags, so the row duplicated it.

## Options considered

1. **Keep the column and make it collapsible.** It still takes width while open and keeps two layouts (side and stacked).
2. **A floating dock over the gallery.** The grid keeps its width; the dock is small until something is selected and grows only as far as the active tab needs.
3. **Keep popular tags in the dock.** It would need room the dock no longer has, plus a startup query, managed state and an IPC command for a list the autocomplete already covers.

## Decision

- Bulk actions live in `BulkDock` (`src/features/bulk/BulkDock.tsx`), positioned absolutely at the bottom centre of `.workspace-viewport`. With nothing selected it is a 100px chip. Once something is selected (or a selection notice must be shown) it shows the tag input, the favorite heart and a selected-count button that clears the selection on one row, the feedback of those header controls (tag validation, load and save errors, partial-write counts) under that row on either tab, then a Tags tab and a Group & order tab. The Group & order tab has a fixed tall height and holds the group key, Apply, and an inline order grid.
- The order grid is one component, `GroupOrderBoard`, used inline and by the larger `GroupOrderModal`. Only one is mounted at a time. Apply group sends the inline draft as the explicit order when it differs from the controller order; the modal starts from the inline draft and keeps its own draft.
- The dock is centred with auto margins, not a transform. The board's `position: fixed` drag ghost is portaled to `document.body`, so neither the dock's position nor its stacking context (below the workspace header) affects it. Its size changes instantly; only content fades in (no layout animation).
- `get_startup_popular_tags`, `StartupPopularTags`, `db::list_popular_tags` and the frontend `getStartupPopularTags` are removed, along with `e2e/specs/popular-tags.e2e.js` and the `bulk.panel.mostUsedTags` locale key. Autocomplete stays in the bulk input, search and the lightbox.

## Consequences

- Removing `get_startup_popular_tags` is a non-additive IPC change; there are no other callers. The wire snapshot in `src-tauri/tests/architecture_boundaries.rs` no longer lists it.
- Startup no longer runs the popular-tag aggregate, so `open_storage` captures only the scan-root snapshot.
- The gallery gets 88px of bottom padding in bulk mode so its last row can scroll out from under the collapsed chip. The expanded dock covers part of the gallery while it is open; that is the intended trade for keeping the grid full width.
- E2E reads the selected count from `data-selected-count` on `[data-testid="bulk-action-panel"]` instead of the old "Selected: N" header text. The group key input and the inline order grid are in the Group & order panel (`[data-testid="bulk-group-panel"]`), shown by the tab button `[data-testid="bulk-tab-group"]`. Inline grid test IDs use the `bulk-inline-order-` prefix; the modal keeps `bulk-order-`.
- Tests: `BulkDock.test.tsx`, `GroupOrderModal.test.tsx`, `e2e/specs/group-order.e2e.js` (modal and inline ordering), and the layout checks in `e2e/specs/ui.redesign.e2e.js`.
