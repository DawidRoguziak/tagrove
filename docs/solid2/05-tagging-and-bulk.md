# Stage 5: Shared tag editing and bulk actions

Previous: [gallery and search](04-gallery-and-search.md).
Next: [lightbox and images](06-lightbox-and-images.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Provide one reusable tag editor and complete Solid bulk workflows with the existing
selection, persistence, error recovery, and focus behavior.

## Implementation

- Implement the shared tag editor in `features/tagging`, using the suggestion client
  introduced in stage 4. Keep chips and input within the current wrapping field.
- Preserve controlled tags and drafts, loading/disabled guards, blank-space focus,
  IME behavior, suggestion keyboard interaction, and scroll-to-input behavior.
- Implement bulk selection state, metadata loading, sidebar presentation, favorites,
  tag merges, media-group changes, and the group-order dialog.
- Retain authoritative selected IDs across page eviction. Resolve selected metadata
  independently of the gallery's loaded summaries.
- Require complete canonical tags before replacement or merge decisions. Use the shared
  mutation coordinator and publish canonical results before cache patches.
- Preserve drafts and card-specific errors on failure. Completion from an obsolete
  selection must not clear a current draft or restore focus into the wrong editor.
- Preserve current responsive sidebar placement, dimensions, and action availability.

## Tests and acceptance

- Port TagEditor, BulkActionsSidebar, selection-controller, bulk favorites, tag merge,
  and media-group/order tests.
- Use deferred reads and saves to cover selection replacement, contention, retries,
  rejected operations, and focus restoration after the input becomes enabled.
- Verify multi-selection writes, offscreen IDs, missing IDs, and favorites-only refreshes.
- Run tag-editor and gallery-selection desktop suites with both themes and narrow widths.
- Check persisted tags after reload and retain the vocabulary IPC assertions: unchanged
  vocabulary causes no extra pages, changed vocabulary triggers the existing refresh.

## Review and local commit

Review canonical tag ownership, selection identity, save completion, focus, and query
invalidation. Apply the complete stage gate from the README.

Intended commit: `feat: migrate tag editing and bulk actions to Solid`.
