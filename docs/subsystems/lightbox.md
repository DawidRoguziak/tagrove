# Lightbox

Implementation entry points: [selection lifecycle](../../src/features/lightbox/createLightbox.ts), [modal composition](../../src/features/lightbox/LightboxModal.tsx), [native session model](../../src/features/lightbox/video/createNativeVideoSession.ts), [native playback](../../src-tauri/src/services/video_player_service.rs), [GTK surface](../../src-tauri/src/video_surface.rs).

The lightbox is the selected-asset workspace layered over the gallery. It owns full-media presentation, per-asset editing, deletion confirmation, image transforms, video playback, fullscreen, and keyboard navigation. Selection and mutation state live above the modal in `createLightbox`; `LightboxModal` and its presentation models own transient interaction state.

Original images and GIFs load through the asset protocol with `crossorigin="anonymous"`,
matching thumbnail requests. WebKitGTK requires this CORS mode from the HTTP development
origin. The shared failure message says that the image could not be displayed; it does
not imply that the selected file is a GIF.

Related documentation:

- [Library query and gallery](library-query-and-gallery.md) describes query sessions, summary objects, the global index, and the paged frontend cache used by lightbox navigation.
- [Search, tags, and media groups](search-tags-and-media-groups.md) defines tag normalization, favorites, and group semantics shared with the lightbox editors.
- [IPC contract](../architecture/ipc-contract.md) covers the Tauri command boundary and camel-case request payloads.
- [Frontend architecture and UI conventions](../frontend/architecture-and-ui-conventions.md) defines model, component, platform, modal, and accessibility responsibilities.
- [Database](database.md) covers library revisions changed by lightbox mutations.
- [Thumbnails](thumbnails.md) covers gallery previews; the lightbox itself loads the original media URL rather than a thumbnail.

The tagging panel uses the shared `TagEditor` with chips and input inside one border. It retains the 200px scrolling limit, wraps long names, and keeps the focused input visible. The input has an accessible Add tag name without a separate visible label. Empty editable fields show the existing placeholder; details-loading status and retry controls remain outside the field. Autosave and draft ownership remain in the lightbox model.

## Frontend owners

`src/features/lightbox/createLightbox.ts` owns selection, position recovery, tag-save intents
and group drafts at application lifetime. It consumes the shared library's bounded details
cache and mutation coordinator. Rapid navigation advances a synchronous global target;
query refresh resolves the current ID in the new session without walking intervening pages.
A changed metadata generation causes a current detail read to wait for writes and refetch.
Identity reset closes the selection and discards retained save intents.

`LightboxModal.tsx` owns the mounted image/GIF/video presentation, inline confirmation, tag input,
clipboard feedback, sidebar choice and activity timer. It uses the shared tag editor and
layer manager. The presence owner retains all displayed model values during exit, while
logical close releases keyboard and background locks. Navigation retains the shell and
opening animation; closing and reopening starts a fresh presentation owner. The application
captures the gallery focus target before loading the lazy view.

`image/createImageControls.ts` uses the pure fit, zoom and pan calculations in `lightboxPanZoomService.ts`. It
measures the viewport, coalesces transform writes per frame, and releases pointer capture,
observers, listeners and frames with its owner. Metadata-only updates retain image/GIF DOM
and zoom. `video/createNativeVideoSession.ts` owns native playback through the existing platform
wrappers. Its session identity changes only for asset, activation or container changes;
translated labels and callbacks update without reopening. It buffers the latest early event
per type, accepts the returned session ID, cancels pending opens and closes late sessions
when their presentation owner retires. Bounds requests serialize and retain the latest pending
measurement. The sidebar waits for acknowledged clearance; narrow drawers hide the media
column through their exit before restoring trigger focus. Native fullscreen keeps the same
player node and session, removes the top reservation, and hides DOM action panels.

Logical close disposes native playback before the retained modal fades. The player publishes
explicit string values for boolean snapshot data attributes, preserving their reference
contract. Native controls, persistence and decoding remain owned by the Rust/GTK backend.

## Ownership and composition

`createAppRuntime` creates `createLibrary` and supplies it to `createLightbox`. The model consumes query accessors, shared details/tags and mutation commands, and is exposed as `runtime.lightbox`. `App` activates the lazy `LightboxModal` on first selection and keeps its presence owner mounted so closing fades can finish.

The runtime path is:

1. A gallery tile calls `runtime.lightbox.select(summaryAsset, index)`.
2. `createLightbox` sets its current selection synchronously and publishes reactive state so the modal opens without waiting for IPC.
3. It loads through `library.details.load(asset.id)` and replaces the summary with `AssetDetails` if that request is still current. Until a complete tag list is available from details or the shared authoritative tag state, tag editing is disabled.
4. `LightboxModal` delegates persistent edits back to `createLightbox`; local models and components manage panels, confirmation, clipboard feedback, media transforms, fullscreen, and shortcuts. Detail-load failure and tag-save failure are displayed and retried independently.

At widths of at least 768 px the modal initially shows the media stage beside the action sidebar. Below 768 px the sidebar starts as a closed right drawer. Both layouts allow the sidebar to slide off the right edge in 200 ms, with reduced motion respected. The covered media controls and action island are inert while the drawer is open. An explicit open/closed choice survives asset navigation and resizing until the lightbox closes; without a choice the default follows the breakpoint. Editors remain mounted while collapsed. The sidebar uses shrinkable grid tracks, wraps long tags, scrolls vertically, and keeps actions reachable. Shell height stays within the viewport. Desktop minimum window dimensions remain unchanged.

