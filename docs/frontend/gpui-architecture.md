# GPUI interface: architecture

This page owns the architecture of the second Tagrove interface, the GPUI application in the workspace `src-gpui/`. It is the entry point for work on that interface: how its two crates are composed, who owns which state, the rules a view follows, the traps of the pinned GPUI version, and what is known to differ from the Tauri application. The decisions behind it are [ADR 0036](../adr/0036-gpui-second-adapter-in-its-own-workspace.md) to [ADR 0046](../adr/0046-gpui-lightbox-reads-and-decodes-the-original-itself-within-a-budget.md); the plan and its epics are in [docs/plans/gpui-port](../plans/gpui-port/README.md).

The Tauri/Solid application is the released interface and the behaviour reference. The GPUI interface has no release profile and is not packaged; that, GIF animation and video playback are the deferred epics E20 to E22 ([what is missing](#what-is-missing)).

## Where each topic is described

The subsystem pages describe the GPUI side of their subsystem next to the Tauri side, so that one page states the behaviour once.

| Topic | Section |
| --- | --- |
| Workspace, crates, startup order, boundary checks, the `Backend` port and the model convention | [system overview](../architecture/system-overview.md#gpui-port-a-second-adapter-in-its-own-workspace) |
| The port against the IPC contract | [IPC contract](../architecture/ipc-contract.md#the-same-contract-without-ipc-the-backend-port) |
| Runtime, window, shell | [frontend conventions](architecture-and-ui-conventions.md#gpui-port-runtime-window-and-shell) |
| Text field, theme, controls, layers and modals, key contexts | [text field](architecture-and-ui-conventions.md#gpui-port-the-text-field), [theme, controls and overlays](architecture-and-ui-conventions.md#gpui-port-theme-controls-and-overlays), [layers and modals](architecture-and-ui-conventions.md#gpui-port-layers-and-modals), [keys and text fields](architecture-and-ui-conventions.md#gpui-port-keys-and-text-fields) |
| Query model, page cache, gallery grid, position bar, timeline | [library query and gallery](../subsystems/library-query-and-gallery.md#gpui-port-the-query-model) |
| Thumbnail pipeline, decoding, picture cache | [thumbnails](../subsystems/thumbnails.md#gpui-port-the-thumbnail-pipeline) |
| Metadata writes, vocabulary, selection, search, tag list, bulk dock | [search, tags and media groups](../subsystems/search-tags-and-media-groups.md#gpui-port-metadata-mutations-and-the-vocabulary) |
| Lightbox model and view | [lightbox](../subsystems/lightbox.md#gpui-port-the-lightbox-model) |
| Settings operations, transfer, duplicates | [settings operations](../subsystems/settings-operations.md#gpui-port-settings) |
| Texts and month names | [localization](../development/localization.md#gpui-port) |
| Tests, measurements, the comparison harness | [testing](../development/testing.md#gpui-workspace) |
| Building, the pinned Zed commit | [setup and build](../development/setup-and-build.md#gpui-adapter) |

## Composition

```
media_tagger_gpui  (views, executor glue, binary)        gpui, gpui_platform
        │  reads state, sends inputs
        ▼
media_tagger_client  (models as state machines, no GUI)  ── Backend port ──▶  media_tagger_core
```

- **`media_tagger_client`** holds everything that can be decided without a window: the models (`runtime.rs`, `library/`, `gallery/`, `search/`, `bulk/`, `lightbox/`, `settings/`), the measures and colours copied from the Solid classes (`*/layout.rs`, `theme/`, `controls/`), the texts (`i18n/`, compiled in from `src/i18n/locales`), the text editing model (`text/`) and the `Backend` port with its one adapter file, `backend/core_backend.rs`. It has no GUI dependency and no async runtime.
- **`media_tagger_gpui`** draws. `boot.rs` composes the core at startup, `runtime.rs` holds the client's `AppRuntime` in a GPUI entity, `ui/` has the building blocks GPUI does not ship (text field, button, slider, segmented control, switch, modal, layer stack, icons, fonts), `views/` has one module per screen area, `thumbs.rs` and `media.rs` decode thumbnails and originals off the UI thread.

The direction `media_tagger_gpui` → `media_tagger_client` → `media_tagger_core` is checked by the compiler and by `src-tauri/tests/architecture_boundaries.rs`: the client names the core's use cases only in `backend/core_backend.rs`, and the adapter names the core only in `boot.rs` and never a `db::` path, a lock helper, `Connection` or `admit`.

### One input, one frame

1. A view turns a click or a key into a `RuntimeInput` and sends it to the `Runtime` entity (`Runtime::send`, or `runtime::send` from a callback).
2. The model computes its next state and a list of effects. It never calls the backend itself.
3. The runner starts each effect on the `Executor`: a backend call through `Dispatch::call` on a background thread, or a timer. The UI thread goes on.
4. The reply comes back as another input through the model's inbox, carrying the request token or generation it was started with. The inbox raises the `WakeSignal`; a task on GPUI's foreground executor pumps the inbox on the UI thread.
5. A model that has moved on drops the reply. Otherwise it publishes state from the backend's canonical result, the entity notifies, and the views that read it draw the next frame.

A call that panics, and a task the executor dropped, still answer with a `WorkerFailed` error, so no operation waits forever ([ADR 0037](../adr/0037-gpui-client-backend-port-and-model-convention.md)).

## State ownership and lifetimes

| Owner | Lifetime | Holds |
| --- | --- | --- |
| `AppRuntime` in the `Runtime` entity | The window | View (gallery or settings), theme, language, tile size, the library with its query and page cache, selection, bulk model, lightbox model, search, tag list, settings model, scan roots, the startup scan |
| `Thumbnails` entity (owned by `Runtime`) | The window | Thumbnail demand, the byte-bounded cache of decoded pictures, textures to release |
| `ShellView` | The window | The current view entity, the lightbox view, the drag surface state |
| `LightboxView` | The window | The original's loader (`Originals`), the modal state, the three fields; a `Session` (pan, zoom, activity, sidebar, delete confirmation) made anew for every opening |
| `GalleryView`, `SettingsView` | Until the other view is shown | Scroll position of the grid or the page, focus, open lists, the dock's local state, the resolver's form |

A view that comes back is a new entity and starts from the runtime's state, as a remounted Solid view does. What must survive a visit to the settings page therefore lives in the runtime (the search draft, the bulk drafts, the selection).

Only the theme and the language are stored, in `ui-preferences.json` of the profile (`FilePrefsStore`, decision D6), as the Tauri interface stores only those in localStorage.

## Rules for a view

- **Thin.** A rule that can be tested without a window belongs in the client crate as a pure function or a state machine; the view draws its result and forwards input. Measures come from the `layout` module of the area, which names the Solid class each one copies, and a unit test there reads the Solid source so that a changed class fails the test.
- **Never block the UI thread.** No backend call, file read or decode in `render`, in an event handler or in a foreground task.
- **Publish after success.** A view shows what the model published. An optimistic value belongs to the model as an explicit pending intent (the tag chip), not to the view.
- **Texts** come from `runtime.i18n()`; no literal user-visible string.
- **Overlays** register with `ui::layers` or use `ModalState` with `Modal`; a view binds no Escape of its own.
- **Shortcuts** are entries of `media_tagger_client::shortcuts::SHORTCUTS`, bound in a key context, and with `outside_text_input` when the key types a character.
- **Controls are controlled:** they show the owner's value and ask for a change ([ADR 0039](../adr/0039-gpui-overlay-layers-and-controls.md)).
- **Debug selectors** (`debug_selector`) name what a window test looks up; they exist only in debug builds, so a measurement in a release build must not need one.

## Traps of the pinned GPUI

These cost time once each. They hold for the Zed commit pinned in `src-gpui/Cargo.toml`; check them again after a bump ([setup and build](../development/setup-and-build.md#updating-the-pinned-zed-commit)).

- **A deferred element is painted without the text styles around it.** `deferred(...)` (every dialog, the lightbox, a suggestion list) restores the text style stack of its ancestors for prepaint, not for paint. An element of our own that calls `window.text_style()` or `window.line_height()` in `paint` reads the window's defaults there (16 px, line height 26 px). Read them in `request_layout` or `prepaint` and keep the value. The text field does (`ui/text_input.rs`, `PrepaintState::line_height`); before that the text of a field in a dialog sat 3 px below its middle.
- **`overflow_hidden` clips to a rectangle,** not to the rounded corners. An edge that must follow the corners is an inset box shadow (the active link of the settings navigation).
- **An outer box shadow is drawn beneath the whole element.** Under a translucent fill it shows through, so such an element keeps only its inset shadows (`ui::theme::shadows_beneath`) and draws a ring as an absolutely placed child (the icon tile of the empty state).
- **Edges are rounded to whole pixels one by one.** A box whose top is at a fractional position can be 1 px off the webview's; that is not worth a correction ([parity](#parity-with-the-tauri-interface)).
- **No letter spacing, and a variable font only in its default instance.** Headings and buttons can be a pixel or two wider or narrower than in the webview, and a long heading can wrap. With only the bundled variable Inter, every weight is drawn as 400; the decision is part of E22.
- **Tab stops follow the drawing order, unless a tab group says otherwise.** Draw things in the order Tab should visit them (the settings navigation before the sections). What must be drawn on top of something else, and so later, keeps its place in the Tab order with `tab_group()`: the stops of a group come after the stops beside it, whatever was drawn first. The lightbox makes its stage, action rail and side panel groups, so the floating buttons drawn above the stage come first, as in the Solid DOM. Use `tab_group()` alone: `tab_index(n)` on an element with an id also makes that element a tab stop and focusable by a press.
- **An absolutely placed child is laid out from the padding box,** inside the borders of its parent, while a CSS outline is measured from the border box. `ui::theme::focus_ring` therefore takes the borders of the element it is drawn around (`RingBorder`); a ring drawn without them lies a border's width too close. `square_focus_ring` is the ring of an element without a corner radius.
- **A disabled `Button` tracks no focus handle,** so a focused button that becomes disabled drops the focus. The shell gives it to the layer root (`layers::focus_root`), where Escape and Tab still work; a dialog with keys of its own takes it back itself (`LightboxView::focus_lost`).
- **Without a focused element no binding matches.** The window focuses the layer root when it opens and whenever the focus is lost.
- **A press is not stopped by the element under the pointer.** `hit_test` collects every hitbox down to the first `occlude`; a control over a clickable area stops the press itself (`block_mouse_except_scroll`, `cx.stop_propagation()`).
- **GPUI never frees an image texture.** A picture dropped from a cache is released explicitly (`cx.drop_image`; `Thumbnails::release_textures`, `Originals`).
- **The `h-5 w-5` of `UiIcon` wins over a smaller or `size-*` class** in the generated stylesheet of the Tauri interface: the utilities have the same specificity and are emitted in the order `size-6`, `h-3`, `h-3.5`, `h-4`, `h-5`, `h-8`, `h-[18px]`, so an icon given `h-3 w-3`, `h-3.5 w-3.5`, `h-4 w-4` or `size-6` is 20 px there, and only `h-8 w-8` (the empty state) and `h-[18px] w-[18px]` (a scan root) take effect. Take an icon's size from a Tauri capture, not from the class. The captures of the comparison scenarios show every such icon at the size GPUI draws except two that no scenario shows: the cross of a tag's remove button and the icon of the collapsed dock's chip, both 20 px by the stylesheet order.
- **A test window needs a drawn frame after `settle`** (`window.simulate_next_frame`) before the focus of a dialog or a slide is where the code put it.

## Keyboard and focus

The contract is [keyboard and accessibility](architecture-and-ui-conventions.md#keyboard-and-accessibility) with its GPUI section, and [fullscreen and keyboard behavior](../subsystems/lightbox.md#fullscreen-and-keyboard-behavior) for the lightbox. The review of epic E19 went through both against the code and the window tests:

| Rule of the Tauri interface | GPUI | Covered by |
| --- | --- | --- |
| Escape closes only the top layer; a locked top layer keeps it from every layer | Same (`LayerStack`, `ui::layers`) | `tests/overlays.rs` |
| A dialog takes the focus on its first tab stop, Tab wraps inside, closing restores the trigger | Same | `tests/overlays.rs`, the window tests of each dialog |
| Closing content is `inert` | No key reaches a dialog that fades out, and Tab passes over its tab stops (`ui::layers` keeps the containers of released modals while they are drawn) | `tests/overlays.rs` (`tab_passes_over_a_dialog_that_fades_out`) |
| Lightbox: Escape closes the confirmation, then the narrow drawer, then image fullscreen, then the lightbox | Same (`Session::escape`) | `tests/lightbox_ported.rs` of both crates |
| Lightbox: Left and Right, `F`, `0`, `+`/`=`, `-`; the first key after three idle seconds reveals the controls and acts | Same; the keys that type a character do nothing in a field | `tests/lightbox_viewer_ported.rs`, `tests/lightbox_ported.rs` |
| Lightbox: no shortcut while the delete confirmation is open | Same | `tests/lightbox_ported.rs` |
| Video keys (seek, pause, native fullscreen) | Not present: the video is a placeholder (D8, E21) | n/a |
| Ctrl+K focuses the search field, not under a dialog and not on the settings page | Same | `tests/search_ported.rs` |
| Gallery: Home, End, Page keys and arrows scroll; Ctrl+A selects all in selection mode | Same, as bindings of the `Gallery` context | `tests/gallery_grid.rs`, `tests/gallery_selection.rs` |
| Dock tabs: one tab stop, arrows, Home, End; order handles: arrows | Same | `tests/bulk_dock_window.rs`, `tests/bulk_dock_ported.rs` |
| Settings: Tab goes header, navigation, sections; an activated link focuses its section | Same | `tests/settings_window.rs` |
| `:focus-visible`: a ring after keyboard input only | Same (`window.last_input_was_keyboard()`) | the window tests of the controls |
| Suggestion lists are a `listbox` with a translated name and options | Same since E19 (`views::search::suggestion_list_body`: role `ListBox`, the name of `listboxAriaLabel`, options with `aria-selected`) | the role is set in code; the test platform builds no accessibility tree |
| Lightbox: Tab follows the DOM (floating reopen and close buttons, action rail, side panel) | Same (tab groups, see the traps above) | `tests/lightbox_window.rs` (`tab_goes_round_the_dialog_in_the_order_of_the_solid_dom`) |
| A press on a part of the dock that cannot be focused takes the focus out of its field | Same; the window's root stands for the body | `tests/bulk_dock_window.rs` (`a_press_on_blank_space_of_the_dock_takes_the_focus_out_of_its_field`) |
| The `:focus-visible` outline lies at the rule's offset from the border box | Same, also around a bordered control (`RingBorder`) | `tests/controls.rs` (`the_focus_ring_lies_at_the_outline_s_distance_from_the_border_box`), `tests/settings_data_window.rs` |

Known differences in this area:

- **Tab order of the gallery:** position bar, timeline, then Retry.
- **No live regions.** GPUI has no counterpart of `aria-live`, so the announcement of a moved tile on the order board (`bulk.orderModal.moved`) and the status regions are not announced.
- **No tooltips.** The `title` texts of the Tauri interface (icon buttons, a truncated file name or path, the state of a tag in the tag list, the search syntax hint) are not shown on hover. Icon buttons carry the same text as their accessible name.
- **No `inert` or `aria-hidden` on the page under a dialog;** the dialog occludes the pointer and keeps Tab.
- **A press on a tag chip** of the tag editor keeps the focus where it is; in the webview it leaves the field, as a press on any part of the page that cannot be focused does.
- Accessibility roles and names are set in code and were not checked with a screen reader.

## Parity with the Tauri interface

Parity means the same layout, tokens, content and state, not the same pixels (decision D10). `bun run compare:ui` captures both applications in each of the 23 scenarios ([comparison harness](../development/testing.md#comparison-harness-tauri-and-gpui)). The run of epic E19 found no blocking difference. Six differences were corrected in that epic (the settings header, the size of the end-of-results mark, the ring of the empty state's icon tile, the height of the tag list's filter, the text of fields in dialogs, the heavier cross of the dock's counter and of a tag's remove button) and one was a drawing rule (the edge of the active settings link). The audit after the epic corrected four more: the focus ring of a bordered control lay 1 px closer than its CSS outline, the text of the resolver's disabled rename field was dimmed twice (0.30 instead of 0.22), the lightbox's floating buttons came after the action rail in the Tab order, and a press on a blank part of the dock kept the focus in its field. What remains is deliberate or belongs to a deferred epic:

| Difference | Where | Why it stays |
| --- | --- | --- |
| Text is a pixel or two wider or narrower; a long line wraps elsewhere; a file name on the order board is cut one character earlier | Everywhere | Another rasterizer, no letter spacing, glyph advances not rounded to pixels (D10) |
| Weights other than 400 look heavier or lighter | Headings, buttons | Variable Inter is drawn in its default instance only; decision in E22 |
| A box is 1 px higher, lower or shorter | Tools row of the gallery, suggestion rows, the Danger zone card, the cards of the duplicate resolver | Fractional positions rounded edge by edge |
| The gallery always draws its own position bar | Gallery with more rows than fit | [ADR 0042](../adr/0042-gpui-gallery-grid-logical-offset-and-bounded-pictures.md); the webview shows an overlay scrollbar until the windowed mode |
| No scrollbar on the settings page, the tag list, the lightbox's panel and the resolver's list | Those scrollers | GPUI ships no scrollbar; the wheel, the keys and the navigation links scroll. Listed for E22 |
| The caret shows in a focused field of a capture | `search-suggestions` | The webview draws no caret in a window without input focus |
| A native tooltip can be in the Tauri capture | `tag-list-modal` | See tooltips above |
| GIF: first frame with a chip; video: poster, metadata and a notice | Lightbox | Decision D8; epics E20 and E21 |
| A small picture enlarged many times has soft edges; a strongly reduced one is drawn without mipmaps; one over the decode budget shows the media error | Lightbox stage | The GPU sampler and [ADR 0046](../adr/0046-gpui-lightbox-reads-and-decodes-the-original-itself-within-a-budget.md) |

## Performance on one million results

`tests/million.rs` of the adapter measures the main window against the real core on a library of 1,000,000 synthetic assets, seeded with the SQL of `src-tauri/tests/grid_baseline.rs` ([testing](../development/testing.md#gpui-workspace)). It is a measurement without thresholds. The numbers below are one run in release mode on one desktop host (16 hardware threads, the temporary profile on a disk-backed file system) on 2026-11-24, next to `query_rebuild_and_page_baseline` of the grid baseline run on the same host in the same hour. That baseline times the core calls the Tauri commands make, without IPC and the webview.

| What | GPUI interface | Core baseline of the Tauri workspace |
| --- | --- | --- |
| A frame while scrolling 48 px a frame, 72 tiles (188 px) | mean 2.1 ms, p95 2.3 ms, max 3.4 ms | n/a |
| The same with 126 tiles (140 px) | mean 3.3 ms, p95 3.7 ms, max 5.4 ms | n/a |
| A frame while scrolling 900 px a frame (demand paused) | mean 1.1 ms, p95 3.4 ms, max 4.0 ms | n/a |
| A page of 128 rows (`get_asset_query_page`) | median 0.32 ms | p50 0.13 ms |
| Opening the window: first frame with tiles | 18 ms on the UI thread; first query 75 ms and the month sections 31 ms in the backend | sections 27 ms |
| Refresh with nothing changed | 10 ms on the UI thread, query start 0.15 ms | cached start 0.09 ms |
| Refresh after a write that changed the order (ID list rebuilt) | 11 ms on the UI thread, query start 29 ms, sections 28 ms | rebuild 32 ms, sections 27 ms |
| New query: favourites (50,000 results) | query start 48 ms | 48 ms |
| New query: tag `common` (200,000) | 96 ms | 99 ms |
| New query: tag `rare` (5,000) | 3.9 ms | 6.2 ms |
| New query: without `common` (800,000) | 405 ms | 403 ms |
| New query: videos (50,000) | 35 ms | 35 ms |
| Jump to the end or the start of the library | 1.7 ms and 2.4 ms on the UI thread (median), the pages 0.3 ms | n/a |

The frame is the event, the model's work, the callbacks of the frame before it and `Window::draw` on GPUI's test platform: layout and paint without a GPU, with text shaped by the platform's stand-in. Rasterization is not in it. With every thumbnail answered at once the same frames have a p95 of 8 ms (72 tiles) and 14 ms (126 tiles), and 60 ms at 900 px a frame, because the test platform decodes the 390 px JPEGs on the test's thread, a screen of them in one step after a jump; the application decodes on GPUI's background executor. For the same reason the first sample of a jump or of a new query takes 80 to 160 ms on the test's thread and the later ones 2 to 5 ms. The backend columns agree because both interfaces call the same core; the Tauri interface adds the IPC round trip and JSON for every call, which this comparison does not measure.

Memory of the test process, resident set and the bytes Rust code holds (a counting allocator; what SQLite allocates in C is only in the resident set):

| Moment | Resident | Held by Rust code |
| --- | --- | --- |
| After the seed, which runs in the same process | 216 MiB | 2 MiB |
| After the first query | 420 MiB | 21 MiB |
| Picture cache full: 587 decoded thumbnails, 256 MiB, its budget | 568 MiB | 280 MiB |
| After every filter was queried | 755 MiB | 290 MiB |
| After each of 24 tours of the long session | 760 to 797 MiB, 773 MiB after the last | 289 MiB after every tour |

A tour of the long session, as `tests/million.rs` runs it, is 5,000 frames of scrolling (3,000 at 48 px a frame, 1,000 and 500 at 900 px, 100 after each of the five filters), both far jumps and every filter once. The recorded run made 24 tours; the test makes 8 unless `MEDIATAGGER_GPUI_PERF_TOURS` says otherwise, so the table above is reproduced with:

```sh
MEDIATAGGER_GPUI_PERF_TOURS=24 TMPDIR=/var/tmp cargo test --manifest-path src-gpui/Cargo.toml --locked --release \
  -p media_tagger_gpui --test million -- --ignored --nocapture
```

The page cache stays at its bound (24 pages) and the picture cache at its budget throughout, no dropped picture waits for its texture to be released, and no timer of a model is left waiting. A long session does not grow the process.

The Tauri frontend at the same scale is measured by its own fixture with a synthetic transport ([million-result gallery layout](../development/testing.md#million-result-gallery-layout)); the two are not the same measurement and are not put in one table.

## What is missing

Deferred epics, each needing a new decision and a plan of its own ([plan](../plans/gpui-port/README.md#epiki)):

- **E20, GIF animation.** The lightbox shows the first frame with a chip.
- **E21, video playback.** The lightbox shows a poster, the metadata and a notice; libmpv in a GPUI window is an architecture decision.
- **E22, cutover.** A release identifier with the migration of the profile, packaging, static font instances or a newer GPUI for font weights, desktop E2E for the GPUI window, and removing the Tauri adapter and the Solid frontend. The open differences above that are more than rasterization belong to its plan: scrollbars for the scrollers besides the gallery, tooltips, live regions, a check with a screen reader, a multi-month seed for the comparison of the timeline, and a look at the window on a real Wayland compositor and X11 window manager (fullscreen answers, the file dialogs of the XDG portal, a real mouse wheel).
