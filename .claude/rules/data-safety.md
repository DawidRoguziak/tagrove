---
paths:
  - "src-tauri/src/services/asset_mutation_service.rs"
  - "src-tauri/src/services/backup_service.rs"
  - "src-tauri/src/services/csv_service.rs"
  - "src-tauri/src/commands/import_export.rs"
  - "src-tauri/src/commands/assets.rs"
---

# Destructive operations, CSV, backup/restore

Read `docs/subsystems/data-safety-and-portability.md`. For a restore change, also read `database.md`, `settings-operations.md`, and `testing.md`.

- Deleting or renaming a source file stages it as a hidden `.mediatagger-*.pending` sibling and writes the `pending_file_operations` journal with a FULL-sync connection. The DB transaction is the commit point. Startup recovery reconciles the journal.
- Results keep `deleted`, `missing`, and `cleanup_pending` distinct. Never collapse them.
- Thumbnail cleanup is best effort and runs after commit.
- Bundle export/import run under the maintenance gate. Restore is staged and recovered at startup.
- On the frontend, CSV import, restore, and clear-library run inside `library.tags.runWithMutationBarrier`.
- Test destructive paths with lower-layer edge cases plus isolated desktop E2E on temporary media only.
