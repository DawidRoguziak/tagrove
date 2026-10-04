# Stage 2: Platform boundaries and Solid shell

Previous: [reference and toolchains](01-reference-and-toolchains.md).
Next: [shared library state](03-library-state.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Establish a typed platform boundary and a Solid shell with the current appearance,
language/theme behavior, window controls, and shared interaction controls.

## Implementation

- Split the existing API functions and transport types into `platform/tauri` modules by
  subsystem. Move video transport types out of the old lightbox component directory.
  Keep temporary API re-exports for React consumers and preserve every wire shape.
- Centralize Tauri invoke, channel, event, dialog, media URL, and native window access.
  Keep business workflows in library or feature models.
- Separate i18next initialization/resources from the React binding. Add a small Solid
  binding with application-owned language subscriptions and reactive translations.
  Preserve all 11 languages, storage keys, fallback rules, and document language updates.
- Implement Solid bootstrap, initial theme before rendering, native theme synchronization,
  window controls, context-menu suppression, and cleanup on hot reload or disposal.
- Port shared buttons, icons, chips, alerts, progress, sliders, and segmented controls.
  Retain semantic HTML, accessible names, CSS classes/tokens, and controlled input behavior.
- Implement modal layers, portals, focus trapping/restoration, Escape ordering, background
  inertness, scroll locking, presence, and reduced motion. Closing animations retain DOM
  without retaining the logical modal lock.
- Establish shell view switching and lazy-load boundaries with existing loading/failure
  presentations and delayed prefetch. Later stages fill in the feature views.

## Tests and acceptance

- Move transport and framework-independent utility tests without changing their assertions.
- Port window-control, shared-control, modal, presence, and lazy-failure tests.
- Add language/theme propagation and subscription-cleanup tests, including remounting.
- Verify late async event registration cleans itself up after disposal.
- Exercise nested dialogs, keyboard focus, Escape, reopen during exit, and reduced motion.
- Compare shell controls in both themes and English/Polish on the isolated desktop.
- Keep the React entry and its tests passing after the shared boundary extraction.

## Review and local commit

Review transport equivalence, reactive props, event semantics, owned cleanup, focus,
and import direction. Apply the complete stage gate from the README.

Intended commit: `refactor: establish Solid shell and platform boundaries`.
