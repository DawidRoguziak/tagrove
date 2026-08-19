# Data safety and portability

This page describes the current, implemented safety boundary for destructive media operations, CSV metadata transfer, and database-bundle backup and restore. It is intentionally narrower than the UI confirmation flow in [settings operations](settings-operations.md): a confirmation reduces accidental invocation, but it does not make a multi-step database/filesystem workflow atomic. SQLite structure, transactions, WAL behavior, and the library revision are defined in [database persistence](database.md). Scan ownership is defined in [scanning and indexing](scanning-and-indexing.md), thumbnail lifecycle in [thumbnails](thumbnails.md), and the crossing payloads in the [IPC contract](../architecture/ipc-contract.md).

The implementation in `commands/assets.rs`, `commands/scan.rs`, `commands/thumbs.rs`, `commands/import_export.rs`, `services/backup_service.rs`, `services/thumb_service.rs`, and `db.rs` is authoritative.

## Safety model

MediaTagger has three distinct kinds of state:

- source media at indexed paths, normally outside the application-data directory;
- SQLite state in `media.db` and, while WAL is active, `media.db-wal` and `media.db-shm`; and
- generated files below the profile's thumbnail directory.

A SQLite transaction can protect only database statements. Filesystem rename/delete and ZIP or CSV I/O occur outside those transactions. Most deletion workflows therefore make the database authoritative first and treat thumbnail cleanup as best effort. The important exception is asset rename, which moves the source file first and then tries to compensate if the database update fails.

### Operation safety matrix

| Operation | Backend lock | Database effect and order | Source-media effect | Thumbnail effect | Rollback and returned summary |
| --- | --- | --- | --- | --- | --- |
| Remove scan root | Scan mutex, then exclusive thumbnail write lock | In one transaction: delete the normalized root, let its mappings cascade, delete every asset with no remaining root mapping, clean failures and orphan tags, commit. Bump revision separately. | Never deletes source media. Assets shared by another root remain indexed. | After the bump, attempt each recorded orphan thumbnail deletion. | No filesystem rollback. `removed_assets` is committed orphan-row count; `removed_thumbnails` counts only successful file removals. Missing/failed files are not errors. A failed revision bump leaves the root/assets changed and skips thumbnail cleanup. |
| Delete asset | Scan mutex, then exclusive thumbnail write lock | Read path/thumb, delete its failure row, delete the asset, clean orphan tags, then bump revision. These cleanup statements and the bump are not one transaction. | After the bump, delete the indexed source path if it currently exists. | Then attempt the one recorded thumbnail path, if any. | No rollback. Missing ID is an error. On success `removed_assets` is always `1`; `removed_media_file` is `false` for both a missing source and a failed delete; `removed_thumbnails` counts successful deletes. A bump failure can report an error after the row was removed but before either file cleanup. |
| Rename asset file | Scan mutex, then exclusive thumbnail write lock | Validate and resolve the new basename; rename the source first. Then, in one DB transaction, update `path`, `file_name`, and lowercase key, clear `thumb_path`, delete its failure row, and commit. Delete the old thumbnail and bump revision afterward. | Renames within the existing parent directory. It rejects blank, `.`/`..`, separators, and the reserved characters colon, asterisk, question mark, quotation mark, angle brackets, and vertical bar; it also rejects a case-insensitively equal path and an existing target. | Old recorded thumbnail deletion is best effort after the DB commit. | If the DB update fails or finds no row after the filesystem move, it attempts to rename the file back and ignores compensation failure. It cannot roll back a successful DB update when thumbnail deletion or the later bump fails. Success returns IDs/old/new paths and only the count of thumbnails actually removed. |
| Clear all thumbnails | Exclusive thumbnail write lock only | Read distinct non-null paths, set all `assets.thumb_path` values to null, then delete all failure rows. Those statements are not one transaction and do not bump the library revision. | None. | After DB references are cleared, attempt every recorded file deletion with progress. | No rollback. Return value is only the number of successful file deletions. Failed/missing files can remain unreferenced; a DB error can leave partially cleared metadata and prevents later file cleanup. |
| Clear library | Scan mutex, then exclusive thumbnail write lock | Read distinct thumbnail paths; separately delete assets, failures, tags, and scan roots; then bump revision. Asset mappings cascade. | Never deletes source media. | After the bump, attempt recorded thumbnail deletes with progress. | No rollback. The DB deletes are separate autocommit statements. Success reports removed asset/root rows and successful thumbnail deletes, not undeleted files. A mid-DB or bump failure can leave partial committed clearing; the same-shell frontend invalidates identity-bound state and attempts a best-effort refresh before showing the error. |
| Find duplicates | No backend scan/thumb lock | Read-only query; no revision change. Groups use stored lowercase basename identity. | None during scan. Resolver deletes call `delete_asset`; resolver renames call `rename_asset_file`. | Preview generation uses the normal thumbnail path. | Scan summary reports group count and the total number of assets in those groups, not excess copies. Applying a resolver batch is sequential IPC, not a transaction: earlier changes remain if a later one fails. |

