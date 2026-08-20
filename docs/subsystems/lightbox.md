# Lightbox

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

The modal shell contains an absolute toolbar and a full-size media stage. The deletion confirmation is a separate fixed overlay rendered beside the shell inside the lightbox backdrop.

## Summary-to-details selection lifecycle

The query cache stores `AssetSummary` values converted to the frontend `Asset` shape. Until details arrive, `path` is `preview_path` or `file_name`, `size_bytes` is `0`, and `tags` is empty; GIF and video summaries include a full preview path while ordinary images do not. Kind, dimensions, duration, favorite state, media group, and thumbnail fields come from the summary. This provisional object is intentionally usable by the modal for media display, but its empty `tags` array is never authoritative. While the complete tag base is unknown the tag input and chip removal are disabled. A null or rejected `getAssetDetails` result shows a dedicated loading error and Retry action; successful Retry, or a canonical mutation published by bulk, unlocks editing.

Every `selectAsset` call, including close, increments `selectionRequestRef`. A detail response may replace `selected` only when its captured request ID still equals the current request ID. Consequently, a slow response for A cannot replace B or reopen a closed lightbox. The request is not cancelled at the transport layer.

Successful current details are stored in an unbounded, hook-local `Map<assetId, Asset>`. Their `tags` field is overlaid from shell-owned `useAssetTagState`, which stores the latest complete canonical list plus a global identity epoch and per-asset generation. A mutation exclusively acquires the asset before IPC begins, advances the generation, remains an active read barrier while pending, and advances again when it settles. A second lightbox/bulk writer cannot acquire the same asset and therefore sends no IPC. Details started before or during the write cannot publish. Selecting a cached ID uses it immediately and does not issue another details request. After the current asset's first successful details fetch, the hook calculates previous and next global indices with modulo arithmetic, resolves those summaries through `getAssetAtAsync`, and fetches their details into the same map. Prefetch is skipped for a one-item result and for an adjacent ID already in the details map.

The selection-derived editors are reset from every new `selected` object:

- tags remain a `string[]`, preserving internal whitespace inside each chip;
- a null group key becomes an empty string;
- a null group order becomes an empty string, otherwise it is stringified;
- closing clears all three editors and the selected global index.

## Global navigation and library synchronization

Selection records a global result index supplied by navigation or discovered by `getAssetIndex`. Arrow navigation uses the query's total count, not the currently materialized `assets` array. Previous is `(index - 1 + count) % count`; next is `(index + 1) % count`. `getAssetAtAsync` returns a cached page entry or loads the page containing the target, so navigation crosses page boundaries and wraps from first to last and last to first. Navigation is inert when the index is unknown, the result count is zero, or the target cannot be loaded.

`useSelectionState` also watches the library cache for the selected ID. When a matching summary changes, it merges that summary into the selected details while preserving the current full `path` and `size_bytes`. It preserves current tags when the summary has no tags, because query summaries do not carry tags. An equality check over every displayed field avoids a redundant state update. If no matching asset exists, the lightbox closes only when the materialized asset cache is completely empty; absence from one non-empty cache snapshot is not treated as deletion.

This synchronization makes the selected object and its editors follow local cache mutations. The mutation helpers additionally use ID-checked selected-state updaters, so a response started for A cannot overwrite B after the user navigates.

The old `selectNextLightboxAssetAction` and `selectPreviousLightboxAssetAction` helpers are not used by the controller path. They are non-wrapping loaded-array helpers and must not be treated as the current navigation contract.

## Persistent edits and refresh rules

All lightbox mutations are backend-first: the action awaits its API command before updating frontend state. If the command rejects, local state is not intentionally changed.

| Operation | Backend command | Local update after success | Follow-up |
| --- | --- | --- | --- |
| Add/remove tags | `set_asset_tags` | Replace tags on the cached asset, details cache, and still-matching selection | Start best-effort `refreshKnownTags()` |
| Toggle favorite | `set_asset_favorite` | Replace `is_favorite` in both places | Refresh the query only when removing a favorite while the applied favorites-only filter is active |
| Apply media group | `set_asset_media_group` | Replace group key and order in both places | No query or tag refresh |
| Delete | `delete_asset` | Remove the ID from loaded pages and close the matching selection | Await known-tag refresh, then full query refresh |

