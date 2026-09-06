# Search, tags, and media groups

Implementation entry points: [search state](../../src/components/app/hooks/useAppSearchFilters.ts), [tag coordinator](../../src/components/app/hooks/useAssetTagState.ts), [bulk controller](../../src/components/app/hooks/useBulkSelectionController.ts), [tag browser](../../src/components/tag-list/TagListModal.tsx).

This page owns search syntax, tag discovery, bulk selection, and tag/favorite/group editing. The [parser](../../src/components/app/services/filterService.ts) and [tag normalization](../../src/utils/media.ts) complement the controller entry points above.

Transport names, payload casing, and command validation are specified in the [IPC contract](../architecture/ipc-contract.md). Query sessions and the sparse gallery cache are described in [library query and gallery](library-query-and-gallery.md), while SQL filtering, normalization, revision bumps, and group ordering are described in [database](database.md). The single-asset editing surface is described in [lightbox](lightbox.md).

## Workspace presentation

Search text and its explicit Search action occupy the first header row. Media kind,
favorites, tag-list browsing, the result count, bulk mode, and thumbnail sizing occupy the
second. The draft/applied split and submission behavior are unchanged. Bulk selection uses
a green outline and checkmark. Its 360px inspector scrolls on desktop and moves below the
gallery under 1000px. Bulk suggestions use a viewport portal to escape inspector clipping.
Assigned tags use muted green chips with explicit removal controls; tag-list include/exclude
states retain their distinct accessible labels and keyboard shortcuts.

## Draft filters, applied filters, and refreshes

`useAppSearchFilters` owns two versions of search state. `filterInput`, `mediaKind`, and `favoritesOnly` are the editable values shown by the top bar. Separate applied values feed `useLibraryBrowser`; typing alone only changes the draft input and clears any displayed validation error.

Search, Enter, tag-list Apply, media-kind changes, and favorites use one parse/validate/commit function. Invalid input retains the prior applied query and displays its validation error. Clearing commits the default values through the same function.

The shell keys query refreshes with a JSON-serialized normalized descriptor containing sorted, deduplicated include/exclude tags, the complete meta filter, media kind, and favorites. Pipe characters within a tag cannot collide with a tag boundary. Submitting an unchanged normalized descriptor explicitly refreshes the current query, including equivalent case/order spellings.

## Search grammar and filter semantics

The parser trims the complete input first. Matching of reserved words is case-insensitive.

| Input form | Exact parsing rule | Result |
| --- | --- | --- |
| Empty or whitespace | No tokens | No tag or meta filter. |
| Normal tokens | Split on one or more whitespace characters. Each token is trimmed and lowercased. A token beginning with `-` and containing at least one following character is excluded; every other token is included. Each side is de-duplicated independently in first-seen order. | Included tags are ANDed; any matching excluded tag rejects the asset. Other filter families are also ANDed. |
| `tags` | Must be the only whitespace-delimited token. | Exact tag count `0`. Despite the historical backend variant name `HasNoTags`, this is an exact-count filter. |
| `tags:N` | Must be the only token, and `N` must contain one or more ASCII decimal digits only. | Exact non-negative tag count `N`; leading zeroes are accepted. |
| `gN:value` | The whole trimmed input must begin with `gN:`. Everything after the first colon is one value, trimmed at its outer edges; internal spaces are retained. | Exact group-name filter, case-insensitive and outer-whitespace-insensitive in SQLite. |

`tags:`, signed values, decimal values, and non-digits produce `hasNoTagsInvalidCount`. A blank `gN:` value produces `groupNameMissingValue`. A `tags`/`tags:...` token mixed with another whitespace token, or a `gN:` token that occurs after another token, produces `metaTagRequiresSolo`. Because the anchored group rule consumes the remainder, `gN:Trip 2026` is one valid group-name filter rather than a mixed expression.

