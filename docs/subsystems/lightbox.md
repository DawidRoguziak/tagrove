# Lightbox

Implementation entry points: [selection lifecycle](../../src/hooks/useSelectionState.ts), [modal composition](../../src/components/lightbox/LightboxModal.tsx), [video adapter](../../src/components/lightbox/MpvMediaAdapter.ts), [native playback](../../src-tauri/src/services/video_player_service.rs), [GTK surface](../../src-tauri/src/video_surface.rs).

The lightbox is the selected-asset workspace layered over the gallery. It owns full-media presentation, per-asset editing, deletion confirmation, image transforms, video playback, fullscreen, and keyboard navigation. Selection and mutation state live above the modal in `useSelectionState`; `LightboxModal` and its hooks own transient interaction state.

Related documentation:

- [Library query and gallery](library-query-and-gallery.md) describes query sessions, summary objects, the global index, and the paged frontend cache used by lightbox navigation.
- [Search, tags, and media groups](search-tags-and-media-groups.md) defines tag normalization, favorites, and group semantics shared with the lightbox editors.
- [IPC contract](../architecture/ipc-contract.md) covers the Tauri command boundary and camel-case request payloads.
- [Frontend architecture and UI conventions](../frontend/architecture-and-ui-conventions.md) defines controller, hook, service, modal, and accessibility responsibilities.
- [Database](database.md) covers library revisions changed by lightbox mutations.
- [Thumbnails](thumbnails.md) covers gallery previews; the lightbox itself loads the original media URL rather than a thumbnail.

## Ownership and composition

`useAppShellController` creates `useLibraryBrowser` and passes its loaded assets, total result count, global lookup functions, refresh functions, and cache setter to `useSelectionState`. It exposes the returned selection state as the `lightbox` props. `App` lazy-loads and renders `LightboxModal` only while `selected` is non-null.

The runtime path is:

1. A gallery tile calls `selection.setSelected(summaryAsset)`.
2. `useSelectionState` publishes that object synchronously so the modal opens without waiting for IPC.
3. It calls `getAssetDetails(asset.id)` and replaces the summary with `AssetDetails` if that request is still current. Until a complete tag list is available from details or the shared authoritative tag state, tag editing is disabled.
4. `LightboxModal` delegates persistent edits back to `useSelectionState`; local hooks manage panels, confirmation, clipboard feedback, media transforms, fullscreen, and shortcuts. Detail-load failure and tag-save failure are displayed and retried independently.

At widths of at least 768 px the modal initially shows the media stage beside the action sidebar. Below 768 px the sidebar starts as a closed right drawer. Both layouts allow the sidebar to slide off the right edge in 200 ms, with reduced motion respected. An explicit open/closed choice survives asset navigation and resizing until the lightbox closes; without a choice the default follows the breakpoint. Editors remain mounted while collapsed. The sidebar uses shrinkable grid tracks, wraps long tags, scrolls vertically, and keeps actions reachable. Shell height stays within the viewport. Desktop minimum window dimensions remain unchanged.

A collapsed sidebar leaves one 44 px reopen button at the shell's top-right corner, inset 10 px from both edges. Mouse movement anywhere in the app reveals it, and three seconds of inactivity fades it without intercepting pointer input. Keyboard activity reveals it; keyboard focus and touch input keep it discoverable. Hidden panel content is inert and aria-hidden. Opening moves focus into the panel without scrolling the shell. Keyboard collapse returns focus to the trigger; mouse collapse focuses the viewer so the idle button can fade. Offscreen drawer transforms are clipped without creating a scrollable area. Delete confirmation and pending deletion prevent collapse. The confirmation remains inline at the bottom of the sidebar.

`useLightboxSidebar` owns this session-scoped choice and retains occupied space through the closing transition. `useLightboxActivity` owns the cleaned-up activity listeners and idle timer. Selection-scoped info/confirmation state remains in `useLightboxModalHandlers`.

## Summary-to-details selection lifecycle

The query cache stores `AssetSummary` objects verbatim. Selecting a tile builds a `SelectedAsset` view: `path` starts as the summary's `preview_path` (a valid source for GIF and video; null for ordinary images), `size_bytes` starts as null, and tags are empty. Kind, dimensions, duration, favorite state, media group, and thumbnail fields come from the summary. The media stage never guesses a source from the file name: until details arrive it renders a loading placeholder, and a failed detail read renders a failure notice instead of media. The empty tag list is never authoritative; while the complete tag base is unknown the tag input and chip removal are disabled. A null or rejected `getAssetDetails` result shows the loading error and Retry action; successful Retry, or a canonical mutation published by bulk, unlocks editing.