The frontend serializes settings actions within one mounted controller and asks for confirmation before root removal, clear-library, bundle overwrite, and duplicate batches containing deletion. Confirmed bundle restore and clear-library additionally use the shell-owned tag-mutation maintenance barrier: they drain already-dispatched tag writes, block new ones, and invalidate identity-bound tag/details state before releasing the barrier whether the command resolves or rejects. On rejection they then attempt root/library refreshes because either backend workflow may have changed live state before returning an error. This remains user-interface orchestration only; direct IPC callers and other windows are not covered. Exact success refresh/reset order is documented in [settings operations](settings-operations.md).

### Duplicate identity

A duplicate means that at least two indexed rows have the same nonempty `file_name_key`. That key is the lowercase basename, not the full path, byte content, content hash, dimensions, size, or timestamp. Consequently, unrelated files named `IMG_0001.JPG` and `img_0001.jpg` are one duplicate group even if their contents differ, while byte-identical files with different basenames are not duplicates.

Groups sort by asset count descending and key ascending. Members sort by `modified_at` descending and then ID descending. The displayed group filename is a representative stored spelling; it is not an identity stronger than the lowercase key. A resolver rescan is only a point-in-time read. Its staged actions are applied one at a time in frontend array order, and each command acquires and releases its own backend locks.

## CSV metadata transfer

CSV transfers metadata only. They neither copy media nor add missing assets.

### Export format

Export writes a comma-delimited CSV with standard `csv` crate field quoting and these exact headers, in this order:

```text
file_name,tags,favorite,media_group_key,media_group_order
```

Rows are selected by asset ID ascending. `file_name` is derived from the stored path's basename. Tags are joined into one field with a single ASCII space, favorite is `1` or `0`, missing group values are empty strings, and a present group order uses Rust floating-point string formatting. The success summary's `rows` is the number of records written.

The rows come from one SQLite SELECT, so that statement reads one SQLite snapshot even if unrelated metadata writes occur concurrently. The command does not acquire scan or thumbnail locks. It creates parent directories and opens/truncates the destination before iteration; a query, encoding, flush, or disk error may leave a partial output file and returns no success summary.

The tag field is not a lossless per-tag encoding. Import treats commas, semicolons, and all whitespace as delimiters, so an existing tag containing any of those characters cannot round-trip as one tag. Export also promises no stable ordering among an asset's tags beyond the order produced by the current aggregate query.

### Import headers, parsing, and matching

The reader expects a header row, trims all fields, and uses normal comma-delimited CSV parsing. Header names are matched case-insensitively after trimming:

- `file_name` and `tags` fall back to columns 0 and 1 respectively when those names are absent;
- `favorite`, `media_group_key`, and `media_group_order` are optional and have no positional fallback;
- unknown columns are ignored.

Tags split on comma, semicolon, or whitespace, then are trimmed, lowercased, de-duplicated in first-seen order, and merged into existing tags. Import never removes an existing tag, and an empty tags field does not clear tags.

Favorite parsing accepts `1`, `true`, `yes`, `y`, and `on` as true and `0`, `false`, `no`, `n`, and `off` as false, case-insensitively. Blank or unknown favorite text means leave the current value unchanged. If the group-key column exists, blank means explicitly clear the key and nonblank text is retained after outer trimming; if the column is absent, the key is unchanged. If the group-order column exists, blank explicitly clears the order, a finite `f64` replaces it, and invalid/non-finite text leaves it unchanged. Key and order are applied independently, so import can create a key without an order or an order without a key.

Each nonblank imported filename is lowercased and matched exactly against `assets.file_name_key`. Matching is basename-only and fans out to every indexed asset with that key, across all roots and directories. It does not use the CSV path, hash, or any other identity. Repeated CSV rows may update and count the same asset repeatedly.

### Merge, counters, and partial failure

For a successfully returned import:

