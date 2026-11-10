# 0015: Restore drops unresolved thumbnail references and accepts the nullable `file_name` column of an earlier migration

- Status: Accepted
- Date: 2026-11-10
- Areas: data safety (bundle restore, backup validation), database (`init_schema` legacy columns), thumbnails
- Related: [data safety](../subsystems/data-safety-and-portability.md#bundle-import-and-replacement-sequence), [database](../subsystems/database.md#in-place-initialization-migrations-and-backfills)

## Context

Restore validates a bundle twice in staging (before and after `init_schema`) and once more after the swap. The audit run of 2026-11-08 found two cases in which that validation rejected a bundle the application itself had written or had promised to migrate:

- **Nullable `file_name`.** `ensure_assets_file_name_column` (`src-tauri/src/db/schema.rs`) added the column to a legacy `assets` table as plain `TEXT`, while a created schema declares `TEXT NOT NULL DEFAULT ''`. `init_schema` then stamps the current identity, and `validate_backup_constraints` required `NOT NULL` for every identified schema. A legacy bundle without `file_name` passed the first validation and always failed the second, and a library migrated in place from that shape could not inspect or restore any of its own backups.
- **Unresolved format-2 thumbnail reference.** Export archives the regular files it finds below the thumbnail directory and never compares `assets.thumb_path` with them. `resolve_imported_thumbnail` (`src-tauri/src/services/backup_service.rs`) turned a format-2 reference outside the manifest's thumbnail directory, or one without an archive entry, into a hard error that discarded the whole staged restore. Such rows exist after the cache was cleaned outside the application, after the profile directory was moved (for example native to Flatpak, or a changed `XDG_DATA_HOME`), or after a crash between a thumbnail file delete and its database write. Only legacy references became null.

Thumbnails are a derived cache: a null `thumb_path` is the normal "not rendered yet" state and is regenerated on demand. SQLite cannot change a column's `NOT NULL` declaration with `ALTER TABLE`.

## Options considered

For the column:

1. **Declare it like the created schema from now on and accept both declarations in validation (chosen).** New migrations match a created schema. Libraries already migrated keep working, and their backups restore. Cost: the declared nullability of `file_name` is no longer something validation proves; the stored-value check has to carry it.
2. **Only fix the `ALTER TABLE`.** Rejected: every library an earlier build migrated would still be unable to restore its own backups.
3. **Rebuild `assets` in `init_schema` to change the declaration.** Rejected: it copies the largest table at startup, has to run with foreign keys off around cascading children, triggers and a dozen indexes, and a failure half-way leaves more to recover than the defect it repairs.
4. **Edit the stored table SQL through `PRAGMA writable_schema`.** Rejected: SQLite documents that route for removing constraints, not for adding `NOT NULL`, and a mistake corrupts the database.

For the thumbnail references:

1. **Drop an unresolved reference at import (chosen).** Repairs every bundle already written and needs no format change.
2. **Null or skip dangling rows at export.** Rejected as the fix: it does not help existing bundles and adds a write to the snapshot. It is not needed once import tolerates them.
3. **Refuse to export while a reference is dangling.** Rejected: it blocks a backup of user metadata because of a cache inconsistency the user cannot see or repair.
4. **Fall back to the legacy basename match for format 2.** Rejected: the exact source-relative path is what keeps a format-2 reference from binding to another asset's thumbnail.

## Decision

- The legacy column addition is `ALTER TABLE assets ADD COLUMN file_name TEXT NOT NULL DEFAULT ''`, the created-schema declaration. The blank-fill pass that follows is unchanged.
- For every identified schema (`Current`, `Version1`, `Version2`), backup validation accepts `assets.file_name` with or without `NOT NULL`, and still requires declared type `TEXT` and that it is not part of the primary key. `validate_backup_data` keeps rejecting any row whose `file_name` is not non-blank text, so a bundle with a null value is rejected under either declaration.
- A non-null thumbnail reference that cannot be matched to an extracted file becomes null for every bundle format. For format 2 that covers a path outside the manifest's `source_thumbs_dir`, a path that is not a plain relative path below it, and a relative path without an archive entry. Matching rules are unchanged: exact source-relative path for format 2, unique basename for legacy input.
- Restore still rejects everything else it rejected before: foreign or future identity, structural and stored-value violations, asset paths outside their roots, mapping collisions, and unsafe archive entries.

## Consequences

- A legacy bundle without `file_name`, and a backup of a library migrated from that shape by any build, restore. A library migrated by an earlier build keeps its nullable declaration for good; nothing rewrites it. Application code always writes a value, and `init_schema` fills null or blank names.
- An older build still rejects a bundle whose `file_name` column is nullable. Bundles from libraries created or migrated with this change are accepted by older builds as before.
- A restore can install fewer thumbnail references than the source library stored. The affected assets are rendered again on demand. Nothing reports how many references were dropped; `restored_thumbnails` still counts extracted files. Extracted files that no row references are installed as before.
- Validation no longer proves that a format-2 bundle's thumbnail references are complete. It never proved that the files were the right pictures.
- No bundle-format, manifest, schema-version, `performance_schema_version`, or IPC change.
- Tests: `db.rs` (`legacy_file_name_migration_declares_the_fresh_column_and_validates_as_a_backup`, `backup_validation_accepts_the_nullable_file_name_column_of_an_earlier_migration`, `backup_validation_rejects_file_name_as_part_of_the_primary_key`) and `services/backup_service.rs` (`restores_a_legacy_bundle_without_file_name_and_its_later_backup`, `restores_a_backup_of_a_library_whose_file_name_column_an_earlier_build_added`, `format_two_import_drops_unresolved_thumbnail_references_instead_of_rejecting`).
