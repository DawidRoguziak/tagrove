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
sidebar uses it for complete single-asset tags and add-only multi-selection feedback.
`library/metadata/createMutations.ts` owns admission, canonical publication and refreshes.
Single replacements require a complete canonical base; multi-selection merges use the
backend's complete lists. Synchronous operation locks reject repeated same-turn commands.
Obsolete selections cannot publish editor feedback or restore focus to a new draft.

`features/bulk/GroupOrderModal.tsx` keeps an isolated order draft, virtualizes fixed-height rows
through `ui/dom/createVirtualList.ts`, and supports pointer and keyboard ordering; a keyboard move
scrolls the target row into view. Failure or partial success retains
the draft. Logical close releases interaction ownership while the closing frame retains its
rows and metadata. The runtime loads the startup popular-tag snapshot once.

## Workspace presentation

Search text and its explicit Search action occupy the first header row. Media kind,
favorites, tag-list browsing, the result count, bulk mode, and thumbnail sizing occupy the
second. The draft/applied split and submission behavior are unchanged. Bulk selection uses
a green outline and checkmark. Its 360px inspector scrolls on desktop and moves below the
gallery under 1000px. Bulk suggestions use a viewport portal to escape inspector clipping.
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

Viewport suggestions retain their requested above/below placement rather than flipping automatically. A below-input list can have zero available height when the input sits at the viewport bottom, including the narrow bulk layout.

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

Bulk selection is a spec ([ADR 0008](../adr/0008-selection-spec-resolved-in-rust.md)): index ranges of the current query session plus two small maps, `excluded` (unselected IDs inside the ranges) and `included` (selected IDs outside them). `isSelected(index, id)` is a binary search over the ranges plus a map lookup, and the selected count is covered positions − excluded + included, so neither needs IDs. Ranges covering at most `MATERIALIZE_LIMIT` (10,000) positions are resolved right away with one binary ID-range read and stored as `included` IDs, exactly like the earlier ID set; larger ranges stay ranges, and the gallery, counter and bulk writes use them without any ID read. IDs held directly (clicks, small ranges) are authoritative independently of gallery-page eviction and current filters. When the query result changes while ranges exist (a new filter, a scan, a reorder; anything that yields another session ID) the whole selection is cleared and the bulk panel shows “Selection cleared”; a cached session with the same ID keeps the ranges. While ranges are held, the selection reads one ID of their session at once and then every `SESSION_KEEPALIVE_MS` (60 s, at most one read in flight) so the backend's five-minute idle expiry never drops it: a selection survives any idle time, and the next bulk write applies to it. The timer stops when the ranges go (clear, Escape, selection mode off, identity reset, a new session) and on unmount. If the library really changed, a later read is `stale`, the debounced refresh builds a new session, and the selection is cleared with the same notice; a write that races the change is refused as `stale` and writes nothing ([ADR 0029](../adr/0029-range-selection-keeps-its-session-alive.md)). If the read made when the ranges appear is already `stale`, the session was gone before the selection (it expired after five idle minutes, or a change landed just before; the backend's `stale` does not say which), so the ranges were never usable: the selection is cleared at once and the panel says “The selection expired before it could be used” (`bulk.panel.selectionExpiredDescription`, same title and `data-testid="bulk-selection-cleared"`). A refresh that fails (`pageFailureEpoch` advances while the query has no session) clears a range selection with the same expired notice: the query no longer owns the ranges' session, so the keepalive would skip it and it would expire under the selection (a later write would then come back `stale`). IDs held directly stay selected. A failed page read keeps the session and the ranges. `createSelectedSummaries` retains ordering/display metadata only for those IDs, seeds it from cached pages, and hydrates missing summaries through batches of at most 256 IDs with at most four requests in flight. Omitted records and failed batches show a retryable hydration error. Turning selection mode off clears the selected-ID set and anchor. A new query session clears the global-index anchor and invalidates in-flight selection ranges. Enabling bulk mode adds a sticky right column beside the gallery, bounded to the viewport height. The column scrolls vertically when its content exceeds the viewport, with its header sticky at the top. Long tag content also scrolls inside the shared editor field. The sidebar has an 8px rounded bottom-left corner. Changing the selected-ID set resets its group draft, order, tag feedback, and request state.