| Counter | Meaning |
| --- | --- |
| `rows_read` | Data records successfully parsed, including blank, unmatched, and no-op records. The header is excluded. |
| `rows_applied` | Records with a nonblank filename, at least one effective metadata instruction, and at least one matching asset. A no-op match still counts. |
| `assets_matched` | Sum of matched asset IDs over applied rows; it is not a distinct-asset count. |
| `assets_updated` | Sum of row/asset applications that changed at least one field; the same asset can count more than once. |

Import is not one transaction. For each matched asset, tag replacement uses its own transaction, while favorite and group changes use separate statements. Processing continues row by row and the library revision is bumped once, only after the entire file succeeds and at least one row/asset application changed. A malformed later record or later database error can therefore leave earlier changes committed with no final revision bump and no summary. There is no automatic compensation. Before invoking `import_tags_csv`, the frontend requests the shared tag coordinator's maintenance barrier. That synchronously prevents new full tag replacements, waits for already-dispatched replacements to settle, invokes the import, and keeps maintenance ownership through authoritative/local details-cache invalidation whether the command resolves or rejects. The barrier then releases reliably before the library refresh (best effort on rejection). This prevents committed early rows from being hidden by pre-import cached tags; it does not make the import atomic or claim that the database identity changed. Recovery is to refresh/restart, inspect the affected metadata, correct the CSV, and re-import; tag merge is additive/idempotent for the same normalized inputs, but favorite/group assignments may intentionally overwrite or clear values.

See [search, tags, and media groups](search-tags-and-media-groups.md) for tag normalization and group semantics, and [database persistence](database.md) for why a missing revision bump can leave an existing query session apparently current.

## Database bundle format

The bundle is a deflated ZIP with a versioned manifest but no media payload. Legacy archives without a manifest remain accepted because they are migration inputs. Recognized entries are:

```text
media.db                  required
media.db-wal              optional
media.db-shm              optional
manifest.json             present in new exports
thumbs/<relative path>    zero or more files, recursively copied
```

Export copies `media.db` first, then existing WAL and SHM sidecars, then regular files found below the thumbnail directory. `copied_files` counts the database plus included sidecars; `copied_thumbnails` counts thumbnail files. Source media is never included.

Bundle export holds the scan mutex and exclusive thumbnail lock, preventing the application's locked scan, destructive asset, bundle, and thumbnail command families from overlapping. It does not stop ordinary tag/favorite/group mutations, CSV operations, asset queries, direct SQLite users, or already outstanding connections merely by taking those locks. It also does not checkpoint SQLite or use SQLite's online-backup API before copying the main file and sidecars sequentially. Therefore the archive contents are not guaranteed to be one atomic database snapshot if the database changes during export. A failed export can leave a partial or truncated destination ZIP.

## Bundle import and replacement sequence

Import also holds the scan mutex followed by the exclusive thumbnail lock. Within that boundary it performs the following current sequence:

1. Require an existing regular source file and open it as ZIP. Scan entries until an exact sanitized `media.db` entry is found; reject the archive if none exists.
2. Resolve the app-data parent and use fixed staging paths `restore-staging/media.db` and `restore-staging/thumbs`. Delete a pre-existing `restore-staging` tree, then create the thumbnail staging directory.
3. Extract recognized files. Backslashes are normalized to slashes; absolute paths, parent components, rooted paths, and platform prefixes are rejected. Safe unrecognized entries are ignored. Parent directories are created as needed.
4. Open the staged database with the normal WAL connection settings, run `init_schema`, and execute `PRAGMA wal_checkpoint(TRUNCATE)`. This applies current in-place schema initialization/backfills and folds a usable staged WAL into the main database.
5. Define recovery paths beside the live data: `media.db.restore-previous` and `thumbs.restore-previous`. Remove any older file/directory already at those paths.
6. Invalidate the registered query pool for the live DB path and clear the process-wide asset-query manager. Open the current database, checkpoint it with `wal_checkpoint(TRUNCATE)`, then best-effort remove its WAL and SHM paths.
7. Rename the current DB, if present, to `media.db.restore-previous`, then rename the current thumbnail directory, if present, to `thumbs.restore-previous`.
8. Install by renaming the staged DB to the live DB path and staged thumbnails to the live thumbnail path. If either install rename fails, remove any partially installed live targets and attempt to rename both previous targets back; rollback errors are ignored, and the original install error is returned.
9. On successful install, immediately best-effort delete both previous targets and the remaining staging tree. Open the installed DB, run `init_schema` again, bump its library revision, and invalidate the query pool again.
10. Return `restored_files` for extracted DB/main-sidecar entries and `restored_thumbnails` for extracted thumbnail entries. The same-shell frontend invalidates identity-bound state before releasing its tag-mutation barrier, then reloads roots and refreshes assets and known tags. It performs the invalidation and best-effort refresh on rejection too because a late error may follow live swap activity.

