# Stage 8: Settings and maintenance workflows

Previous: [native video](07-native-video.md).
Next: [parity and cutover](09-parity-and-cutover.md).
Read the [architecture and completion rules](README.md) and update [progress](PROGRESS.md).

## Outcome

Complete Solid settings with the current operation ordering, confirmations, progress,
startup scanning, and data-safety behavior.

## Implementation

- Build scanning, appearance, language, import/export, duplicate, danger-zone, and About
  sections, navigation, and dialogs with current layout and localized text.
- Construct settings models at application lifetime. Closing Settings must not cancel
  operations or lose locks, messages, progress, roots, or duplicate results.
- Preserve immediate synchronous operation exclusion and per-section feedback. Subscribe
  to progress before dispatch; filter phases and unregister in every completion path.
- Keep thumbnail cancellation callable while the exclusive operation is active. Its own
  pending state must not release the original operation's lock.
- Preserve root selection/add/remove/rescan behavior and startup-scan launch semantics.
  Theme and language changes remain outside the maintenance operation runner.
- Port CSV and database-bundle dialogs, root mapping, overwrite confirmation, duplicate
  staging/application, delete confirmations, and clear-library confirmation.
- Preserve the tag maintenance barrier across CSV import, restore, and clear. Block new
  writes, drain active writes, invalidate the correct caches even on uncertain failures,
  and release in `finally` before required refreshes.
- Follow the existing success/reset matrix; do not replace distinct reset paths with a
  blanket refresh. Preserve legal text, publisher metadata, and allowed external links.

## Tests and acceptance

- Port settings actions, operation runner, confirmation flow, startup scan, section,
  duplicate resolver, progress translation, and About tests.
- Test synchronous duplicate actions, listener registration failure, navigation during
  operations, cancellation, cleanup, and rejection after possible backend changes.
- Verify import/restore/clear cannot overlap unsafe metadata writes and that barrier
  failures do not permanently disable editing.
- Run applicable Rust maintenance/integration tests and isolated desktop scenarios for
  settings, startup scanning, import/export, duplicates, confirmations, language, and About.
- Use only disposable paths for destructive scenarios. Preserve profile and cleanup guards.

## Review and local commit

Review operation admission, listener lifecycle, confirmation ordering, partial failure
handling, and exact invalidation behavior. Apply the complete stage gate from the README.

Intended commit: `feat: migrate settings and maintenance workflows to Solid`.