Double quotes mark a literal tag: `"tags"`, `"tags:3"`, `"gn:trip"`, and `"-holiday"` include those stored names instead of invoking an operator. A minus outside the quotes excludes the literal, for example `-"-holiday"`. Within quotes, `\"` means a literal quote and `\\` means a literal backslash. Quotes must enclose the entire tag; unfinished quotes, unsupported escapes, empty names, and the usual invalid tag characters reject submission. Whitespace inside quotes does not create a valid tag. Unquoted operators keep their existing meaning; quotes embedded in an unquoted tag remain ordinary characters.

Tag-list Apply and search suggestion insertion share `serializeTagToken`, which quotes names beginning with `-`, reserved-looking names, and names containing quotes or backslashes. Parsed literal names feed the same normalized query descriptor and IPC arrays as ordinary tags; resubmitting the displayed text preserves membership. Stored tags are unchanged.

A lone `-` is a normal included token, and the same normalized tag may be present on both the include and exclude sides. Unrecognized strings such as `tagsfoo`, `-tags`, or `gnn:value` are normal tag tokens.

The frontend sends only applied parsed values. The Rust query boundary normalizes tag arrays again, accepts only `image`, `gif`, and `video`, rejects negative exact counts and blank group names, and forwards the normalized filters to the session query. The complete SQL membership and group-ordering rules are in [database](database.md).

## Search autocomplete

`SearchTagator` discovers the active token around the current caret, scanning left and right to whitespace boundaries. The active query is trimmed, lowercased, and stripped of one leading `-`; replacement covers the entire token even when the caret is in its middle, and preserves the negative prefix. Caret position is restored immediately after the inserted tag.

A lazy shell-owned `SuggestionProvider` shares one worker between tag editors. The worker owns Fuse indexing and searching, using match ranges, sorting, threshold `0.35`, and minimum match length one. It considers at most 20 eligible matches and displays at most eight. Hard exclusions are removed before that limit; tags already used in the query are omitted unless they equal the active query. Unquoted positive metatag tokens suppress suggestions. Quoted literals and negative tag tokens remain eligible, and suggestion replacement escapes the complete literal while retaining an exclusion prefix. Used-tag comparisons decode search syntax but treat vocabulary and hard-exclusion entries as stored names.

Only focused, enabled inputs with an eligible token request suggestions. Vocabulary versions and request IDs reject obsolete results. One search runs at a time and pending searches coalesce per editor. Worker failure leaves ordinary text submission available and displays an explicit suggestion Retry. The shell disposes the worker on unmount. Filtering assigned lightbox tags uses worker exclusions and does not copy or reindex the vocabulary on thumbnail updates.

Suggestions open only while the input is focused and its overall trimmed value is nonempty. Arrow keys wrap through results, Escape closes the list, and Enter chooses the active result or submits when no result is active. Main search auto-selects the first suggestion. Lightbox and bulk tag editors do not; their first Enter submits the typed draft unless the user first moves into the suggestion list. Those editors receive a picked tag through `onSuggestionPick`, keep focus, and can keep the list open.

## Known tags and the tag-list browser

`useLibraryKnownTags`, owned by `useLibraryBrowser`, is the shared in-memory tag source for search, lightbox, bulk tagging, and the tag-list fallback. Hydration paginates `listTags` in pages of 200 until `total` is loaded. Overlapping refreshes share one drain promise and request a latest-generation pass. Superseded pagination stops between requests. Reset and unmount invalidate pending reads without publishing a partial vocabulary. Successful tag changes and relevant deletion/import/clear workflows refresh or reset this state according to their owning action. CSV import uses the global metadata-mutation barrier: it denies new tag, favorite, and group writes and drains already-dispatched writes before import IPC starts. After the atomic import settles, including rejection, the shell resets authoritative per-asset tags and both detail caches before releasing the barrier and refreshing, so reopened editors load canonical post-import tags. Confirmed database restore and clear-library use the same barrier and perform their broader identity/cache reset before release on both success and rejection; an uncertain rejection is followed by a best-effort library refresh.

Rust stores case-insensitively unique tag names and `list_tags`:

- trims the query;
- uses a case-insensitive substring-style `lower(name) LIKE "%query%"` lookup;
- sorts by tag name ascending;
- floors offset at zero; and
- clamps every requested limit to 1–200.