Opening and initializing the staged database is the current validation gate. Import does not run `PRAGMA integrity_check` or `foreign_key_check`, authenticate the archive, require an application/version manifest, enforce entry count or uncompressed-size limits, reject duplicate recognized paths, or verify thumbnail contents. Running `init_schema` can mutate an old staged schema; it is not read-only validation. An exact `media.db` entry is required, but WAL, SHM, and thumbnails are optional.

### Restore rollback and recovery limits

The swap is staged, but it is not an atomic, fully recoverable transaction:

- Failure while reading, sanitizing, extracting, initializing, or checkpointing staging occurs before the live swap. The live DB/thumb directories remain in place, but partial `restore-staging` data may remain until the next import removes it.
- Deleting older `*.restore-previous` targets discards any recovery copy left by an earlier attempt.
- Moving the current DB and current thumbnails aside is outside the guarded install closure. If moving the DB succeeds and moving the thumbnail directory fails, the function returns without automatically restoring the DB from `media.db.restore-previous`.
- Install failure triggers only best-effort rollback. Individual cleanup or rename-back failures are ignored, so callers must inspect all live, previous, and staging paths before retrying.
- After both staged targets install, the previous copies are deleted before the final open, second `init_schema`, and revision bump. Failure in those final steps returns an error without an old copy to restore automatically.
- Pool invalidation removes the registry entry but does not forcibly close a checked-out pooled connection. The locks also do not stop ordinary direct DB commands or query reads. An in-flight caller can retain a connection to the replaced file. Query sessions are explicitly cleared. The frontend tag-mutation barrier coordinates tag writes started through the same mounted shell, but there is no process-wide database maintenance barrier.

Before a Windows backup is installed on Linux, `inspect_db_bundle` reports every source root and the UI requires an existing absolute target directory for each one. Import rewrites `assets.path`, `scan_roots`, `asset_scan_roots`, derived filename fields, and unambiguous legacy thumbnail references in staging. It preserves asset IDs and relationship tables, rejects target path collisions, clears stale thumbnail failures, and nulls thumbnail paths that cannot be matched uniquely. Source media is never copied, so missing mapped files remain indexed until a later scan reconciles them.

If restore returns an error after swap activity, stop mutating the library and preserve copies of the app-data directory before trying again. Inspect `media.db`, `media.db.restore-previous`, `restore-staging/media.db`, `thumbs`, `thumbs.restore-previous`, and `restore-staging/thumbs`; keep matching DB sidecars with their database until SQLite has safely opened/checkpointed it. Prefer selecting a verified backup over manually combining generations. Restart the application after manual recovery so stale direct/pooled connections and frontend caches are gone.

## Current guarantees and known limits

Current guarantees are deliberately modest:

- root removal preserves assets still mapped to another root and never deletes source files;
- clear-library and clear-thumbnails never delete source media;
- single-asset delete targets only the recorded source path and recorded thumbnail path;
- rename cannot directly move a file to another directory because separators are rejected;
- destructive asset/root/bundle commands and clear-thumbnail work use the backend scan/thumb locks described above;
- ZIP entry sanitization blocks direct absolute-path and `..` traversal extraction;
- import stages and initializes the candidate database before intentionally replacing live paths;
- successful query-visible mutations normally bump the library revision, and successful restore also clears query sessions and invalidates the pool.

Known limits that must remain visible in changes and user messaging:

- Database commits, revision bumps, source operations, thumbnail cleanup, archive I/O, and frontend refreshes are separate failure boundaries. A command error does not prove that nothing changed; a success count does not prove that every requested file was removed.
- Best-effort deletion intentionally suppresses individual filesystem errors. Summaries count successes and do not list failed paths or distinguish missing files from permission/I/O failures.
- Remove-root and clear-library database work differ: root/orphan cleanup is transactional, while clear-library and single-asset cleanup contain multiple autocommit statements. Revision increments are separate in all of these workflows.
- The fixed restore staging/previous names are not generation-retained backups. Successful restore deletes the previous generation, and a new attempt removes leftovers.
- Archive export has no atomic SQLite snapshot guarantee; archive import has no integrity/authenticity/resource-limit checks and only attempted rollback.
- Duplicate and CSV identity is case-insensitive basename, not content identity. Collision fan-out is expected behavior.
- CSV cannot faithfully encode tags containing its tag delimiters and cannot remove tags.
- Source and thumbnail paths make bundles environment-dependent. The bundle is a profile-state backup, not a self-contained media archive.
- Backend locks cover only callers that use them. The same-shell frontend barrier prevents its tag setters from overlapping confirmed restore/clear, but CSV import/export, duplicate scanning, metadata setters from other callers, query reads, and outstanding DB connections can still overlap backend maintenance work.