An open sidebar keeps Close preview beside the collapse arrow in its header.
A collapsed sidebar leaves a reopen button floating at the media column's top
right, alongside Close preview. Images and GIFs sit underneath these controls. Windowed
native video reserves 44px above the surface so GTK cannot cover either button. This
space uses the media background without a divider; video fullscreen removes it and hides
the controls. The filename remains in the dialog's accessible name and sidebar information. The sidebar remains mounted, inert and aria-hidden while hidden. Opening
moves focus into the panel after GTK acknowledges bounds that leave it clear. Keyboard
collapse returns focus to the trigger. In a narrow video layout, restoration waits for
the closing transition to reveal the media column that contains the trigger. Delete confirmation and pending deletion prevent
collapse. `createLightboxSidebar` in `createLightboxPresentation.ts` retains occupied space through
the closing transition. `LightboxModal` owns selection-scoped information and confirmation state.

Group drafts live in `createLightbox`. Selected-ID changes initialize fields; canonical arrivals update untouched fields and preserve dirty ones. The same model owns asynchronous global-index navigation and favorite/group pending state; `library.mutations` owns persistence. Favorite, group and tag writes for one asset share the per-asset write queue, so a write behind another pending write waits instead of failing. A genuine backend rejection still shows the existing failure UI.

Complete detail responses and adjacent prefetches must pass metadata-generation validation. An old favorite, group, path, or tag snapshot cannot enter the details cache by overlaying only its tags. Current selection reads wait for mutation settlement and re-fetch when their generation changed. Detail cache entries carry generation stamps as well as the identity epoch.

## Summary-to-details selection lifecycle

The query cache stores `AssetSummary` objects verbatim. Selecting a tile builds a `SelectedAsset` view: `path` starts as the summary's `preview_path` (a valid source for GIF and video; null for ordinary images), `size_bytes` starts as null, and tags are empty. Kind, dimensions, duration, favorite state, media group, and thumbnail fields come from the summary. The media stage never guesses a source from the file name. It renders a loading placeholder while the view has no verified path, and a failed detail read then renders a failure notice. GIF/video summaries already carry a verified preview path, so the lightbox can present that source before complete details arrive; a later detail failure appears alongside the media. The empty tag list is never authoritative; while the complete tag base is unknown the tag input and chip removal are disabled. A null or rejected `getAssetDetails` result shows the loading error and Retry action; successful Retry, or a canonical mutation published by bulk, unlocks editing. A range-selection write (`enqueueLibraryMutation`) drops every cached tag list when it starts and again when it ends, including one read while it ran. When the selected asset loses its complete tag list while its details are `ready`, `createLightbox` shows the tag-loading notice, waits until no metadata write is admitted, and re-reads details once, so editing unlocks with the post-write tags. A save queued behind that write fails because its base was dropped; it keeps its chips and its Retry rebases onto the re-read tags. The same holds for an edit coalesced behind a pending save when a range write is admitted as that save's turn is released, before its promise settles: the edit was never sent, so it shows the failed state with Retry instead of an idle editor that looks saved, also when the lightbox is reopened on that asset later.

Every `select` call, including close, increments `selectionRequest`. A detail response may replace `selected` only when its captured request ID still equals the current request ID. Consequently, a slow response for A cannot replace B or reopen a closed lightbox. The request is not cancelled at the transport layer.

Successful current details are stored in the library-owned `createDetailsCache`, one 256-entry LRU shared with bulk. Pending reads for the same ID share one promise. Each entry carries the tag-state identity epoch at insertion time: an entry stamped with an older epoch is discarded on lookup, and starting a new asset-query session (`queryEpoch`) clears the whole map so reused IDs cannot serve rows from before a restore or rescan. Their `tags` field is overlaid from shell-owned `createAssetTagState`, which stores the latest complete canonical list plus that identity epoch and per-asset generation. A mutation exclusively acquires the asset before IPC begins, advances the generation, remains an active read barrier while pending, and advances again when it settles. A second lightbox/bulk writer cannot acquire the same asset and therefore sends no IPC. Details started before or during the write cannot publish. Only detail-complete records (real path and byte size) may enter the cache; a cached ID reselects without another details request. After a successful current details fetch or a cached selection, when the selected index is known, the model calculates distinct previous and next global indices with modulo arithmetic, resolves those summaries through `getAssetAtAsync`, and fetches their details into the same map. Prefetch is skipped for a one-item result and for an adjacent ID already in the details map.

Selected-ID changes initialize group drafts. Later canonical arrivals update untouched fields and preserve dirty group drafts; tag intents retain unsaved edits:

- tags remain a normalized `string[]`; whitespace and CSV delimiters are invalid inside a tag;
- a null group key becomes an empty string;
- a null group order becomes an empty string, otherwise it is stringified;
- closing clears all three editors and the selected global index.

## Global navigation and library synchronization

Selection records a global result index supplied by navigation or discovered by `getAssetIndex`. Arrow navigation uses the query's total count, not the currently materialized `assets` array. Previous is `(index - 1 + count) % count`; next is `(index + 1) % count`. Each key press synchronously advances a separate target index before page loading starts, so rapid presses accumulate rather than repeatedly calculating from the last committed selection. Each lookup captures a navigation request ID, and only the latest request may call `select` (`createLightbox.ts`); direct selection, close, deletion, and identity reset invalidate pending navigation. `getAssetAtAsync` returns a cached page entry or loads the page containing the target, so navigation crosses page boundaries and wraps from first to last and last to first. Navigation is inert when the index is unknown or the result count is zero.

