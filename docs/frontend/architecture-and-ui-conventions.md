# Frontend architecture and UI conventions

Implementation entry points: [app composition](../../src/App.tsx), [shell controller](../../src/components/app/hooks/useAppShellController.ts), [UI layer manager](../../src/components/UI/UiLayerProvider.tsx), [styles](../../src/styles.css).

This document describes the frontend structure that exists today. The React code and its tests are authoritative; this guide is a map for making compatible changes, not a proposal for a new framework.

Feature behavior belongs in the narrower canonical guides:

- [library queries and the gallery](../subsystems/library-query-and-gallery.md)
- [search, tags, and media groups](../subsystems/search-tags-and-media-groups.md)
- [lightbox behavior](../subsystems/lightbox.md)
- [settings operations](../subsystems/settings-operations.md)
- [IPC contracts](../architecture/ipc-contract.md)
- [localization](../development/localization.md)
- [testing](../development/testing.md) and [setup and build](../development/setup-and-build.md)

## Bootstrap and shell composition

[`src/main.tsx`](../../src/main.tsx) is the browser bootstrap. It imports i18n and global CSS before rendering, applies the stored light/dark theme before the first React render, optionally installs performance instrumentation when `VITE_MEDIATAGGER_PERF=1`, and mounts `<App />` inside `React.StrictMode`.

Strict Mode means mount effects must tolerate setup, cleanup, and setup again during development. Effects that install listeners, animation frames, timers, observers, or async generations must provide cleanup or stale-result protection. Production behavior must not depend on an effect running exactly once merely because it has an empty dependency list.

[`src/App.tsx`](../../src/App.tsx) is intentionally a thin composition surface:

1. `useAppShellController()` creates the long-lived application state and callbacks.
2. `App` chooses either `AppGalleryView` or `AppSettingsView`.
3. The gallery view conditionally mounts the bulk sidebar, while `App` conditionally mounts the lightbox overlay.
4. View components receive controller-shaped prop groups instead of discovering application state themselves.

[`useAppShellController`](../../src/components/app/hooks/useAppShellController.ts) is the composition root for frontend hooks. It combines search filters, library browsing, selection, bulk selection, settings actions, appearance, and settings-view state. It also owns the shell-only tile-size scheduler and translates feature APIs into the typed props consumed by [`AppGalleryView`](../../src/components/app/AppGalleryView.tsx), [`AppSettingsView`](../../src/components/app/AppSettingsView.tsx), and lazy overlays. The prop contracts in [`src/components/app/types.ts`](../../src/components/app/types.ts) are useful seams: extend the narrow controller group that a view needs rather than passing the entire shell controller through the component tree.

## State ownership and lifetime

Keep state at the lowest owner that must coordinate every consumer. The current ownership is:

| State | Owner | Lifetime and persistence |
| --- | --- | --- |
| Current full-page view and one-shot scan-section highlight | `useSettingsView` and `useAppShellController` | Lives while `App` is mounted; not persisted across launches. |
| Tile size and coalesced slider/wheel work | `useAppShellController` | Survives gallery/settings switching because the shell stays mounted; resets to `188` on a new app mount and is not in local storage. |
| Search draft and applied filters | `useAppSearchFilters` | Shell lifetime; draft and applied values are intentionally separate. See the search guide for semantics. |
| Query pages, asset summaries, totals, loading, and query session references | `useLibraryAssets`, composed by `useLibraryBrowser` | Shell lifetime; in-memory bounded page cache, reset or refreshed by lifecycle events. See the gallery guide. |
| Known tags | `useLibraryKnownTags`, composed by `useLibraryBrowser` | Shell lifetime; refreshed independently from asset pages. |
| Thumbnail paths, queue state, rendering IDs, and thumbnail store | `useLibraryBrowser` and `useThumbnailQueue` | Shell lifetime; reset when query/library lifecycle requires it. |
| Authoritative complete tag lists, identity epoch, and per-asset mutation generations | `useAssetTagState` | Shell lifetime; shared by lightbox and bulk. Mutation barriers are exclusive per asset, start before IPC, and reject details from before/during a pending write. A caller that cannot acquire every required asset sends no IPC and releases any locks already acquired. Restore/clear advances the identity epoch; deletion leaves a per-ID invalidation generation. |
| Selected asset, loaded details, lightbox editor drafts, and serialized tag saves | `useSelectionState` | Shell lifetime; overlays shared authoritative tags onto its local detail cache, blocks replacement while the complete tag base is unknown, and clears selection-scoped state when appropriate. |
| Bulk mode, selected IDs, anchor, sidebar drafts, detail cache, and applying flags | `useBulkSelectionController` | Shell lifetime; retains selected IDs across page eviction, consumes shared tag state, separates detail-load and mutation failures, and resets drafts when the selected-ID set changes. |
| Settings operation locks, progress/messages, scan roots, pending confirmations, and duplicate state | `useSettingsActions` and its focused settings hooks | Shell lifetime even while the settings component is unmounted; backend data is refreshed by the owning action hook. See the settings guide. |
| Theme | `useAppTheme` plus `themeService` | React state for the mounted shell, mirrored to `<html data-theme>`, local storage, and the native window when running under Tauri. |
| Language | the i18next singleton plus `useAppLanguage` | Process-wide i18n state, mirrored to `<html lang>` and local storage. |
| Input drafts, open popovers, and other purely presentational interaction state | The closest feature component or focused feature hook | Normally resets when that component unmounts; a modal may also explicitly reset its draft when `open` becomes false. |