In bulk mode, an ordinary click toggles only the clicked tile: an unselected tile is added, and a selected tile is removed while other selected IDs are preserved. Ctrl/Cmd-click has the same toggle behavior. Both set its global-index anchor. Shift-click resolves the inclusive range from the anchor, replacing selection; Ctrl/Cmd+Shift adds that range. Shift does not move an existing anchor. Without an anchor, Shift-click adds the tile and establishes an anchor. Ctrl/Cmd+A in bulk mode selects every result of the query as one range, except while focus is in a text field or a modal layer is above the gallery. A click inside a range toggles the ID in `excluded`; outside it, in `included`. Adding a range re-selects excluded IDs it covers and absorbs included IDs whose position it covers; IDs kept from an earlier query (position unknown) are located with one ID-range read of the new range when that range is large.

Clicking gallery gaps, padding, or unused space below tiles preserves selection and the anchor. Toolbar, sidebar, scrollbar, and interactive controls do not clear selection. Escape clears the IDs and anchor, cancels any rectangle or pending selection read, and keeps bulk mode enabled. The shared UI layer manager gives an open modal priority over gallery Escape handling. When at least one item is selected, the sidebar header shows a Clear selection button and a short Esc hint below the selected count. The button uses the same selection-clear action, including invalidating pending selection reads, while keeping bulk mode open.

Rectangle selection starts on primary-pointer movement only after the pointer has been held for at least 150 ms and moved at least five pixels from a tile or empty gallery space. Releasing sooner follows ordinary click handling; holding still does not start a rectangle. Tiles with any positive-area overlap are highlighted as the rectangle grows or shrinks in any direction. A rectangle replaces selection, or adds to the previous IDs when Ctrl/Cmd was held at pointer down. Geometry uses global indices, independent of mounted tiles and cached pages. The primary-color rectangle is clipped to the gallery viewport, and holding the pointer near its top or bottom scrolls the gallery. Native image dragging and the click generated after a rectangle are suppressed. A new primary-pointer left-button press clears any suppression left by the previous gesture, even when bulk mode is now disabled. If the browser never delivered the drag-ending click, the first new click still opens the image or toggles selection according to the current mode.

The gallery owns the temporary highlights. On release, the controller merges the ranges; small ones are resolved with one binary read of their envelope (at most one selection read in flight) and committed once, large ones are committed as ranges at once. The anchor is cleared. The sidebar updates only after that commit. Bulk writes are disabled and guarded while a gesture or range read is pending. A failed or incomplete read preserves the previous IDs and exposes the gallery error/Retry UI; Retry repeats the full selection read. New input, query changes, identity resets, disabled selection, and unmount invalidate older results and failed retry intent. Pointer cancellation, lost capture, and window blur cancel a held rectangle gesture, not a released range read or its failed Retry intent. Layout changes cancel a held gesture and release capture and animation frames, but preserve a released range read or its failed Retry intent. Re-registering gallery listeners after an error banner moves the grid does not cancel that intent. The preview is removed when its geometry becomes obsolete; Retry still repeats the original ranges with the original additive/replacement mode.