Tag chips are the draft model: tags are trimmed, lowercased, de-duplicated, and empty values are discarded. `useSelectionState` is the only owner of save serialization/coalescing; `useLightboxTagging` only manages input interaction and delegation. A replacement is guarded unless the shared complete tag base is known, and its exclusive coordinator mutation token is acquired before calling `set_asset_tags`. Lock contention retains the desired editor state and exposes Retry; when the winning external write settles, the add/remove intent is rebased onto its canonical tags before Retry. Save failure keeps unsaved chips visible and exposes its own Retry, while success publishes canonical response tags before patching the details cache and starting the best-effort known-tag refresh. Adding a draft or removing a chip immediately updates the editor and starts a save; merely typing a draft does not save. Known-tag suggestions exclude already-selected tags case-insensitively. Opening the tag panel focuses its input on the next animation frame, and a successful add clears and refocuses it.

Media-group apply trims the key and converts an empty key to null. Empty order becomes null; any finite JavaScript number, including a decimal, is accepted; non-finite or unparseable input does nothing. The generate button uses `crypto.randomUUID()` when available, with a timestamp/random fallback. Applying does not mutate the editor controls directly; the selected-object update rehydrates them.

The Rust mutation commands normalize again and bump the library revision. Delete runs under the scan/thumbnail lock, removes the database asset and bumps the revision first, then best-effort removes the original file and its thumbnail files. The frontend currently ignores `DeleteAssetSummary`, so a successful command closes the lightbox even when `removed_media_file` is false.

## Toolbar, popovers, clipboard, and confirmation

The toolbar exposes tagging/group editing, information, favorite, group copy, reset zoom, image fullscreen, delete, and close. It has no pointer navigation buttons; navigation is keyboard-driven. The info popover shows kind, formatted byte size, dimensions, duration, and full path.

Tag and info popovers are mutually exclusive. Toggling one closes the other. A shell-level pointer-down capture closes an open popover when the target lies outside that popover's container. Changing selected ID, opening delete confirmation, or entering fullscreen closes both; changing selected ID also resets confirmation/submission state. The shell's click handler stops propagation so ordinary interaction does not close the backdrop.

Group copy is enabled only when the trimmed editor key is non-empty and `navigator.clipboard.writeText` exists. Success swaps the copy icon to a confirmed state for 1,600 ms. Selection changes, key edits, failures, and unmount clear confirmation or its timer.

Delete opens a nested modal, closes both popovers, clears its text each time it closes, and focuses the confirmation input on the next animation frame. Confirm is enabled only when trimmed input equals the localized confirmation word, case-insensitively. Submission is guarded against duplicates; close/cancel and both buttons are blocked while it is pending. The confirmation backdrop stops propagation before closing itself, and its inner surface stops propagation without closing. A successful delete closes the selection through the action; the confirmation hook also closes its own overlay when the promise resolves.

## Media presentation

Image and GIF source paths pass through `toMediaSrc`.

- `image` and `gif` use the same non-draggable `<img>` stage. GIF animation is browser-native, and both kinds receive the image fit, zoom, and pan behavior.
- `video` requests `get_video_stream_url` by asset ID and passes the returned tokenized loopback HTTP URL directly to Vidstack `MediaPlayer`, `MediaProvider`, and `DefaultVideoLayout`. The backend revalidates the SQLite row and streams GET/HEAD byte ranges, so large files and seeking do not pass through IPC or a frontend Blob. Source changes remount the player. It starts automatically, unmuted at volume 1, loops, plays inline, and preloads metadata. `onCanPlay` retries `play()` if autoplay left it paused. Picture-in-picture, Cast, audio gain, and the chapter title slot are disabled or removed.
- Video metadata supplies intrinsic dimensions when the provider is a video provider. Before that, the selected dimensions are used; otherwise the aspect-ratio fallback is `16 / 9`.