Every `selectAsset` call, including close, increments `selectionRequestRef`. A detail response may replace `selected` only when its captured request ID still equals the current request ID. Consequently, a slow response for A cannot replace B or reopen a closed lightbox. The request is not cancelled at the transport layer.

Successful current details are stored in a bounded LRU `Map<assetId, AssetDetails>` (256 entries, refreshed on read). Each entry carries the tag-state identity epoch at insertion time: an entry stamped with an older epoch is discarded on lookup, and starting a new asset-query session (`queryEpoch`) clears the whole map so reused IDs cannot serve rows from before a restore or rescan. Their `tags` field is overlaid from shell-owned `useAssetTagState`, which stores the latest complete canonical list plus that identity epoch and per-asset generation. A mutation exclusively acquires the asset before IPC begins, advances the generation, remains an active read barrier while pending, and advances again when it settles. A second lightbox/bulk writer cannot acquire the same asset and therefore sends no IPC. Details started before or during the write cannot publish. Only detail-complete records (real path and byte size) may enter the cache; a cached ID reselects without another details request. After the current asset's first successful details fetch, the hook calculates previous and next global indices with modulo arithmetic, resolves those summaries through `getAssetAtAsync`, and fetches their details into the same map. Prefetch is skipped for a one-item result and for an adjacent ID already in the details map.

The selection-derived editors are reset from every new `selected` object:

- tags remain a normalized `string[]`; whitespace and CSV delimiters are invalid inside a tag;
- a null group key becomes an empty string;
- a null group order becomes an empty string, otherwise it is stringified;
- closing clears all three editors and the selected global index.

## Global navigation and library synchronization

Selection records a global result index supplied by navigation or discovered by `getAssetIndex`. Arrow navigation uses the query's total count, not the currently materialized `assets` array. Previous is `(index - 1 + count) % count`; next is `(index + 1) % count`. Each key press synchronously advances a separate target index before page loading starts, so rapid presses accumulate rather than repeatedly calculating from the last committed selection. Each lookup captures a navigation request ID, and only the latest request may call `selectAsset`; direct selection, close, deletion, and identity reset invalidate pending navigation. `getAssetAtAsync` returns a cached page entry or loads the page containing the target, so navigation crosses page boundaries and wraps from first to last and last to first. Navigation is inert when the index is unknown, the result count is zero, or the target cannot be loaded.

Two guards keep navigation aligned with reality. When the resolved record for the target index is missing (`undefined`) or the page load rejects, the target index rolls back to its pre-navigation value, so the next key press retries the same position instead of skipping ahead. When a new query session starts while the lightbox is open, the selected record's global index is recomputed through `getAssetIndex`; if the record no longer appears in the loaded snapshot pages the index is invalidated and relative navigation becomes inert until a fresh selection supplies a new position.

`useSelectionState` also watches the library cache for the selected ID. When a matching summary changes, it merges that summary into the selected details while preserving the current full `path` and `size_bytes`. It preserves current tags when the summary has no tags, because query summaries do not carry tags. An equality check over every displayed field avoids a redundant state update. If no matching asset exists, the lightbox closes only when the materialized asset cache is completely empty; absence from one non-empty cache snapshot is not treated as deletion.

This synchronization makes the selected object and its editors follow local cache mutations. The mutation helpers additionally use ID-checked selected-state updaters, so a response started for A cannot overwrite B after the user navigates.

## Persistent edits and refresh rules

All lightbox mutations are backend-first: the action awaits its API command before updating frontend state. If the command rejects, local state is not intentionally changed.

| Operation | Backend command | Local update after success | Follow-up |
| --- | --- | --- | --- |
| Add/remove tags | `set_asset_tags` | Publish canonical tags to the shared coordinator, details cache, and still-matching selection | Best-effort known-tag refresh; restart the query when changed tags touch applied include/exclude filters |
| Toggle favorite | `set_asset_favorite` | Replace `is_favorite` in both places | Refresh the query only when removing a favorite while the applied favorites-only filter is active |
| Apply media group | `set_asset_media_group` | Patch loaded summaries, matching selection, and cached details | The selection controller starts a fresh query because grouping changes order and adjacency |
| Delete | `delete_asset` | Remove the ID from loaded pages and close the matching selection after a committed structured result | Start best-effort known-tag and query refreshes; refresh failure does not relabel the committed delete |