Refs are used for mutable coordination that should not render: active request generations, in-flight page offsets, scheduled animation frames, selection indices, element handles, and caches. Do not move user-visible state into a ref merely to avoid renders.

## Gallery and settings lifecycle

The gallery and settings are mutually exclusive children of the same `<main>` element. Opening settings unmounts `AppGalleryView` and mounts the lazy `AppSettingsView`; going back does the reverse. Consequently:

- shell-owned hooks continue running and their state survives the switch;
- view-local DOM state, virtualization measurements, and component-local state do not survive unmounting;
- Settings scrolls inside the shared `<main>`, while the gallery mounts its own scroll element below the header; gallery scroll position resets when that element remounts, and neither view explicitly saves its position;
- settings actions are constructed even while the gallery is visible, because they belong to the shell;
- opening settings unmounts the gallery-owned bulk sidebar while its shell-owned selection state remains available;
- the scan-root list and known tags are hydrated on shell mount, and applied search filters drive library refreshes from shell effects.

Settings closes through Back or the layer registered by `SettingsViewLayer` in `App.tsx`. `useSettingsView` owns only the open boolean and callbacks. A normal settings open starts without a highlighted scan section; the empty-library "add first folder" route opens the same view with the scan section highlighted. Closing the view clears that transient highlight.

## Lazy loading and delayed prefetch

The gallery is eagerly imported. Settings, the bulk sidebar, and lightbox use `React.lazy`. Settings shows a localized status while loading; lightbox shows a named loading dialog. `LazyErrorBoundary` supplies localized failure UI with reload and Back/Close actions. Its reset key follows settings visibility or selected asset ID; resetting the boundary alone does not guarantee a fresh download of a rejected lazy module.

After `App` mounts, an effect schedules dynamic imports for settings and lightbox after 1,500 ms. The bulk sidebar loads on demand. Opening one earlier starts its import immediately. The timer is cleared on cleanup; module loading itself is cached by the JavaScript module loader. Preserve this split when adding a large, infrequently used surface: make the initial render path explicit, cancel delayed work in cleanup, and do not assume prefetch completed before user interaction.

## Layer responsibilities

### Components

Components render semantic HTML, compose UI primitives, translate display text, and turn DOM events into prop callbacks. They may own interaction state that no parent needs, such as an input draft or popover toggle. App-wide fetching, persistence, mutation ordering, and cross-feature synchronization do not belong in leaf components.

Full-page components accept grouped controller props. Reusable components should prefer explicit values and callbacks over importing the shell hook. Backend calls should not originate in render components.

### Controller and state hooks

Hooks own React state, effects, refs, and coordination across components. A controller hook adapts lower-level hooks and action functions into a stable view contract. Focused hooks are preferred over growing a component or `useAppShellController` with feature internals.

Effects must clean up browser registrations and guard stale asynchronous work. Callback dependencies remain explicit; the few intentionally mount-only effects in the shell carry local lint suppressions and should not become the default pattern.

### Pure and action services

Pure services contain deterministic normalization, validation, comparison, and immutable state transformations. Examples include tile-size clamping, filter comparison, and asset patch helpers. They do not use React or access the DOM and are tested with direct input/output assertions.

