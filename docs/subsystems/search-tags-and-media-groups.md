# Search, tags, and media groups

Implementation entry points: [search state](../../src/features/search/createSearch.ts), [tag coordinator](../../src/library/metadata/createAssetTagState.ts), [bulk controller](../../src/features/bulk/createBulk.ts), [tag browser](../../src/features/search/tag-list/TagListModal.tsx).

This page owns search syntax, tag discovery, bulk selection, and tag/favorite/group editing. The [parser](../../src/features/search/filterService.ts) and [tag normalization](../../src/utils/media.ts) complement the controller entry points above.

Transport names, payload casing, and command validation are specified in the [IPC contract](../architecture/ipc-contract.md). Query sessions and the sparse gallery cache are described in [library query and gallery](library-query-and-gallery.md), while SQL filtering, normalization, revision bumps, and group ordering are described in [database](database.md). The single-asset editing surface is described in [lightbox](lightbox.md).

## Frontend ownership

`features/search/createSearch.ts` keeps synchronous command drafts
and reactive applied state; the shared parser and normalized descriptor determine query
payloads. A valid submission starts one query even when its normalized filters are unchanged.
Typing clears validation feedback without submitting. IME Enter leaves composition untouched.

`createAppRuntime` owns one lazy `SuggestionClient`; Solid inputs use
`features/search/suggestions/createSuggestions.ts` to cancel obsolete requests and hide stale
results. The worker protocol, indexing and token helpers live beside this client.
`features/search/tag-list` owns paginated discovery, local fallback and delayed click intent.
When a failed page has no new fallback entries, automatic pagination stops. A later scroll
can explicitly retry instead of immediately repeating the failed request.

`features/gallery/createSelection.ts` retains selected IDs across cache eviction and query
changes, resolves each selection's range envelope with one binary ID-range read (no page
loads; [ADR 0005](../adr/0005-binary-id-ranges-and-anchored-refresh.md)) and preserves the prior selection on failure.
Query changes cancel pending ranges and clear the index anchor; identity resets clear IDs.
`features/bulk/createBulk.ts` owns bulk metadata and group drafts under the application
runtime. `createSelectedSummaries` retains only selected records, hydrates missing summaries
in batches of 256 with four requests in flight, and overlays committed metadata on older
responses. Missing records and failed reads require explicit retry. Query changes preserve
selected IDs and drafts; library identity changes clear them.

`features/tagging/TagEditor.tsx` shares the application suggestion client. The bulk
dock uses it for complete single-asset tags and add-only multi-selection feedback.
`library/metadata/createMutations.ts` owns admission, canonical publication and refreshes.
Single replacements require a complete canonical base; multi-selection merges use the
backend's complete lists. Synchronous operation locks reject repeated same-turn commands.
Obsolete selections cannot publish editor feedback or restore focus to a new draft.

`features/bulk/GroupOrderBoard.tsx` virtualizes fixed-height rows of the order grid through
`ui/dom/createVirtualList.ts` and supports pointer and keyboard ordering; a keyboard move scrolls
the target row into view. The board only reports the next order; its owner keeps the draft.
`BulkDock` uses it inline in its group tab, and `GroupOrderModal.tsx` uses it as the larger view
with its own draft. Failure or partial success retains the modal draft. Logical close releases
interaction ownership while the closing frame retains its rows and metadata.

## Workspace presentation

Search text and its explicit Search action occupy the first header row. Media kind,
favorites, tag-list browsing, the result count, bulk mode, and thumbnail sizing occupy the
second. The draft/applied split and submission behavior are unchanged. Bulk selection uses
a green outline and checkmark. Bulk actions live in a dock floating at the bottom centre of the
gallery viewport ([ADR 0035](../adr/0035-bulk-actions-floating-dock.md)). With nothing selected it
is a 100px chip; once something is selected it grows upward to show the tag input, the favorite
heart and a counter that clears the selection (all on one row). Feedback for those header
controls (invalid tag, tag load failure with Retry, save failures and partial-write counts) sits
directly under that row, so it is visible on either tab. Below it come a Tags tab (compact, with
the tag-mode description) and a Group & order tab (tall, with the group key, Apply and the inline
order grid).
The gallery gets bottom padding so its last row can scroll out from under the chip. Bulk
suggestions open above the input through a viewport portal, so the dock cannot clip them.
After a bulk save, focus returns to the enabled input without scrolling its parents;
the tag field reveals the input within its own scroll area.
Assigned tags use muted green chips with explicit removal controls; tag-list include/exclude
states retain their distinct accessible labels and keyboard shortcuts.

## Draft filters, applied filters, and refreshes

`createSearch` owns two versions of search state. `filterInput`, `mediaKind`, and `favoritesOnly` are the editable values shown by the top bar. Separate applied values feed `createLibrary`; typing alone only changes the draft input and clears any displayed validation error.

Search, Enter, tag-list Apply, media-kind changes, and favorites use one parse/validate/commit function. Invalid input retains the prior applied query and displays its validation error. Clearing commits the default values through the same function.

`createSearch` compares applied state with a JSON-serialized normalized descriptor containing sorted, deduplicated include/exclude tags, the complete meta filter, media kind, and favorites. Pipe characters within a tag cannot collide with a tag boundary. Every valid submission calls `createLibrary.applyFilters` once and refreshes the current query, even for an unchanged descriptor, including equivalent case/order spellings.

## Search grammar and filter semantics

The parser trims the complete input first. Matching of reserved words is case-insensitive.

| Input form | Exact parsing rule | Result |
| --- | --- | --- |
| Empty or whitespace | No tokens | No tag or meta filter. |
| Normal tokens | Split on one or more whitespace characters. Each token is trimmed and lowercased. A token beginning with `-` and containing at least one following character is excluded; every other token is included. Each side is de-duplicated independently in first-seen order. | Included tags are ANDed; any matching excluded tag rejects the asset. Other filter families are also ANDed. |
| `tags` | Must be the only whitespace-delimited token. | Exact tag count `0`. Despite the historical backend variant name `HasNoTags`, this is an exact-count filter. |
| `tags:N` | Must be the only token, and `N` must contain one or more ASCII decimal digits only, with a value no greater than `Number.MAX_SAFE_INTEGER` (9007199254740991). | Exact non-negative tag count `N`; leading zeroes are accepted. |
| `gN:value` | The whole trimmed input must begin with `gN:`. Everything after the first colon is one value, trimmed at its outer edges; internal spaces are retained. | Exact group-name filter, case-insensitive and outer-whitespace-insensitive in SQLite. |

`tags:`, signed values, decimal values, non-digits, and counts above `Number.MAX_SAFE_INTEGER` produce `hasNoTagsInvalidCount`. The upper bound keeps the parsed count exact and inside the backend's `i64` `tagCount`, so an oversized count never reaches the command boundary as an argument that fails to deserialize. A blank `gN:` value produces `groupNameMissingValue`. A `tags`/`tags:...` token mixed with another whitespace token, or a `gN:` token that occurs after another token, produces `metaTagRequiresSolo`. Because the anchored group rule consumes the remainder, `gN:Trip 2026` is one valid group-name filter rather than a mixed expression.

Double quotes mark a literal tag: `"tags"`, `"tags:3"`, `"gn:trip"`, and `"-holiday"` include those stored names instead of invoking an operator. A minus outside the quotes excludes the literal, for example `-"-holiday"`. Within quotes, `\"` means a literal quote and `\\` means a literal backslash. Quotes must enclose the entire tag; unfinished quotes, unsupported escapes, empty names, and the usual invalid tag characters reject submission. Whitespace inside quotes does not create a valid tag. Unquoted operators keep their existing meaning; quotes embedded in an unquoted tag remain ordinary characters.

Tag-list Apply and search suggestion insertion share `serializeTagToken`, which quotes names beginning with `-`, reserved-looking names, and names containing quotes or backslashes. Parsed literal names feed the same normalized query descriptor and IPC arrays as ordinary tags; resubmitting the displayed text preserves membership. Stored tags are unchanged.

A lone `-` is a normal included token, and the same normalized tag may be present on both the include and exclude sides. Unrecognized strings such as `tagsfoo`, `-tags`, or `gnn:value` are normal tag tokens.

The frontend sends only applied parsed values. The Rust query boundary normalizes tag arrays again, accepts only `image`, `gif`, and `video`, rejects negative exact counts and blank group names, and forwards the normalized filters to the session query. The complete SQL membership and group-ordering rules are in [database](database.md).

Lightbox and bulk actions share `TagEditor`. Chips and the input wrap together inside one border, with long names wrapping within chips. The bulk field retains its 144px height limit and viewport suggestions. Empty editable fields show the input placeholder; empty selection and loading messages remain visible. Single selection exposes removal controls; multiple selection shows successful additions for the current selection and stays add-only. Draft normalization, failure recovery, and selection resets remain in the existing callers. Bulk focus restoration waits for the enabled input to commit after a save; changing selection cancels that pending focus.

## Search autocomplete

`SearchTagator` discovers the active token around the current caret, scanning left and right to whitespace boundaries. The active query is trimmed, lowercased, and stripped of one leading `-`; replacement covers the entire token even when the caret is in its middle, and preserves the negative prefix. Caret position is restored immediately after the inserted tag.