Tag chips are the draft model: tags are trimmed, lowercased, de-duplicated, and empty values are discarded. `useSelectionState` is the only owner of save serialization/coalescing; `useLightboxTagging` only manages input interaction and delegation. A replacement is guarded unless the shared complete tag base is known, and its exclusive coordinator mutation token is acquired before calling `set_asset_tags`. Lock contention retains the desired editor state and exposes Retry; when the winning external write settles, the add/remove intent is rebased onto its canonical tags before Retry. Save failure keeps unsaved chips visible and exposes its own Retry, while success publishes canonical response tags before patching the details cache and starting the best-effort known-tag refresh. Adding a draft or removing a chip immediately updates the editor and starts a save; merely typing a draft does not save. Known-tag suggestions exclude already-selected tags case-insensitively. A successful add clears and refocuses the input.

Media-group apply trims the key and converts an empty key to null. Empty order becomes null; any finite JavaScript number, including a decimal, is accepted; non-finite or unparseable input does nothing. The generate button uses `crypto.randomUUID()` when available, with a timestamp/random fallback. Applying does not mutate the editor controls directly; the selected-object update rehydrates them.

Delete runs under the scan/thumbnail lock. An existing source is journaled and staged before one database/revision transaction; a missing source explicitly removes stale metadata. The frontend inspects `DeleteAssetSummary`: missing source and post-commit cleanup staging are announced separately, while a pre-commit filesystem rejection remains in the confirmation dialog and states that metadata was preserved.

## Toolbar, sidebar, confirmation, and clipboard

On a wide viewport the sidebar occupies `clamp(18rem, 22vw, 22rem)` and the media uses the remaining width. It contains the action buttons (favorite, group copy, reset zoom, info, image fullscreen, delete, close), tagging/media-group editing, and an inline file-information section. Navigation remains keyboard-driven; there are no pointer navigation buttons. The sidebar keeps a pinned header and one-row action rail while the middle area scrolls.

The information section is toggleable and collapsed by default; its info button carries `aria-expanded`. The media-group editor follows it, with tagging directly below. Tag suggestions open below the input through a viewport-positioned portal, so sidebar overflow cannot clip them.

Below 768 px the drawer starts closed. A compact header above the media opens it or closes the lightbox; keeping these controls outside the media bounds makes them visible above the native GTK video surface. The drawer has a maximum width of 22 rem; its scrim and Escape close only the drawer, then return focus to its trigger. Changing the selected asset or entering fullscreen closes it. While delete confirmation is open, its Escape and submitting guards take priority over drawer dismissal.

Info and tagging are inline sections in the sidebar rather than popovers. There are no mutually exclusive popovers and no outside-pointer-dismiss behavior; selecting a new asset resets the delete-confirmation and submission state. The shell's click handler stops propagation so ordinary interaction does not close the backdrop.

Group copy is enabled only when the trimmed editor key is non-empty and `navigator.clipboard.writeText` exists. Success swaps the copy icon to a confirmed state for 1,600 ms. Selection changes, key edits, failures, and unmount clear confirmation or its timer.

Delete renders inline in the sidebar, not as a nested modal. Opening it clears its text and focuses the confirmation input on the next animation frame. Confirm is enabled only when trimmed input equals the localized confirmation word, case-insensitively. Submission is guarded against duplicates; close/cancel and both buttons are blocked while it is pending. While the confirmation is open the lightbox shortcut listener is disabled and Escape closes only the confirmation (restoring focus to the delete button), never the lightbox. A committed delete closes the selection; missing/cleanup outcomes are announced first. A rejected pre-commit delete keeps the confirmation open with an alert, and post-commit refresh failures are only best-effort follow-up failures.

## Media presentation

Image and GIF source paths pass through `toMediaSrc`.

