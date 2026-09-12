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

The gallery header displays Tagrove with the Obsidian Grove SVG from `public/tagrove.svg`. The image supplies the accessible app name; the adjacent visible wordmark is hidden from assistive technology to avoid repeating it. The same SVG is the HTML favicon and the source for native PNG icons. See [branding assets](../development/setup-and-build.md#branding-assets) for regeneration and desktop metadata.

[`src/main.tsx`](../../src/main.tsx) is the browser bootstrap. It imports i18n and global CSS before rendering, applies the stored light/dark theme before the first React render, optionally installs performance instrumentation when `VITE_MEDIATAGGER_PERF=1`, and mounts `<App />` inside `React.StrictMode`.

Before React renders, the bootstrap installs a window-level capture listener that calls `preventDefault()` on every `contextmenu` event. Native WebView context menus are disabled in development, E2E, and production, including images, editable fields, empty areas, and modal or other content outside the React root. Descendant handlers that stop propagation cannot bypass suppression. Mouse and keyboard context-menu invocation share this policy; event propagation, ordinary clicks, typing, and clipboard shortcuts remain available. Vite hot-module disposal removes the listener before the bootstrap reloads.

Strict Mode means mount effects must tolerate setup, cleanup, and setup again during development. Effects that install listeners, animation frames, timers, observers, or async generations must provide cleanup or stale-result protection. Production behavior must not depend on an effect running exactly once merely because it has an empty dependency list.

[`src/App.tsx`](../../src/App.tsx) is intentionally a thin composition surface:

1. `useAppShellController()` creates the long-lived application state and callbacks.
2. `App` chooses either `AppGalleryView` or `AppSettingsView`.
3. The gallery view conditionally mounts the bulk sidebar, while `App` activates the lazy lightbox on first selection and leaves its presence owner mounted for later closing fades.
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
| Tag suggestions | Shell `SuggestionProvider` and its lazy worker | One worker across editors; versioned vocabulary/requests and per-editor pending coalescing; terminated on shell unmount. |
| Known tags | `useLibraryKnownTags`, composed by `useLibraryBrowser` | Shell lifetime; refreshed independently from asset pages. |
| Thumbnail paths, rendering IDs, and queue progress | `ThumbnailStore` and `useThumbnailQueue` | Shell lifetime; reset when query/library lifecycle requires it. |
| Authoritative complete tag lists, identity epoch, and per-asset mutation generations | `useAssetTagState` | Shell lifetime; shared by lightbox and bulk. Mutation barriers are exclusive per asset, start before IPC, and reject details from before/during a pending write. A caller that cannot acquire every required asset sends no IPC and releases any locks already acquired. Restore/clear advances the identity epoch; deletion leaves a per-ID invalidation generation. Active editors and mutations pin records; inactive canonical tags and clocks share a 512-entry retention bound with globally increasing generations. |
| Selected asset, loaded details, lightbox editor drafts, and serialized tag saves | `useSelectionState`, `useMediaGroupDraft`, `useSelectionNavigation`, and `useSelectionMetadataActions` | Shell lifetime; overlays shared authoritative tags onto its local detail cache, blocks replacement while the complete tag base is unknown, and clears selection-scoped state when appropriate. |
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
- the scan-root list and known tags are hydrated on shell mount, which also invokes the process-guarded startup scan through the settings runner; applied search filters drive library refreshes from shell effects.

Settings closes through Back or the layer registered by `SettingsViewLayer` in `App.tsx`. `useSettingsView` owns only the open boolean and callbacks. A normal settings open starts without a highlighted scan section; the empty-library "add first folder" route opens the same view with the scan section highlighted. Closing the view clears that transient highlight.

## Lazy loading and delayed prefetch

The gallery is eagerly imported. Settings, the bulk sidebar, and lightbox use `React.lazy`. Settings shows a localized status while loading; lightbox shows a named loading dialog. `LazyErrorBoundary` supplies localized failure UI with reload and Back/Close actions. Its reset key follows settings visibility or lightbox opening/asset changes, preserving a closing failure dialog until its fade ends; resetting the boundary alone does not guarantee a fresh download of a rejected lazy module.

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
- `UiSegmentedControl` owns controlled native radio groups with translated labels, independent names, and content-based wrapping. An accessibility-hidden bold copy reserves each label’s selected width. The media filter uses it; filter submission stays in `SearchTagatorWrapper`.
- `UiModal` is the standard portal dialog primitive described below.
- `browserAssistDisabledProps` is shared by exact-token/confirmation inputs where browser autocorrection would be harmful.

[`src/styles.css`](../../src/styles.css) imports Tailwind CSS, disables DaisyUI's stock theme set, and declares the app's `light` and `dark` DaisyUI themes. Components combine Tailwind utility classes, DaisyUI component classes such as `btn`, `progress`, and `loading`, and app CSS variables.

Prefer semantic DaisyUI colors (`primary`, `base-*`, `error`, and similar) and the app tokens for surfaces, borders, radii, shadows, fields, and window chrome. Reuse `--surface-*`, `--border-*`, `--radius-*`, and `--shadow-*` rather than adding isolated literal values for the same role. Keep theme differences in the theme blocks, not conditional class lists in components. Pass `className` for layout or a deliberate contextual adjustment; shared visual behavior belongs in the primitive.

Global styles establish full-height roots, typography, field treatment, visible keyboard focus for buttons, links, inputs, and selects, and themed scrollbars. Preserve native elements where possible so keyboard and accessibility behavior come for free.

## Motion

`src/styles.css` owns the shared motion tokens: `--motion-feedback` is 100 ms,
`--motion-entrance` is 150 ms, `--motion-exit` is 100 ms, and `--motion-ease` is a restrained ease-out curve.
Buttons, interactive chips, search options, media segments, settings navigation and
theme choices transition background, border color, text color and shadow. Keyboard
focus indicators remain immediate. DaisyUI button presses do not translate controls.

Use `motion-enter` for an opacity-only entrance after the element's layout is ready.
Positioned suggestion lists, bulk inspector sections and inline lightbox
information/confirmation use it. Suggestions keep the same mounted
list while typing, hiding it and pausing the fade while worker results are empty.
Their fade starts only once results and viewport positioning are available.
`UiModal` and `LightboxModal` use `useModalPresence` instead. Their backdrop and DOM
content fade together using the shared easing. `UiModal` opens in 150 ms and closes
in 100 ms. The lightbox uses local `--modal-entrance` and `--modal-exit` tokens for
75 ms opening and 50 ms closing, leaving inline entrances and other motion unchanged.
The hook tracks opening, open, closing and hidden states. Interrupted
animations cancel their completion callbacks and fallback timers, so stale cleanup
cannot remove a reopened dialog. Animations and timers are cleaned on unmount.
A fullscreen image shell receives its own fade because browser top-layer elements
ignore ancestor opacity. Native video playback and GTK controls keep their existing
startup/shutdown behavior, while the WebView backdrop and dialog controls fade.

Keep modal owners mounted when their logical `open` becomes false. The standard
modal retains its last visible children and presentation props until exit completes;
the lightbox retains its asset/editor props. Asset navigation keeps the same presence
owner and does not replay the entrance. On logical close, release the layer, focus
trap, background inertness and body scroll lock immediately. The closing content is
inert and aria-hidden; the backdrop remains a pointer shield until removal. Reduced
motion shows and removes dialogs immediately, including when the preference changes
during a fade. No persistent image compositing hints are added.
Gallery hover/selection shadows and the checkmark opacity use control feedback.
Tiles, thumbnail arrivals, reordering and virtual scrolling have no entrance animation.

Outside the segmented highlight described below, never transition dimensions, spacing,
offsets or grid tracks, or add scale-up, bounce, stagger, delayed layout or automatic scrolling. Preserve scroll containers, gutters,
portal positioning and clipping; do not hide overflow globally to conceal a defect.
Functional layout changes remain immediate. Other decorative motion needs no React state,
effect or timer. The lightbox's existing clipped 200 ms drawer slide and matching
retention timer remain the exception, including native-video bounds coordination.
Keep media, zoom/pan transforms, fullscreen geometry and native containers outside
decorative motion. DOM controls still receive shared feedback.

`UiSegmentedControl` is a bounded exception. Only its non-interactive, absolute-positioned
highlight transitions translation, width and height over 150 ms with `--motion-ease`.
Labels and adjacent controls keep their geometry when selection changes. Measurement
stays inside the component: a layout effect places the highlight before paint, and
`ResizeObserver` snaps it after wrapping, resizing or font changes. Translated-label
changes also snap. Native radio selection and external controlled-value updates slide
from the current highlight position, including during rapid changes. Observers disconnect
on cleanup. No dependency, timer or application state coordinates this animation.

Under `prefers-reduced-motion: reduce`, decorative entrances and transitions stop;
loading spinners and pulses continue. State feedback and existing modal focus,
inertness and keyboard handling remain available. The desktop animation regression
in `e2e/specs/animations.e2e.js` samples live scroll geometry across interactions and
compares layout-changing actions with animations disabled.

## Light and dark visual system

Both themes share geometry and typography. The palette is defined in `src/styles.css`:

| Role | Light | Dark |
| --- | --- | --- |
| Workspace | `#F4F6F5` | `#141618` |
| Toolbar and panels | `#FFFFFF` | `#202224` |
| Raised controls | `#E8EEEB` | `#232528` |
| Dividers | `#CCD7D0` | `#2C2E31` |
| Main text | `#17241E` | `#F4F6F5` |
| Secondary text | `#52635A` | `#A5B0AA` |
| Primary green | `#087F5B` | `#10B981` |

The root stays at 16px for Tailwind rem sizing; body copy is 13px, utility labels 12px,
and headings 18/24px. Inter and JetBrains Mono are bundled under `public/fonts/` with
SIL OFL licenses. Inter covers the interface; paths and metadata use JetBrains Mono.
System fonts remain fallbacks for other scripts. Tile corners are 4px, controls 6px,
and panels/dialogs 8px. Buttons are 32px high. Green text in the light theme uses
`--color-primary-text: #076C4D` for 4.5:1 contrast on raised controls, while filled
buttons retain the specified `#087F5B` accent and white text. Both themes use the same geometry.

`TopBar` composes the identity, search field and Settings action in a 52px desktop row.
The second row targets 44px and contains media kind, favorites, tag browsing, the result
count, bulk mode and thumbnail sizing. Media kind is a native radio group styled as a
segmented selector, with localized All, Images, GIF and Video choices. The track uses a muted
background and an inset border; the selected segment has a solid primary-green fill and
primary-content text. Only unselected segments receive the raised hover background. Arrow keys change
kind; the existing parent-commit search semantics are preserved. Both rows belong to a
full-width, non-scrolling header and can grow when controls wrap, with search on its own
row below 700px. The gallery scroll viewport fills the remaining height. Ctrl+K / Cmd+K focuses the existing search input without changing or applying its draft. An open modal or inert search field blocks the shortcut. The placeholder gives one valid query; the field tooltip separates further examples onto individual lines.
The gallery retains square tiles and measured 12px gaps. Bulk selection uses a green
outline plus a checkmark; neutral, opaque media-kind badges remain readable over thumbnails.
Group backings retain their dedicated `--gallery-group-bg` and `--gallery-group-edge` tokens.

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

Full-page settings uses 240px navigation beside mounted sections at widths of at least
1000px, and wrapping navigation above the content below that width. Navigation focuses and
scrolls to the corresponding section without changing the URL or unmounting operation UI.
The active marker follows the scroll position; settings section choice is not persisted.
Sections appear in scan, appearance, language, import/export, duplicates, and danger-zone order.
Operations pair descriptions with actions in compact rows, stacking below 700px.
Appearance uses native light/dark radio choices with decorative CSS previews. Only the
selected choice is a Tab stop; arrow keys select the adjacent choice. The preference values,
dark default, storage service and native theme synchronization are unchanged.

Keep shared field rules in the CSS base layer so component utilities can make deliberate
adjustments. Avoid adding nested field outlines, panel blur, decorative gradients, or
pill-shaped buttons. Native video controls retain their contrast gradient over the picture.

## Modal and overlay conventions

[UiLayerProvider](../../src/components/UI/UiLayerProvider.tsx), mounted by `App`, owns the DOM layer stack. [UiModal](../../src/components/UI/UiModal.tsx) and the specialized lightbox register modal layers and portal into `document.body`. `SettingsViewLayer` registers a nonmodal layer for page-level Escape. While bulk mode is enabled, the gallery registers a nonmodal layer that clears selection and cancels rectangle work on Escape. A modal above it receives Escape first.

- Only the top registered layer receives managed Escape dismissal. A locked top layer does not select a lower layer as a fallback. UiModal backdrop dismissal also checks top-layer ownership.
- A top modal makes `#root` and lower registered modal elements inert and aria-hidden, and locks body scrolling. Cleanup removes those attributes and restores the previous body overflow.
- Initial focus selects `[data-initial-focus]`, then the first tabbable descendant, then the container. Tab and Shift+Tab wrap inside the top modal.
- Unregistering the top layer restores the supplied trigger or the previously focused connected element. If unavailable, it tries the next layer's initial focus.
- `UiModal` requires exactly one of `ariaLabel` or `labelledBy` in its props and renders `role="dialog"` with `aria-modal="true"`. It supports four width presets and independently configurable Escape/backdrop closing.

Provide translated names and explicit close actions. Register new overlays with the shared manager instead of adding independent global Escape listeners. Lightbox image/native fullscreen and inline confirmation still have specialized key handling, described in [lightbox](../subsystems/lightbox.md#fullscreen-and-keyboard-behavior).

## Keyboard and accessibility

Native controls, translated accessible names, pressed states, listbox suggestions, alerts, and status regions are the normal conventions. Global styles provide immediate focus-visible feedback and a reduced-motion rule that disables decorative motion and smooth scrolling while preserving loading indicators.

The layer manager's tabbable selector excludes disabled controls, hidden elements, and inert ancestors, but it does not perform a complete computed-visibility or browser tab-order calculation. Standalone components rendered without the provider receive only fallback Escape handling. Tests that claim focus trapping or nested dismissal must mount the provider.

GTK video controls are native widgets outside the DOM layer manager. Changes to video bounds, fullscreen, or sidebar visibility need desktop verification as well as DOM tests. See [native media presentation](../subsystems/lightbox.md#media-presentation).

Bulk group drag handles support ArrowUp/ArrowDown reordering. The larger group-order modal also supports ArrowLeft/ArrowRight for adjacent positions, with Up/Down moving by grid row. It uses the shared modal layer, keeps changes in a local draft until Save order, and restores the triggering button on close. `UiProgressBar` uses its supplied label as its accessible name. Component tests cover selected roles and focus transitions; they do not establish full screen-reader, contrast, zoom, or keyboard-only accessibility.

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