Some feature `services` are action services rather than pure functions. They sequence an API call and then invoke state setters or refresh callbacks. Keep that distinction visible in names and signatures. The hook remains responsible for busy flags, retry/close behavior, and presenting failures; the action service owns the successful operation sequence.

### API wrapper

[`src/api.ts`](../../src/api.ts) is the frontend boundary for Tauri commands, channels, and media-source conversion. Hooks and action services call these typed wrappers; components should not call `invoke` or construct command payloads. Any command name, payload, result, event, or serialization change must follow the [IPC contract guide](../architecture/ipc-contract.md).

## Successful mutations and local patches

Asset mutations use a pessimistic-local-update sequence:

1. Await the backend API wrapper.
2. Publish canonical per-asset tags from the successful response when the mutation changes tags.
3. Only after success, immutably patch the in-memory asset/detail caches and, when applicable, the selected asset.
4. Refresh secondary data such as known tags, or refresh the query when a mutation can change current result membership.

A complete tag replacement must never be sent from a summary placeholder or a failed details request. Tag editors stay disabled until `useAssetTagState` has a known complete list; loading and persistence failures have separate retry paths.

CSV import parses and validates the complete document before applying all matching rows and the revision bump in one database transaction. The shell-owned tag coordinator also provides an asynchronous global maintenance barrier. Requesting it immediately denies new per-asset mutation tokens, waits for every already-dispatched asset metadata write to settle, and only then invokes the maintenance operation. CSV import holds the barrier while success or rejection advances the coordinator epoch, clearing lightbox/bulk details caches, pending tag requests, and editor state without resetting unrelated library/thumbnail identity. Confirmed database restore and clear-library use the same barrier and hold it through their broader lifecycle reset on both success and rejection, because either command may reject after changing live state. Release occurs in `finally`; queued maintenance is serialized, and thrown operations or coordinator unmount cannot permanently retain the barrier. The library query refreshes after release, best-effort on uncertain failures.

The helpers in [`assetMutationService.ts`](../../src/components/app/services/assetMutationService.ts) keep list and selected-object updates consistent. State setters use functional updater callbacks, and selected-object helpers verify the asset ID before patching. Bulk tag and media-group actions follow the same "API first, patch second" order. Delete removes the cached asset and clears a matching selection after success, then refreshes relevant data. Unfavoriting inside a favorites-only query triggers a refresh because the item no longer belongs in the result.

Do not patch before a successful command unless a feature deliberately introduces a complete optimistic-update and rollback design. On failure, retain retry context when safe; the bulk sidebar keeps group and tag input available, reports the failed card, and clears only the applying flag.

## Runtime appearance and language effects

Theme and language are the only frontend preferences currently persisted in local storage:

- `media-tagger.theme` accepts only `light` or `dark`; invalid or absent values resolve to `dark`.
- `main.tsx` applies the initial theme before React renders to avoid a theme flash.
- `useAppTheme` reapplies changes through `applyTheme`, which sets `data-theme`, writes local storage, and best-effort synchronizes the native window theme under Tauri. Native synchronization errors are intentionally ignored.
- `media-tagger.language` is normalized against the supported language list. With no valid stored value, initialization uses the browser language when supported, then English.
- i18n initialization and every `languageChanged` event update `document.documentElement.lang` and local storage. `useAppLanguage` subscribes through `useTranslation` and exposes the active normalized language.

Do not access these storage keys from new components. Extend the owning service/hook so validation, DOM effects, and native synchronization remain centralized. Localization resource structure, key naming, and update checks belong in the [localization guide](../development/localization.md).

## UI primitives and styling

Reusable primitives live in [`src/components/UI`](../../src/components/UI):

- `UiButton` supplies consistent sizing, variants, disabled styling, and `type="button"` by default.
- `UiIconButton` layers active and danger states over `UiButton`; callers still provide an accessible name.
- `UiIcon` centralizes the SVG set. Icons with no title or ARIA metadata are decorative and receive `aria-hidden`; a title or explicit ARIA metadata makes the icon expose image semantics.
- `UiChip`, `UiAlert`, `UiProgressBar`, and `UiSlider` centralize common display/control styling. `UiAlert` always uses `role="alert"`.
- `UiModal` is the standard portal dialog primitive described below.
- `browserAssistDisabledProps` is shared by exact-token/confirmation inputs where browser autocorrection would be harmful.

