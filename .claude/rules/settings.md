---
paths:
  - "src/features/settings/**"
  - "src/app/AppSettingsView.tsx"
---

# Settings operations

Read `docs/subsystems/settings-operations.md` (its refresh/reset table defines what each operation invalidates).

- `createSettingsOperationRunner` serializes settings work in one frontend through a synchronous `operationLock`. It is UI-only, not a backend guarantee.
- Progress: subscribe to `process-progress` before invoking and accept only the operation's own phases. Translated phase names take precedence over `message`.
- Destructive actions use in-app confirmations. Clear-library, CSV import, and restore enter the tag-mutation barrier.
- Thumbnail Stop calls `cancelRenderAllThumbnails` outside the runner.
- Dialogs register with the shared UI layer manager. `SettingsView` owns page-level Escape.