The tag-list modal is lazy and requests pages of 100. Its trimmed query is passed through `useDeferredValue`. Every new open/query/fallback change increments a request ID, resets to a locally derived first page, scrolls to the top, and ignores results or finalizers from older request IDs. Scrolling within 120 px of the bottom loads another page, and an effect also loads more while the content is too short to become scrollable. Guards prevent requests while closed, during initial or incremental loading, or after `items.length >= total`. Page merging removes case-insensitive duplicates.

If a tag-list request fails, the modal filters and pages the shared known-tag snapshot locally. That fallback trims values, removes blank and case-insensitive duplicates while keeping the first display spelling, and sorts with `localeCompare(..., { sensitivity: "base" })`.

Tag selections survive query and page changes while the modal remains open. A single click is delayed 220 ms so it can be distinguished from a double click. Single click toggles inactive → include → inactive. Double click maps inactive → exclude, include → exclude, and exclude → include. Clicking another tag while a single click is pending commits the earlier click before scheduling the new one. Apply flushes the last pending click, sorts included and excluded display values case-insensitively, emits all includes followed by `-`-prefixed excludes, and atomically replaces the entire search text. It does not merge with or initialize from the current search. Media-kind and favorite drafts remain unchanged.

Apply disables closing interactions while in flight. Success closes the modal; rejection leaves it open for retry. Closing resets its query, selection, loading, and pending-click state.

## Bulk selection

Bulk selection stores authoritative IDs independently of gallery-page eviction and current filters. `useSelectedSummaries` retains ordering/display metadata only for those IDs, seeds it from cached pages, and hydrates missing summaries through batches of at most 256 IDs with at most four requests in flight. Omitted records and failed batches show a retryable hydration error. Turning selection mode off clears the selected-ID set and anchor. A new query session clears the global-index anchor and invalidates an in-flight Shift range. Enabling bulk mode adds a sticky right column beside the gallery, bounded to the viewport height. The column itself does not scroll: long ordering content scrolls inside the compact thumbnail list, while long tag content scrolls inside the tag list so headings and controls remain visible. Changing the selected-ID set resets its group draft, order, tag feedback, and request state.

Current pointer and modifier behavior is:

- A primary-button pointer down without Ctrl/Cmd or Shift starts additive drag selection and immediately adds that tile. Entering more tiles while held adds them; drag never removes an item. The following ordinary click is suppressed.
- Ctrl-click or Cmd-click toggles one ID and makes it the new anchor.
- Shift-click uses the global-index anchor and resolves the inclusive ID range through the active query session, replacing the prior set; Ctrl/Cmd+Shift unions that range into the prior set. A range operation does not move the anchor. A response captured for an older query epoch or superseded by another range request is ignored.
- If Shift has no usable anchor, handling falls through to an ordinary single-item replacement and sets that item as anchor.
- A plain click delivered without the preceding pointer-down path, such as a synthesized click, replaces selection with that one ID and sets the anchor.
The sidebar's group and tag controls remain disabled until at least one item is selected. The ordering list is virtualized and requests thumbnails only for its visible range plus overscan. Group submission stays disabled until every selected ID has ordering metadata.

## Tag mutations

One tag cannot contain whitespace, comma, semicolon, or a control character. Lightbox, bulk, normal search, CSV, and backend command boundaries apply this rule; the backend remains authoritative. Replacements are serialized per asset by the shell-lived selection owner and rapid edits coalesce to the latest desired complete array. Both lightbox and bulk consume shell-owned `useAssetTagState`, which records whether each complete tag base is known. Every affected ID must acquire an exclusive mutation-start generation before IPC. A second writer for the same ID sends no command. Multi-asset bulk is all-or-none at lock acquisition: if any selected ID is busy, already-acquired tokens are released and the command is not sent with a reduced subset. Favorite and group writes acquire the same ownership so CSV maintenance can drain every field it may overwrite. Pending writes block detail publication, and settlement advances the generation again so reads started before or during the mutation stay invalid even after failure. A summary-derived editor placeholder is never accepted as a complete tag base. The backend normalizes again, validates the asset, replaces mappings when needed, repairs `tag_count`, canonicalizes legacy tag spelling, removes orphan rows, and conditionally bumps the library revision in the same transaction. After backend success the frontend publishes canonical response tags, patches loaded/detail state, and refreshes known tags best-effort.

