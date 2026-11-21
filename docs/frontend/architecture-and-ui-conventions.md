# Frontend architecture and UI conventions

Implementation entry points: [app composition](../../src/app/App.tsx), [application runtime](../../src/app/createAppRuntime.ts), [UI layer manager](../../src/ui/overlays/UiLayerProvider.tsx), [styles](../../src/styles.css).

Solid 2 is the only frontend. Source and executable configuration establish current
behavior. The dependency directions and coding rules below are intended conventions;
the tests listed here verify specific behavior, not every architectural rule.

Feature contracts live in [gallery and queries](../subsystems/library-query-and-gallery.md),
[search and editing](../subsystems/search-tags-and-media-groups.md),
[lightbox](../subsystems/lightbox.md), [settings](../subsystems/settings-operations.md),
[IPC](../architecture/ipc-contract.md), [localization](../development/localization.md),
and [testing](../development/testing.md).

## Bootstrap and shell composition

[`main.tsx`](../../src/app/main.tsx) imports global CSS and calls
[`mountApplication`](../../src/app/mountApplication.tsx) for `#root`. Bootstrap applies
the stored theme before rendering and waits for `i18nReady`. Preference storage failures
do not prevent module initialization or mounting. It installs window capture
suppression for native context menus, including editable fields and portals outside the
root. The listener only prevents the default menu; ordinary event propagation and clipboard
shortcuts remain available. `VITE_MEDIATAGGER_PERF=1` enables a PerformanceObserver for
long tasks and measures where supported. Disposal cancels a pending mount, unmounts Solid,
disconnects the observer and removes the listener. Vite HMR uses this disposer.

[`App`](../../src/app/App.tsx) nests `LanguageProvider`, `AppRuntimeProvider` and
`UiLayerProvider`. The runtime provider constructs one
[`createAppRuntime`](../../src/app/createAppRuntime.ts) under the application owner.
Context exposes those models without creating another state owner. `onSettled` calls the
idempotent `runtime.start()`, which refreshes library data,
invokes startup scanning and refreshes scan roots. Backend startup-scan consumption remains
a separate process-wide guarantee.