With a range selection the sidebar loads no summaries: the favorite button shows the generic toggle, tags are add-only, and the group order editor is not offered. Tag, favorite and group writes go through `apply_selection_mutation` with the spec (see [tag mutations](#tag-mutations)).

The sidebar's group and tag controls remain disabled until at least one item is selected. The sidebar offers the ordering heading and modal button without an inline list or thumbnail requests. Only the modal virtualizes ordering previews and requests thumbnails for its visible range plus overscan. Group submission stays disabled until every selected ID has ordering metadata.

## Startup popular tags

The bulk sidebar shows up to ten wrapping tag buttons directly beneath its tag input,
under the localized "Most used tags" label. The backend counts assignments across the
whole library after initialization and recovery, before automatic startup scanning.
Unused tags are excluded; assignment count descending determines rank, with tag name
ascending using SQLite's NOCASE collation for ties. Only names are displayed.

`StartupPopularTags` holds the immutable process snapshot. `createAppRuntime.start()` loads
it on mount through `getStartupPopularTags` and passes it through the bulk controller.
Sidebar remounts, filters, tag edits, scans, imports, restore, library clearing, and frontend
reloads retain that launch ranking. Only a new desktop process calculates a new snapshot.
Calculation or loading failures are logged and leave the section hidden without blocking tagging.

Each button uses the same `addTagAndRestoreFocus` path as autocomplete picks and input
submission, preserving selected-ID targeting, duplicate handling, saving/error feedback,
failed drafts, and focus restoration. Buttons share the input's disabled conditions.
All startup buttons remain visible after applying them, including tags already assigned
to a single selected asset. An empty snapshot hides the section.

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

With one bulk-selected asset, the sidebar loads current tags through `get_asset_details` unless the shared authoritative state already knows them. Each chip removal or submitted tag sends the complete replacement through `set_asset_tags` only while that base is known; the replacement is computed from the canonical tags when the queued write starts. An invalid submitted tag keeps its draft and shows the shared tag-validation alert without sending a command. If a query refresh or a write discards the details read, the sidebar re-reads up to three times before showing the loading error. Loading and save failures are distinct: a loading failure leaves editing disabled and has a details Retry, while a save failure retains the draft and reports a persistence error.

With multiple selected assets, the sidebar is add-only. Every submitted nonempty trimmed, lowercased tag is immediately sent through `merge_asset_tags_bulk`; the backend keeps existing tags and merges the new value into every existing target without duplicates. Tags successfully added during the current selection are shown as non-removable confirmation chips. A range selection sends the tag once with its spec (`apply_selection_mutation`, `mergeTags`): the backend resolves the spec on the session vector and, inside one IMMEDIATE transaction, first checks that the session's dependency revision is unchanged. A changed or missing session returns `stale`, nothing is written, the card shows the save error and the query refreshes (which clears the range selection with its notice). The spec merge is set-based (one insert per tag over a temporary ID table, exact `tag_count` recount for assets that gained a tag); it does not canonicalize legacy tag rows the way the per-asset merge does. The write takes a library-wide turn in the metadata queue: it waits for every earlier write, later writes wait for it, and its turn drops every cached complete tag list and detail because the affected IDs are unknown to the frontend.

After tag backend success, the frontend consumes canonical returned tags, publishes each returned asset independently to the shared coordinator, patches authoritative cached details, and refreshes known tags as best-effort secondary work. A multi-asset result with `processed_assets === 0` is not shown as success: the draft is retained and the library refreshes so stale selections can be pruned. Partial results settle canonical success only for `results[]` IDs, settle the other mutation barriers without publishing tags, retain success for the durable processed subset, and also refresh the library. Full DB restore/library clear advances a global coordinator epoch and clears the shared details cache and pending editor state; deletion retains a per-ID tombstone generation. `updated_assets === 0` remains a valid no-op success when at least one asset was processed. Failed lightbox drafts remain visible with an explicit Retry action. The secondary known-tag refresh is best-effort after the durable mutation. After a bulk-sidebar tag submission settles, the tag input regains focus so another tag can be entered immediately; success clears the draft, while a backend rejection preserves it for correction or retry. A rejection performs no local patch and displays an inline error in the tag card.

All lightbox and bulk tag paths consume the backend's `query_impact` through the shared mutation coordinator. It evaluates the current applied filter when the mutation settles. Only included/excluded changed names, changed counts under exact-count filters, or full repair impact cause a refresh. Canonical no-ops and unrelated tag edits preserve cached pages, thumbnail state, and scroll position. Missing-ID results retain their existing refresh recovery.

## Favorites and media groups

Favorite toggle is backend-first: it sends the inverse of the selected asset's current flag, then patches the cached object and matching selected object. Removing a favorite while the applied query is favorites-only also refreshes the library so the item disappears. Other favorite changes rely on the local patch. They advance the global revision while preserving sessions without a favorites dependency. Refresh decisions use the query active at commit.

The bulk inspector header has a heart toggle using the lightbox button style. It operates on the complete selected-ID set, including evicted gallery rows. In one transaction, the backend reads existing selected rows: if all are favorites it removes them from favorites; otherwise it favorites them all. Missing IDs are skipped, returned as absent from the processed IDs, and pruned from selection before a query refresh. An empty processed set does not bump the revision. The button is disabled for an empty selection and during the write. It is filled only when every selected ID has a loaded summary and every summary is a favorite; mixed or incompletely loaded selections use an outline and a generic toggle label.

A range selection toggles favorites through the spec (`toggleFavorite`) with the same all-or-nothing rule and the same backend toggle semantics; the visible rows are re-read afterwards. Bulk favorites on IDs queue behind pending writes on any selected asset and acquire every selected asset's metadata mutation token before dispatch. Success patches loaded summaries, bulk details, and lightbox details/selection; stale identity results do not patch. Rejection keeps the selection and displays an inline retry message. Success retains the selection and refreshes a favorites-only query after either toggle direction, so favoriting selected items outside its loaded results makes them appear again. Selection changes during a write do not change its captured target IDs or display its errors against a different selection.

A media group consists of a nullable display key and nullable numeric order:

- Single editing trims the key and maps blank to `null`. Empty order becomes `null`; nonempty lightbox edits accept only digits representing a positive safe integer. Leading zeros are accepted. Signs, zero, decimals, exponent notation, whitespace, and unsafe integers are rejected without stripping characters. Invalid existing drafts remain visible but disable Apply until corrected or cleared. The Apply handler independently uses the same validation. This editor restriction does not change stored values or the backend numeric contract. Rust trims the key again and maps blank to `null`; a non-finite order arriving through another caller becomes `null`.
- The bulk sidebar pre-fills a shared group key case-insensitively. Items from one group start in numeric group order; ungrouped or conflicting selections start in selected-ID insertion order, independent of page-cache insertion/eviction, and conflicts leave the key blank with a warning. Ordering belongs to a media group, so the sorting modal button is shown only for multiple selected items with a nonblank group-key draft. Apply with a key assigns one-based orders `1..N`; a single unchanged group preserves its current finite order. Blank Apply clears both key and order for every selected item.
- A range selection groups through the spec (`setMediaGroup`): orders `1..N` follow gallery order, then the IDs selected outside the ranges; a blank key clears key and order. Grouping reorders the gallery, so the next write on the same spec is refused as stale and the range selection is cleared.
- The bulk command skips missing assets, replaces both key and order for every existing target in one transaction, and bumps the revision only if a row changed. It returns processed IDs, processed/updated counts, and the normalized key. The frontend patches acknowledged IDs only and reports partial results.

The ordering heading offers **Sort in larger view** for a nonblank group draft and multiple selected items. It opens the shared extra-large modal with numbered, uncropped previews around 220 pixels wide. This editor includes the current selection only, including selected GIFs and videos, and is available before a new group is saved. Its grid virtualizes rows and requests thumbnails for visible rows plus overscan. The entire tile, including its image and caption, supports pointer dragging with an insertion line and edge scrolling. The grip remains a focusable keyboard control. The line appears after a later target or before an earlier target, matching the saved order, and disappears when the drag ends or leaves the grid. Left/Right move one position and Up/Down move one grid row. Keyboard moves retain focus and announce the new position.

The modal copies the controller order into a local draft. **Save order** submits that explicit complete selection through the existing group mutation path and applies the current group key with one-based positions. Complete success synchronizes the sidebar and closes the modal; rejection retains the draft for retry. Partial success reports the processed count and stays open; closing keeps acknowledged writes. Cancel, Escape, and backdrop dismissal discard only unsaved modal edits. Saving disables dismissal and ordering. Selection or group-context changes invalidate the modal. Apply group remains available in the sidebar with the current controller order. Reordering takes place only in the modal.

Both single and bulk group actions await backend success before patching loaded objects. Bulk failure preserves the sidebar draft and displays an inline error; success keeps the sidebar and selection open with the saved state. Lightbox tag replacements are serialized per asset and expose saving/failure/Retry state; favorite and group controls disable while their own write is pending (a double-click guard) and expose failures. A favorite or group write behind a pending tag save queues instead of failing. Backend group filtering is an exact match on the trimmed lowercase key, while stored display spelling is retained. Media-group membership affects adjacency and global ordering as detailed in [database](database.md) and [library query and gallery](library-query-and-gallery.md).

Revision behavior for tag, favorite, and group writes is defined in [database persistence](database.md#library-revision-and-query-sessions). Favorite and single-group setters bump even for a missing ID or a no-op, in the same transaction as the update. Tag replacement rejects missing IDs and bumps only for a change or invariant repair.

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

- Startup popular-tag tests in `src-tauri/src/app/state.rs` cover empty/unused tags, global counts, alphabetical ties, the ten-tag limit, and immutable snapshots after mutations and clearing. Sidebar/App/API tests cover ordered chips, keyboard activation, disabled states, failed-draft retry, duplicate handling, and shell retention across filters and remounts. `e2e/specs/popular-tags.e2e.js` verifies persisted chip assignments, fixed ranking through edits/scans/reloads, and recalculation on process restart using disposable media. Run it with `bun run test:e2e:tauri --spec e2e/specs/popular-tags.e2e.js` in the isolated desktop environment described in [testing](../development/testing.md#real-desktop-e2e).
- `src/utils/__tests__/media.test.ts`, `src/features/search/__tests__/createSearch.test.ts`, and `src/features/search/__tests__/filterService.test.ts` cover normalization, grammar validation (including the safe-integer bound of `tags:N`), applied state, repeat refresh, and meta keys.
- `src/features/search/suggestions/__tests__` and `src/features/search/__tests__/SearchTagator.test.tsx` cover caret tokens, replacement, used-tag filtering, Fuse/fallback result shaping, reserved meta tokens, the anchored `gN:` input that has no active token and submits on Enter, keyboard selection, focus, and editor auto-selection modes.
- `src/features/search/tag-list/__tests__/TagListModal.test.tsx` and `src/app/__tests__/gallery.test.tsx` cover lazy opening, focus, fallback de-duplication/filtering, click arbitration, deterministic output, apply failure, pagination, and page merging.
- `src/features/gallery/__tests__/GalleryGrid.test.tsx`, `createGridHandlers.test.tsx` and `createSelection.test.ts` cover event reporting and global selection. `src/features/bulk/__tests__/createBulk.test.ts` covers stale detail protection and bulk tag/group flows. `src/app/__tests__/runtimeIntegration.test.ts` covers shared mutation ownership, and `rangeSelectionKeepalive.test.ts` runs the runtime against a backend double with the session TTL: a held range selection idle past the TTL still writes (also when it was made late in the session's idle window), a cleared one stops reading, a real revision change clears it with the "results changed" notice, and an already expired session or a failed refresh (directly or after a stale write) clears it with the "expired" notice and leaves nothing reading or writing the old session. `BulkActionsSidebar.test.tsx` checks which description each notice shows. Geometry and handler tests cover movement threshold, partial overlap in all directions, shrinking, edge scrolling, unloaded rows, click suppression, and cancellation. Controller tests cover global Shift ranges, bounded rectangle reads beyond the cache, stale results, retries, eviction, and anchor reset. The opt-in `e2e/specs/gallery-selection.e2e.js` uses native pointer actions in both themes and 2,051 temporary mixed-media fixtures.
- Bulk tagging, grouping, controller, and sidebar tests cover normalization, stable merge/order construction, payload filtering, local patches, stale detail responses, reordering, and panel interaction states. Modal coverage includes isolated drafts, cancellation/focus restoration, exact-order submission, failed/partial saves, context changes, and bounded rendering for large selections. `e2e/specs/group-order.e2e.js` checks real pointer drops, cancellation, edge scrolling, wide/narrow layout, and persisted order using copied temporary PNG fixtures.
- `src/features/bulk/__tests__/createBulk.test.ts`, the sidebar/API tests, and Rust bulk-favorite tests cover mixed/all-favorite toggles, evicted IDs, missing IDs, revision behavior, transaction rollback, contention, duplicate clicks, maintenance draining, and stale identity results.
- Lightbox model and library mutation tests cover tag normalization, serialized/coalesced saves, retry, stale-detail protection, local updates/known-tag refresh, conditional favorite refresh, group parsing, and backend-failure non-patching.
- `src-tauri/src/utils/tags.rs` and `src-tauri/src/db.rs` unit tests cover normalization, merge order, tag listing/search, exact filters, tag replacement, orphan cleanup, favorites, group mutations, and grouped ordering.
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