With one bulk-selected asset, the sidebar loads current tags through `get_asset_details` unless the shared authoritative state already knows them. Each chip removal or submitted tag sends the complete replacement through `set_asset_tags` only while that base is known. Loading and save failures are distinct: a loading failure leaves editing disabled and has a details Retry, while a save failure retains the draft and reports a persistence error.

With multiple selected assets, the sidebar is add-only. Every submitted nonempty trimmed, lowercased tag is immediately sent through `merge_asset_tags_bulk`; the backend keeps existing tags and merges the new value into every existing target without duplicates. Tags successfully added during the current selection are shown as non-removable confirmation chips.

After tag backend success, the frontend consumes canonical returned tags, publishes each returned asset independently to the shared coordinator, patches authoritative cached details, and refreshes known tags as best-effort secondary work. A multi-asset result with `processed_assets === 0` is not shown as success: the draft is retained and the library refreshes so stale selections can be pruned. Partial results settle canonical success only for `results[]` IDs, settle the other mutation barriers without publishing tags, retain success for the durable processed subset, and also refresh the library. Full DB restore/library clear advances a global coordinator epoch and clears both tag detail caches and pending editor state; deletion retains a per-ID tombstone generation. `updated_assets === 0` remains a valid no-op success when at least one asset was processed. Failed lightbox drafts remain visible with an explicit Retry action. The secondary known-tag refresh is best-effort after the durable mutation. After a bulk-sidebar tag submission settles, the tag input regains focus so another tag can be entered immediately; success clears the draft, while a backend rejection preserves it for correction or retry. A rejection performs no local patch and displays an inline error in the tag card.

## Favorites and media groups

Favorite toggle is backend-first: it sends the inverse of the selected asset's current flag, then patches the cached object and matching selected object. Removing a favorite while the applied query is favorites-only also refreshes the library so the item disappears. Other favorite changes rely on the local patch even though the backend bumps the library revision.

The bulk inspector header has a heart toggle using the lightbox button style. It operates on the complete selected-ID set, including evicted gallery rows. In one transaction, the backend reads existing selected rows: if all are favorites it removes them from favorites; otherwise it favorites them all. Missing IDs are skipped, returned as absent from the processed IDs, and pruned from selection before a query refresh. An empty processed set does not bump the revision. The button is disabled for an empty selection and during the write. It is filled only when every selected ID has a loaded summary and every summary is a favorite; mixed or incompletely loaded selections use an outline and a generic toggle label.

Bulk favorites acquire every selected asset's metadata mutation token before dispatch. Success patches loaded summaries, bulk details, and lightbox details/selection; stale identity results do not patch. Rejection keeps the selection and displays an inline retry message. Success retains the selection and refreshes a favorites-only query after either toggle direction, so favoriting selected items outside its loaded results makes them appear again. Selection changes during a write do not change its captured target IDs or display its errors against a different selection.

A media group consists of a nullable display key and nullable numeric order:

- Single editing trims the key and maps blank to `null`. Blank order becomes `null`; a nonblank order is converted with `Number` and an invalid or non-finite value prevents the frontend save. Rust trims the key again and maps blank to `null`; a non-finite order arriving through another caller becomes `null`.
- The bulk sidebar pre-fills a shared group key case-insensitively. Items from one group start in numeric group order; ungrouped or conflicting selections start in selected-ID insertion order, independent of page-cache insertion/eviction, and conflicts leave the key blank with a warning. Ordering belongs to a media group, so the compact ordering list is shown only for multiple selected items with a nonblank group-key draft. The list uses a dedicated pointer drag handle and supports ArrowUp/ArrowDown on its drag-handle buttons. Apply with a key assigns one-based orders `1..N`; a single unchanged group preserves its current finite order. Blank Apply clears both key and order for every selected item.
- The bulk command skips missing assets, replaces both key and order for every existing target in one transaction, and bumps the revision only if a row changed. It returns processed IDs, processed/updated counts, and the normalized key. The frontend patches acknowledged IDs only and reports partial results.

