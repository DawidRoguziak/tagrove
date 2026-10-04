# Stage 6: Lightbox selection and image behavior

Previous: [tagging and bulk](05-tagging-and-bulk.md).
Next: [native video](07-native-video.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Provide the Solid lightbox for image and GIF browsing, editing, navigation, and deletion
with the existing visual and interaction behavior.

## Implementation

- Implement application-owned selection, details loading, bounded detail prefetch, and
  navigation over the query's global indices rather than its loaded summary array.
- Advance navigation targets synchronously so rapid key presses accumulate correctly.
  A failed target rolls back appropriately; obsolete lookups cannot replace selection.
- Keep summary placeholders distinct from complete details. Editing remains disabled
  until the canonical tag base is known; media waits for the existing details boundary.
- Reuse shared tag editing and metadata mutation commands. Preserve serialized/coalesced
  saves, retry intent, latest-filter invalidation, and dirty group drafts.
- Port the modal, image/GIF stage, fit/zoom/pan, fullscreen, action rail, sidebar/drawer,
  clipboard actions, information panel, delete confirmation, and keyboard shortcuts.
- Preserve responsive defaults, explicit sidebar choices across navigation, nested
  overlay priority, backdrop-close safety, idle controls, and reduced motion.
- Retain presentation props during exit without keeping logical focus/inertness locks.
  Navigating assets must not restart the opening animation.
- Reserve the native-video integration contract for stage 7 without claiming playback
  parity at this stage.

## Tests and acceptance

- Port selection-state, query-refresh navigation, lightbox, image controls, tagging,
  media-group, clipboard, keyboard, sidebar, and deletion tests.
- Exercise page boundaries, wraparound, rapid navigation, stale detail/prefetch results,
  mutation contention, query reorder, failed navigation, and restore-related ID reuse.
- Verify failed details and failed saves retain independent retry behavior.
- Run image/GIF desktop scenarios, nested modal focus, responsive layouts, fullscreen,
  close/reopen races, animations, and reduced-motion variants.
- Check persisted edits and deletion results through the existing backend contracts.

## Review and local commit

Review details authority, draft lifetime, navigation targets, modal presence, DOM geometry,
and keyboard/focus behavior. Apply the complete stage gate from the README.

Intended commit: `feat: migrate lightbox and image interactions to Solid`.