[`src/styles.css`](../../src/styles.css) imports Tailwind CSS, disables DaisyUI's stock theme set, and declares the app's `light` and `dark` DaisyUI themes. Components combine Tailwind utility classes, DaisyUI component classes such as `btn`, `progress`, and `loading`, and app CSS variables.

Prefer semantic DaisyUI colors (`primary`, `base-*`, `error`, and similar) and the app tokens for surfaces, borders, radii, shadows, fields, and window chrome. Reuse `--surface-*`, `--border-*`, `--radius-*`, and `--shadow-*` rather than adding isolated literal values for the same role. Keep theme differences in the theme blocks, not conditional class lists in components. Pass `className` for layout or a deliberate contextual adjustment; shared visual behavior belongs in the primitive.

Global styles establish full-height roots, typography, field treatment, visible keyboard focus for buttons, links, inputs, and selects, and themed scrollbars. Preserve native elements where possible so keyboard and accessibility behavior come for free.

## Compact studio visual system

The full redesign is tracked in [the redesign plan](../ui-redesign-plan.md). Both themes use
neutral workspace/panel colors, with primary green `#7CB87C` in dark mode and `#2D5A2D` in
light mode. The root stays at 16px for Tailwind rem sizing; body copy is 14px. Controls use
4px corners, panels 6px, dialogs 8px. Standard buttons are 36px high and compact tools 32px.
System sans fonts serve prose and controls; paths and technical metadata use monospace.

`TopBar` composes the identity, search field and Settings action in one header row. A second
row contains media kind, favorites, tag browsing, the result count, bulk-mode toggle and
thumbnail sizing. Both rows belong to a full-width, non-scrolling header with natural height.
The gallery scroll viewport fills the remaining height, so its scrollbar starts below both rows even when they wrap. The gallery uses the available
width with square tiles and its existing measured 10px gaps. Bulk selection uses a green
outline plus a checkmark; media-kind badges remain readable over arbitrary thumbnails.

Desktop windows omit native decorations. `WindowControls` adds localized minimize,
maximize/restore and close buttons to the gallery and settings headers. Only the first gallery header
row and the settings header use Tauri's `data-tauri-drag-region="deep"`: background, gaps,
identity, headings and other non-interactive text can move the window. Native buttons,
selects, labels and interactive roles are excluded by Tauri. The whole search field and window-control
group explicitly set the attribute to `false`. The second gallery toolbar row does not drag
the window. Drag areas use the default cursor without changing control cursors.
Double-clicking a draggable area maximizes/restores the window. No separate move handle is
needed, and descendants retain pointer events so inputs and action icons stay interactive.
Invisible edge/corner handles support native resizing and disappear while maximized or fullscreen.
The controls are omitted in browser previews. Window actions use Tauri's window API; resize
listeners are removed on view changes, including delayed registration under Strict Mode.
Modal overlays keep their existing dismissal controls and keyboard behavior.

Bulk editing occupies a 360px inspector at window widths of at least 1000px, and follows
the gallery below that width. The inspector sticks to the gallery viewport's top and scrolls independently on desktop, with its maximum height supplied by the surrounding CSS size container. Its tag
suggestions use the existing viewport portal so inspector overflow cannot clip them.

Full-page settings uses 200px navigation beside mounted sections at widths of at least
1000px, and wrapping navigation above the content below that width. Navigation focuses and
scrolls to the corresponding section without changing the URL or unmounting operation UI.
The active marker follows the scroll position; settings section choice is not persisted.
Sections appear in scan, appearance, import/export, duplicates, and danger-zone order.

Keep shared field rules in the CSS base layer so component utilities can make deliberate
adjustments. Avoid adding nested field outlines, panel blur, decorative gradients, or
pill-shaped buttons. Native video controls retain their contrast gradient over the picture.

## Modal and overlay conventions

[UiLayerProvider](../../src/components/UI/UiLayerProvider.tsx), mounted by `App`, owns the DOM layer stack. [UiModal](../../src/components/UI/UiModal.tsx) and the specialized lightbox register modal layers and portal into `document.body`. `SettingsViewLayer` registers a nonmodal layer for page-level Escape.