## Safe change and recovery checklist

1. Write down the intended commit point and exact ordering across DB, revision, source media, thumbnail files, cache invalidation, and frontend refresh. Do not label a workflow atomic unless all of those boundaries support it.
2. Preserve the lock order: scan mutex before thumbnail write lock. Decide explicitly whether any new DB command must participate in a broader maintenance barrier rather than relying on the frontend runner.
3. For deletion, decide whether DB-first behavior and success-only counters remain acceptable. If failures become actionable, return failed paths/reasons without redefining existing counts silently.
4. For rename, test filesystem-first failure, DB failure, compensation failure, thumbnail cleanup failure, and revision failure. Never assume the attempted rename-back succeeded.
5. Keep duplicate identity, stored `file_name_key`, CSV matching, rename, and scan-derived basenames aligned. A switch to content hashing is a contract and migration change, not a query-only refactor.
6. Keep the five CSV headers and parsing rules synchronized with Rust models, TypeScript types, UI messages, and the [IPC contract](../architecture/ipc-contract.md). Test missing/reordered/case-varied headers, quoting, malformed later rows, repeated rows, basename fan-out, no-ops, and delimiter-bearing tags.
7. If CSV import becomes transactional, include all asset fields and the revision bump in the same designed boundary, and define how a large file affects lock time and progress.
8. For bundle export, prefer a SQLite-supported consistent snapshot/checkpoint design before claiming consistency. Test concurrent metadata writes and failure after destination truncation.
9. For restore, validate every archive entry before live mutation; bound entry count and expanded size; add integrity/foreign-key/application compatibility checks as requirements dictate. Keep staging on the same filesystem if rename atomicity is required.
10. Exercise failures at every rename in both move-aside and install phases. Preserve recovery generations until final initialization, revision bump, pool/query invalidation, and a verification read all succeed; surface rollback failure separately.
11. Define path-remapping policy before calling bundles portable across profiles or machines. Test absent media, stale absolute thumbnail paths, optional sidecars, no thumbnails, legacy schemas, and a DB with a future or foreign schema.
12. After an uncertain destructive or restore failure, make a byte-for-byte copy of all live, previous, staging, WAL, and SHM files before reopening or retrying. Reconcile the filesystem against DB rows, then restart and force a library/query refresh.

### Relevant existing tests

- `src-tauri/src/commands/scan.rs` tests invalid/valid roots and root removal with thumbnail cleanup.
- `src-tauri/src/commands/assets.rs` tests rename filename normalization; `src-tauri/src/db.rs` tests delete-path return values, duplicate basename groups, rename DB updates, thumbnail-reference clearing, root-prefix boundaries, and cleanup helpers.
- `src-tauri/src/services/thumb_service.rs` tests successful thumbnail-file counting and selected thumbnail lifecycle behavior.
- `src-tauri/src/commands/import_export.rs` tests favorite and group-order scalar parsing. `src-tauri/src/utils/tags.rs` tests tag normalization, merge, and CSV delimiters.
- `src-tauri/src/services/backup_service.rs` tests traversal sanitization, inclusion of DB sidecars and nested thumbnails, rejection without `media.db`, and a successful DB/thumbnail restore.
- `src-tauri/tests/backend_integration.rs` covers file-backed root-prefix deletion and clear-library DB state. `src-tauri/tests/backend_e2e.rs` covers a DB-level CSV-style tag merge, group metadata, and library clear workflow; it does not invoke the CSV command parser itself.
- Frontend settings tests cover confirmation gates, exclusive-runner sequencing, summary display, refresh/reset follow-ups, and sequential duplicate actions. They do not turn those frontend behaviors into backend atomicity guarantees.

There is currently no fault-injection coverage for suppressed filesystem deletion failures, rename compensation failure, mid-CSV partial commit, inconsistent concurrent bundle export, move-aside failure, install rollback failure, post-install initialization failure, archive resource exhaustion, or cross-profile path remapping. Treat those as known untested recovery boundaries, not implemented hardening.