- `image` and `gif` use the same non-draggable `<img>` stage. GIF animation is browser-native, and both kinds receive the image fit, zoom, and pan behavior.
- On a wide viewport the media stage occupies only the left shell column. `measureNativeVideoBounds` therefore reports a rect inside that column, so the native GTK surface cannot cover the sidebar's DOM controls. Opening the narrow drawer hides the video stage; its resize observer publishes zero bounds, which hides the native GTK surface until the drawer finishes closing. A windowed collapsed video reserves a 54 px right-edge area outside native bounds for the reopen button; its fade does not resize the video. Sidebar opening waits for acknowledged native bounds that leave the panel clear. Native GTK pointer motion is observed in capture phase without consuming input and forwards session-scoped `pointerActivity` events, throttled to at most ten per second, to the same frontend idle timer. `video` keeps Video.js 10 `VideoPlayer` and a hidden `MinimalVideoSkin` as the media-state bridge, attaches `MpvMediaAdapter` through public `useMediaAttach`, and renders no `<video>`. Visible controls belong to the native GTK surface. `MpvMediaComponent` opens an authorized native session by asset ID, sends adapter controls and localized native-control labels to Tauri, applies session-filtered backend events, publishes measured bounds, and closes the session on unmount.
- The backend validates the SQLite row, video kind, canonical regular file, assigned roots, and symlink containment before passing the canonical path to libmpv. One process-wide player uses `vo=libmpv`, `hwdec=auto-safe`, infinite looping, volume 100, and unmuted playback. GTK positions an input-pass-through `GLArea` above the WebView. The control bar is a direct `GtkOverlay` child sized only to the bottom 68 px of the video rectangle, so it accepts input without blocking the rest of the WebView. It contains play/pause, mute, time, seek, playback-rate, and fullscreen controls. The fitted dimensions describe the complete decoded-video footprint; no separate DOM control rail is reserved below it. The controls use a transparent-to-dark gradient only for contrast rather than a bordered panel. The non-fullscreen video dialog keeps at least 20 px from every window edge, and its shell owns the only visible border, radius, and shadow. The hidden Video.js skin remains the media-state adapter, while GTK receives authoritative playback state from the same session-checked libmpv events. This avoids relying on WebKitGTK alpha compositing, which cannot reliably place DOM controls over a sibling native widget. The render context must be ready before libmpv receives `loadfile`.
- libmpv metadata events supply duration and intrinsic dimensions. Before that, selected dimensions are used; otherwise the aspect-ratio fallback is `16 / 9`. `I18nProvider` follows the active i18next language. Picture-in-picture and remote playback are unavailable through the adapter.

Image/GIF load failures, native-session open failures, and backend video error events replace the failed media with a localized alert. Every asset ID/kind/path activation receives a new generation, so A→B→A retries A and a late callback from the first A activation cannot fail the new one. Open tokens reject superseded backend work; session IDs filter late events and commands. Original media is independent of thumbnail generation and thumbnail cache state.

## Image fit, zoom, and pan

`ResizeObserver` tracks the media viewport. Fit scale is `min(viewportWidth / intrinsicWidth, viewportHeight / intrinsicHeight)` and the `<img>` receives the resulting fitted pixel width and height. Missing dimensions produce fit scale 1 and no explicit display size. Natural image dimensions replace summary/details dimensions after load.

Zoom is a factor relative to the fitted size:

- minimum `1`, maximum `12`;
- discrete click and keyboard multiplier `1.25`;
- single click zooms in one step around the pointer;
- `+` or `=` zooms in and `-` zooms out around viewport center;
- `0` and the reset button restore factor 1 and pan `(0, 0)`;
- the double-click helper toggles between reset and `clamp(max(2, 1 / fitScale), 1, 12)`, intended to reach at least 2x or pixel-perfect scale.

Wheel zoom is cursor-anchored and prevents native scrolling. Delta mode 1 is converted with a 16 px line height; mode 2 uses viewport height; the resulting delta is clamped to `[-100, 100]`. The factor is `current * exp(-delta * 0.002)`, then clamped to `[1, 12]`.

Zoom preserves the source point under the pointer or viewport center. Pan is clamped independently per axis to half the overflow of `(fitted dimension * zoom) - viewport dimension`; an axis with no overflow remains centered. At factor 1, with a `0.001` epsilon in `clampPan`, pan is forced to zero. The DOM transform is `translate3d(xpx, ypx, 0) scale(factor)`.

Primary-button drag starts only above factor 1. Pointer capture is preferred; window pointer listeners provide a fallback and window blur ends the drag. Movement over 3 px marks the gesture as a drag, suppressing the following image click for 120 ms. Selection changes reset zoom, pan, dragging, click suppression, and intrinsic dimensions. Viewport changes re-clamp pan while zoomed.