- Only the top registered layer receives managed Escape dismissal. A locked top layer does not select a lower layer as a fallback. UiModal backdrop dismissal also checks top-layer ownership.
- A top modal makes `#root` and lower registered modal elements inert and aria-hidden, and locks body scrolling. Cleanup removes those attributes and restores the previous body overflow.
- Initial focus selects `[data-initial-focus]`, then the first tabbable descendant, then the container. Tab and Shift+Tab wrap inside the top modal.
- Unregistering the top layer restores the supplied trigger or the previously focused connected element. If unavailable, it tries the next layer's initial focus.
- `UiModal` requires exactly one of `ariaLabel` or `labelledBy` in its props and renders `role="dialog"` with `aria-modal="true"`. It supports four width presets and independently configurable Escape/backdrop closing.

Provide translated names and explicit close actions. Register new overlays with the shared manager instead of adding independent global Escape listeners. Lightbox image/native fullscreen and inline confirmation still have specialized key handling, described in [lightbox](../subsystems/lightbox.md#fullscreen-and-keyboard-behavior).

## Keyboard and accessibility

Native controls, translated accessible names, pressed states, listbox suggestions, alerts, and status regions are the normal conventions. Global styles provide focus-visible feedback and a reduced-motion rule that minimizes animations/transitions and disables smooth scrolling.

The layer manager's tabbable selector excludes disabled controls, hidden elements, and inert ancestors, but it does not perform a complete computed-visibility or browser tab-order calculation. Standalone components rendered without the provider receive only fallback Escape handling. Tests that claim focus trapping or nested dismissal must mount the provider.

GTK video controls are native widgets outside the DOM layer manager. Changes to video bounds, fullscreen, or sidebar visibility need desktop verification as well as DOM tests. See [native media presentation](../subsystems/lightbox.md#media-presentation).

Bulk group drag handles support ArrowUp/ArrowDown reordering. `UiProgressBar` uses its supplied label as its accessible name. Component tests cover selected roles and focus transitions; they do not establish full screen-reader, contrast, zoom, or keyboard-only accessibility.

## Safe change checklist

Before changing frontend structure or UI behavior:

1. Choose the lowest stable state owner and document whether the state survives view switches, component close/reopen, and app restart.
2. Keep `App` declarative. Add feature orchestration to a focused hook/service and expose only the typed prop group the view needs.
3. Keep components free of raw Tauri `invoke` calls; update `src/api.ts` and follow the IPC contract for boundary changes.
4. For mutations, await backend success before applying local patches. Update every in-memory representation, and refresh tags or query membership when affected.
5. Make effects Strict-Mode-safe: unregister listeners/observers, cancel timers/frames, and ignore stale async responses.
6. Preserve lazy boundaries, visible loading states, and failure recovery for settings and lightbox. Test a rejected chunk as well as a delayed one.
7. Reuse UI primitives and semantic theme tokens. Verify both light and dark themes and avoid hard-coded foreground/background pairs.
8. Localize visible text, titles, labels, status text, and accessible names; follow the localization guide rather than editing one locale in isolation.
9. For dialogs, provide a name, explicit close action, busy-state close policy, initial focus, Escape/backdrop tests, and a plan for restoring focus. Consider interaction with another open layer.
10. Test at the layer where behavior lives, then run the relevant integration coverage and a build.

## Test map

The existing tests exercise these boundaries:

- `themeService`, `tileSizeService`, `filterService`, and `assetMutationService` tests cover deterministic policy and immutable patches.
- `UiModal` tests cover closed rendering, backdrop/Escape options, top-layer dismissal, locked nested layers, focus trapping/restoration, app-root inertness, and responsive sizes; `UiAlert` tests cover semantic role and tone styling.
- Hook tests cover query-cache lifecycle, stale/duplicate work protection, selection/editor synchronization, thumbnail queues, settings actions, confirmation flows, and shell search behavior.
- `App.test.tsx` is the integration seam for initial hydration, gallery/settings replacement, Back and Escape navigation, modal flows, mutation wiring, and runtime theme application.
- Focused component tests cover specialized keyboard, labeling, selection, and modal behavior. End-to-end Tauri tests cover behavior that requires the desktop runtime.

Add a direct service test for pure policy, a hook test for ownership/effect behavior, a component test for semantics and interaction, and an `App` integration test only when the composition contract changes. Run targeted Vitest files while iterating, then `bun run test` and `bun run build` in proportion to the change; consult the [testing guide](../development/testing.md) for the maintained command matrix.