Image/GIF load failures, stream-URL IPC failures, and Vidstack video errors replace the failed media with a localized alert. Video URL requests use a generation guard, so navigation clears the old source and late success or failure cannot affect the new selection. Native `MediaError` code, category, and message are logged for transport, decode/codec, unsupported-source, and aborted failures. The visible failure is scoped to asset ID, kind, and source path, so navigation or a corrected detail path immediately retries rendering. Original media is independent of thumbnail generation and thumbnail cache state.

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

For images and GIFs, fullscreen targets the lightbox shell. If another element is fullscreen, it is exited first. `fullscreenchange` derives `isFullscreen` from whether that exact shell is the fullscreen element; fullscreen CSS removes the shell's bounds, radius, border, and shadow. Toolbar and `F` can toggle it.

For video, fullscreen delegates to the Vidstack player. The custom toolbar fullscreen button is hidden, but `F` still toggles the player when the event is outside the player subtree and Vidstack's own controls remain available. Player fullscreen events update lightbox state. Entering either fullscreen mode closes popovers. Fullscreen API failures are intentionally swallowed.

The window-level shortcuts while an asset is selected are:

| Key | Behavior |
| --- | --- |
| `Escape` | Close the lightbox unless any document element is fullscreen; in fullscreen the browser/player handles exit first |
| `ArrowLeft` / `ArrowRight` | Previous/next global result with wrap |
| `F` | Toggle image-shell or video-player fullscreen |
| `0` | Reset image/GIF zoom |
| `+` / `=` | Zoom image/GIF in by 1.25 |
| `-` | Zoom image/GIF out by 1.25 |

After the fullscreen/Escape rule, shortcuts are suppressed when the event target is an `INPUT`, `TEXTAREA`, or contenteditable element. This lets tag suggestions and group/confirmation editors own arrows and typing. For video, all remaining lightbox shortcuts are also suppressed when the target is inside `[data-lightbox-video-player]`, leaving player keyboard behavior intact. Image-only zoom keys do nothing for video.

## Backdrop-close and nested-overlay safety

Clicking the outer backdrop calls `tryCloseLightbox`; clicking the shell stops propagation. A live drag always blocks backdrop close. Zoom/pan gestures additionally set short time gates so the click synthesized after an interaction cannot close the modal:

- wheel or image click: 360 ms;
- double click: 420 ms;
- drag start: 500 ms;
- drag finish: 240 ms.

The delete overlay stops propagation at both its backdrop and surface, so closing or operating the nested confirmation does not also close the lightbox. Its own submitting guard prevents backdrop cancellation until deletion settles.

## Current guarantees

- A late primary details response cannot replace a newer selection or reopen a closed lightbox.
- The current global index and modulo count provide cross-page, wrapping navigation when the index is known.
- Persistent local state changes occur only after the backend command succeeds.
- Mutation responses update selection only when its ID still matches the mutated asset.
- Editor state follows selected details/cache state rather than maintaining a second saved model.
- Popover, clipboard timeout, drag listener, resize observer, fullscreen listener, keyboard listener, and animation-frame cleanup are scoped to their owning hook/component.
- Input editing and Vidstack controls do not accidentally trigger navigation or image shortcuts.
- Delete requires explicit localized text confirmation and prevents duplicate submission.

## Known limitations and maintenance hazards

- `detailsCacheRef` remains unbounded and lacks general invalidation for favorite/group mutations, ordinary query refresh, and scan updates. Full DB restore and library clear advance the shared identity epoch and clear selection/bulk detail caches, requests, and pending tag mutation state. Successful deletion creates a per-ID tombstone generation before refresh work, so an older detail response cannot repopulate a deleted or reused ID.
- Adjacent prefetch has no in-flight de-duplication or selection request generation. Rejections are caught and ignored. With exactly two results, previous and next resolve to the same index and can start duplicate details calls. Prefetch work may continue after selection or query changes, but a response cannot replace tags published by a newer mutation.
- A cached selection returns before adjacent prefetch, so revisiting an asset does not warm its new neighbors.
- Global index is captured at selection time and is not recomputed when the query/filter/order changes. A non-empty cache that no longer contains the selected ID leaves the lightbox open with the old index. Concurrent unresolved navigation page loads are not ordered, so the last promise to resolve can win rather than the last key pressed.
- Tag replacements are serialized per asset, coalesce rapid drafts, and expose saving/failure/Retry state. Favorite and media-group writes still have no submitting state or mutation request guard, so rapid writes for those fields may resolve out of order.
- The double-click and legacy mouse-down handlers are returned by `useLightboxImageControls` and unit-tested directly, but `LightboxMediaStage` currently wires neither one. Rendered images therefore single-click through 1.25x steps; the pixel-perfect double-click toggle is not reachable from the modal.
- The lightbox shell itself has no dialog role, focus trap, or focus restoration. `Escape` is checked before form-control suppression, so pressing it in the nested delete input closes the entire lightbox when not fullscreen rather than only dismissing the confirmation.
- The frontend treats a successful delete command as complete without inspecting whether physical media-file removal succeeded. Database removal can therefore succeed while the original file remains.
- Fullscreen, clipboard, autoplay, and details failures are intentionally silent.