When a target page fails or returns no record, navigation rolls its pending index back to the committed selected index (not to an earlier press's uncommitted target), retains the failed target, and shows the localized navigation failure with Retry. This includes a page blocked by an earlier adjacent prefetch failure. Retry clears only that target page's failure and loads it again in the existing query session; another failure remains visible. Cached Previous/Next stays usable. New navigation, direct selection, close, query refresh and disposal retire the retry intent or its response. Position-lookup recovery still refreshes the query and resolves the selected ID. A query refresh invalidates pending navigation immediately and shows a localized “Finding this photo in the results…” notice. Once the new session is ready, selection first checks its sparse cache, then resolves an uncached photo through `getAssetPosition`. The backend searches its ordered ID snapshot; the frontend does not fetch every intervening page.

A resolved index enables Next/Previous in the new order. A missing photo stays open with a notice explaining that it no longer belongs to the results, including when the query has zero matches. A failed lookup or refresh shows Retry, which refreshes the query and resolves the position again. Keys do not navigate while the position is unresolved. Another refresh, selection, close, or unmount invalidates the outstanding position response. A query that no longer contains the selected ID leaves the lightbox open with missing-position feedback.

`createLightbox` also watches the library cache for the selected ID. When a matching summary changes, it merges that summary into the selected details while preserving the current full `path` and `size_bytes`. It preserves current tags when the summary has no tags, because query summaries do not carry tags. An equality check over every displayed field avoids a redundant state update. Absence from cached pages does not close the lightbox. A committed deletion or library identity reset explicitly closes the affected selection.

This synchronization makes the selected object and its editors follow local cache mutations. The mutation helpers additionally use ID-checked selected-state updaters, so a response started for A cannot overwrite B after the user navigates.

## Persistent edits and refresh rules

All lightbox mutations are backend-first: the action awaits its API command before updating frontend state. If the command rejects, local state is not intentionally changed.

| Operation | Backend command | Local update after success | Follow-up |
| --- | --- | --- | --- |
| Add/remove tags | `set_asset_tags` | Publish canonical tags to the shared coordinator, details cache, and still-matching selection | Best-effort mutation-aware known-tag refresh; apply the returned query impact to the filter active at commit |
| Toggle favorite | `set_asset_favorite` | Replace `is_favorite` in both places | Refresh the query only when removing a favorite while the applied favorites-only filter is active |
| Apply media group | `set_asset_media_group` | Patch loaded summaries, matching selection, and cached details | The selection controller starts a fresh query because grouping changes order and adjacency |
| Delete | `delete_asset` | Remove the ID from loaded pages and close the matching selection after a committed structured result | Start best-effort known-tag and query refreshes; refresh failure does not relabel the committed delete |

Accepted tag saves use the shell's shared impact predicate: `none` preserves the query, `all` refreshes, and `tags` refreshes only when changed names intersect included/excluded names or an exact-count filter sees a count change. Same-count replacements preserve count-filter sessions. Obsolete and failed saves do not trigger success-driven refreshes. Stable callbacks read the latest applied filter after pending writes finish.

Tag chips are the draft model: tags are trimmed, lowercased, de-duplicated, and empty values are discarded. `createLightbox` is the only owner of save serialization/coalescing; the mounted tag panel and `TagEditor` manage input interaction and delegation. A replacement is guarded unless the shared complete tag base is known, and its exclusive coordinator mutation token is acquired from the per-asset write queue before calling `set_asset_tags`. A save queued behind another writer stays pending; when it starts, an add/remove intent whose base changed meanwhile is rebased onto the canonical tags and the merged list is sent, without Retry. An invalid tag (whitespace, comma, semicolon or control character) keeps the draft and shows the shared tag-validation alert; nothing is saved. Save failure keeps unsaved chips visible and exposes its own Retry, while success publishes canonical response tags before patching the details cache and passing `vocabulary_changed` to the best-effort known-tag refresh callback. A current vocabulary skips unchanged saves; a failed refresh remains dirty and retries on the next accepted save. Adding a draft or removing a chip immediately updates the editor and starts a save; merely typing a draft does not save. Known-tag suggestions exclude already-selected tags case-insensitively. A successful add clears and refocuses the input.

Media-group apply trims the key and converts an empty key to null. The order uses a text field with a numeric keyboard hint and no native spinner arrows. Empty order becomes null. Nonempty edits must contain only digits and represent a positive safe integer, at most `9007199254740991`. Leading zeros are accepted and saved as the numeric value. Invalid typing or paste is rejected as a whole change without stripping characters. The field and Apply handler share validation; invalid preloaded drafts disable Apply and cannot be submitted. Existing stored orders remain visible and unchanged until explicitly edited or cleared. The generate button uses `crypto.randomUUID()` when available, with a timestamp/random fallback. Applying does not mutate the editor controls directly; the selected-object update rehydrates them.

Delete runs under the scan/thumbnail lock but never waits for a scan: when a scan or another combined workflow holds the scan lock, `delete_asset` rejects at once with the backend text `i18n:common.libraryBusy`, nothing is staged or written, and the confirmation shows the translated message with Cancel, Close and navigation usable; the user retries after the scan ([ADR 0022](../adr/0022-lightbox-delete-fails-fast-during-a-scan.md)). The pending state below therefore lasts only for the delete itself. An existing source is journaled and staged before one database/revision transaction; a missing source explicitly removes stale metadata. The frontend inspects `DeleteAssetSummary`: missing source and post-commit cleanup staging are announced separately, while a pre-commit filesystem rejection remains in the confirmation dialog and states that metadata was preserved.

Favorite pending ownership records the asset ID, identity epoch and number of queued
favorite writes. Closing and reopening within that epoch preserves pending state, which ends
when the last queued favorite write settles. A repeated toggle queues behind the pending one
and reverses its requested value. The heart button stays disabled during its own write as a
double-click guard. A reused ID in a new epoch is available for a new write, and an old
completion cannot publish state or release the newer operation.
After a favorite write succeeds, a Favorites-only refresh runs independently. Its failure
exposes query/navigation recovery while the saved favorite stays changed and the save-error
indicator stays clear. Retrying navigation refreshes the query without another favorite write.
A genuine write rejection still uses the favorite save-error indicator.

A delayed metadata save restores captured full details only within the same details-cache
generation. After Settings rename/rescan refreshes the query, reopening fetches current
path and size while retaining accepted canonical tags.

## Visual treatment

The media frame and inspector use the Obsidian Grove tokens in both themes. The fixed
340px inspector contains group editing, autosaved tags, expandable details and the inline
delete confirmation. Paths and metadata use bundled JetBrains Mono. Only available
size, dimension and duration values are shown.

GTK playback controls remain dark in both themes. Their buttons use 6px corners and
`#10B981` for seek progress and focus. Native session logic, gradient and control bounds
are unchanged.

## Toolbar, sidebar, confirmation, and clipboard

The media column contains the viewport and a centered action island below it, with
reopen and close controls floating at the top right only while the sidebar is collapsed.
Close preview moves into the open sidebar header and is disabled during pending deletion.
Favorite, reset zoom, fullscreen and delete
remain reachable with the inspector collapsed. The island wraps in narrow windows and
stays outside native GTK video bounds. Reset is disabled for video. Both native video and
image fullscreen use the existing fullscreen action. Image fullscreen retains the chrome;
native fullscreen hides it and preserves the prior sidebar choice when returning.

`createLightboxActivity.visible` controls the floating buttons and bottom action row. After
three seconds without activity they fade to zero opacity over 200 ms and stop receiving
pointer input. Reduced motion makes the opacity change immediate. Both stay mounted and
retain their layout space, including the native video's top clearance, so fading does not
resize media. The open sidebar and its header remain visible.

Hovering anywhere inside the bottom action island, including gaps and metadata, keeps
it visible and clickable. Surrounding row space does not hold it open. Top controls
still follow the inactivity timer. Leaving or cancelling the pointer restarts the normal
three-second countdown. Narrow-sidebar coverage and unmounting clear the hover state.

Actual pointer movement, pointer presses, wheel, scroll, keyboard input, and native-video
activity events reveal controls and restart the timer. Repeated movement events at unchanged
coordinates do not reset it. The first action also performs its normal function. Keyboard
input reveals controls before focus navigation; focus alone does not extend visibility.
Touch devices keep controls visible. Closing removes activity listeners and cancels the timer.

Information starts collapsed and its toggle lives only in the sidebar. Group editing precedes tagging and details. Suggestions
use the existing viewport portal. Navigation remains keyboard-driven across global results.

Below 768px the sidebar starts as a closed drawer, with width `min(340px, 100%)`. Scrim
and Escape close the drawer first. The covered media controls and action island are inert while the drawer is open. An explicit open/closed choice survives asset navigation
and resizing until the lightbox closes. Native fullscreen temporarily hides the drawer;
it does not discard that choice. Editors stay mounted while collapsed.

The group input row contains the input, Copy group name, and Generate UUID, in that order,
with 32px icon buttons. Group copy requires a nonempty trimmed editor key and clipboard availability. Success
shows confirmation for 1,600ms; selection/key changes, failure and unmount reset it.

Delete opens the panel and its existing inline confirmation. The input receives focus only
after the panel becomes available, including GTK bounds acknowledgement, and only once per
open, so a same-asset metadata write does not pull focus back from the tag input. Cancel clears
confirmation and restores focus to the bottom delete button on desktop or the drawer collapse control on narrow windows. Confirm is enabled when the typed text matches the localized
confirmation word (`lightbox.deleteConfirm.confirmWord`) after trimming, NFC normalization and
`toLocaleLowerCase` in the active language, so `YES`, `yes` and `Yes` (or `TAK`, `ДА`, `SÍ`) all
match while accents stay significant. The comparison deliberately avoids `localeCompare`, whose
base sensitivity does not fold case in WebKitGTK. The duplicate-submit guard, locked pending
state and error handling are unchanged.
The error text is backend display text and passes through `translateBackendText`
(`src/i18n/backendText.ts`) when it is rendered, so an `i18n:` key such as the busy-library
rejection follows the current language.
Escape dismisses confirmation before the drawer or lightbox, wherever focus is inside the
lightbox: the confirmation's own handler takes it inside the section, and otherwise the
lightbox layer's `onEscape` dismisses the open confirmation instead of the drawer or preview
(the native GTK `escape` intent does the same). While a delete is pending, Escape is ignored. A
committed delete closes the selection; a rejected pre-commit delete retains the confirmation and
error. Tag saves, favorite toggles and other metadata writes on the same asset republish the
selection under a new object identity but keep the confirmation, its typed word and the
Information panel open, because `LightboxContent` resets that state only when the selected ID
changes.

## Opening and closing presence

`App` activates the lazy `LightboxModal` on first selection and keeps that presence
owner mounted. It uses the same `createModalPresence` lifecycle as `UiModal`: a 75 ms
opacity entrance, an open state, a 50 ms opacity exit, then hidden. Local
`--modal-entrance` and `--modal-exit` tokens leave other animations unchanged. The backdrop
and DOM controls fade together. Image/GIF pixels also fade; native video pixels and
GTK controls retain their current startup and shutdown behavior. Reduced motion
skips both animations and the closing delay.

Clearing selection releases the layer manager and keyboard shortcuts immediately.
It also unmounts the native video player immediately, cancelling pending opens and
closing the session. The closing frame retains the displayed asset, editor props,
zoom/pan and shell layout. Its content is inert and hidden from accessibility APIs;
the backdrop intercepts pointer input until removal. Image fullscreen shells fade
in the browser top layer without resizing. Closing a native fullscreen video still
restores the window through the existing native session cleanup.

Navigation changes the asset inside the same open presence owner and never restarts
the fade. Closing and reopening cancels stale animation completion and fallback
timers, uses the new selection, and resets the lightbox session's local editor and
sidebar state. After the exit, the retained content unmounts and its references clear.

## Media presentation

Image and GIF source paths pass through `toMediaSrc`.

- `image` and `gif` use the same non-draggable `<img>` stage. GIF animation is browser-native, and both kinds receive the image fit, zoom, and pan behavior.
- The `<img>` is keyed by the asset activation generation. Changing asset ID, kind, or path mounts a fresh image, including on revisits, to prevent accelerated GIF timing after navigation in WebKit. Metadata edits, zoom, and unrelated renders keep the same image element. Only the image is replaced; the stage, sidebar, and modal presence persist.
- On a wide viewport the media stage occupies only the left shell column. `measureNativeVideoBounds` therefore reports a rect inside that column, so the native GTK surface cannot cover the sidebar's DOM controls. Opening the narrow drawer hides the video stage; its resize observer publishes zero bounds, which hides the native GTK surface until the drawer finishes closing. The windowed header and action island occupy rows above and below native bounds, including when the sidebar is collapsed. Sidebar opening waits for acknowledged native bounds that leave the panel clear. Besides the player container, the session observes the media stage as its layout root: the player is centered in the stage, so a sidebar toggle can move a height-limited video by half the 340 px column without changing the player's size, and ResizeObserver never reports such a position-only move on the container itself. A stage resize therefore republishes bounds, which acknowledges the reopened panel and keeps the GTK surface under the DOM player after the panel collapses. Native GTK pointer motion is observed in capture phase without consuming input and forwards session-scoped `pointerActivity` events, throttled to at most ten per second, to the same frontend idle timer. `createNativeVideoSession` reserves an open request before resolving an authorized video by asset ID, accepts events for its returned session, publishes measured bounds, and cancels pending opens and closes playback on unmount. Solid renders a plain container and no `<video>` or hidden control rail. Resize work keeps one outstanding request and only the latest pending measurement. Labels update through a separate command without restarting playback.
- The backend validates the SQLite row, video kind, canonical regular file, assigned roots, and symlink containment before passing the canonical path to libmpv. One process-wide player uses `vo=libmpv`, `hwdec=auto-safe`, infinite looping, `video-timing-offset=0`, and `audio-fallback-to-null=yes`; the last two are explained under render scheduling and audio output below. Its remaining options keep it on that one file and are explained under reference blocking below. Each video autoplays from the beginning; volume, mute, and speed survive navigation until app exit. Startup defaults are volume 100, unmuted, and speed 1. GTK positions an input-pass-through `GLArea` above the WebView. The controls are a direct `GtkOverlay` child sized only to the bottom 88 px of the video rectangle, clamped to the video height for shorter surfaces, so they accept input without blocking the rest of the WebView. Both rows overlay the picture in windowed and fullscreen modes. A full-width seek bar sits above a button row with play/pause, mute, a 0–100% volume slider, and elapsed / duration time on the left, then flexible space before playback-rate and fullscreen controls on the right. Raising the volume unmutes playback; mute preserves the selected level. Unmuting at zero restores 100%. The volume slider supports pointer dragging and keyboard increments, and snapshot updates do not move its thumb during a drag. The fitted dimensions describe the complete decoded-video footprint; no separate DOM control rail is reserved below it. The controls use a transparent-to-dark gradient only for contrast rather than a bordered panel. The non-fullscreen video dialog keeps at least 20 px from every window edge, and its shell owns the only visible border, radius, and shadow. GTK receives authoritative playback snapshots from the same session as the Solid frontend. A single worker executes libmpv commands and consumes events. Initial pause/load commands are submitted asynchronously through the session event client; completion errors are consumed there, so initialization does not make synchronous libmpv calls while holding the mutex shared with GTK. The worker blocks on its command channel when there is no session. GTK rendering wakes from the existing bounded callback channel rather than a permanent 8 ms timer, and that wake-up runs at idle priority (render scheduling below); the controls timer is suspended without an active surface. Replacing a file stops the previous decoder and retires its event client before creating the next client. Paused, seeking, and buffering are independent properties. GTK callbacks enqueue commands without blocking the render thread. Seeking stays disabled until duration is known, and progress updates do not move the thumb during a drag. This avoids relying on WebKitGTK alpha compositing, which cannot reliably place DOM controls over a sibling native widget. The render context must be ready before libmpv receives `loadfile`.
- libmpv metadata events supply duration and intrinsic dimensions. Before that, selected dimensions are used; otherwise the aspect-ratio fallback is `16 / 9`. Native control labels follow the active i18next language. Picture-in-picture and remote playback are unavailable.

Image/GIF load failures, native-session open failures, and backend video error events replace the failed media with a localized alert. Every asset ID/kind/path activation receives a new generation, so A→B→A retries A and a late callback from the first A activation cannot fail the new one. Open tokens reject superseded backend work; session IDs filter late events and commands. Original media is independent of thumbnail generation and thumbnail cache state. Video errors expose Retry, which starts a fresh activation. Recoverable control errors leave playback running and show a dismissible localized message above the native bounds. Fatal decoder or rendering errors stop playback and hide the native surface. Bounds can update only an active surface; they cannot reactivate a closed session.

When the current OpenGL renderer identifies itself as llvmpipe, render-context initialization selects libmpv's `gpu-dumb-mode=yes`. This uses basic scaling and disables advanced shader processing. The mpv 0.41/Mesa 26.1 software path produced black video in package tests; the basic path passed decoded-frame, seek, fullscreen and audio checks. Hardware contexts retain `auto`, including after context recreation. GPU X11 and Wayland playback use the regular rendering path.

Render scheduling: the `GLArea` render handler runs on the GTK main thread, the thread that also serves the WebView and Tauri IPC, so it never waits for libmpv's frame timing ([ADR 0012](../adr/0012-non-blocking-native-video-render.md)).

- Every `mpv_render_context_render` call passes `MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME=0`. With libmpv's default the call waits until the frame's display time. `video-timing-offset=0` makes libmpv announce a frame at its display time, not up to 50 ms ahead, so frame timing does not depend on that wait.
- The task that turns libmpv's update callback into `queue_render` runs at GLib `DEFAULT_IDLE` priority. GTK paints at a higher priority, and wry delivers custom-protocol responses that a worker thread produces, which includes every async IPC response, from `DEFAULT_IDLE` sources. A wake-up at default priority keeps a redraw pending for as long as frames arrive, and those responses then never run. That was measured under Xvfb, where a timer drives GTK's frame clock; a compositor-paced clock was not measured. At `DEFAULT_IDLE` each redraw is preceded by a drain of the pending responses.

Before this change, async IPC was answered only when playback paused or looped: up to 8 seconds late with the 8-second E2E fixtures, and not at all during a long video, which froze the WebView. Under Xvfb with llvmpipe the main thread spent 84 to 88% of its time inside the render call. Audio made no difference: the freeze was the same without an audio track, with PipeWire reachable, with no reachable audio server, and with `ao=null`. When the main loop is busy, video frames now wait for it, and libmpv drops a frame that the surface has not taken within 200 ms. Pacing and A/V sync on a hardware compositor were not measured.

Reference blocking: the media kind comes from the file extension, so a file named `*.mp4` can contain a playlist, a manifest or a script. The player opens the authorized file and nothing that its content names ([ADR 0028](../adr/0028-media-tools-open-only-the-file-they-are-given.md)):

- `access-references=no` refuses playlist (m3u, pls, HLS), EDL, CUE and archive content, and every file or URL that a libavformat demuxer asks libmpv to open, such as the representations of a DASH manifest.
- `demuxer-lavf-o=protocol_whitelist=data` leaves no usable protocol to the demuxers that open their references themselves (`concat` scripts, SDP descriptions). libmpv reads the opened file through its own stream layer, so playback of that file needs no libavformat protocol.
- `ordered-chapters=no` turns off Matroska segment linking, which scans the folder for sibling files.
- `ytdl=no` and `load-scripts=no` load neither the hook that starts `yt-dlp`/`youtube-dl` from `PATH` nor any script from a configuration folder. A libmpv built without script support, as in the Flatpak, does not have these options; the player skips an option that libmpv reports as unknown.
- `config=no` and `load-unsafe-playlists=no` state libmpv's embedding defaults: no user mpv configuration is read.

A file with such content ends in the video error state with Retry, like a corrupt file, and the player stays usable. libmpv still loads sidecar subtitles and cover art from the video's own folder. Ordered-chapter Matroska sets, CUE and EDL files and archives named as videos do not play.

Audio output: libmpv probes its audio outputs when a file with an audio track starts. If none can be opened, for example because no PipeWire or PulseAudio server is reachable, `audio-fallback-to-null=yes` selects libmpv's null output. The video then plays without sound, the audio track stays selected, volume and mute keep their values, and no error event is sent. The player goes idle between files, which closes the output, so the next opened video probes again. The UI does not indicate that playback is silent.

Playback snapshots and decoder-stop polling read libmpv flags through a private `c_int` adapter. `MPV_FORMAT_FLAG` writes a C integer; the installed libmpv2 6 wrapper's generic Rust `bool` getter allocates only one byte. The adapter converts the initialized integer only after a successful read and preserves each caller's error default. Dependencies and public payloads are unchanged.

The native speed menu offers 0.5, 0.75, 1, 1.25, 1.5 and 2×. Selection queues a worker command; the authoritative playback snapshot updates the speed label. A GTK capture gesture claims the entire outside click, including release, over the picture, native controls, sidebar and backdrop. That click only dismisses the menu. The trigger toggles it, selection closes it, and the GTK window capture handler consumes Escape to dismiss it first. With no menu open, that handler explicitly exits fullscreen for the current video session and consumes the event, including when WebView or native controls have focus. When windowed GTK controls have focus, the same capture handler consumes Escape and sends a session-scoped `escape` intent. With WebView focus it leaves Escape to DOM handlers, including tag suggestions. The session model rejects retired input before calling `LightboxContent`. That owner ignores input while inactive, covered by a higher layer, fullscreen, or deleting. Otherwise it dismisses delete confirmation, then an open narrow drawer, then the preview. Control failures use the existing error report. The popover is nonmodal because capture owns outside dismissal. Its visibility and trigger state are cleared together on close, replacement, focus loss and zero visible bounds. Dismissal runs outside the player mutex and surface borrow, and choice callbacks hold weak popover references.

Normal lightbox close/reopen hides and reuses the renderer. GLArea unrealize makes its owning context current before unregistering the libmpv update callback and freeing the renderer. Callback storage survives renderer destruction. GTK's private GLArea input window also passes input through; setting pass-through only on the parent overlay leaves picture clicks intercepted.

Video bounds clamp against the GTK content allocation in logical pixels. On Wayland, popover configure events can replace Tao's cached window size with the popup dimensions. Using that cache shrinks the video and controls on reopening even though the WebView reports full-size bounds. Session checks still reject stale bounds and close operations.

## Image fit, zoom, and pan

`ResizeObserver` tracks the media viewport. Fit scale is `min(viewportWidth / intrinsicWidth, viewportHeight / intrinsicHeight)`. The `<img>` layout width and height are the fitted dimensions multiplied by `--lightbox-image-zoom`, which defaults to 1. Once dimensions are known, the image has no viewport maximum width or height; the stage clips overflow. Missing dimensions produce fit scale 1 and no explicit display size, with viewport maximums retained until load. Natural image dimensions replace summary/details dimensions only after a load from the current image element under an active controls owner. The stage also checks its keyed asset-ID/kind/path activation and disposal before forwarding a load. Detached images, retired sources and closed or disposed owners cannot replace dimensions or the transform target.

Zoom is a factor relative to the fitted size:

- minimum `1`, maximum `12`;
- discrete click and keyboard multiplier `1.25`;
- single click zooms in one step around the pointer;
- `+` or `=` zooms in and `-` zooms out around viewport center;
- `0` and the reset button restore factor 1 and pan `(0, 0)`;
- the pure `getDoubleClickZoomFactor` helper calculates `clamp(max(2, 1 / fitScale), 1, 12)`, but the rendered stage does not bind a double-click action.

Wheel zoom is cursor-anchored and prevents native scrolling. Delta mode 1 is converted with a 16 px line height; mode 2 uses viewport height; the resulting delta is clamped to `[-100, 100]`. The factor is `current * exp(-delta * 0.002)`, then clamped to `[1, 12]`.

Zoom preserves the source point under the pointer or viewport center. Pan is clamped independently per axis to half the overflow of `(fitted dimension * zoom) - viewport dimension`; an axis with no overflow remains centered. At factor 1, with a `0.001` epsilon in `clampPan`, pan is forced to zero. The DOM transform is `translate(xpx, ypx)`. Zoom changes layout dimensions so the browser paints original-image detail at the requested size. Keep transform scaling, `will-change: transform`, and hidden backface styling off the image: the former composited, fitted-size image lost fine detail when enlarged in WebKitGTK, especially for tall originals.

Primary-button drag starts only above factor 1. Pointer capture is preferred; window pointer listeners provide a fallback and window blur ends the drag. Movement over 3 px marks the gesture as a drag, suppressing the following image click for 120 ms. Selection changes reset zoom, pan, dragging, click suppression, and intrinsic dimensions. Viewport changes re-clamp pan while zoomed.

Zoom-property and translation writes are coalesced to one `requestAnimationFrame`: callers update synchronous local transform state and refuse to queue a second frame, and the frame writes both values. Solid tracks fitted dimensions; the image controls owner writes the zoom custom property and translation, so unrelated renders preserve zoom. The pending frame is cancelled on unmount. Resize measurement and tag-input focus also use their respective observer/animation-frame lifecycles.

## Fullscreen and keyboard behavior

For images and GIFs, fullscreen targets the lightbox shell. If another element is fullscreen, it is exited first. `fullscreenchange` derives `isFullscreen` from whether that exact shell is the fullscreen element; fullscreen CSS removes the shell's bounds, radius, border, and shadow. The sidebar remains part of the fullscreen shell, so its controls stay reachable. The fullscreen button and `F` toggle it. Image DOM fullscreen does not outlive the image: when the selection moves to a video, or the lightbox is deactivated or disposed, the shell exits DOM fullscreen (`document.exitFullscreen`, guarded by `document.fullscreenElement === shell`) and `isFullscreen` becomes false, so `F` and `Escape` then act on the video or the lightbox rather than on a stale image state.

For video, fullscreen delegates through the local `LightboxVideoPlayerHandle` and the session model. Frontend and native GTK controls share the Rust fullscreen operation. It rechecks session identity on the GTK thread, toggles the Tauri window, records the state, and sends a `fullscreen` event. The lightbox keeps the same mounted media stage when entering and leaving fullscreen, preserving the playback session and position. Native fullscreen removes backdrop padding and lightbox chrome and hides the sidebar and trigger. `F` reaches the native action from both WebView and GTK controls; `Escape` exits fullscreen before closing the lightbox. Closing or replacing a fullscreen surface restores the window. The sidebar choice survives fullscreen. Frontend fullscreen failures show a recoverable control error.


The window-level shortcuts while an asset is selected are:

| Key | Behavior |
| --- | --- |
| `Escape` | Dismiss the native speed menu first, then exit image or native video fullscreen; otherwise close the lightbox |
| `ArrowLeft` / `ArrowRight` | Seek video backward/forward 5 seconds when the player or a native playback button has focus; otherwise previous/next global result with wrap |
| `F` | Toggle image-shell or video-player fullscreen |
| `0` | Reset image/GIF zoom |
| `+` / `=` | Zoom image/GIF in by 1.25 |
| `-` | Zoom image/GIF out by 1.25 |

The first key press after three seconds of inactivity both reveals the controls and executes its eligible shortcut. `createImageControls` owns one keyboard listener for its mounted scope. The listener reads current reactive options when handling the event, so activity updates do not replace it. Asset changes and control visibility do not replace the listener. Logical close disables handling through the `active`/`keyboardEnabled` options; disposal removes it.

While delete confirmation is open, the lightbox shortcut listener is disabled and Escape dismisses the confirmation, whether focus is inside it or elsewhere in the lightbox. Otherwise, form controls keep their keys except video fullscreen Escape. The WebView fallback consumes fullscreen Escape before other listeners and ignores repeated keydown events. It remains enabled while fullscreen hides an open narrow sidebar. Native video `F` is handled by the WebView shortcut path or GTK controls; the GTK window captures fullscreen Escape regardless of focus. Global navigation is suppressed when the target is inside `[data-lightbox-video-player]`; image-only zoom keys do nothing for video.

A primary-button click on the video picture focuses its WebView container and toggles pause. The GTK picture passes pointer input through to that container; native controls handle their own clicks. With the picture focused, unmodified Left/Right seek by five seconds and Space/Enter toggle pause. Native play, mute, and fullscreen buttons also handle unmodified Left/Right; sliders and the speed menu retain their keyboard behavior. Seeking preserves pause state and libmpv bounds the position to the media timeline. Pause toggles and relative seeks run on the playback worker, including accumulation of rapid relative seeks against libmpv's pending target. Frontend controls queue in input order and discard queued work for retired sessions; failures use the existing recoverable control-error UI.

## Backdrop-close and nested-overlay safety

Clicking the outer backdrop closes an open narrow sidebar first, otherwise calls the image controls' `tryClose`; clicking the shell stops propagation. A live drag always blocks backdrop close. Zoom/pan gestures additionally set short time gates so the click synthesized after an interaction cannot close the modal:

- wheel or image click: 360 ms;
- drag start: 500 ms;
- drag finish: 240 ms.

The inline delete confirmation is part of the sidebar, so while it is open a lightbox shortcut listener is disabled and Escape (from any focus in the lightbox) targets only the confirmation; its submitting guard prevents backdrop cancellation until deletion settles.

## Known limitations and maintenance hazards

- The shared details cache (256-entry LRU, identity- and query-guarded) also validate metadata-generation stamps, rejecting cached snapshots made obsolete by favorite/group mutations. Full DB restore and library clear advance the shared identity epoch and clear the shared details cache, requests, and pending tag mutation state. Successful deletion creates a per-ID tombstone generation before refresh work, so an older detail response cannot repopulate a deleted or reused ID.
- Adjacent prefetch deduplicates neighbor indices and shares in-flight details requests. Rejections are caught and ignored. Work can continue after a selection change, but query and metadata generations reject stale cache publication.
- Query changes resolve an uncached selected ID through the backend position command. Navigation stays disabled until resolution or explicit retry succeeds.
- The unused `getDoubleClickZoomFactor` calculation remains in `lightboxPanZoomService.ts`; neither `createImageControls` nor `LightboxMediaStage` binds a double-click action. Rendered images therefore single-click through 1.25x steps; the pixel-perfect double-click toggle is not reachable from the modal.
- The lightbox is a named portal dialog registered with the shared layer manager. Focus restoration depends on the saved trigger still being connected; a virtualized gallery tile can disappear before close.
- Final staged-delete cleanup can fail after DB commit; the UI reports the recovery path and the durable journal retries cleanup at startup.
- Details and media failures have localized error UI; details also has Retry. Clipboard failure clears feedback, and some fullscreen shortcut failures are caught without an alert.
- A video whose audio track cannot be played because no audio device opens plays silently, with no indication in the UI.

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
10. Keep the GTK render path free of waits on libmpv: no frame-timing wait and no synchronous libmpv call in the `GLArea` render handler, and frame wake-ups stay at idle priority ([ADR 0012](../adr/0012-non-blocking-native-video-render.md)).

## Tests

- [`createLightbox.test.ts`](../../src/features/lightbox/__tests__/createLightbox.test.ts)
  covers summary-to-details loading, A/B and close races, adjacent prefetch/cache reuse
  including sequential navigation through cached selections,
  coalesced saves/retry, contention, dirty group fields, metadata-generation refetch,
  identity reset and ID reuse, deletion, rapid navigation (including a failed page after
  rapid presses), real page boundaries, wrap, query-position recovery, same-turn refresh
  invalidation, the detail re-read after a range-selection write, and a queued edit that a
  range write overtakes failing with Retry instead of being dropped.
- [`mutations.test.ts`](../../src/library/metadata/__tests__/mutations.test.ts) covers the
  shared persistence path, backend-first publication, partial results and query impact.
  [`runtimeIntegration.test.ts`](../../src/app/__tests__/runtimeIntegration.test.ts) composes
  actual models for CSV draining/invalidation, cleared-ID reuse and bidirectional
  lightbox/bulk ownership, with mocked IPC.
- [`LightboxModal.test.tsx`](../../src/features/lightbox/__tests__/LightboxModal.test.tsx)
  covers presence, immediate native-player disposal on logical close, retained closing
  state, tag input/suggestions, favorite/group actions, inline delete confirmation (including
  the translated busy-library rejection with every exit usable, Escape from outside the
  confirmation, the case-insensitive word with `localeCompare` stubbed as in WebKitGTK, and
  the confirmation surviving same-asset tag and favorite writes) and
  image/GIF activation identity. The backend side of that rejection is covered by
  `delete_fails_fast_while_a_scan_holds_the_scan_lock_and_changes_nothing` in
  `src-tauri/src/commands/assets.rs`. [`presentation.test.ts`](../../src/features/lightbox/__tests__/presentation.test.ts)
  covers sidebar, activity and clipboard state. [`keyboard.test.tsx`](../../src/features/lightbox/__tests__/keyboard.test.tsx)
  covers current callbacks, idle reveal, form/fullscreen keys and listener lifetime.
- [`imageControls.test.tsx`](../../src/features/lightbox/__tests__/imageControls.test.tsx)
  covers wheel anchoring, zoom/pan bounds, natural dimensions, navigation reset, detached
  loads across ID and path changes, inactive/disposed owners and frame coalescing/cancellation. It does not establish decoded GIF timing or image sharpness.
- [`video/__tests__`](../../src/features/lightbox/video/__tests__) covers native session
  reservation/cancellation, early and retired events, coalesced bounds acknowledgements,
  layout-root republishing for position-only moves (sidebar toggle with a fixed-size player),
  labels, recoverable/fatal errors, keyboard ownership, native Escape session/layer
  priority, pending deletion locks and fullscreen continuity.
- [`lightboxRecovery.test.tsx`](../../src/app/__tests__/lightboxRecovery.test.tsx) covers
  failed-view reopening and preservation of a healthy view during navigation.
- [`UiModal.test.tsx`](../../src/ui/overlays/__tests__/UiModal.test.tsx) covers nested layers,
  presence, reduced motion, focus and inertness. These tests require the layer provider.
- The Rust unit tests in `src-tauri/src/video_surface.rs` cover the non-blocking render
  parameters and a GLib model in which continuous frame wake-ups must not starve an
  idle-priority source. The real-libmpv tests in
  `src-tauri/src/services/video_player_service.rs` cover the player options and silent
  playback of an audio track when no audio device opens. They also open eleven text files
  named `*.mp4` (m3u, pls, HLS, EDL, DASH, `concat`, SDP) that point at a loopback
  listener, at a clip outside the folder and at a sibling clip: each must end in an error
  event with no connection, no open of a named file (inotify) and no loaded metadata, and
  a real clip must then play on the same player. A companion test turns
  `access-references` back on and requires the same files to reach the listener and the
  outside clip. Neither creates a GTK or OpenGL context.
- `e2e/specs/app.workflows.e2e.js` opens a 60-second video with a silent audio track and
  requires ten async IPC round trips to stay under two seconds each while it plays.
- `e2e/specs/native-video-escape.e2e.js` checks XTest Escape after proven GTK play
  and volume keyboard focus, WebView focus, speed-menu/fullscreen priority, inline
  confirmation and the narrow drawer.
- `e2e/specs/lightbox-images.e2e.js` and `e2e/specs/video-speed.e2e.js` exercise the actual
  WebView/native presentation with synthetic media. The latter checks GTK menu dismissal,
  focus/fullscreen ownership and 30 reopen cycles with decoded-frame comparisons. See
  [desktop opt-ins](../development/testing.md#solid-feature-verification) and
  [native setup](../development/testing.md#native-speed-menu-and-repeated-playback).

Use deferred IPC for lifecycle changes and the shared query/details owners for cache and
navigation changes. Desktop pixels, GTK focus and playback need the corresponding native
suite; DOM tests alone do not establish those properties.