The gallery header uses `public/tagrove.svg` as its accessible app identity; the adjacent
wordmark is hidden from assistive technology. The same SVG supplies the favicon and native
icon source. See [branding](../development/setup-and-build.md#branding-assets).

## Directory ownership and dependency direction

| Directory | Responsibility |
| --- | --- |
| `src/app` | Bootstrap, feature composition, view choice, runtime context, lazy boundaries and window controls. |
| `src/platform/tauri` | Typed command payloads, channels, events, native dialogs, media URLs and window operations. Wire types live in `contracts/`. It does not import feature state or UI. |
| `src/library` | Query/page cache, shared details, metadata coordination/mutations and vocabulary. It does not import feature components. |
| `src/features/gallery` | Own virtualizer (`gridGeometry`, `createVirtualGrid`), tiles, gesture geometry and selected IDs. |
| `src/features/search` | Search drafts, applied descriptors, suggestions/worker and tag-list browser. |
| `src/features/tagging` | Shared `TagEditor`, consumed by lightbox and bulk. |
| `src/features/bulk` | Selected metadata, group drafts, the floating bulk dock, the order board and the order dialog. |
| `src/features/lightbox` | Selected asset and navigation model, presentation, image controls and native video session. |
| `src/features/settings` | Operation models, progress, section views and confirmations. |
| `src/ui` | Reusable controls, DOM behavior and overlay ownership. It does not import feature or library code. |
| `src/i18n` | Resources, initialization, persistence and the Solid language binding. |
| `src/utils` | Framework-free helpers: tag normalization (`media.ts`) and search-token quoting (`searchTokens.ts`). |
| `src/test` | Shared DOM/Tauri mocks and owned test helpers. Tests otherwise live beside their owners. |

Feature views consume explicit models and callbacks. Application-wide composition belongs
in `app`; common persistence and cache coordination belong in `library`. Keep deterministic
helpers beside their domain, such as `filterService`, `selectionGeometry` and
`lightboxPanZoomService`. Components may manage local DOM interactions and call platform
adapters for view-owned actions such as clipboard/window behavior, but must not construct
raw invoke payloads. Follow the [IPC contract](../architecture/ipc-contract.md) when a
boundary changes.

## State ownership and lifetime

| State | Owner | Lifetime and reset |
| --- | --- | --- |
| View, scan highlight, theme and tile size | `createAppRuntime` | Application lifetime. Tile size starts at 188; only theme is persisted. Closing settings clears the highlight. |
| Search draft/applied descriptor | `features/search/createSearch` | Application lifetime. Commands keep synchronous drafts and publish reactive state; every valid submission invokes the library once. |
| Sparse query pages and navigation snapshot | `library/query/createAssetQuery` | Application lifetime; 128-item pages and at most 24 pages, mutated in place behind a `version` signal. Generations discard obsolete responses. |
| Complete details | `library/metadata/createDetailsCache` | One shared 256-entry LRU for lightbox and bulk. Query changes clear it; identity/generation checks reject stale metadata. Concurrent reads for one ID share a promise. |
| Canonical tags and metadata write ownership | `library/metadata/createAssetTagState` | Application lifetime; per-ID FIFO write queue with exclusive tokens ([ADR 0001](../adr/0001-per-asset-metadata-write-queue.md)), identity epoch, deletion invalidation and maintenance barrier. Active users pin records; inactive records have a 512-entry retention bound. |
| Known tags and suggestion worker | `createKnownTags` and runtime `SuggestionClient` | Application lifetime. The worker starts lazily, coalesces requests per editor and terminates on disposal. |
| Thumbnail loading | Each `ThumbnailImage` (`thumb://` URL from `thumbnailUrl`) and WebKit's image cache | Per mounted image. No frontend queue or store; load, retry and failure state belong to the current `src`. |
| Selected IDs and range anchor | `features/gallery/createSelection` | Survives view/query changes and page eviction. Query changes cancel ranges and clear the anchor; identity reset clears IDs. |
| Bulk metadata and group drafts | `features/bulk/createBulk` | Application lifetime; drafts reset when the selected-ID set changes. The mounted dock owns tag input/focus, its tab, the inline order draft and the order dialog. |
| Lightbox selection, navigation and save intents | `features/lightbox/createLightbox` | Application lifetime; close resets selection state, identity reset discards pending intents. |
| Lightbox sidebar, activity, confirmation and image controls | `createLightboxPresentation.ts`, `LightboxModal`, `image/createImageControls` | Mounted presentation lifetime; survives asset navigation where appropriate, resets on close/reopen. |
| Native playback | `video/createNativeVideoSession` | Active presentation session; logical close retires playback before the retained modal exits. |
| Settings locks, roots, progress and confirmations | `features/settings/createSettings` and its focused models | Application lifetime, including while the settings view is unmounted. |
| Language | i18next core and `LanguageProvider` | Singleton resources/persistence; one language subscription per provider tree, removed on disposal. |
| Geometry, gestures, popovers and local drafts | Mounted feature/UI owner | Observers, listeners, pointer capture, timers and frames are released with that owner. |

## Reactivity and cleanup

Models expose read-only reactive getters/accessors and explicit commands. Signals publish
independent values; memos derive state without effects copying values between owners.
Read reactive props inside JSX, memos or effect computations. Destructuring a stable model
object is safe; destructuring its current reactive values at component initialization freezes
them. Keep transport DTOs separate from editor drafts and the lightbox `SelectedAsset` view.

Solid 2 effects use compute/apply functions. Track dependencies in the computation and put
external writes or registrations in apply; return registration cleanup or use `onCleanup`
for the owning scope. Use `onSettled` for work that needs attached DOM. Ref callbacks capture
elements; subscriptions and async work still need an owner and disposal path.

Signal publication is batched. Admission locks, request generations, pending registries,
confirmation snapshots and navigation targets use synchronous local state so repeated
commands in one turn cannot dispatch duplicate writes or read an old target. Derive command
inputs before publishing; do not add production `flush()` calls to repair sequencing.
User-visible values still publish through reactive state. Tests use explicit scheduling
boundaries, deferred promises, disposal and reopen checks.

Late async listener registration must remove its listener immediately if the owner has
already disposed. Query/detail replies and video channels use generations or tokens
to prevent stale publication. Disposal does not imply cancellation of already-dispatched
Rust work; document cancellation separately for each operation. Lightbox image load callbacks
also check the keyed source activation, current element and active/disposed controls owner;
a DOM ref must not be reassigned by a late load event.

## Gallery and settings lifecycle

The gallery and settings are mutually exclusive children of `<main>`. View switching
unmounts DOM and local state while runtime models remain alive. Gallery owns a scroll
element below its header; settings scrolls in `<main>`. Neither saves scroll position
across remounts. Grid measurements and observers are recreated on return. Bulk selection
survives even though its dock unmounts.

`SettingsView` in `App.tsx` registers a nonmodal layer for Escape. Back and Escape call
`runtime.closeSettings`. The empty-library first-folder action sets the scan highlight
before opening settings. Settings operations continue while the gallery is visible.

## Lazy loading and delayed prefetch

`AppGalleryView` eagerly imports gallery, search and the bulk dock. Settings view and
`LightboxModal` use Solid `lazy`; their models are already in the runtime. `Loading` provides
a localized status for settings and a named loading dialog for lightbox. `LazyErrorBoundary`
and `LightboxBoundary` use `Errored` for localized failure UI and reload/Back/Close actions.
A new lightbox activation resets a failed view, including reopening the same ID; healthy
navigation retains its presentation owner. Reload remains the explicit recovery for a
rejected module download.

App schedules settings and lightbox imports after 1,500 ms and cancels the timer on disposal.
Opening a view sooner begins its import immediately. The lightbox presence owner stays
mounted after first activation so its closing content can finish the exit animation.

## Successful mutations and local patches

`library/metadata/createMutations.ts` queues each write per asset, starts it once earlier
writes on its IDs settle, awaits the platform command, publishes canonical tag results and patches loaded summaries/shared details.
`queryImpact.ts` evaluates the filter active when the write settles. Known-tag refresh is
best effort; group changes always refresh query order. Favorites refresh when membership
requires it. Feature models retain drafts, pending states and retry intent.

A summary or failed detail request cannot establish a complete tag base. Editors disable
replacement until canonical tags are known. An unsuccessful command makes no success patch.
Partial bulk results publish only acknowledged IDs and trigger missing-ID recovery.

Settings CSV import, restore and clear use `library.tags.runWithMutationBarrier` to deny
new metadata writes and drain admitted writes, both queued and dispatched. They invalidate affected identity/cache
state inside the barrier on both success and rejection, then refresh after release.
Restore resets the query; clear also empties vocabulary. See the exact
[settings reset matrix](../subsystems/settings-operations.md#exact-success-refresh-and-reset-matrix).
These frontend rules do not replace backend transactions or workflow locks.

## Runtime appearance and language effects

Theme and language are the only frontend preferences persisted in local storage:

- `media-tagger.theme` accepts `light` or `dark`, defaulting to `dark`. Bootstrap and
  runtime changes use `ui/dom/theme.ts`, which sets `<html data-theme>`, saves the preference
  and best-effort synchronizes the native window. Unavailable storage or failed reads use
  `dark`. Failed writes do not block DOM, runtime state or native theme changes.
  Native theme errors are ignored.
- `media-tagger.language` uses the supported language registry, then browser language,
  then English. `i18n/core.ts` owns initialization and persistence to storage and `<html lang>`.
  `i18n/solid.tsx` provides the reactive translator and normalized language accessor.
  Unavailable storage or failed reads fall back to browser language, then English.
  Failed writes do not interrupt language listeners, reactive UI updates or `<html lang>`.

Both owners catch storage-property access and storage-method errors at the preference
boundary. A failed write leaves the selected preference active for the current runtime;
it does not guarantee persistence on the next launch. Bootstrap and settings callers
use these same owners without additional storage guards.

Extend these owners for preference changes rather than reading storage in leaf components.
The [localization guide](../development/localization.md) owns resource invariants.

The GPUI port keeps the same two preferences in `ui-preferences.json` in its profile
directory (`{"theme":"light","language":"pl"}`), behind the `PrefsStore` port of
`media_tagger_client::prefs` (plan decision D6). `FilePrefsStore` writes a staging file
`.mediatagger-ui-preferences-<pid>-<n>.pending`, flushes it, renames it over the old file
and flushes the directory. Reading never fails: a missing, unreadable or damaged file
reads as nothing stored, and each member is read on its own like the two storage keys
(a theme other than exactly `light` or `dark` is ignored, a language is normalized).
The defaults are the ones above: dark, then system language, then English.

## UI primitives and styling

Reusable controls live in [`src/ui/controls`](../../src/ui/controls); modal primitives live in [`src/ui/overlays`](../../src/ui/overlays):

- `UiButton` supplies consistent sizing, variants, disabled styling, and `type="button"` by default.
- `UiIconButton` layers active and danger states over `UiButton`; callers still provide an accessible name.
- `UiIcon` centralizes the SVG set. Icons with no title or ARIA metadata are decorative and receive `aria-hidden`; a title or explicit ARIA metadata makes the icon expose image semantics.
- `UiChip`, `UiAlert`, `UiProgressBar`, and `UiSlider` centralize common display/control styling. `UiAlert` always uses `role="alert"`.
- `UiSegmentedControl` owns controlled native radio groups with translated labels, independent names, and content-based wrapping. An accessibility-hidden bold copy reserves each label’s selected width. The media filter uses it; filter submission stays in `SearchTagatorWrapper`.
- `UiModal` is the standard portal dialog primitive described below.
- `TagEditor` composes assigned chips and a borderless `SearchTagator` input inside one wrapping field. Callers supply controlled tags, draft, accessible labels, disabled/loading state, and mutation callbacks. Blank field space focuses the input; input focus highlights the field. The shared suggestion Retry stays mounted when focus leaves the input for the button. Activating it restores input focus and opens a new request; ordinary blur still closes the suggestion list. The field scrolls to keep the focused input visible after chip changes and resizing.
- Exact-token and confirmation inputs disable browser autocorrection in their owning components.

[`src/styles.css`](../../src/styles.css) imports Tailwind CSS, disables DaisyUI's stock theme set, and declares the app's `light` and `dark` DaisyUI themes. Components combine Tailwind utility classes, DaisyUI component classes such as `btn`, `progress`, and `loading`, and app CSS variables.

Prefer semantic DaisyUI colors (`primary`, `base-*`, `error`, and similar) and the app tokens for surfaces, borders, radii, shadows, fields, and window chrome. Reuse `--surface-*`, `--border-*`, `--radius-*`, and `--shadow-*` rather than adding isolated literal values for the same role. Keep theme differences in the theme blocks, not conditional class lists in components. Pass `className` for layout or a deliberate contextual adjustment; shared visual behavior belongs in the primitive.

Global styles establish full-height roots, typography, field treatment, visible keyboard focus for buttons, links, inputs, and selects, and themed scrollbars. Preserve native elements where possible so keyboard and accessibility behavior come for free.

The GPUI port carries the tokens of `src/styles.css` as Rust constants in `media_tagger_client::theme` (`theme/tokens.rs`): the colours of both theme blocks with the `:root` aliases resolved (`--bg-*`, `--surface-*`, `--window-chrome-bg`, the `color-mix()` tokens), radii, shadows, motion timings and the type scale (`text-xs` and `text-xl` and every line height are Tailwind defaults that `styles.css` does not override). `theme/parity_tests.rs` parses the stylesheet and fails when a value differs on either side or when a theme block, `[data-theme]` block or `:root` gains a custom property the constants do not carry; change a token in both places in one change.

### GPUI port: the text field

GPUI has no text field, so the port has one of its own ([ADR 0038](../adr/0038-gpui-text-field-own-element-and-key-context.md)), in two layers:

- `media_tagger_client::text` is the editing model without a GUI. `LineEditor` holds the text, the selection (anchor and head, UTF-8 byte offsets), the text an input method is composing and the undo history. The caret moves by extended grapheme clusters and by words (UAX #29, `text/boundary.rs`); `text/utf16.rs` converts to and from the UTF-16 offsets of the platform's input protocol, and every entry point moves an offset to a character boundary before it is used. `KEYMAP` is the table of keys, `scroll_to_reveal` the horizontal shift that keeps the caret visible, and `field_visuals` the colours, focus ring and padding of the `input` rules of `src/styles.css` (its tests fail when those rules change).
- `media_tagger_gpui::ui::text_input` draws it. `TextInput` is an entity that owns one `LineEditor`: it shapes the line, paints selection, text and caret clipped to the field, shows the placeholder while the text is empty, and connects keys, pointer (click, Shift+click, drag, double click for a word, triple click for everything), clipboard and GPUI's input handler to the model. `FieldChrome::Bare` drops the frame for a field inside a container that draws its own, as the tag editor does. A disabled field is dimmed, takes no input and cannot be focused.

The field is the only holder of the text being edited. Its owner listens for `TextInputEvent::Changed` and `SelectionChanged` and reads `text()` and `cursor()`; it writes with `set_text` (caret to the end, history dropped, no change when the text is the same) and `replace_range` (one undo step, caret behind the inserted text: how a suggestion completes a token). `set_accept` restricts what can be entered, which the group order uses for digits.

Inserted text is one line (`text::single_line`): line breaks at the end are dropped, every other line break becomes one space, with `\r\n` counting once, a tab becomes a space, and other control characters are dropped. This applies to typing, paste, composition and `set_text`. WebKit treats pasted line breaks the same way; it keeps a pasted tab.

Undo steps: consecutive typing is one step, a word typed after whitespace starts the next; a run of Backspace presses is one step, and so is a run of Delete presses; paste, cut, word deletion, typing over a selection and a finished composition are steps of their own; moving the caret closes the current step. A field keeps 100 steps.

Not built: a multi-line field, a blinking caret, middle-click paste of the primary selection, auto-scroll while a drag is held outside the field, and select-all on Tab focus.

A field made with `TextInput::themed` draws with the active theme and follows it when it changes; `TextInput::new` keeps the theme it is given.

### GPUI port: theme, controls and overlays

The port's counterparts of `src/ui/controls` and `src/ui/overlays` ([ADR 0039](../adr/0039-gpui-overlay-layers-and-controls.md)) are split like the text field:

- `media_tagger_client::controls` resolves each control's looks from the theme tokens and the Tailwind and DaisyUI classes of the Solid control (`button_visuals`, `icon_button_visuals`, `chip_visuals`, `alert_visuals`, `progress_visuals`, `slider_visuals`, `segmented_visuals`, `switch_visuals` for `toggle toggle-sm toggle-primary`, `spinner_visuals`) and holds what needs no GUI: `SliderRange` (step snapping, the keys of a range input: arrows by a step, Page keys by a tenth of the range, Home, End), `SegmentedKey` (arrows wrap), `HighlightTrack` (a port of `measureSegmentedHighlight`: a new selection slides, a changed layout or label snaps) and `DelayedReveal` (`.thumbnail-loader`: 300 ms invisible, then a 150 ms ease-out fade). Their tests read `src/styles.css` and the control sources and fail when a copied class changes. `media_tagger_client::icons` names the 22 icons of `UiIcon`; `media_tagger_client::overlay` holds the layer stack and the modal presence.
- `media_tagger_gpui::ui` draws them. `theme` keeps the active theme as a GPUI global (`set_active` redraws every window) and converts tokens; every control reads the active theme unless given one with `.theme(..)`. `fonts::register` adds Inter and JetBrains Mono from `public/fonts`. `assets::Assets` serves the icon files in `src-gpui/crates/media_tagger_gpui/assets/icons/` (one SVG per icon, the stroke style of `UiIcon`; the filled variant is derived on load, and a test compares every file with `UiIcon.tsx`). Controls: `Button` (`Button::new` for `UiButton`, `Button::icon` for `UiIconButton` with `active` and `danger`), `Chip`, `Alert` (role `Alert`), `ProgressBar`, `Slider`, `SegmentedControl`, `Switch`, `Spinner`, `Icon`.

The slider and the segmented control are entities and are controlled: they emit `SliderEvent::Input` and `SegmentedEvent::Change` and draw the value their owner sets, so an owner that refuses a value keeps the old one on screen. A segmented group is one tab stop (the selected segment), the arrow keys move the focus around it and ask for the segment they land on, and the highlight slides only by position and size, never by layout. A click or Space asks only for a segment that is not selected. A switch asks with `on_change` and shows `checked` as given. Buttons, switches and segments are tab stops while enabled and draw the `:focus-visible` outline (a ring outside the element at the rule's offset) only after keyboard input. Enter and Space activate a focused button or switch (GPUI's keyboard click).

GPUI draws an outer box shadow beneath the whole element, where CSS draws it only outside the border box, so a button whose fill lets the background through (an active icon button, a disabled button) draws without its outer `--shadow-control` (`ui::theme::shadows_beneath`); its colour inside matches the browser's. The segmented highlight is measured while a frame is prepainted, where a notify does not redraw the window, so a new position is drawn with the next frame.

Not built in GPUI: letter spacing (`tracking-[-0.01em]` of buttons), the multi-turn dash animation of DaisyUI's `loading-spinner` (the arc turns at a fixed length), and the virtual list of `ui/dom/createVirtualList` (epic E13).

## Motion

`src/styles.css` owns the shared motion tokens: `--motion-feedback` is 100 ms,
`--motion-entrance` is 150 ms, `--motion-exit` is 100 ms, and `--motion-ease` is a restrained ease-out curve.
Buttons, interactive chips, search options, media segments, settings navigation and
theme choices transition background, border color, text color and shadow. Keyboard
focus indicators remain immediate. DaisyUI button presses do not translate controls.

Use `motion-enter` for an opacity-only entrance after the element's layout is ready.
Positioned suggestion lists, the bulk dock and its tab panels and inline lightbox
information/confirmation use it. Suggestions keep the same mounted
list while typing, hiding it and pausing the fade while worker results are empty.
Their fade starts only once results and viewport positioning are available.
`UiModal` and `LightboxModal` use `createModalPresence` instead. Their backdrop and DOM
content fade together using the shared easing. `UiModal` opens in 150 ms and closes
in 100 ms. The lightbox uses local `--modal-entrance` and `--modal-exit` tokens for
75 ms opening and 50 ms closing, leaving inline entrances and other motion unchanged.
The presence model tracks opening, open, closing and hidden states. Interrupted
animations cancel their completion callbacks and fallback timers, so stale cleanup
cannot remove a reopened dialog. Animations and timers are cleaned on unmount.
A fullscreen image shell receives its own fade because browser top-layer elements
ignore ancestor opacity. Native video playback and GTK controls keep their existing
startup/shutdown behavior, while the WebView backdrop and dialog controls fade.

Keep modal owners mounted when their logical `open` becomes false. The standard
modal (`UiModal`) retains only its presentation props (`ariaLabel`, `labelledBy`, `describedBy`, `size`, `className`, `contentClassName`) until exit completes and renders its children live, so an owner whose content depends on state that changes at close snapshots that content itself with `createModalSnapshot`;
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
Functional layout changes remain immediate. Other decorative motion belongs in CSS. The lightbox's existing clipped 200 ms drawer slide and matching
retention timer remain the exception, including native-video bounds coordination.
Keep media, zoom/pan transforms, fullscreen geometry and native containers outside
decorative motion. DOM controls still receive shared feedback.

`UiSegmentedControl` is a bounded exception. Only its non-interactive, absolute-positioned
highlight transitions translation, width and height over 150 ms with `--motion-ease`.
Labels and adjacent controls keep their geometry when selection changes. Measurement
stays inside the component: the owned `segmentedHighlight` DOM helper places the highlight after attachment, and
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
| Favorite heart | `#C64B57` | `#D95D67` |

`UiIconButton` uses `--color-favorite` for active hearts in the favorites filter,
bulk favorites, and lightbox favorite control. The filled heart uses the solid color,
with a 12% background tint and a 40% border. Other active icons keep their primary
color treatment. Disabled opacity and keyboard focus follow the shared button styles.

The root stays at 16px for Tailwind rem sizing; body copy is 13px, utility labels 12px,
and headings 18/24px. Inter and JetBrains Mono are bundled under `public/fonts/` with
SIL OFL licenses. Inter covers the interface; paths and metadata use JetBrains Mono.
System fonts remain fallbacks for other scripts. Tile corners are 4px, controls 6px,
and panels/dialogs 8px. Buttons are 32px high. Green text in the light theme uses
`--color-primary-text: #076C4D` for 4.5:1 contrast on raised controls, while filled
buttons retain the specified `#087F5B` accent and white text. Both themes use the same geometry.

`SearchTagatorWrapper` composes the identity, search field and Settings action in a 52px desktop row.
The second row targets 44px and contains media kind, favorites, tag browsing, the result
count, bulk mode and thumbnail sizing. Media kind is a native radio group styled as a
segmented selector, with localized All, Images, GIF and Video choices. The track uses a muted
background and an inset border; the selected segment has a solid primary-green fill and
primary-content text. Only unselected segments receive the raised hover background. Arrow keys change
kind; the existing parent-commit search semantics are preserved. Both rows belong to a
full-width, non-scrolling header and can grow when controls wrap, with search on its own
row below 700px. The gallery scroll viewport fills the remaining height. Ctrl+K / Cmd+K focuses the existing search input without changing or applying its draft. An open modal or inert search field blocks the shortcut. The placeholder gives one valid query; the field tooltip (`search.syntaxHint`, localized; the `tags` and `gN` grammar tokens stay literal) separates further examples onto individual lines.
The gallery retains square tiles and measured 12px gaps. Bulk selection uses a green
outline plus a checkmark; opaque Video badges use purple `#6D28D9` and GIF badges use blue `#1D4ED8`, both with white text in either theme. Their bottom and right offsets are 4px; selection checkmarks retain their original position.
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
listeners are removed on view changes, including registration that resolves after owner disposal.
Modal overlays keep their existing dismissal controls and keyboard behavior.

Bulk editing uses a dock floating at the bottom centre of the gallery viewport
(`.bulk-dock`, positioned in `.workspace-viewport`; [ADR 0035](../adr/0035-bulk-actions-floating-dock.md)).
The gallery keeps its full width. The dock is a 100px chip until something is selected, then
grows to `min(760px, 100% - 32px)`, and its Group & order tab gets a fixed tall height
(`min(72cqh, 680px)`, from the viewport's CSS size container). The size changes instantly; only
the content fades in (`motion-enter`), per the no-layout-animation rule. The dock is centred
with auto margins rather than a transform. The order board's drag ghost is portaled to
`document.body`, so it stays viewport-relative and the dock's stacking context (z-index 30, below
the workspace header) cannot paint over it. Its tag suggestions open above the input through the existing viewport
portal so dock overflow cannot clip them.

Full-page settings uses 240px navigation beside mounted sections at widths of at least
1000px, and wrapping navigation above the content below that width. Navigation focuses and
scrolls to the corresponding section without changing the URL or unmounting operation UI.
The active marker follows the scroll position; settings section choice is not persisted.
Sections appear in scan, appearance, language, import/export, duplicates, danger-zone and About order.
Operations pair descriptions with actions in compact rows, stacking below 700px.
Language keeps its selector below the description at every viewport width.
Appearance uses native light/dark radio choices with decorative CSS previews. Only the
selected choice is a Tab stop; arrow keys select the adjacent choice. The preference values,
dark default, storage service and native theme synchronization are unchanged.

Keep shared field rules in the CSS base layer so component utilities can make deliberate
adjustments. Avoid adding nested field outlines, panel blur, decorative gradients, or
pill-shaped buttons. Native video controls retain their contrast gradient over the picture.

## Modal and overlay conventions

[UiLayerProvider](../../src/ui/overlays/UiLayerProvider.tsx), mounted by `App`, owns the DOM layer stack. [UiModal](../../src/ui/overlays/UiModal.tsx) and the specialized lightbox register modal layers and portal into `document.body`. `SettingsView` registers a nonmodal layer for page-level Escape. While bulk mode is enabled, the gallery registers a nonmodal layer that clears selection and cancels rectangle work on Escape. A modal above it receives Escape first.

- Only the top registered layer receives managed Escape dismissal. A locked top layer does not select a lower layer as a fallback. UiModal backdrop dismissal also checks top-layer ownership.
- A top modal makes `#root` and lower registered modal elements inert and aria-hidden, and locks body scrolling. Cleanup removes those attributes and restores the previous body overflow.
- Initial focus selects `[data-initial-focus]`, then the first tabbable descendant, then the container. Tab and Shift+Tab wrap inside the top modal.
- Unregistering a layer restores the supplied trigger or the previously focused connected element on the next frame. If unavailable, it tries the next layer's initial focus while that layer is still registered. Nonmodal layers never block the restore: leaving Settings with bulk mode on restores `#open-settings-button` even though the remounted gallery registers its bulk layer first. The restore is skipped only when a modal layer sits above the released layer, or when a modal layer registered after the release, because that modal takes its own initial focus. A layer released below a nonmodal layer restores only focus that was lost (focus on `body` or a disconnected element).
- `UiModal` requires exactly one of `ariaLabel` or `labelledBy` in its props and renders `role="dialog"` with `aria-modal="true"`. It supports four width presets and independently configurable Escape/backdrop closing.

Provide translated names and explicit close actions. Register new overlays with the shared manager instead of adding independent global Escape listeners. Lightbox image/native fullscreen and inline confirmation still have specialized key handling, described in [lightbox](../subsystems/lightbox.md#fullscreen-and-keyboard-behavior).

### GPUI port: layers and modals

`media_tagger_client::overlay::LayerStack` makes the decisions of `UiLayerProvider` above (top layer only for Escape, initial focus of a modal still on top one frame later, the restore rules on release) and `media_tagger_gpui::ui::layers` applies them ([ADR 0039](../adr/0039-gpui-overlay-layers-and-controls.md)):

- Every window's content goes inside `layers::root(cx)`, which binds Escape, Tab and Shift+Tab in the key context `UiLayers` and tracks the root focus handle. The window focuses the root when it opens and from `on_focus_lost` (`layers::focus_root`): GPUI sends keys along the focused element's path, and with nothing focused no binding of the application matches. A click on an empty part of the window focuses the root, as a click on the body does in a browser.
- An overlay registers with `layers::register` (`LayerOptions::modal` or `nonmodal`) and releases with `layers::release`, optionally naming where the focus goes back (`getRestoreFocus`). Escape that closes nothing propagates.
- Tab inside a top modal wraps from its last tab stop to its first and back, and comes back into it from anywhere; a modal without a tab stop keeps the focus on its container.
- `ModalState` plus `Modal` is `UiModal`: the owner keeps the state and renders the element every frame with its content. Escape (top layer, unless `set_close_on_escape(false)`) and a press on the backdrop (open, top layer, unless `set_close_on_overlay_click(false)`) emit `ModalEvent::Dismiss`; the owner closes. The dialog draws over the whole window in the four width presets of `ModalSize`, with role `Dialog` and the given label while open. It fades in over 150 ms and out over 100 ms by opacity only (`ModalMotion::LIGHTBOX` has 75 ms and 50 ms), stays drawn and blocks the pointer while it fades out, and takes no input then (a focus inside it moves to the window's root at once, as a browser moves it off `inert` content); `ModalSnapshot` keeps the values it showed open. With GPUI's reduced motion the fades end at once.

Not modelled in GPUI: `inert`/`aria-hidden` on the background and body scroll locking; a modal occludes the pointer and traps Tab instead.

## Keyboard and accessibility

Native controls, translated accessible names, pressed states, listbox suggestions, alerts, and status regions are the normal conventions. Global styles provide immediate focus-visible feedback and a reduced-motion rule that disables decorative motion and smooth scrolling while preserving loading indicators.

The layer manager's tabbable selector excludes disabled controls, hidden elements, and inert ancestors, but it does not perform a complete computed-visibility or browser tab-order calculation. Standalone components rendered without the provider receive only fallback Escape handling. Tests that claim focus trapping or nested dismissal must mount the provider.

When the lightbox panel is collapsed, reopen and close controls float at the top right of the media column. An open panel places Close beside its collapse arrow. Floating controls and the bottom action row fade after three seconds of inactivity; the open panel remains visible. Images and GIFs extend underneath them; windowed native video reserves 44px above its surface. Video fullscreen hides the controls and removes that space. Information controls live in the sidebar only.

GTK video controls are native widgets outside the DOM layer manager. Windowed Escape from those controls reaches `LightboxContent` through the session-checked video channel. The owner checks active/top-layer state and deletion/fullscreen locks, then dismisses confirmation before a narrow drawer or the preview. The event does not synthesize a DOM key or bypass layer ownership. WebView-focus Escape retains its normal DOM handling. Changes to video bounds, fullscreen, or sidebar visibility need desktop verification as well as DOM tests. See [native media presentation](../subsystems/lightbox.md#media-presentation).

Bulk ordering uses one `GroupOrderBoard` in two places: inline in the dock's Group & order tab and in the larger group-order modal. Only one is mounted at a time (the inline board unmounts while the modal is open), and each registers its drag listeners only while active. The dock tabs are a `role="tablist"` with roving `tabindex` and ArrowLeft/ArrowRight/Home/End. Board handles support ArrowLeft/ArrowRight for adjacent positions, with Up/Down moving by grid row. The modal uses the shared modal layer, keeps changes in a local draft until Save order, and restores the triggering button on close. `UiProgressBar` uses its supplied label as its accessible name. Component tests cover selected roles and focus transitions; they do not establish full screen-reader, contrast, zoom, or keyboard-only accessibility.

### GPUI port: keys and text fields

A DOM key handler asks whether the event target is editable. GPUI has no such check: key bindings run before text input, and a binding is chosen by how deep its key context sits in the ancestry of the focused element. The port's [text field](#gpui-port-the-text-field) binds its keys (`media_tagger_client::text::KEYMAP`) in the key context `TextInput`, so with the focus in a field they are the deepest match and a shortcut bound further out does not fire for a key the field handles. That holds only when every shortcut follows two rules ([ADR 0038](../adr/0038-gpui-text-field-own-element-and-key-context.md)):

- A shortcut is never bound without a key context. A binding without one counts as deep as the focused element and, registered after the field's, would take Enter, Escape or Ctrl+A away from it.
- A shortcut on a key that types a character (a letter, a digit, Space, with or without Shift) is bound with `text_input::outside_text_input(context)`, which is `(context) && !TextInput`. The field cannot claim such a key itself.

A shortcut on a key the field does not use, such as Ctrl+K, fires with the focus in a field, as in the Tauri interface.

| Key in a field | What happens |
| --- | --- |
| Left, Right, Home, End, with Shift to select and Ctrl for words | The caret moves by grapheme cluster or word. Left and Right collapse a selection to its end on that side. |
| Up, Down | `actions::Up` and `Down`: the owner may take them in the capture phase (`capture_action`) to walk a list; otherwise the caret goes to the start or the end. Shift+Up and Shift+Down select to there. |
| Backspace, Delete, with Ctrl for a word | Deletes the selection, else one grapheme cluster or word. |
| Ctrl+A, Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y | Select all, undo, redo. |
| Ctrl+C, Ctrl+X, Ctrl+V (also Ctrl+Insert, Shift+Delete, Shift+Insert) | Clipboard. Copy and cut do nothing without a selection. |
| Enter, Escape | `actions::Confirm` and `Cancel`, which the field leaves to its owner: an element around the field handles them with `on_action`. An owner that has nothing to do calls `cx.propagate()`, and the keystroke falls through to the next binding further out (the Escape of a dialog). While an input method is composing, they end the composition and go no further. |

Disabling a field that has the focus removes the focus from the window. The application shell has to take it back (`on_focus_lost`), because without a focused element no key context is in the dispatch path and no shortcut matches.

The field sets the accessibility role `TextInput` and the label given with `set_label`.

The candidate window of an input method is placed from the caret's bounds, and GPUI's Linux backends get them in two ways. Between compositions the field calls `Window::invalidate_character_coordinates` in every frame in which the caret can have moved (the field took the focus, the text or the selection changed, the line moved or scrolled), and GPUI hands the bounds over after that frame; the backends ignore them while a composition runs. During a composition they ask `bounds_for_range` right after changing the text, before the frame with that text is drawn. The field then answers from the line of the last frame: offsets inside the text both have in common are exact, and anything behind it is placed at its end, which is where the composition starts. Composition was tested through GPUI's input handler in that order of calls; a real input method on X11 or Wayland has not been driven.

## Safe change checklist

1. Choose an owner and state whether values survive view switching, close/reopen and restart.
2. Keep application composition in `app`, feature models beside their views and shared cache/mutation behavior in `library`.
3. Construct IPC only in `platform/tauri`; preserve backend-first writes and canonical publication.
4. Clean up registrations, frames and timers; reject late async publication after disposal or identity changes.
5. Preserve loading/error boundaries, modal presence, focus restoration and logical-close cleanup.
6. Reuse controls and theme tokens; localize visible and accessible text in every locale.
7. Verify at the layer that observes the change, adding desktop checks for actual WebView/native behavior.

## Test map

- `src/app/__tests__` covers bootstrap/disposal, runtime composition, view replacement,
  language/theme persistence, lazy loading/failure and lightbox error recovery.
- `src/library/**/__tests__` covers query/cache bounds, races, metadata admission,
  canonical publication, maintenance, and vocabulary.
- `src/features/**/__tests__` covers feature models and DOM behavior. Virtual grid, order and
  duplicate-list tests drive `features/gallery/gridGeometry.ts`, `createVirtualGrid` and
  `ui/dom/createVirtualList.ts` with controlled DOM geometry.
- `src/ui/**/__tests__` covers modal presence, nested dismissal, focus, inertness,
  controls and thumbnail images. Mount `UiLayerProvider` when asserting layer behavior.
- GPUI port: `src-gpui/crates/media_tagger_client/src/text/` and
  `src-gpui/crates/media_tagger_client/tests/text_editor.rs` cover the editing model of the
  text field, `src-gpui/crates/media_tagger_gpui/tests/text_input.rs` the field itself and
  `tests/text_input_owner.rs` the field inside an owner, both on GPUI's test platform
  ([GPUI workspace](../development/testing.md#gpui-workspace)).

Use direct inputs/outputs for pure helpers, owned `renderHook` for models and Testing
Library for components. `src/test/renderSolid.tsx` changes props under the same owner;
`settle.ts` provides explicit batch boundaries. Deferred IPC establishes race ordering.
Consult [testing](../development/testing.md) for commands and desktop opt-ins.