## Change checklist

When changing lightbox behavior:

1. Keep `AssetSummary` usable for immediate display and preserve the request-ID check around current detail replacement.
2. Decide explicitly whether a change must update the query page cache, selected details, editor state, details cache, known tags, result count/order, or query session.
3. Preserve backend-first mutation ordering and ID-check selected updates; add a pending/error policy when introducing a control that can race.
4. Use global indices and `getAssetAtAsync` for navigation; cover first/last wrap and unloaded page boundaries.
5. Reset selection-scoped transient state when selected ID changes and clean up timers, observers, listeners, pointer capture, and queued animation frames.
6. Keep form controls and Vidstack targets isolated from global shortcuts, and define how nested overlays consume backdrop clicks and `Escape`.
7. For transform changes, test fit scale, min/max clamp, anchor preservation, pan bounds, resize re-clamping, drag fallback, and RAF coalescing.
8. Verify image, GIF, and video separately, including metadata fallback and both fullscreen implementations.
9. If details-cache behavior changes, add an invalidation contract for mutations, refresh/import/clear, deletion, query generation, and ID reuse.

## Tests

Current focused coverage is split across:

- `src/hooks/__tests__/useSelectionState.test.ts`: editor hydration, cache-to-selection synchronization, empty-library close, tag-mutation serialization/retry, pending-mutation detail barriers, restore/reset ID reuse, deletion tombstones, stale-detail protection, and delegation to mutation actions. It does not currently cover every details/prefetch race, global wrap, or unloaded-page navigation.
- `src/components/lightbox/__tests__/LightboxModal.test.tsx`: arrow navigation, video loop, input suppression, tag add/remove/suggestions/focus, group apply, favorite, popover outside-click, and delete confirmation.
- `src/components/lightbox/__tests__/LightboxModal.copy.test.tsx`: copy availability, clipboard write, confirmed state, and 1,600 ms reset.
- `src/components/lightbox/__tests__/LightboxDeleteConfirmDialog.test.tsx` and `hooks/__tests__/useLightboxModalHandlers.test.ts`: confirmation text, nested close/confirm callbacks, popover reset/outside click, media-group parsing, and pending-delete guards.
- `src/components/lightbox/__tests__/useLightboxImageControls.test.ts`: keyboard navigation/zoom/fullscreen, wheel suppression, direct double-click zoom, drag fallback, and close timing.
- `src/components/lightbox/services/__tests__`: backend-first local updates and follow-up refresh rules for tags, favorite, group, and deletion. The `selectPrevious/Next` service tests cover legacy non-runtime helpers only.
- `src/components/app/services/__tests__/assetMutationService.test.ts`: immutable list/selection update helpers.
- `src/__tests__/api.test.ts`: API payload/source conversion plus single and bulk tag mutation result mapping. `src/__tests__/App.test.tsx` mocks `getAssetDetails`, but currently has no end-to-end lightbox selection/details/mutation case.

Minimum regression additions for lifecycle work should include deferred A/B details responses, close-before-response, cache revisit, two-item prefetch de-duplication, first/last wrap across unloaded pages, query refresh while open, mutation followed by cached revisit, rapid out-of-order mutations, nested-confirmation `Escape`, rendered double-click wiring, GIF transforms, video-target key ownership, and failed physical-file deletion reporting.