Both single and bulk group actions await backend success before patching loaded objects. Bulk failure preserves the sidebar draft and displays an inline error; success keeps the sidebar and selection open with the saved state. Lightbox tag replacements are serialized per asset and expose saving/failure/Retry state; favorite and group controls disable while their write is pending and expose failures, including busy metadata ownership. Backend group filtering is an exact match on the trimmed lowercase key, while stored display spelling is retained. Media-group membership affects adjacency and global ordering as detailed in [database](database.md) and [library query and gallery](library-query-and-gallery.md).

Revision behavior for tag, favorite, and group writes is defined in [database persistence](database.md#library-revision-and-query-sessions). Favorite and single-group setters bump even for a missing ID or a no-op, in the same transaction as the update. Tag replacement rejects missing IDs and bumps only for a change or invariant repair.

## Known limitations

- SQL tag-list search interpolates the user's `%` and `_` characters into a `LIKE` pattern without an escape clause, so they act as SQLite wildcards rather than literal characters. Backend ordering and the locale-sensitive fallback ordering can also differ.
- There is no quoted-tag grammar. Whitespace and CSV delimiters are rejected rather than represented inside one tag.
- Tag-list selection is always a new expression. Opening it does not show or preserve the current include/exclude choices, and applying it discards any existing normal or meta-filter search text. Its 220 ms timer is mouse-oriented; selection does not expose an equivalent explicit include/exclude keyboard command beyond activating the tag buttons.
- Selected IDs and selection metadata survive page eviction and filter changes. A new query invalidates the Shift anchor rather than guessing its new position. Group submission waits for all required summaries; tag and favorite targets remain the complete selected-ID set.
- Drag selection is add-only, and an ordinary pointer click also follows the additive pointer-down path; individual deselection requires Ctrl/Cmd.
- Local patches alone do not rebuild session membership or order. Production controllers restart after group writes and tag changes touching applied include/exclude filters; favorite removal restarts a favorites-only query. Exact tag-count filters also restart after changed tags; see [gallery invalidation](library-query-and-gallery.md#refresh-and-invalidation-paths).
- Gallery summaries have no tag field. Only complete details or canonical mutation responses can establish the shared coordinator's authoritative tag base.
- Metadata mutation and revision commit together on the backend. Frontend patches and secondary refreshes remain separate and can fail after a durable write.
- Lightbox tag replacements expose saving, failure, and Retry state. A failed draft remains visible and retry targets the latest coalesced replacement.
- Single tag replacement rejects missing IDs. Bulk tag merge skips missing IDs, returns processed IDs and canonical tags, and triggers a library refresh when processed IDs differ from the submission. Favorite and media-group setters retain their existing missing-ID behavior.
- Bulk grouping can clear both group key and order by applying a blank key. Single editing can clear a key and may retain a non-null order beside it; neither frontend nor database enforces that key and order are both null or both non-null. Differently cased stored group keys match one group search but can form separate ordering buckets; see [database](database.md).

## Safe change checklist

1. Keep the parser, validation messages, autocomplete meta-token suppression, TypeScript filter union, Rust `AssetMetaFilterInput`, and SQL semantics synchronized. Update the [IPC contract](../architecture/ipc-contract.md) for any boundary change.
2. Preserve the draft/applied split. Test initial refresh, changed filters, exact-repeat refresh, invalid-submit non-refresh, clear-at-default refresh, media/favorite immediate apply, and semantically equal spellings.
3. If tag syntax gains quoting or escaping, define one tokenizer for parsing, caret replacement, used-tag detection, tag-list serialization, and tag editors. Add round trips for whitespace, leading `-`, reserved prefixes, `%`, `_`, `|`, Unicode case, and duplicate tags.
4. Keep autocomplete limits, Fuse options, caret restoration, negative-prefix preservation, keyboard wrapping, blur/escape behavior, and the different auto-select policies explicit and covered by tests.
5. Preserve complete known-tag pagination in backend-sized pages and its request-generation guard. Test more than 200 tags, request races, query changes, failed later pages, duplicate page boundaries, and fallback ordering.
6. Preserve the tag-list single/double-click arbiter when changing event handling. Test a pending click followed by Apply, different-tag clicks within 220 ms, every include/exclude transition, close/unmount cleanup, and deterministic full search replacement.
7. Treat bulk selection indices as global snapshot indices or as cache indices consistently. Test plain, Ctrl/Cmd, Shift, Ctrl/Cmd+Shift, drag, filter refresh, page eviction, sparse pages, and selections larger than one frontend page.
8. Normalize tags and IDs on both sides of IPC. For replacement and merge, test blank/case duplicates, stable order, missing IDs, orphan cleanup, `tag_count`, transactional rollback, summary counts, local patches, and known-tag refresh failure.
9. For favorite or group changes, decide whether the visible query must refresh immediately. Test favorites-only removal, exact normalized group search, blank/finite order handling, one-based bulk order, overwrite behavior, missing IDs, no-ops, and group reordering across page boundaries.
10. Keep mutation and revision behavior aligned with [database](database.md) and query invalidation aligned with [library query and gallery](library-query-and-gallery.md). Add an explicit user-visible error/retry policy before changing backend-first sequencing.
11. Choose checks from [the test-level matrix](../development/testing.md#choosing-the-test-level). For pagination or selection changes, exercise more than 200 tags and more assets than the frontend page cache holds.

### Relevant existing tests

- `src/utils/__tests__/media.test.ts`, `src/components/app/hooks/__tests__/useAppSearchFilters.test.ts`, and `src/components/app/services/__tests__/filterService.test.ts` cover normalization, grammar validation, applied state, repeat refresh, and meta keys.
- `src/components/search/services/__tests__/*` and `src/components/search/__tests__/SearchTagator.test.tsx` cover caret tokens, replacement, used-tag filtering, Fuse/fallback result shaping, reserved meta tokens, keyboard selection, focus, and editor auto-selection modes.
- `src/components/tag-list/__tests__/TagListModal.test.tsx` and `TagListSearchLauncher.test.tsx` cover lazy opening, focus, fallback de-duplication/filtering, click arbitration, deterministic output, apply failure, pagination, and page merging.
- `src/components/gallery/__tests__/GalleryGrid.test.tsx`, `src/components/app/hooks/__tests__/useBulkSelectionController.test.ts`, and `src/__tests__/App.test.tsx` cover modifier/drag event reporting, stale detail-request protection, and primary bulk tag/group flows. The bulk controller tests also cover global Shift ranges, stale-range rejection, cache eviction, and query-epoch anchor reset.
- Bulk tagging, grouping, controller, and sidebar tests cover normalization, stable merge/order construction, payload filtering, local patches, stale detail responses, reordering, and panel interaction states.
- `useBulkFavorites.test.ts`, the sidebar/API tests, and Rust bulk-favorite tests cover mixed/all-favorite toggles, evicted IDs, missing IDs, revision behavior, transaction rollback, contention, duplicate clicks, maintenance draining, and stale identity results.
- Lightbox service and hook tests cover tag normalization, serialized/coalesced saves, retry, stale-detail protection, local updates/known-tag refresh, conditional favorite refresh, group parsing, and backend-failure non-patching.
- `src-tauri/src/utils/tags.rs` and `src-tauri/src/db.rs` unit tests cover normalization, merge order, tag listing/search, exact filters, tag replacement, orphan cleanup, favorites, group mutations, and grouped ordering.
- `src-tauri/tests/backend_integration.rs` covers file-backed tag query/merge and bulk group replacement. `src-tauri/tests/backend_e2e.rs` covers combined CSV/tag/group workflows; browser smoke tests cover bulk tag and group actions in a real window.
