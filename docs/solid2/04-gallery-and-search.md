# Stage 4: Gallery, search, and tag discovery

Previous: [shared library state](03-library-state.md).
Next: [tagging and bulk](05-tagging-and-bulk.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Browse and search the real library through Solid with the current gallery layout,
virtualization behavior, search grammar, and tag discovery interactions.

## Implementation

- Build a narrow adapter around the existing TanStack virtual-core version. Own the
  virtualizer and observers within the gallery view and dispose them on unmount.
- Retain row geometry, overscan, global indices, placeholders, group backplates, resize
  anchoring, scroll offsets, footer behavior, and bounded DOM work on cold mount.
- Choose list identity explicitly: stable virtual slots must update when their asset
  arrives, and asset-specific subscriptions must follow the current asset identity.
- Preserve image/GIF thumbnail presentation and avoid rebuilding row measurements on
  thumbnail, progress, or unrelated page changes.
- Implement pointer rectangle selection, edge scrolling, capture/cancellation, preview
  highlights, range resolution, and retry through the shared selection contract.
- Implement search draft/applied state, literal tag tokens, meta filters, media kind,
  favorites, clear, and explicit resubmission of unchanged filters.
- Port autocomplete and tag-list browsing. Keep one application-owned worker with the
  existing protocol, vocabulary versioning, per-editor coalescing, and stale-result guards.
- Preserve Gallery/Settings remount behavior, including the existing gallery scroll reset.

## Tests and acceptance

- Port search, tag-list, suggestion-client/worker, gallery, footer, and geometry tests.
- Run real virtual-core tests with 10,000 and 26,000 slots. Verify initial null refs,
  bounded intermediate ranges, stable measurements, resize anchoring, and cleanup.
- Exercise cache eviction and backward scrolling beyond 12 pages, sparse tiles,
  placeholder replacement, and thumbnail demand after page arrival.
- Verify keyboard, IME, literal include/exclude tokens, meta filters, empty/error states,
  rectangle cancellation, and stale suggestion responses.
- Run applicable gallery performance, gallery selection, footer, thumbnail-fade, and
  search desktop scenarios against synthetic fixtures. Compare both themes and languages.

## Review and local commit

Review adapter lifecycle, range math, fine-grained subscriptions, search semantics,
selection cancellation, and worker ownership. Apply the complete stage gate from the README.

Intended commit: `feat: migrate gallery and search to Solid`.