The application-owned `SuggestionClient` lazily starts one worker shared by search and tag editors. The worker owns Fuse indexing and searching, using match ranges, sorting, threshold `0.35`, and minimum match length one. It considers at most 20 eligible matches and displays at most eight. Hard exclusions are removed before that limit; tags already used in the query are omitted unless they equal the active query. Unquoted positive metatag tokens suppress suggestions. An input that the anchored `gN:` rule consumes (`readGroupNameFilterValue` in `src/utils/media.ts`, shared with the parser) has no active token at all, so no word of a multi-word group name such as `gN:Trip 2026` gets suggestions and Enter submits the group filter unchanged. This applies to every `SearchTagator`, as the metatag suppression does. Quoted literals and negative tag tokens remain eligible, and suggestion replacement escapes the complete literal while retaining an exclusion prefix. Used-tag comparisons decode search syntax but treat vocabulary and hard-exclusion entries as stored names.

Only focused, enabled inputs with an eligible token request suggestions. Vocabulary versions and request IDs reject obsolete results. One search runs at a time and pending searches coalesce per editor. Worker failure leaves ordinary text submission available and displays an explicit suggestion Retry. The failure belongs to the token, used/excluded tags and vocabulary snapshot, independently of input focus. Blur cancels obsolete reads and hides results but preserves that Retry during pointer or keyboard focus transfer. Retry focuses the input, opens its suggestion session and retries the worker. A changed request context invalidates the old failure; disabled inputs hide recovery. The shell disposes the worker on unmount. Filtering assigned lightbox tags uses worker exclusions and does not copy or reindex the vocabulary on thumbnail updates.

Suggestions open only while the input is focused and its overall trimmed value is nonempty. `SearchTagator` tracks focus with `focusin`/`focusout` rather than `focus`/`blur`, because WebKitWebDriver typing can focus the input without a `focus` event and would otherwise leave suggestions closed. Arrow keys wrap through results, Escape closes the list, and Enter chooses the active result or submits when no result is active. Main search auto-selects the first suggestion. Lightbox and bulk tag editors do not; their first Enter submits the typed draft unless the user first moves into the suggestion list. Those editors receive a picked tag through `onSuggestionPick`, keep focus, and can keep the list open.