Transform writes are coalesced to one `requestAnimationFrame`: callers update refs immediately, `requestImageTransform` refuses to queue a second frame, and the frame writes the transform once. The pending frame is cancelled on unmount. Resize measurement and tag-input focus also use their respective observer/animation-frame lifecycles.

## Fullscreen and keyboard behavior

For images and GIFs, fullscreen targets the lightbox shell. If another element is fullscreen, it is exited first. `fullscreenchange` derives `isFullscreen` from whether that exact shell is the fullscreen element; fullscreen CSS removes the shell's bounds, radius, border, and shadow. The sidebar remains part of the fullscreen shell, so its controls stay reachable. The fullscreen button and `F` toggle it.

For video, fullscreen delegates through the local `LightboxVideoPlayerHandle`, which calls `MpvMediaAdapter.requestFullscreen()` or `exitFullscreen()`. The adapter and the native GTK fullscreen button share the same checked Rust fullscreen operation; Rust toggles the Tauri window, records the state on the active session, and returns a `fullscreen` event. The stock Video.js fullscreen control and the rest of its DOM control rail are hidden. Native fullscreen removes backdrop padding and lightbox chrome, expands the shell to the window, and hides the entire sidebar while the native surface covers the window. `F` always reaches the native action, including while a GTK control owns focus, and `Escape` exits native fullscreen before it can close the lightbox. Closing or replacing a fullscreen video session restores the window first. Fullscreen does not overwrite the session sidebar choice; video fullscreen omits the panel and trigger and restores that choice on exit. Fullscreen API failures from frontend shortcuts are intentionally swallowed.

The window-level shortcuts while an asset is selected are:

| Key | Behavior |
| --- | --- |
| `Escape` | Exit image or native video fullscreen first; otherwise close the lightbox |
| `ArrowLeft` / `ArrowRight` | Previous/next global result with wrap |
| `F` | Toggle image-shell or video-player fullscreen |
| `0` | Reset image/GIF zoom |
| `+` / `=` | Zoom image/GIF in by 1.25 |
| `-` | Zoom image/GIF out by 1.25 |

While delete confirmation is open, the lightbox shortcut listener is disabled and the inline confirmation owns Escape. Otherwise, form controls keep their keys. Native video `F` and fullscreen `Escape` are handled by the WebView shortcut path or, when focus is inside the native overlay, by GTK. Other shortcuts are suppressed when the target is inside `[data-lightbox-video-player]`; image-only zoom keys do nothing for video.

## Backdrop-close and nested-overlay safety

Clicking the outer backdrop calls `tryCloseLightbox`; clicking the shell stops propagation. A live drag always blocks backdrop close. Zoom/pan gestures additionally set short time gates so the click synthesized after an interaction cannot close the modal:

- wheel or image click: 360 ms;
- double click: 420 ms;
- drag start: 500 ms;
- drag finish: 240 ms.

The inline delete confirmation is part of the sidebar, so while it is open a lightbox shortcut listener is disabled and Escape targets only the confirmation; its submitting guard prevents backdrop cancellation until deletion settles.

## Known limitations and maintenance hazards

- The bounded details caches (256-entry LRU, epoch- and session-stamped) still rely on the authoritative-tag overlay for ordinary favorite/group mutations; only identity resets and query-session restarts clear them wholesale. Full DB restore and library clear advance the shared identity epoch and clear selection/bulk detail caches, requests, and pending tag mutation state. Successful deletion creates a per-ID tombstone generation before refresh work, so an older detail response cannot repopulate a deleted or reused ID.
- Adjacent prefetch has no in-flight de-duplication or selection request generation. Rejections are caught and ignored. With exactly two results, previous and next resolve to the same index and can start duplicate details calls. Prefetch work may continue after selection or query changes, but a response cannot replace tags published by a newer mutation.
- A cached selection returns before adjacent prefetch, so revisiting an asset does not warm its new neighbors.
- Query changes recompute the selected index from loaded pages. If the selected asset is absent there, the lightbox remains open but relative navigation becomes inert until a fresh selection provides an index.
- Tag replacements serialize/coalesce drafts and expose saving, failure, and Retry state. Favorite and group writes also acquire per-asset mutation ownership, but provide no equivalent local pending/error UI; a click while the asset is busy sends no additional command.
- The double-click and legacy mouse-down handlers are returned by `useLightboxImageControls` and unit-tested directly, but `LightboxMediaStage` currently wires neither one. Rendered images therefore single-click through 1.25x steps; the pixel-perfect double-click toggle is not reachable from the modal.
- The lightbox is a named portal dialog registered with the shared layer manager. Focus restoration depends on the saved trigger still being connected; a virtualized gallery tile can disappear before close.
- Final staged-delete cleanup can fail after DB commit; the UI reports the recovery path and the durable journal retries cleanup at startup.
- Details and media failures have localized error UI; details also has Retry. Clipboard failure clears feedback, and some fullscreen shortcut failures are caught without an alert.