The list stays mounted during that interaction while the worker replaces results. Empty
or pending results hide the list, remove its options and pause its entrance fade. The
combobox reports collapsed until current results are available. This keeps the 150 ms
fade from replaying on every keystroke without exposing stale results. Viewport lists
also wait for positioning before fading. Dismissing visible suggestions with Escape,
blurring or disabling the input, or clearing its value unmounts the list immediately. See the shared
[motion rules](../frontend/architecture-and-ui-conventions.md#motion).

Viewport suggestions retain their requested above/below placement rather than flipping automatically. A below-input list can have zero available height when the input sits at the viewport bottom; the bulk dock avoids this by opening its suggestions above the input.

## Known tags and the tag-list browser

`createKnownTags`, owned by `createLibrary`, is the shared in-memory tag source for search, lightbox, bulk tagging, and the tag-list fallback. Hydration paginates `listTags` in pages of 200 until `total` is loaded. Explicit refreshes and vocabulary-changing saves share one drain promise and request a latest-generation pass. Unchanged saves join an active refresh without advancing its generation; with a current snapshot they perform no fetch. Superseded pagination stops between requests. Reset and unmount invalidate pending reads without publishing a partial vocabulary. Lightbox saves, single-selection bulk additions/removals, and multi-selection merges pass `vocabulary_changed` to `refreshKnownTagsAfterMutation`. Failed refreshes retain the previous snapshot and remain dirty, so the next accepted save retries even if its flag is false. Refresh failure does not fail a successful save. After complete pagination, equal array lengths and exact ordered values reuse the previous array, avoiding redundant suggestion-worker vocabulary messages. Startup hydration and explicit deletion/import/clear lifecycle refreshes or resets remain in place. CSV import uses the global metadata-mutation barrier: it denies new tag, favorite, and group writes and drains already-dispatched writes before import IPC starts. After the atomic import settles, including rejection, the shell resets authoritative per-asset tags and the shared details cache before releasing the barrier and refreshing, so reopened editors load canonical post-import tags. Confirmed database restore and clear-library use the same barrier and perform their broader identity/cache reset before release on both success and rejection; an uncertain rejection is followed by a best-effort library refresh.

Rust stores case-insensitively unique tag names and `list_tags`:

- trims the query;
- uses a case-insensitive literal-substring `lower(name) LIKE "%query%" ESCAPE '\'` lookup (the query's `\`, `%` and `_` are escaped, so they match themselves);
- sorts by tag name ascending;
- floors offset at zero; and
- clamps every requested limit to 1–200.

The tag-list modal component stays mounted with the gallery toolbar, but prepares its fallback vocabulary and requests pages of 100 only while open. Returning from Settings remounts the toolbar without deduplicating or sorting the known tags for the closed modal. Its trimmed query is read in the model effect; there is no deferred query copy. Every new open/query/fallback change increments a request ID, resets to a locally derived first page, scrolls to the top, and ignores results or finalizers from older request IDs. Scrolling within 120 px of the bottom loads another page, and an effect also loads more while the content is too short to become scrollable. Guards prevent requests while closed, during initial or incremental loading, or after `items.length >= total`. Page merging removes case-insensitive duplicates.

If a tag-list request fails, the modal filters and pages the shared known-tag snapshot locally. That fallback trims values, removes blank and case-insensitive duplicates while keeping the first display spelling, and sorts with one `Intl.Collator(undefined, { sensitivity: "base" })` per sort. The comparator reuses that collator for every pair of tags.

Tag selections survive query and page changes while the modal remains open. Include/exclude selections use maps keyed by normalized stored names; `constructor` and `__proto__` have the same selection, exclusion and serialization behavior as other literal tags. A single click is delayed 220 ms so it can be distinguished from a double click. Single click toggles inactive → include → inactive. Double click maps inactive → exclude, include → exclude, and exclude → include. Clicking another tag while a single click is pending commits the earlier click before scheduling the new one. Apply flushes the last pending click, sorts included and excluded display values case-insensitively, emits all includes followed by `-`-prefixed excludes, and atomically replaces the entire search text. It does not merge with or initialize from the current search. Media-kind and favorite drafts remain unchanged.

The tag-list summary and accessible chip states use the active language, including the
interpolated included/excluded counts and stored tag name. Search validation translates
all four parser error variants while retaining the literal `gN` and `tags` grammar tokens.
The [locale regressions](../development/localization.md#search-and-tag-list-translations)
exercise all eleven languages; translated labels do not change selection or parsing rules.

Apply disables closing interactions while in flight. Success closes the modal; rejection leaves it open for retry. Closing resets its query, selection, loading, and pending-click state.

## Bulk selection

Bulk selection is a spec ([ADR 0008](../adr/0008-selection-spec-resolved-in-rust.md)): index ranges of the current query session plus two small maps, `excluded` (unselected IDs inside the ranges) and `included` (selected IDs outside them). `isSelected(index, id)` is a binary search over the ranges plus a map lookup, and the selected count is covered positions − excluded + included, so neither needs IDs. Ranges covering at most `MATERIALIZE_LIMIT` (10,000) positions are resolved right away with one binary ID-range read and stored as `included` IDs, exactly like the earlier ID set; larger ranges stay ranges, and the gallery, counter and bulk writes use them without any ID read. IDs held directly (clicks, small ranges) are authoritative independently of gallery-page eviction and current filters. When the query result changes while ranges exist (a new filter, a scan, a reorder; anything that yields another session ID) the whole selection is cleared and the bulk panel shows “Selection cleared”; a cached session with the same ID keeps the ranges. While ranges are held, the selection reads one ID of their session at once and then every `SESSION_KEEPALIVE_MS` (60 s, at most one read in flight) so the backend's five-minute idle expiry never drops it: a selection survives any idle time, and the next bulk write applies to it. The timer stops when the ranges go (clear, Escape, selection mode off, identity reset, a new session) and on unmount. If the library really changed, a later read is `stale`, the debounced refresh builds a new session, and the selection is cleared with the same notice; a write that races the change is refused as `stale` and writes nothing ([ADR 0029](../adr/0029-range-selection-keeps-its-session-alive.md)). If the read made when the ranges appear is already `stale`, the session was gone before the selection (it expired after five idle minutes, or a change landed just before; the backend's `stale` does not say which), so the ranges were never usable: the selection is cleared at once and the panel says “The selection expired before it could be used” (`bulk.panel.selectionExpiredDescription`, same title and `data-testid="bulk-selection-cleared"`). A refresh that fails (`pageFailureEpoch` advances while the query has no session) clears a range selection with the same expired notice: the query no longer owns the ranges' session, so the keepalive would skip it and it would expire under the selection (a later write would then come back `stale`). IDs held directly stay selected. A failed page read keeps the session and the ranges. `createSelectedSummaries` retains ordering/display metadata only for those IDs, seeds it from cached pages, and hydrates missing summaries through batches of at most 256 IDs with at most four requests in flight. Omitted records and failed batches show a retryable hydration error. Turning selection mode off clears the selected-ID set and anchor. A new query session clears the global-index anchor and invalidates in-flight selection ranges. Enabling bulk mode shows the floating bulk dock over the gallery (see [workspace presentation](#workspace-presentation)); the gallery keeps its full width. The expanded dock is bounded to the viewport height and scrolls when its content exceeds it; long tag content also scrolls inside the shared editor field. Changing the selected-ID set resets its group draft, order, tag feedback, and request state, and clearing the selection returns the dock to the Tags tab.

In bulk mode, an ordinary click toggles only the clicked tile: an unselected tile is added, and a selected tile is removed while other selected IDs are preserved. Ctrl/Cmd-click has the same toggle behavior. Both set its global-index anchor. Shift-click resolves the inclusive range from the anchor, replacing selection; Ctrl/Cmd+Shift adds that range. Shift does not move an existing anchor. Without an anchor, Shift-click adds the tile and establishes an anchor. Ctrl/Cmd+A in bulk mode selects every result of the query as one range, except while focus is in a text field or a modal layer is above the gallery. A click inside a range toggles the ID in `excluded`; outside it, in `included`. Adding a range re-selects excluded IDs it covers and absorbs included IDs whose position it covers; IDs kept from an earlier query (position unknown) are located with one ID-range read of the new range when that range is large.

A long press on a tile in bulk mode (held `LONG_PRESS_MS` = 500 ms without Ctrl, Shift, Alt or Meta and without moving 5 px) opens that asset in the lightbox; the selection, anchor and bulk mode stay unchanged and the release click is suppressed. A move cancels the long press and may start a rectangle instead (`createGridHandlers.ts`). Clicking gallery gaps, padding, or unused space below tiles preserves selection and the anchor. Toolbar, bulk dock, scrollbar, and interactive controls do not clear selection. Escape clears the IDs and anchor, cancels any rectangle or pending selection read, and keeps bulk mode enabled. The shared UI layer manager gives an open modal priority over gallery Escape handling. When at least one item is selected, the dock shows the selected count as a button with a close mark (`data-testid="bulk-clear-selection"`). It uses the same selection-clear action, including invalidating pending selection reads, while keeping bulk mode open.

Rectangle selection starts on primary-pointer movement only after the pointer has been held for at least 150 ms and moved at least five pixels from a tile or empty gallery space. Releasing sooner follows ordinary click handling; holding still does not start a rectangle. Tiles with any positive-area overlap are highlighted as the rectangle grows or shrinks in any direction. A rectangle replaces selection, or adds to the previous IDs when Ctrl/Cmd was held at pointer down. Geometry uses global indices, independent of mounted tiles and cached pages. The primary-color rectangle is clipped to the gallery viewport, and holding the pointer near its top or bottom scrolls the gallery. Native image dragging and the click generated after a rectangle are suppressed. A new primary-pointer left-button press clears any suppression left by the previous gesture, even when bulk mode is now disabled. If the browser never delivered the drag-ending click, the first new click still opens the image or toggles selection according to the current mode.

The gallery owns the temporary highlights. On release, the controller merges the ranges; small ones are resolved with one binary read of their envelope (at most one selection read in flight) and committed once, large ones are committed as ranges at once. The anchor is cleared. The dock updates only after that commit. Bulk writes are disabled and guarded while a gesture or range read is pending. A failed or incomplete read preserves the previous IDs and exposes the gallery error/Retry UI; Retry repeats the full selection read. New input, query changes, identity resets, disabled selection, and unmount invalidate older results and failed retry intent. Pointer cancellation, lost capture, and window blur cancel a held rectangle gesture, not a released range read or its failed Retry intent. Layout changes cancel a held gesture and release capture and animation frames, but preserve a released range read or its failed Retry intent. Re-registering gallery listeners after an error banner moves the grid does not cancel that intent. The preview is removed when its geometry becomes obsolete; Retry still repeats the original ranges with the original additive/replacement mode.

With a range selection the dock loads no summaries: the favorite button shows the generic toggle, tags are add-only, and the group order editor is not offered. Tag, favorite and group writes go through `apply_selection_mutation` with the spec (see [tag mutations](#tag-mutations)).

The dock's group and tag controls appear only once at least one item is selected (or a selection notice explains why it was cleared, with the controls disabled). The inline order grid exists only while the Group & order tab is open and is unmounted while the larger view is open; each grid requests thumbnails only for its visible rows plus overscan. Group submission stays disabled until every selected ID has ordering metadata.

## Metadata coordination

`src/library/metadata/createAssetTagState.ts` owns canonical tags, the per-asset write
queue and its exclusive mutation tokens, identity epochs, tombstones, pins and the 512-entry
inactive-record limit ([ADR 0001](../adr/0001-per-asset-metadata-write-queue.md)).
`enqueueMutation(ids)` admits a write behind every earlier admitted write on any of its IDs
(FIFO per asset). Admission links all target IDs at once, so overlapping bulk writes run in
admission order and cannot deadlock. An idle write starts in the same tick; a queued one starts
when it reaches the head of every target queue. A write is rejected only when maintenance is
active at admission, when the asset was deleted or the identity epoch changed while it waited,
or after disposal. A failed write releases its queue slot, so later writes still run. An
identity reset starts empty queues for the new epoch. Maintenance admission blocks new writes
immediately and drains every admitted write, queued or dispatched, before its operation.
Disposal releases drain waiters; queued maintenance rejects before running its operation.

`createMutations.ts` centralizes tag, favorite, group and deletion commands. A multi-asset
write waits for all of its IDs and is never sent with a reduced subset. The cached details it
patches are read when the write starts, not when it was queued, so a queued write cannot
reinsert a predecessor's stale snapshot. `saveTags` also accepts a function that computes the
complete replacement from the canonical tags at start, which keeps queued add/remove edits
from undoing earlier writes. Backend acknowledgements publish only accepted identities, patch
cached details and summaries, notify consumers, and evaluate the current query's refresh
rules. Summary rows never gain detail-only tags. `queryImpact.ts` owns the refresh predicate and its direct tests.

`src/library/vocabulary/createKnownTags.ts` retains complete pagination, latest-generation
refresh draining, unchanged-save reuse and dirty retry behavior. `createLibrary.runMaintenance` provides metadata/identity reset within the barrier for direct
model consumers. Production settings uses `library.tags.runWithMutationBarrier` with its
operation-specific reset callbacks, preserving the restore/clear vocabulary distinction.
Settings passes its operation-specific reset callbacks through the same barrier and owns
subsequent refresh and user feedback. Bulk and lightbox editors consume the mutation commands;
their feature models retain
editor drafts and retry intent. The lightbox serializes/coalesces replacements per ID and
rebases failed add/remove intent onto an external writer's canonical tags before retry.

## Tag mutations

One tag cannot contain whitespace (including U+FEFF, which JavaScript's `\s`/`trim()` treat as whitespace and the backend matches explicitly), comma, semicolon, or a control character. U+FEFF is trimmed at a tag's edges and splits CSV and legacy tag text. Tags already stored with U+FEFF by earlier versions are not migrated: backup restore still accepts them, CSV import (when it writes the asset's tags) and export, and multi-asset bulk tag merges by ID, split them on U+FEFF into separate tags before validating (other invalid characters still fail on writes that change tags). The tag editors never show a tag with an inner U+FEFF, because `normalizeTags` drops it from the canonical base (an edge U+FEFF is trimmed, the tag shows normally and is saved in the trimmed spelling), so the next single-asset replacement (a lightbox edit or a one-asset bulk edit, both through `set_asset_tags`) removes it. Rationale and rejected alternatives: [ADR 0033](../adr/0033-feff-is-tag-whitespace-in-rust-and-legacy-rows-are-split-on-read.md). A merge over a ranged selection (`apply_selection_mutation`) leaves legacy rows as stored. Lightbox, bulk, normal search, CSV, and backend command boundaries apply this rule; the backend remains authoritative. Replacements are serialized per asset by the shell-lived selection owner and rapid edits coalesce to the latest desired complete array. Both lightbox and bulk consume shell-owned `createAssetTagState`, which records whether each complete tag base is known. Every affected ID acquires an exclusive mutation-start generation from the per-asset write queue before IPC. A second writer for the same ID waits in that queue and sends its command after the first settles, whether it succeeded or failed; it is not rejected as busy. Single-asset bulk edits and lightbox replacements compute their complete list from the canonical tags at that point, so a queued edit merges onto the earlier result. A multi-asset bulk write waits for every selected ID and is never sent with a reduced subset. Favorite and group writes use the same queue, so a heart click while a tag save is pending runs after it, and CSV maintenance can drain every field it may overwrite. Pending writes block detail publication, and settlement advances the generation again so reads started before or during the mutation stay invalid even after failure. A summary-derived editor placeholder is never accepted as a complete tag base. The backend normalizes again, validates the asset, replaces mappings when needed, repairs `tag_count`, canonicalizes legacy tag spelling, removes orphan rows, and conditionally bumps the library revision in the same transaction. After backend success the frontend publishes canonical response tags, patches loaded/detail state, and refreshes known tags best-effort.

With one bulk-selected asset, the dock loads current tags through `get_asset_details` unless the shared authoritative state already knows them. Each chip removal or submitted tag sends the complete replacement through `set_asset_tags` only while that base is known; the replacement is computed from the canonical tags when the queued write starts. An invalid submitted tag keeps its draft and shows the shared tag-validation alert without sending a command. If a query refresh or a write discards the details read, the dock re-reads up to three times before showing the loading error. Loading and save failures are distinct: a loading failure leaves editing disabled and has a details Retry, while a save failure retains the draft and reports a persistence error.

With multiple selected assets, the dock is add-only. Every submitted nonempty trimmed, lowercased tag is immediately sent through `merge_asset_tags_bulk`; the backend keeps existing tags and merges the new value into every existing target without duplicates. Tags successfully added during the current selection are shown as non-removable confirmation chips. A range selection sends the tag once with its spec (`apply_selection_mutation`, `mergeTags`): the backend resolves the spec on the session vector and, inside one IMMEDIATE transaction, first checks that the session's dependency revision is unchanged. A changed or missing session returns `stale`, nothing is written, the card shows the save error and the query refreshes (which clears the range selection with its notice). The spec merge is set-based (one insert per tag over a temporary ID table, exact `tag_count` recount for assets that gained a tag); it does not canonicalize legacy tag rows the way the per-asset merge does. The write takes a library-wide turn in the metadata queue: it waits for every earlier write, later writes wait for it, and its turn drops every cached complete tag list and detail because the affected IDs are unknown to the frontend.

After tag backend success, the frontend consumes canonical returned tags, publishes each returned asset independently to the shared coordinator, patches authoritative cached details, and refreshes known tags as best-effort secondary work. A multi-asset result with `processed_assets === 0` is not shown as success: the draft is retained and the library refreshes so stale selections can be pruned. Partial results settle canonical success only for `results[]` IDs, settle the other mutation barriers without publishing tags, retain success for the durable processed subset, and also refresh the library. Full DB restore/library clear advances a global coordinator epoch and clears the shared details cache and pending editor state; deletion retains a per-ID tombstone generation. `updated_assets === 0` remains a valid no-op success when at least one asset was processed. Failed lightbox drafts remain visible with an explicit Retry action. The secondary known-tag refresh is best-effort after the durable mutation. After a bulk-dock tag submission settles, the tag input regains focus so another tag can be entered immediately; success clears the draft, while a backend rejection preserves it for correction or retry. A rejection performs no local patch and displays an inline error in the tag card.

All lightbox and bulk tag paths consume the backend's `query_impact` through the shared mutation coordinator. It evaluates the current applied filter when the mutation settles. Only included/excluded changed names, changed counts under exact-count filters, or full repair impact cause a refresh. Canonical no-ops and unrelated tag edits preserve cached pages, thumbnail state, and scroll position. Missing-ID results retain their existing refresh recovery.

## Favorites and media groups

Favorite toggle is backend-first: it sends the inverse of the selected asset's current flag, then patches the cached object and matching selected object. Removing a favorite while the applied query is favorites-only also refreshes the library so the item disappears. Other favorite changes rely on the local patch. They advance the global revision while preserving sessions without a favorites dependency. Refresh decisions use the query active at commit.

The bulk dock's input row has a heart toggle using the lightbox button style. It operates on the complete selected-ID set, including evicted gallery rows. In one transaction, the backend reads existing selected rows: if all are favorites it removes them from favorites; otherwise it favorites them all. Missing IDs are skipped, returned as absent from the processed IDs, and pruned from selection before a query refresh. An empty processed set does not bump the revision. The button is disabled for an empty selection and during the write. It is filled only when every selected ID has a loaded summary and every summary is a favorite; mixed or incompletely loaded selections use an outline and a generic toggle label.

A range selection toggles favorites through the spec (`toggleFavorite`) with the same all-or-nothing rule and the same backend toggle semantics; the visible rows are re-read afterwards. Bulk favorites on IDs queue behind pending writes on any selected asset and acquire every selected asset's metadata mutation token before dispatch. Success patches loaded summaries, bulk details, and lightbox details/selection; stale identity results do not patch. Rejection keeps the selection and displays an inline retry message. Success retains the selection and refreshes a favorites-only query after either toggle direction, so favoriting selected items outside its loaded results makes them appear again. Selection changes during a write do not change its captured target IDs or display its errors against a different selection.

A media group consists of a nullable display key and nullable numeric order:

- Single editing trims the key and maps blank to `null`. Empty order becomes `null`; nonempty lightbox edits accept only digits representing a positive safe integer. Leading zeros are accepted. Signs, zero, decimals, exponent notation, whitespace, and unsafe integers are rejected without stripping characters. Invalid existing drafts remain visible but disable Apply until corrected or cleared. The Apply handler independently uses the same validation. This editor restriction does not change stored values or the backend numeric contract. Rust trims the key again and maps blank to `null`; a non-finite order arriving through another caller becomes `null`.
- The bulk dock pre-fills a shared group key case-insensitively. Items from one group start in numeric group order; ungrouped or conflicting selections start in selected-ID insertion order, independent of page-cache insertion/eviction, and conflicts leave the key blank with a warning. Ordering belongs to a media group, so the inline order grid and the sorting modal button are shown only for multiple selected items with a nonblank group-key draft. Apply with a key assigns one-based orders `1..N`; a single unchanged group preserves its current finite order. Blank Apply clears both key and order for every selected item.
- A range selection groups through the spec (`setMediaGroup`): orders `1..N` follow gallery order, then the IDs selected outside the ranges; a blank key clears key and order. Grouping reorders the gallery, so the next write on the same spec is refused as stale and the range selection is cleared.
- The bulk command skips missing assets, replaces both key and order for every existing target in one transaction, and bumps the revision only if a row changed. It returns processed IDs, processed/updated counts, and the normalized key. The frontend patches acknowledged IDs only and reports partial results.

For a nonblank group draft and multiple selected items, the Group & order tab shows an inline order grid (tiles at least 150 pixels wide) and **Sort in larger view**, which opens the shared extra-large modal with numbered, uncropped previews around 280 pixels wide. This editor includes the current selection only, including selected GIFs and videos, and is available before a new group is saved. Its grid virtualizes rows and requests thumbnails for visible rows plus overscan. The entire tile, including its image and caption, supports pointer dragging with an insertion line and edge scrolling. The grip remains a focusable keyboard control. The line appears after a later target or before an earlier target, matching the saved order, and disappears when the drag ends or leaves the grid. Left/Right move one position and Up/Down move one grid row. Keyboard moves retain focus and announce the new position.

The inline grid edits a draft owned by the dock; it restarts from the controller order whenever the selection, the group key or the controller order changes. **Apply group** submits the inline draft as the explicit order when it differs from the controller order, and otherwise lets the controller use its own order. The modal copies the inline draft into its own draft when it opens. **Save order** submits that explicit complete selection through the existing group mutation path and applies the current group key with one-based positions. Complete success synchronizes the dock and closes the modal; rejection retains the draft for retry. Partial success reports the processed count and stays open; closing keeps acknowledged writes. Cancel, Escape, and backdrop dismissal discard only unsaved modal edits. Saving disables dismissal and ordering. Selection or group-context changes invalidate the modal.

Both single and bulk group actions await backend success before patching loaded objects. Bulk failure preserves the dock draft and displays an inline error; success keeps the dock and selection open with the saved state. Lightbox tag replacements are serialized per asset and expose saving/failure/Retry state; favorite and group controls disable while their own write is pending (a double-click guard) and expose failures. A favorite or group write behind a pending tag save queues instead of failing. Backend group filtering is an exact match on the trimmed lowercase key, while stored display spelling is retained. Media-group membership affects adjacency and global ordering as detailed in [database](database.md) and [library query and gallery](library-query-and-gallery.md).

Revision behavior for tag, favorite, and group writes is defined in [database persistence](database.md#library-revision-and-query-sessions). Favorite and single-group setters bump even for a missing ID or a no-op, in the same transaction as the update. Tag replacement rejects missing IDs and bumps only for a change or invariant repair.

## GPUI port: metadata, mutations and the vocabulary

The GPUI port has the same metadata coordination in its client crate, without a GUI
([ADR 0037](../adr/0037-gpui-client-backend-port-and-model-convention.md),
[ADR 0041](../adr/0041-gpui-library-model-queued-writes-as-data.md)). The Tauri path above is
unchanged. The bulk dock is its first user ([below](#gpui-port-bulk-dock-and-group-order)); the
lightbox and the settings operations are later epics.

| Solid | GPUI client (`src-gpui/crates/media_tagger_client/src/library/`) |
| --- | --- |
| `createLibrary`, `metadata/createMutations` | `library_model.rs`: the model `Library` with `LibraryInput`, `LibraryEffect`, `LibraryEvent` and `LibraryRunner`; `mutations.rs`: `Mutation`, `TagEdit`, `WriteRequest`, `WriteReply`, `MetadataPatch`, `MetadataCommit` |
| `metadata/createAssetTagState` | `tag_state.rs`: `AssetTagState` |
| `metadata/createDetailsCache` | `details.rs`: `DetailsCache` |
| `vocabulary/createKnownTags` | `vocabulary.rs`: the model `KnownTags` |
| `normalizeTags`, `isValidTag` of `utils/media.ts` | `tags.rs`, on the core's `utils/tags.rs` |

`Library` is a state machine that owns the query model
([library query and gallery](library-query-and-gallery.md#gpui-port-the-query-model)), the tag
state, the details cache and the known tags. It is embedded in the application runtime: the
query's inputs travel as `RuntimeInput::Query` and pass through the library, every other input
as `RuntimeInput::Library`, and `RuntimeInput::RefreshLibrary` is `refreshLibrary` (the
query's refresh and a pass over the known tags). The constants are the same: 256 cached
details (`MAX_CACHED_DETAILS`), 512 inactive tag records (`MAX_INACTIVE_RECORDS`), tag pages
of 200 (`VOCABULARY_PAGE_SIZE`).

What is the same as described above:

- **Queue.** A write is admitted behind every earlier write on any of its assets, all assets
  linked at once; an idle write is sent in the same turn, a queued one when it reaches the
  head of every queue, and a failed one releases its place. A write on several assets waits
  for all of them and is sent once, for all of them. A write on a selection spec takes the
  library-wide turn and drops every cached tag list and detail when it starts and when it
  ends.
- **Refusals.** Only maintenance, disposal, a deletion of the asset or an identity reset while
  the write waited refuse a write; none of them reaches the backend.
- **Backend first.** Nothing is published when a write is sent. From the backend's answer,
  only the assets it acknowledges get their canonical values: the tag list (normalized), the
  cached details as they were read when the write *started*, and the cached query rows
  (favorite, group key and order; never tags). A partial bulk answer therefore publishes the
  acknowledged IDs only and refreshes the query because assets went missing.
- **Refresh rules.** `query_impact.rs` decides the query refresh after a tag write under the
  filters applied when the write settles; removing a favorite refreshes a favorites-only
  query; a group write and a deletion always refresh. `vocabulary_changed` drives the known
  tags as `refreshKnownTagsAfterMutation` does.
- **Clocks.** A details read publishes only under the generation it captured: not after a write
  on the asset started or ended, a deletion (tombstone), an identity reset, or eviction of the
  record. Details belong to one query epoch and one cache generation.
- **Known tags.** One pass at a time, complete pagination, a refresh during a pass joins it
  and makes the read start over, a failed later page publishes nothing and leaves the
  snapshot dirty for the next save, and an unchanged result keeps the published list (the
  same `Arc`).
- **Resets.** `LibraryReset::Identity` is `resetIdentity`, `Restored` is
  `handleImportDbRestored` (the known tags stay until the next refresh), `Cleared` is
  `handleLibraryCleared`.

What differs, because there are no promises, no closures in inputs and no reactive owner:

- **Writes are data.** `LibraryInput::Mutate { ticket, mutation }` is answered exactly once
  with `LibraryEvent::MutationSettled` (`MutationOutcome` with the backend's summary, or
  `MutationError::{Refused, TagsNotLoaded, Backend}`). `subscribe` is the event
  `LibraryEvent::Committed(MetadataCommit)`. A reply is always applied, also after a reset or
  the disposal, because it releases the queue turn; it then publishes nothing.
- **Tag edits.** `saveTags(id, tags)` is `TagEdit::Replace`; the function form is
  `TagEdit::Change { add, remove }`, applied to the canonical tags when the write leaves the
  queue, so a queued edit merges onto the write before it. `MutationOutcome::TagsSaved`
  carries the list that was sent.
- **Editing needs the complete tag list.** `Library::can_edit_tags` is true once a details read
  or a write published the asset's tags. Until then both forms of a tag edit are refused with
  `TagsNotLoaded` and nothing is sent. Solid keeps that guard for a plain replacement in the
  lightbox and the bulk dock; here the model has it.
- **Ticketed reads.** `details.load` is `LoadDetails`, answered with
  `LibraryEvent::DetailsLoaded`; readers of one asset share one backend call. A read overtaken
  by a new query epoch, a reset or the disposal is answered with nothing at that moment.
  `refreshLibrary` with a ticket is answered with `Refreshed` once the query's start and the
  vocabulary pass have ended, `waitForIdle` with `Idle`. A deletion is answered after the
  query refresh and the vocabulary pass it started have ended; their failure does not fail it.
- **Pins.** `pin` and its release are `PinTags` and `UnpinTags`, counted per asset.
- **Maintenance.** `runWithMutationBarrier` is `BeginMaintenance { ticket }`,
  `LibraryEvent::MaintenanceReady` (or `MaintenanceRejected` after disposal) and
  `EndMaintenance { ticket, reset }`: the owner of the operation runs it between the grant and
  the end, and the reset is applied before the barrier comes down, whether the operation
  succeeded or not.
- **Tag rule.** One implementation: `tags.rs` calls the core's `utils/tags.rs` and, like
  `normalizeTags`, drops a tag that breaks the rule from a list instead of failing it. The
  core trims U+0085 at a tag's edge, where JavaScript keeps it and then drops the tag; inside
  a tag both reject it. The search grammar of `utils/media.ts` is not part of this module.

Tests: `library/tag_state/tests.rs`, `library/vocabulary/tests.rs`,
`library/library_model/tests.rs`, `library/mutations.rs` and `library/tags.rs`, on `FakeBackend`
and `ManualExecutor` with replies run in a chosen order, and one case on `ThreadExecutor`
against the real core through `TempCore`
([testing](../development/testing.md#gpui-workspace)).

## GPUI port: selection and gestures

The GPUI port has the bulk selection described [above](#bulk-selection) with the same rules, split
between its two crates ([ADR 0043](../adr/0043-gpui-selection-synced-by-the-runtime-and-gestures-as-a-state-machine.md)).
The Tauri path is unchanged. The bulk dock, which shows the count and the
notices and sends the writes, is described [below](#gpui-port-bulk-dock-and-group-order).

**Model** (`media_tagger_client::gallery::selection`, the port of `createSelection.ts`).
`Selection` is a state machine in the convention of the client crate: `SelectionInput` in,
`SelectionEffect` out. It holds the ranges of one query session, the `excluded` and `included` ID
maps (in the order the IDs were selected), the Shift anchor, the pending range read with the ranges
it resolves and their retry, the notice (`RangeCleared`, `RangeExpired`) and the error
(`SelectionError`), and exposes `is_selected(index, id)`, `count`, `ids`, `key`, `busy` and
`capture` with the `SelectionSpec`. `MATERIALIZE_LIMIT` (10,000) and `SESSION_KEEPALIVE` (60 s)
are the constants of the Solid model.

- **Embedded in the runtime.** `AppRuntime` owns it next to the library (`RuntimeInput::Selection`,
  `AppRuntime::selection`), so it survives the settings page like the Solid model. It never calls
  the query model: a range read and the keepalive read are effects, which the runtime hands to the
  embedded query model in the same turn as `QueryInput::ReadIdRange` and
  `QueryInput::KeepSessionAlive` with a ticket from the selection's own range
  (`SELECTION_TICKET_BASE` and up; every other caller of ticketed query reads stays below it). The
  answers go back to the selection and never appear in `AppRuntime::take_query_events`.
- **Epochs.** What the Solid model reads through reactive getters arrives as
  `SelectionInput::Sync(QuerySnapshot)` after every input the library took: the query epoch, the
  session, the total, the page-failure epoch and the identity epoch of the tag state. The runtime
  syncs before it passes on the answers that came with the same input, so a range read that a
  refresh overtook is forgotten, not shown as a failure. A new query epoch cancels the pending read,
  its retry and the anchor; another session drops ranges with `RangeCleared` and keeps IDs held
  directly (without publishing, so `key` stays); the same session keeps the ranges. A failed query
  (the failure epoch moves while the query has no session) drops ranges with `RangeExpired`; a
  failed page keeps them. A new identity epoch clears the selection.
- **Keepalive** ([ADR 0029](../adr/0029-range-selection-keeps-its-session-alive.md)). While ranges
  are held, the model asks for one read of their session at once and re-arms a 60 s timer effect
  (`RuntimeEffect::SelectionKeepAlive`, the only selection effect that reaches the runner), with
  one read in flight at most. `Stale` on the first read drops the ranges with `RangeExpired`; a
  later one is left to the query's own refresh.
- **Writes.** `SelectionCapture::spec_mutation` turns a captured range selection into
  `Mutation::MergeTagsInSelection`, `ToggleFavoritesInSelection` or `SetMediaGroupInSelection`,
  which the library model sends through the port's `apply_selection_mutation` on its library-wide
  turn. A `Stale` answer writes nothing and refreshes the query, whose new session clears the
  selection. It answers `None` while a gesture or a range read is pending and for a selection of
  IDs, which the bulk model writes through the ID commands.
- **One difference.** Adding no range at all to a selection that holds a range and an ID whose
  position is unknown (an additive rectangle over empty space) leaves the selection as it is. The
  Solid model fails there with a retryable error, because it reads the first of zero new ranges.

**Gestures** (`gallery::selection_gesture`, `gallery::selection_geometry`; the ports of
`createGridHandlers.ts` and `selectionGeometry.ts`). `SelectionGesture` is the state machine of
the press: the 150 ms hold and 5 px move before a rectangle, the rectangle from the pressed point
(which scrolls with the grid) to the pointer clamped to the viewport, its tiles from global indexes
(`intersected_ranges`), edge scrolling per animation frame (`edge_scroll_speed`, at most 32 ms a
frame), the 500 ms long press that opens the pressed asset, and the one swallowed click after a
rectangle or a long press. The view (`views/gallery/selection.rs`) gives it pointer positions
relative to the viewport, the grid's place in it and times on the executor's clock, and runs its
effects: commands for the selection, `GalleryEvent::OpenAsset`, the long-press timer, scrolling
and frames.

- A press starts a gesture on the grid and the space around its tiles: not on the position bar or
  the timeline (their presses stop there), not on the banner's Retry button, not on the status
  footer and not in the empty state.
- Moves and the release are window-wide listeners registered in every frame. The release is taken
  in the capture phase, because GPUI fires the click of the tile under the pointer in the bubble
  phase of the same event and the gesture has to end first, as `pointerup` precedes `click`. A
  move without the button down ends a held gesture (the release happened outside the window), and
  so does the window losing its activation.
- The preview of a released rectangle stays until the model is no longer busy with its ranges.
- A change of selection mode or of the query epoch ends a held gesture; for a released one only
  the preview goes, since the model cancels its own read on both changes. A change of the grid's
  measures (columns, tile size, count, the error banner, the viewport's size) ends a held gesture
  and hides a released preview without touching the read or its retry.
- A gallery view that goes away (the settings page takes its place) takes its selection work with
  it, held or released, as the unmount of the Solid grid does: the press, a range read on its way
  and a failed range with its Retry are cancelled. The selection and the mode stay.
- While selection mode is on, the gallery view holds a layer of the window's layer stack that is
  not modal: Escape clears the selection and keeps the mode, unless a modal is above. Ctrl+A (and
  the platform key with A) is bound in the `Gallery` key context and selects every result only in
  selection mode and while that layer is the top one; a text field keeps its own Ctrl+A. As with
  the scroll keys, the binding is not reached while the focus is on the window's root.
- The bulk-actions toggle is in the tools row (`controls.bulkActions`, primary while on, its
  accessible name says what a click does). In selection mode the result ends 88 px above the
  viewport's bottom (`.workspace-body--bulk`), the room the dock floats in. A selected tile has the 2 px primary ring over its
  hover edge and the check mark at its top left; a selected slot without a loaded asset has the
  ring only. Tiles under a rectangle are drawn selected while it is dragged; a rectangle with
  Ctrl held at the press keeps what was selected. The rectangle is a primary border with a 20%
  fill, clipped to the viewport. The load error banner also shows for a selection range that
  could not be read, and its Retry repeats the failed pages and that range.

Tests: `gallery/selection/tests.rs`, `gallery/selection_geometry.rs`,
`gallery/selection_gesture.rs` and the `selection` cases of `runtime/tests.rs` in the client
crate; `tests/gallery_selection.rs` in `media_tagger_gpui`
([testing](../development/testing.md#gpui-workspace)).

## GPUI port: search

The GPUI interface (`src-gpui/`, plan `docs/plans/gpui-port`, epic E12) keeps the grammar,
the draft/applied split and the suggestion rules above; the code lives in
`media_tagger_client::search` and the view in `media_tagger_gpui::views::search`.

- **Grammar.** `search::grammar` (`parseSearchFilter`, `readGroupNameFilterValue`) and
  `search::tokens` (`readTagToken`, `serializeTagToken`) split and trim with JavaScript's
  whitespace set (`search::text`: U+FEFF is whitespace, U+0085 is not), match reserved
  words without ASCII case, accept `tags:N` up to `Number.MAX_SAFE_INTEGER` and use the
  core's tag rule through `library::is_valid_tag`. `search::filters` builds the normalized
  descriptor; tags sort by UTF-16 code units as JavaScript's `sort` does.
- **Draft and applied.** `Search` is owned by `AppRuntime`, so the draft survives the
  gallery view. Every command (`SearchCommand`: change, submit, apply input, kind,
  favourites, clear) goes through one submit; a valid one hands `LibraryFilters` to the
  query (`QueryInput::ApplyFilters`) in the same turn and refreshes even when the
  descriptor is unchanged; an invalid one keeps the applied filters and sets the
  validation error, which the next change clears.
- **Suggestions (D13).** `search::matcher` replaces the Fuse.js worker: an approximate
  substring match scored like Fuse.js (`errors / query length + start / 100`, threshold
  0.35, ties in vocabulary order), 20 matches considered and 8 shown, hard exclusions
  removed before the limit, used tags dropped unless equal to the query, unquoted
  positive operators suggest nothing. Highlights are the byte ranges of the characters
  the best alignment matched; Fuse.js marks every query character in its window, so a
  fuzzy match can highlight less here (the `search-suggestions` parity scenario shows
  exact prefixes, where both agree). Offsets of the active token are UTF-8 byte offsets
  of the field, not UTF-16 units.
- **Off the UI thread.** `SuggestionClient` (one per runtime) runs one search at a time
  as a `RuntimeEffect::Suggest` on the executor; each field (`EditorId`) keeps only its
  latest request, a newer request stops its field's running search between tags, and
  an answer for an older request or vocabulary is dropped. The runner keeps the search
  index of the last vocabulary `Arc` and builds it again only for a new one (the
  known tags keep their `Arc` when unchanged). With 100,000 tags a release build builds
  the index in about 8 ms and searches in about 20 ms, both on the executor; the UI
  thread only reads the token. A failed search (a panicked task) fails the client until
  Retry, as the worker failure does.
- **The field.** `search::Combobox` is the session of one suggesting field
  (`SearchTagator` plus `createSuggestions`): open only while focused with a non-blank
  value, results shown only for the current request and vocabulary, Up/Down wrap, Enter
  picks the active item or submits, Escape closes an open list and is otherwise passed
  on to the layer stack, a pick replaces the whole token (quoted when needed, its `-`
  kept) in one undo step. A failure keeps its Retry button across blur until the
  token, used tags, exclusions or vocabulary change. The tag editors of later epics
  use it with `ComboboxConfig { auto_select_first: false, pick_to_owner: true, .. }`.
- **Keys and focus.** Ctrl+K and Meta+K are bound in the shell's key context
  (`shortcuts::ShortcutCommand::FocusSearch`) and focus the gallery's field unless a
  modal layer is open. The field reports focus with GPUI's focus events, which arrive
  only while the window is active; deactivating the window closes the list like a
  browser blur. The frame's primary border also needs an active window, as WebKit's
  `focus-within` does. The focus a comparison scenario scripts (`Scenario::search`)
  opens the session at once even in an inactive window, because the harness's display
  has no window manager to activate it, while WebKit sends `focusin` for that focus.
- **Measures.** `search::layout` holds the Tauri values as rendered: the magnifier is
  20 pixels (`UiIcon`'s `h-5 w-5` wins over the wrapper's `h-4 w-4`), so the input and
  the list under it start 37 pixels into the field; the Ctrl K hint has the line height
  `normal` of its `font` shorthand (14 pixels, 16 with padding); the divider before the
  tag list's button is a filled box one pixel wide, since GPUI draws no left border on a
  box only that wide.
- **Not ported yet.** The `title` tooltips of the field (`search.syntaxHint`) and of the
  favourites button wait for a tooltip primitive. The list does not fade in; it is drawn
  only with items, which is what the Solid list shows.

## GPUI port: tag list and tag editor

Epic E13 ports the tag list dialog and the shared tag editor. The rules of
[known tags and the tag-list browser](#known-tags-and-the-tag-list-browser) above are kept; the code lives in
`media_tagger_client::search::tag_list` and `media_tagger_client::tagging`, the views in
`media_tagger_gpui::views::{tag_list, tag_editor}`.

- **One model in the runtime.** `TagList` is `createTagList` plus the state of
  `TagListModal` (open, filter, choices, the click timer, Apply). It is embedded in
  `AppRuntime` (`RuntimeInput::TagList`); its pages (`list_tags`, 100 per page) and its
  220 ms click timer run on the executor (`RuntimeEffect::TagList`). The runtime hands it
  the known tags after every library input; the fallback is deduplicated and sorted only
  while the list is open. A late page, one that a new filter, a new vocabulary or closing
  overtook, changes nothing. The gallery view drops the dialog with its launcher, so a
  list left open is closed then, as the Solid launcher keeps `open` locally.
- **Apply.** `TagListEffect::ApplySearch` never reaches the runner: the runtime writes the
  filter text into the search draft and submits it (`SearchCommand::ApplyInput`) and
  answers `ApplySettled` once the query it started is no longer pending, `ok` unless the
  query failed to start. An invalid filter is shown under the header and counts as
  applied, as the Solid submit resolves. While Apply runs, the filter, the choices,
  Escape, the backdrop and both close buttons are refused.
- **Pagination.** The view reports the list's scroll metrics after layout
  (`ListMetrics`; the model forgets them whenever the page changes). A scroll within
  120 pixels of the end loads the next page; content no more than 120 pixels taller than
  the list loads on its own until a failed page adds nothing from the fallback.
- **Choices.** A click waits 220 ms; a click on another tag applies the waiting one, a
  double click (GPUI's click count 2), Shift+Enter or Shift+Space cancels it and toggles
  exclusion (bound in the chip's key context `TagListChip`; with Shift GPUI makes no
  keyboard click). The filter text sorts included, then excluded tags with
  `compare_tags`, an approximation of `Intl.Collator` with base sensitivity: case and the
  diacritics of Latin letters are ignored, punctuation sorts before digits and digits
  before letters.
- **Focus.** The dialog is a `UiModal` layer: its filter takes the focus one frame after
  it opens (`data-initial-focus`), and the launcher gets it back when it closes
  (`ModalState::set_restore_focus`). The dialog is drawn from the search header, inside
  the gallery's key context, so the gallery's scroll and select-all keys do nothing while
  a modal layer is open. On a tag button the list takes those keys itself, as the browser
  scrolls the Solid list's overflow box: the arrows move it 40 pixels, Page Up and Page
  Down 87.5 % of its height, Home and End to its edges (`layout::key_scroll_top`). A
  button that gets the keyboard focus is scrolled into view with the least movement
  (`layout::reveal_scroll_top`); either scroll reports the list's metrics, so reaching the
  end by keyboard loads the next page.
- **Drawn rows.** GPUI builds and lays out every drawn element again in every frame (a
  wheel step, a hover), so the view does not draw every loaded tag as the Solid list
  does. It packs the buttons into rows as `flex flex-wrap gap-1.5` does
  (`tag_list::rows::ChipRows`), from each tag's text width measured once per page and
  the list's width, and draws only the rows that reach into the visible part with three
  rows of overscan on each side, plus the row of the focused button and the rows next to
  it, so the focus, Tab, Shift+Tab and the scroll keys work wherever the list is
  scrolled. The rows not drawn take their height as margin, so the content height and
  the 120-pixel pagination are those of the whole list. Before the list's first layout
  its width is unknown: that frame draws the buttons in one flex-wrap and the next packs
  them, as does a change of width. Measured on the test platform in a release build, a
  frame costs about 2 ms with 1,000, 5,000 or 20,000 loaded tags (before: 11, 57 and
  406 ms; [E13 report](../plans/gpui-port/reports/E13-review-open.md)). The loaded tags
  themselves still grow without bound until a new filter, a new vocabulary or closing,
  about 330 bytes each with their buttons' state.
- **Tag editor.** `TagEditor` takes `TagEditorProps` from its owner (tags, texts,
  disabled, loading, remove label, placement) and emits `TagEditorEvent::{DraftChanged,
  AddTag, RemoveTag}`; it writes nothing itself. Its field is a `Combobox` with
  `TAG_EDITOR_COMBOBOX` (no active item before an arrow key, picks to the owner, the list
  stays open) whose exclusions are the assigned tags. A press on the frame's blank space
  focuses the field; a disabled or loading editor refuses focus, typing, Enter and
  removal and closes its list. The list is drawn over the window next to the field
  (`tagging::list_placement`, the `viewport` strategy), so the frame's scrolling never
  clips it; while the field has the focus, the frame keeps it in view when the tags or
  the draft change, and an unfocused editor keeps its scroll position (`revealInput`). The
  bulk dock and the lightbox (`views/lightbox.rs`,
  [lightbox](lightbox.md#gpui-port-the-lightbox-view)) each own one.
- **Filter field.** The tag list's filter is a `TextInput` with `FieldChrome::Outlined`
  (`text/visuals.rs`): the Solid input's `px-3 py-2.5`, a 1px `border-base-content/12`
  border on `bg-base-100` and `--shadow-surface` at rest, with `border-primary/40` and
  `--field-shadow-focus` when focused.
- **Not ported yet.** The `title` tooltips of the tag buttons wait for a tooltip
  primitive.

## GPUI port: bulk dock and group order

Epic E14 ports the bulk dock of [workspace presentation](#workspace-presentation) with the rules
of [tag mutations](#tag-mutations) and [favorites and media groups](#favorites-and-media-groups)
([ADR 0035](../adr/0035-bulk-actions-floating-dock.md),
[ADR 0044](../adr/0044-gpui-bulk-model-reads-its-neighbours-and-the-dock-state-belongs-to-the-view.md)).
The Tauri path is unchanged. The code lives in `media_tagger_client::bulk`, the views in
`media_tagger_gpui::views::bulk_dock`.

| Solid (`src/features/bulk`) | GPUI client (`src-gpui/crates/media_tagger_client/src/bulk/`) |
| --- | --- |
| `bulkTagMergeService.ts` | `tag_merge.rs`: `normalize_bulk_tag`, `normalize_bulk_tag_list`, `merge_tag_lists` |
| `bulkGroupOrderService.ts`, `lightbox/parseMediaGroupOrder.ts` | `group_order.rs`: `normalize_group_key`, `unique_ordered_asset_ids`, `build_bulk_media_group_assignments`, `derive_bulk_group_selection_state`, `reorder_asset_ids_by_drop`, `parse_media_group_order` |
| `createSelectedSummaries.ts` | `selected_summaries.rs`: `SelectedSummaries` |
| `createBulk.ts` | `model.rs`: `Bulk` with `BulkCommand`, `BulkInput`, `BulkEffect`, `BulkEvent` and the read view `BulkView` |
| the signals of `BulkDock.tsx` and `GroupOrderModal.tsx` | `dock.rs`: `Dock` with `DockEffect` |
| the geometry and the drag of `GroupOrderBoard.tsx` | `board.rs`: `BoardLayout`, `BoardDrag` |
| `.bulk-dock*` of `src/styles.css` and the components' classes | `layout.rs` |

- **Model in the runtime.** `Bulk` is embedded in `AppRuntime` next to the library and the
  selection (`RuntimeInput::Bulk`, `AppRuntime::bulk`), so its drafts survive the settings page.
  It reads both neighbours as they are, through the `BulkEnv` the runtime lends it with every
  input, and changes neither: a write is `LibraryInput::Mutate`, a tag read
  `LibraryInput::LoadDetails`, the pin of the single asset `PinTags`/`UnpinTags`, a pruned ID
  `SelectionInput::Remove`, all applied by the runtime in the same turn. After every input of any
  model the runtime sends `BulkInput::Sync`, which runs what the Solid model's effects run, and
  hands the model the library's answers to its own tickets (`BULK_TICKET_BASE`, 2^61, up to the
  selection's 2^62) and every `MetadataCommit`. The lightbox model's tickets are the range below it, 2^60 up to
  2^61 ([lightbox](lightbox.md#gpui-port-the-lightbox-model)). Every other library answer stays
  for `AppRuntime::take_library_events`; a new reader of ticketed library commands keeps its
  tickets below 2^60.
- **Selected summaries.** For a selection of IDs the model keeps the summaries of exactly the
  selected assets: from the query's cached rows when the selection changes, else from
  `get_asset_summaries_by_ids` in batches of 256 with at most four reads on the executor
  (`RuntimeEffect::BulkSummaries`). An answer of a replaced selection or another identity epoch
  is dropped; an asset the backend did not return, a failed read and a deleted asset mark the
  selection as failed until Retry. What a write published since is laid over an older answer.
  A range selection is never materialized: no summaries, no order board.
- **Drafts.** A new selection key or identity epoch empties the group draft, the feedback and
  the added-tags list. Once every summary is there, the group key and the order are derived
  once per selection (`derive_bulk_group_selection_state`); a key the user typed meanwhile
  stays.
- **Tags.** One asset held by ID: its tag record is pinned, its complete tag list is read
  (up to three attempts when a write or a query refresh discarded the read; an unchanged read
  that published nothing is a failure with Retry). The library answers a discarded read the
  moment the query epoch changes, at the start of a refresh and again at its end, so the next
  attempt waits for the refresh on its way and then for the writes (`waitForIdle`): the refresh
  costs one attempt, as it does in Solid, where the reader hears of it when the backend
  answers. An edit is
  `Mutation::SaveTags` with `TagEdit::Change`, resolved on the canonical list at the head of
  the asset's queue. A tag the asset already has is saved without a write. Several assets:
  `Mutation::MergeTags`, add-only. A range: `MergeTagsInSelection` through
  `SelectionCapture::spec_mutation`, where `Stale` counts as failed.
- **Partial answers.** The library publishes canonical values only for the assets the backend
  acknowledged. The model keeps the counts (`partial_result`: processed of requested), adds the
  tag to the added-tags list when at least one asset took it, and reports a merge that reached
  none as failed. A favorite toggle unselects the IDs the backend did not acknowledge. A group
  write that reached fewer assets answers `GroupSaveResult::Partial`.
- **Obsolete answers.** Every write remembers the selection key and the identity epoch it was
  started for. Feedback (failure flags, counts, the saved order and key) is applied only while
  both are current; the answer to the dock says `saved` only while the identity is.
- **Groups.** `Mutation::SetMediaGroups` with orders 1..n in the model's order, or in the
  explicit order of a board, which must name exactly the selected IDs once each (else
  `Ignored`, nothing sent). One asset that stays in its group keeps its order. An empty key
  clears the groups. A range is grouped in gallery order through its spec and takes no explicit
  order.
- **Dock state.** `Dock` belongs to the view and goes with the gallery view, as the Solid
  component's signals do: the tab (back to Tags when the selection becomes empty; a tab picked
  while a notice keeps the dock open with nothing selected stays), the tag draft with
  its validation message, the focus that returns to the tag field one frame after a save unless
  the selection changed, the inline order draft (restarted for every selection, key and saved
  order) and the larger view with its own draft. Apply sends the inline order only when it
  differs from the model's. The larger view starts from the inline draft, closes when everything
  was saved and keeps its draft after a failed or partial save; it closes without saving when
  the selection or the key changes, and the answer to a save of an earlier opening is not its
  own. While it is open the inline board is not drawn. Request ids are unique in the process,
  so a dock made after a visit to the settings page never takes an older dock's answer.
- **Generated key.** "Generate UUID" writes a version 4 UUID into the group draft
  (`dock::uuid_v4` over `dock::random_bits`: the standard library's randomly seeded hasher
  over the time and a counter). It is a key nobody else chose, not a secret.
- **View.** `BulkDock` is made by the gallery view while selection mode is on and floats at the
  bottom centre of its viewport, 16 pixels above the edge: a 100-pixel chip without a
  selection, else up to 760 pixels wide, and on the group tab `min(72 %, 680px)` of the
  viewport tall. A press or the wheel on it never reaches the grid, and with the focus inside
  it the gallery's scroll keys do nothing. The tablist has one tab stop, the selected tab;
  Left and Right wrap, Home and End jump, and the focus follows (key context `BulkDockTab`).
  The key row of the group tab is the Solid grid: the field, a 40-pixel track with the
  36-pixel "Generate UUID" button at its start, and Apply, 8 pixels apart
  (`layout::group::KEY_BUTTON_TRACK`). Escape in the tag field closes its suggestion list
  while that shows items; any other Escape in the tag field or the group key field reaches the
  gallery's selection layer, which clears the selection and keeps selection mode on.
- **Order board.** `BoardLayout` is the grid of `GroupOrderBoard`: as many columns as fit
  tiles of at least 150 pixels inline and 280 in the larger view, rows of the preview height
  plus 56 pixels, and only the rows that reach into the list plus two on each side are drawn.
  A press on a tile that moves 5 pixels becomes a drag (`BoardDrag`); the drop lands on the
  tile under the pointer or the nearest drawn one, the insertion line lies after a later
  target and before an earlier one, and the list scrolls up to 18 pixels a frame while the
  pointer is within 56 pixels of its top or bottom edge. Moves and the release are window-wide
  listeners, the release in the capture phase; a move without the button, a locked board or a
  window that lost its activation ends the press. The arrow keys on a grip (key context
  `GroupOrderHandle`) move the tile one place sideways or one row up or down and scroll its row
  into view. A grip's focus handle is kept by asset ID, so the focus stays on the grip while
  its tile moves. The board's thumbnails are asked for next to the grid's
  (`Thumbnails::sync_board`) and only for the drawn tiles.
- **Not ported.** The live-region announcement of a move (`bulk.orderModal.moved`), the
  `title` tooltips, and the dedicated small spinner of an order tile (the tile loader of the
  grid is used).

## Known limitations

- Backend ordering and the locale-sensitive fallback ordering of the tag list can differ.
- Quoted literals protect reserved-looking tag names and leading minus signs. They do not permit whitespace or CSV delimiters inside stored tags.
- Tag-list selection is always a new expression. Opening it does not show or preserve the current include/exclude choices, and applying it discards any existing normal or meta-filter search text. Its 220 ms timer is mouse-oriented; selection does not expose an equivalent explicit include/exclude keyboard command beyond activating the tag buttons.
- Selected IDs and selection metadata survive page eviction and filter changes. Range selections larger than 10,000 positions are tied to their query session and are cleared, with a notice, when the session changes, when it was already gone when they were made, or when a refresh fails. A new query invalidates the Shift anchor rather than guessing its new position.
- A spec write holds the SQLite write lock for its whole duration: about 2–3 s per 1,000,000 assets for tags or favorites, and about 4 s for a group write that changes all of them (about 1 s for a tenth; release build, [ADR 0026](../adr/0026-set-based-ranged-group-write-with-index-rebuild.md)). Background writers wait within their busy timeout. Group submission waits for all required summaries; tag and favorite targets remain the complete selected-ID set.
- Ordinary clicks and Ctrl/Cmd-clicks toggle individual IDs. Rectangle selection replaces the set unless Ctrl/Cmd was held when it began.
- Local patches alone do not rebuild session membership or order. Production controllers restart after group writes and tag changes touching applied include/exclude filters; favorite removal restarts a favorites-only query. Exact tag-count filters also restart after changed tags; see [gallery invalidation](library-query-and-gallery.md#refresh-and-invalidation-paths).
- Gallery summaries have no tag field. Only complete details or canonical mutation responses can establish the shared coordinator's authoritative tag base.
- Metadata mutation and revision commit together on the backend. Frontend patches and secondary refreshes remain separate and can fail after a durable write.
- Lightbox tag replacements expose saving, failure, and Retry state. A failed draft remains visible and retry targets the latest coalesced replacement.
- Single tag replacement rejects missing IDs. Bulk tag merge skips missing IDs, returns processed IDs and canonical tags, and triggers a library refresh when processed IDs differ from the submission. Favorite and media-group setters retain their existing missing-ID behavior.
- Bulk grouping can clear both group key and order by applying a blank key. Single editing can clear a key and may retain a non-null order beside it; neither frontend nor database enforces that key and order are both null or both non-null. Differently cased stored group keys match one group search but can form separate ordering buckets; see [database](database.md).

## Safe change checklist

1. Keep the parser, validation messages, autocomplete meta-token suppression, TypeScript filter union, Rust `AssetMetaFilterInput`, and SQL semantics synchronized. Update the [IPC contract](../architecture/ipc-contract.md) for any boundary change.
2. Preserve the draft/applied split. Test initial refresh, changed filters, exact-repeat refresh, invalid-submit non-refresh, clear-at-default refresh, media/favorite immediate apply, and semantically equal spellings.
3. When changing quoting or escaping, retain one tokenizer for parsing, caret replacement, used-tag detection, tag-list serialization, and tag editors. Preserve rejection of whitespace and add round trips for leading `-`, reserved prefixes, `%`, `_`, `|`, Unicode case, and duplicate tags.
4. Keep autocomplete limits, Fuse options, caret restoration, negative-prefix preservation, keyboard wrapping, blur/escape behavior, and the different auto-select policies explicit and covered by tests.
5. Preserve complete known-tag pagination in backend-sized pages and its request-generation guard. Test more than 200 tags, request races, query changes, failed later pages, duplicate page boundaries, and fallback ordering.
6. Preserve the tag-list single/double-click arbiter when changing event handling. Test a pending click followed by Apply, different-tag clicks within 220 ms, every include/exclude transition, close/unmount cleanup, and deterministic full search replacement.
7. Treat bulk selection indices as global snapshot indices or as cache indices consistently. Test plain, Ctrl/Cmd, Shift, Ctrl/Cmd+Shift, drag, filter refresh, page eviction, sparse pages, and selections larger than one frontend page.
8. Normalize tags and IDs on both sides of IPC. For replacement and merge, test blank/case duplicates, stable order, missing IDs, orphan cleanup, `tag_count`, transactional rollback, summary counts, local patches, and known-tag refresh failure.
9. For favorite or group changes, decide whether the visible query must refresh immediately. Test favorites-only removal, exact normalized group search, blank/finite order handling, one-based bulk order, overwrite behavior, missing IDs, no-ops, and group reordering across page boundaries.
10. Keep mutation and revision behavior aligned with [database](database.md) and query invalidation aligned with [library query and gallery](library-query-and-gallery.md). Add an explicit user-visible error/retry policy before changing backend-first sequencing.
11. Choose checks from [the test-level matrix](../development/testing.md#choosing-the-test-level). For pagination or selection changes, exercise more than 200 tags and more assets than the frontend page cache holds.

### Relevant existing tests

- `src/features/bulk/__tests__/BulkDock.test.tsx` covers the collapsed chip and expanded dock, the clearing counter, tab switching (click and arrow keys), inline ordering applied with the group, the larger view starting from the inline draft, and tag focus restoration. `GroupOrderModal.test.tsx` covers row virtualization and keyboard scrolling of the shared board. `e2e/specs/group-order.e2e.js` drives both the modal and the inline grid on disposable media.
- `src/utils/__tests__/media.test.ts`, `src/features/search/__tests__/createSearch.test.ts`, and `src/features/search/__tests__/filterService.test.ts` cover normalization, grammar validation (including the safe-integer bound of `tags:N`), applied state, repeat refresh, and meta keys.
- `src/features/search/suggestions/__tests__` and `src/features/search/__tests__/SearchTagator.test.tsx` cover caret tokens, replacement, used-tag filtering, Fuse/fallback result shaping, reserved meta tokens, the anchored `gN:` input that has no active token and submits on Enter, keyboard selection, focus, and editor auto-selection modes.
- `src/features/search/tag-list/__tests__/TagListModal.test.tsx` and `src/app/__tests__/gallery.test.tsx` cover lazy opening, focus, fallback de-duplication/filtering, click arbitration, deterministic output, apply failure, pagination, and page merging.
- `src/features/gallery/__tests__/GalleryGrid.test.tsx`, `createGridHandlers.test.tsx` and `createSelection.test.ts` cover event reporting and global selection. `src/features/bulk/__tests__/createBulk.test.ts` covers stale detail protection and bulk tag/group flows. `src/app/__tests__/runtimeIntegration.test.ts` covers shared mutation ownership, and `rangeSelectionKeepalive.test.ts` runs the runtime against a backend double with the session TTL: a held range selection idle past the TTL still writes (also when it was made late in the session's idle window), a cleared one stops reading, a real revision change clears it with the "results changed" notice, and an already expired session or a failed refresh (directly or after a stale write) clears it with the "expired" notice and leaves nothing reading or writing the old session. `BulkDock.test.tsx` checks which description each notice shows. Geometry and handler tests cover movement threshold, long press (opens without touching the selection; cancelled by movement, modifiers or a short press), partial overlap in all directions, shrinking, edge scrolling, unloaded rows, click suppression, and cancellation. Controller tests cover global Shift ranges, bounded rectangle reads beyond the cache, stale results, retries, eviction, and anchor reset. The opt-in `e2e/specs/gallery-selection.e2e.js` uses native pointer actions in both themes and 2,051 temporary mixed-media fixtures.
- Bulk tagging, grouping, controller, and dock tests cover normalization, stable merge/order construction, payload filtering, local patches, stale detail responses, reordering, and panel interaction states. Modal coverage includes isolated drafts, cancellation/focus restoration, exact-order submission, failed/partial saves, context changes, and bounded rendering for large selections. `e2e/specs/group-order.e2e.js` checks real pointer drops, cancellation, edge scrolling, wide/narrow layout, and persisted order using copied temporary PNG fixtures.
- `src/features/bulk/__tests__/createBulk.test.ts`, the dock/API tests, and Rust bulk-favorite tests cover mixed/all-favorite toggles, evicted IDs, missing IDs, revision behavior, transaction rollback, contention, duplicate clicks, maintenance draining, and stale identity results.
- Lightbox model and library mutation tests cover tag normalization, serialized/coalesced saves, retry, stale-detail protection, local updates/known-tag refresh, conditional favorite refresh, group parsing, and backend-failure non-patching.
- `src-tauri/crates/media_tagger_core/src/utils/tags.rs` and `src-tauri/crates/media_tagger_core/src/db.rs` unit tests cover normalization, merge order, tag listing/search, exact filters, tag replacement, orphan cleanup, favorites, group mutations, and grouped ordering.
- `src-tauri/tests/backend_integration.rs` covers file-backed tag query/merge and bulk group replacement. `src-tauri/tests/backend_e2e.rs` covers combined CSV/tag/group workflows; browser smoke tests cover bulk tag and group actions in a real window.


### Selection in compressed gallery layouts

For large result sets, the gallery caps physical height and translates scrollbar
positions to logical rows. Pointer rectangles add the current scroll translation
before computing global-index ranges. Edge scrolling advances in logical pixels;
Shift-click still uses each tile's global index. Scrolling changes the translation
without cancelling a held gesture. Width/tile-size changes retain their existing
cancellation rules. The native scale fixture exercises click, Shift-click and
rectangle selection around index 500,000 using lazy synthetic range reads; see
[million-result layout verification](../development/testing.md#million-result-gallery-layout).