## Change checklist

When changing lightbox behavior:

1. Keep `AssetSummary` usable for immediate display and preserve the request-ID check around current detail replacement.
2. Decide explicitly whether a change must update the query page cache, selected details, editor state, details cache, known tags, result count/order, or query session.
3. Preserve backend-first mutation ordering and ID-check selected updates; add a pending/error policy when introducing a control that can race.
4. Use global indices and `getAssetAtAsync` for navigation; cover first/last wrap and unloaded page boundaries.
5. Reset selection-scoped transient state when selected ID changes and clean up timers, observers, listeners, pointer capture, and queued animation frames.
6. Keep form controls and native video targets isolated from global shortcuts, and define how nested overlays consume backdrop clicks and `Escape`.
7. For transform changes, test fit scale, min/max clamp, anchor preservation, pan bounds, resize re-clamping, drag fallback, and RAF coalescing.
8. Verify image, GIF, and video separately, including metadata fallback and both fullscreen implementations.
9. If details-cache behavior changes, add an invalidation contract for mutations, refresh/import/clear, deletion, query generation, and ID reuse.

## Tests

Current focused coverage is split across:

- `src/hooks/__tests__/useSelectionState.test.ts`: editor hydration, cache-to-selection synchronization, empty-library close, tag-mutation serialization/retry, pending-mutation detail barriers, restore/reset ID reuse, deletion tombstones, stale-detail protection, rapid Right/Right and Right/Left ordering, and delegation to mutation actions. It also covers query-position recomputation and failed navigation retries, but not every prefetch race or global wrap/unloaded-page combination.
- `src/components/lightbox/__tests__/LightboxMediaStage.test.tsx`: A→B→A failure reset and rejection of a late error callback from an older activation.
- `src/components/lightbox/__tests__/LightboxModal.test.tsx`: arrow navigation, native video opening/error handling, input suppression, tag add/remove/suggestions/focus, group apply, favorite, responsive sidebar/drawer layout, and inline delete confirmation.
- `src/components/lightbox/__tests__/MpvMediaAdapter.test.tsx`: Video.js adapter state, hidden DOM controls, waiting state, fullscreen delegation, native session lifecycle, and absence of an HTML video element.
- `src/components/lightbox/__tests__/LightboxModal.copy.test.tsx`: copy availability, clipboard write, confirmed state, and 1,600 ms reset.
- `src/components/lightbox/__tests__/LightboxDeleteConfirmDialog.test.tsx` and `hooks/__tests__/useLightboxModalHandlers.test.ts`: confirmation text, inline close/confirm callbacks, media-group parsing, and pending-delete guards.
- `src/components/lightbox/__tests__/useLightboxImageControls.test.ts`: keyboard navigation/zoom/fullscreen, wheel suppression, direct double-click zoom, drag fallback, and close timing.
- `src/components/lightbox/services/__tests__`: backend-first local updates and follow-up refresh rules for tags, favorite, group, and deletion. The `selectPrevious/Next` service tests cover legacy non-runtime helpers only.
- `src/components/app/services/__tests__/assetMutationService.test.ts`: immutable list/selection update helpers.
- `src/__tests__/api.test.ts`: API payload/source conversion plus single and bulk tag mutation result mapping. `src/__tests__/App.test.tsx` mocks `getAssetDetails`, but currently has no end-to-end lightbox selection/details/mutation case.

Minimum regression additions for lifecycle work should include deferred A/B details responses, close-before-response, cache revisit, two-item prefetch de-duplication, first/last wrap across unloaded pages, query refresh while open, mutation followed by cached revisit, rapid out-of-order mutations, inline-confirmation `Escape`, rendered double-click wiring, GIF transforms, video-target key ownership, and failed physical-file deletion reporting.
