# Data safety and portability

Implementation entry points: [journaled file mutations](../../src-tauri/src/services/asset_mutation_service.rs), [bundle validation and recovery](../../src-tauri/src/services/backup_service.rs), [CSV transfer](../../src-tauri/src/services/csv_service.rs), [command orchestration](../../src-tauri/src/commands/import_export.rs).

This page owns source-file mutations, CSV/bundle formats, validation, commit points, and recovery limits. [Settings](settings-operations.md) owns confirmation and refresh sequencing; confirmation alone does not make filesystem and database work atomic. [Database](database.md) owns SQLite transactions/revisions, [scanning](scanning-and-indexing.md) owns root membership, and [thumbnails](thumbnails.md) owns cache cleanup.

## Safety model

MediaTagger has three distinct kinds of state:

- source media at indexed paths, normally outside the application-data directory;
- SQLite state in `media.db` and, while WAL is active, `media.db-wal` and `media.db-shm`; and
- generated files below the profile's thumbnail directory.

A SQLite transaction can protect only database statements. Filesystem rename/delete and ZIP or CSV I/O occur outside those transactions. Source-media mutation therefore uses hidden sibling `.mediatagger-*.pending` staging paths on the source filesystem plus a durable `pending_file_operations` journal; thumbnail cleanup remains best effort.

### Operation safety matrix

| Operation | Backend lock | Database effect and order | Source-media effect | Thumbnail effect | Rollback and returned summary |
| --- | --- | --- | --- | --- | --- |
| Remove scan root | Combined workflow locks | In one transaction, delete root/mappings, globally orphaned assets, failures and tags; bump revision only if assets were removed. | Never deletes source media. Assets mapped to another root remain indexed. | Best-effort deletion of recorded orphan thumbnails after commit. | No filesystem rollback; returned counts distinguish committed asset removals from successful thumbnail deletions. |
| Delete asset | Scan mutex, then exclusive thumbnail write lock | Validate the current row; journal and stage an existing source. In one transaction CAS-delete by ID/path/version, clear failure, clean orphan tags, and bump revision. | Missing source is explicit stale-record cleanup. Existing source is removed from staging only after commit. | Recorded thumbnail deletion is best effort after commit. | Pre-commit failure restores the source and preserves metadata. Final cleanup failure returns `cleanup_pending` with a recovery path. `deleted`, `missing`, and `cleanup_pending` are distinct. |
| Rename asset file | Scan mutex, then exclusive thumbnail write lock | Journal and stage the source, atomically install a no-clobber target, then CAS-update path/name/version, clear failure, and bump revision in one transaction. | Same-directory only. Validation rejects separators, controls, Windows-invalid characters, trailing dot/space, reserved device names, same path, and occupied filesystem/indexed targets. | Old thumbnail deletion is best effort after commit. | Pre-commit failure runs reverse-order no-clobber rollback and retains journal data when uncertain. Linux uses `renameat2(RENAME_NOREPLACE)`. |
| Clear all thumbnails | Exclusive thumbnail write lock only | Read distinct non-null paths, set all `assets.thumb_path` values to null, then delete all failure rows. Those statements are not one transaction and do not bump the library revision. | None. | After DB references are cleared, attempt every recorded file deletion with progress. | No rollback. Return value is only the number of successful file deletions. Failed/missing files can remain unreferenced; a DB error can leave partially cleared metadata and prevents later file cleanup. |
| Clear library | Scan mutex, then exclusive thumbnail write lock | In one transaction, read distinct thumbnail paths, delete assets, failures, tags and roots, and bump revision. Asset mappings cascade. | Never deletes source media. | After commit, attempt recorded thumbnail deletes with progress. | A DB or revision error rolls back the complete database clear. Success reports removed asset/root rows and successful thumbnail deletes, not undeleted files. |
| Find/resolve duplicates | Lookup has no workflow lock; apply takes scan mutex then exclusive thumbnail write lock | Scan returns revision plus per-row path/version. Apply validates the whole snapshot and uses one transaction for every CAS mutation, cleanup, and one revision bump. | Existing sources are journaled/staged before final targets. Source-target cycles are rejected; targets are checked against filesystem and DB before mutation. | Preview uses normal thumbnail flow; old thumbnails are best effort after commit. | Apply returns every item and `committed`, `rolled_back`, or `recovery_required`. Unresolved positions retain exact recovery paths and journal rows. |

The frontend serializes settings actions within one mounted controller and asks for confirmation before root removal, clear-library, bundle overwrite, and duplicate batches containing deletion. Confirmed bundle restore and clear-library additionally use the shell-owned tag-mutation maintenance barrier: they drain already-dispatched tag writes, block new ones, and invalidate identity-bound tag/details state before releasing the barrier whether the command resolves or rejects. On rejection they then attempt root/library refreshes because either backend workflow may have changed live state before returning an error. This remains user-interface orchestration only; bundle export/restore separately enforce their process-wide backend maintenance gate for direct IPC callers and other windows. Exact success refresh/reset order is documented in [settings operations](settings-operations.md).

### Duplicate identity

A duplicate means that at least two indexed rows have the same nonempty `file_name_key`. That key is the lowercase basename, not the full path, byte content, content hash, dimensions, size, or timestamp. Consequently, unrelated files named `IMG_0001.JPG` and `img_0001.jpg` are one duplicate group even if their contents differ, while byte-identical files with different basenames are not duplicates.

Groups sort by asset count descending and key ascending. Members sort by `modified_at` descending and then ID descending. The displayed group filename is a representative stored spelling; it is not an identity stronger than the lowercase key. A resolver scan is a point-in-time snapshot carrying the library revision and each row's `record_version`. The complete staged change set crosses one IPC call; stale snapshots and unresolved/new duplicate names are rejected before the first file move.

### File-mutation commit and recovery

The commit point is the SQLite transaction applying all expected ID/path/version mutations and incrementing `library_metadata.revision`. Before it, metadata is unchanged and source moves are compensatable. After it, DB identity is authoritative; rename targets are installed, while staged-delete and thumbnail cleanup may remain.

The journal is persisted before source staging, and its `committed` flag changes in the same transaction as asset mutation and revision. Before moving an existing source, the service canonicalizes it and requires it to remain below at least one assigned scan root; this rejects a source reached through a symlinked-parent escape. Startup reconciles the journal before normal command handling: an uncommitted operation restores staging/final to the original path, a committed delete retries cleanup only after the staging file canonicalizes below a scan root containing the journaled original path, and a committed rename requires its final regular file. On Unix, successful journaled renames and staged-file removals also sync the affected parent directory. Cleanup-only delete failures retain the journal but do not prevent startup; an uncertain active-record/source mapping stops startup rather than discarding recovery data.

## CSV metadata transfer

CSV transfers metadata only. They neither copy media nor add missing assets.

### Export format

Export writes a comma-delimited CSV with standard `csv` crate field quoting and these exact headers, in this order:

```text
file_name,tags,favorite,media_group_key,media_group_order
```

Rows are selected by asset ID ascending. `file_name` is derived from the stored path's basename. Tags are joined into one field with a single ASCII space, favorite is `1` or `0`, missing group values are empty strings, and a present group order uses Rust floating-point string formatting. Stored tags cannot contain whitespace, comma, semicolon, or control characters, so the tag field round-trips through the importer. The exporter preserves values for application round-trip and does not prefix spreadsheet formula markers such as `=`, `+`, `-`, or `@`; treat the result as data when opening it in spreadsheet software. The success summary's `rows` is the number of records written.

The rows come from one SQLite SELECT, so that statement reads one SQLite snapshot even if unrelated metadata writes occur concurrently. The command does not acquire scan or thumbnail locks. It rejects a target inside the active profile or an indexed source root and cannot overwrite an indexed source file. It writes a unique sibling temporary file with `create_new`, flushes and syncs it, atomically replaces the destination, and syncs the parent directory on Unix. Failure before the rename removes the temporary file and preserves an existing destination. A parent-directory sync failure occurs after publication, so the command reports an error even though the new target may already be visible.

Export promises no stable ordering among an asset's tags beyond the order produced by the current aggregate query.

### Import headers, parsing, and matching

The reader accepts files up to 64 MiB, at most 100,000 data records, and fields up to 1 MiB. It expects a header row, trims all fields, and uses normal comma-delimited CSV parsing. Each of the five standard headers must occur exactly once after case-insensitive matching and trimming. Their order may change and unknown columns are ignored. A missing or duplicate standard header rejects the complete document.

Tags split on comma, semicolon, or whitespace, then are trimmed, Unicode-lowercased by Rust, de-duplicated in first-seen order, and merged into existing tags. Import never removes an existing tag, and an empty tags field does not clear tags. The importer checks regular-file status and size on the opened handle, then reads through a 64 MiB plus one-byte bound so path replacement or concurrent growth cannot cause an unbounded read. The same backend validator used by direct tag mutations rejects any resulting tag that violates the storage invariant.

Favorite parsing accepts `1`, `true`, `yes`, `y`, and `on` as true and `0`, `false`, `no`, `n`, and `off` as false, case-insensitively. Blank favorite text leaves the current value unchanged; any other value rejects the document. A blank group key explicitly clears the key and nonblank text is retained after outer trimming. A blank group order clears the order, while a finite `f64` replaces it; invalid or non-finite text rejects the document. Key and order remain independent.

Each nonblank imported filename is lowercased and matched exactly against `assets.file_name_key`. Matching is basename-only and fans out to every indexed asset with that key, across all roots and directories. It does not use the CSV path, hash, or any other identity. Repeated CSV rows may update and count the same asset repeatedly.

### Merge, counters, and transaction failure

For a successfully returned import:

| Counter | Meaning |
| --- | --- |
| `rows_read` | Data records successfully parsed, including blank, unmatched, and no-op records. The header is excluded. |
| `rows_applied` | Records with a nonblank filename, at least one effective metadata instruction, and at least one matching asset. A no-op match still counts. |
| `assets_matched` | Sum of matched asset IDs over applied rows; it is not a distinct-asset count. |
| `assets_updated` | Sum of row/asset applications that changed at least one field; the same asset can count more than once. |

The service parses and validates the complete document before opening the database transaction. It then uses one bounded-retry IMMEDIATE transaction: every matched asset's tag replacement, favorite, and group writes commit together with the single library-revision bump when at least one row changed. A malformed record causes no database work; a database error rolls back every row and leaves revision unchanged. Before invoking `import_tags_csv`, the frontend requests the shared metadata-mutation barrier. That blocks new tag, favorite, and group writes, waits for dispatched writes to settle, invokes the import, and keeps maintenance ownership through authoritative/local details-cache invalidation whether the command resolves or rejects. The barrier releases before the library refresh, which is best effort on rejection. Tag merge is additive and idempotent for the same normalized inputs, while favorite/group assignments may intentionally overwrite or clear values.

See [search, tags, and media groups](search-tags-and-media-groups.md) for tag normalization and group semantics, and [database persistence](database.md) for why a missing revision bump can leave an existing query session apparently current.

## Database bundle format

The bundle is a deflated ZIP with no media payload. Current exports use format version `2`; their manifest identifies `io.github.mediatagger.bundle`, records database schema version `1`, source platform, exact scan roots, and the source thumbnail directory. Import requires those identity fields for every version-2 manifest and rejects future versions, foreign application IDs, unsupported schema versions, and manifest roots that differ from the staged database. Recognized entries are:

```text
media.db                  required
media.db-wal              optional legacy/import compatibility
media.db-shm              optional legacy/import compatibility
manifest.json             required by format version 2
thumbs/<relative path>    zero or more files, recursively copied
```

Export first enters process-wide database maintenance, invalidates/drains the query pool, and takes `scan_lock` followed by the exclusive thumbnail lock. It rejects a database with pending local file operations, creates one standalone `media.db` through SQLite Backup API, then writes the manifest, that snapshot, and regular files found below the thumbnail directory. Current exports never include WAL or SHM, so `copied_files` is `1`; import continues to accept validated legacy sidecars. `copied_thumbnails` counts thumbnail files. Source media is never included.

Compatibility is deliberately limited. Format version `1` manifests from earlier MediaTagger exports and archives without a manifest are legacy inputs only. They are accepted only when the database has either current MediaTagger SQLite markers or zero application/schema markers plus the known legacy core tables, columns, declared types, metadata rows, valid stored scalar types, valid integrity/foreign keys, and no pending local file operations. Legacy input gets migrated only inside staging. A manifestless archive does not gain format-v2 guarantees and future/foreign nonzero SQLite markers are never treated as legacy.

Both inspection and import scan the entire ZIP directory before extraction. The compressed archive is limited to 8 GiB, entry count to 250,000, each expanded entry to 8 GiB, total expanded data to 32 GiB, and per-entry compression ratio to 1000:1; the manifest is limited to 1 MiB. Unsafe paths and ZIP symbolic links are rejected. Duplicate normalized, case-folded destinations are rejected for `manifest.json`, the database and sidecars, and every thumbnail path. Actual extracted byte counts must match ZIP metadata.

Bundle export rejects a target inside the active profile or any indexed root, and therefore cannot target the live database, sidecars, thumbnail tree, or indexed source media. Directory traversal also excludes both the final and temporary target explicitly. The ZIP is written with `create_new` to a unique sibling temporary file, completed, `sync_all`ed, and atomically published over the destination; a failed export removes its temporary archive and leaves an existing destination unchanged. The SQLite snapshot and manifest are created while no application SQLite user can run, so they represent one point in time.

## Bundle import and replacement sequence

Import closes the process-wide database gate, invalidates the pool and sessions, waits for all direct and checked-out pooled connections to close, then takes the scan mutex followed by the exclusive thumbnail lock. New SQLite users wait outside that hierarchy. Within that boundary it performs the following sequence:

1. Require an existing regular source file within the compressed-size limit, open it as ZIP, and preflight every entry for path safety, type, resource limits, duplicate recognized destinations, manifest compatibility, and an exact sanitized `media.db`.
2. Reject an archive stored inside the active profile and any unexplained `*.restore-previous` or restore-journal artifact. Resolve the app-data parent and use fixed staging paths `restore-staging/media.db` and `restore-staging/thumbs`. Delete a pre-existing pre-swap staging tree, then create the thumbnail staging directory.
3. Extract only validated database/sidecar and thumbnail entries with bounded readers. Backslashes are normalized to slashes; absolute paths, parent components, rooted paths, platform prefixes, symbolic links, and size mismatches are rejected. Safe unrecognized entries are ignored.
4. Require a complete SQLite header, valid page size, and page-aligned nontrivial main file. Open the candidate read-only, run `PRAGMA integrity_check` and `foreign_key_check`, classify its application/schema identity, validate required tables/columns/declared types and stored scalar types, reject a nonempty `pending_file_operations`, and compare manifest roots to database roots. Only after success, open staging writable, migrate an accepted legacy schema, checkpoint it, and assign current SQLite identity markers.
5. Validate every scan root and asset path as absolute and free of parent traversal. Every asset must remain within a declared root; mappings must cover every root exactly once, target existing absolute directories, avoid duplicate canonical targets and resulting asset collisions, and existing files must not escape through symlinks. Resolve every non-null thumbnail reference to an extracted staging file and rewrite it below the live thumbnail directory; unresolved legacy references become null. Clear imported thumbnail failures, checkpoint staging again so rewritten rows leave WAL, remove its sidecars, then repeat current-schema integrity, FK, structure, data, and pending-journal validation read-only.
6. Checkpoint the current database, remove its WAL/SHM after the checkpoint, and durably write `restore-journal.json` in phase `swapping`, recording whether the live DB and thumbnails existed. Recovery paths are `media.db.restore-previous` and `thumbs.restore-previous`.
7. Durably rename the current DB and thumbnails aside, then install the staged DB and thumbnails. Every rename is inside one compensation boundary. Any returned failure invokes the same journal recovery used at startup; a rollback failure is appended to the returned error and leaves the journal for another startup attempt.
8. Open the installed DB, run `init_schema`, repeat full backup validation, bump revision, perform revision and root control reads, checkpoint the new DB, and remove its sidecars. The previous generation still exists throughout these checks.
9. Atomically rewrite and sync the journal to phase `committed`. Recovery now preserves the new generation and removes only previous/staging artifacts. Delete the journal only after cleanup succeeds, then invalidate the query pool and sessions again.
10. Return `restored_files` for extracted DB/main-sidecar entries and `restored_thumbnails` for extracted thumbnail entries. The same-shell frontend invalidates tag/details identity state before releasing its tag-mutation barrier, resets thumbnail identity, then reloads roots and refreshes assets and known tags. It performs invalidation and best-effort refresh on rejection too.

Validation authenticates format/application/schema identity, not the archive author or cryptographic integrity; bundles are not signed or encrypted. WAL, SHM, and thumbnails remain optional import inputs, but included sidecars must form a database SQLite can open and validate read-only. Maintenance begins before archive validation so no live database user can outlive validation and enter the replacement window.

### Restore rollback and recovery

- Failure while reading, preflighting, extracting, validating, migrating, rewriting, or checkpointing staging occurs before the live swap. The live DB/thumb directories remain in place, and import attempts to remove partial staging before returning the validation error.
- Once `restore-journal.json` phase `swapping` is durable, any runtime error or startup recovery removes a partial new generation and restores every previous component that originally existed. Recovery is idempotent across interruption between individual rollback operations.
- The journal changes to `committed` only after the installed database passes integrity/structure/data validation, initialization, revision bump, checkpoint, and control reads. Startup then preserves the new generation and finishes cleanup.
- Unexplained previous-generation artifacts are never deleted by a new import. Startup fails on a corrupt or irreconcilable journal rather than guessing or mixing generations.
- A filesystem error can prevent immediate compensation or cleanup. The command reports both the original and rollback error when applicable and retains the journal and matching generations for startup recovery/manual preservation.
- Pool invalidation actively closes idle connections, rejects/retires old-pool checkout/return, and maintenance waits for checked-out and direct connection leases. Query sessions are cleared before and after replacement.

Before a Windows backup is installed on Linux, `inspect_db_bundle` reports every source root and the UI requires an existing absolute target directory for each one. Import validates all source roots and asset membership even when no mapping is requested. A mapped import requires exactly one existing absolute target for every source root and rewrites `assets.path`, `scan_roots`, `asset_scan_roots`, and derived filename fields in staging. Format-v2 thumbnail references are matched by their exact source-relative path; legacy references use only a unique basename. Every accepted reference is rewritten below live thumbs, while missing or ambiguous references become null. Asset IDs and relationship tables are preserved, target and canonical-root collisions are rejected, and stale thumbnail failures are cleared. Source media is never copied, so missing mapped files remain indexed until a later scan reconciles them.

If restore reports that automatic rollback also failed, stop mutating the library and preserve copies of the app-data directory before restarting. Keep `restore-journal.json`, `media.db`, `media.db.restore-previous`, `restore-staging/media.db`, `thumbs`, `thumbs.restore-previous`, and `restore-staging/thumbs` together. Normal startup retries journal recovery before opening SQLite; if that fails, prefer a verified whole generation over manually combining paths.

## Known limitations

- Source staging and the SQLite commit remain separate failure boundaries, but asset mutations expose rollback/recovery state and journal uncertain files. Thumbnail cleanup, archive I/O, and frontend refreshes remain separate.
- Thumbnail best-effort deletion still suppresses individual cache-file errors. Source deletion distinguishes missing input, pre-commit I/O rejection with preserved metadata, and post-commit cleanup with a recovery path.
- Root removal and clear-library each use a database transaction. Thumbnail deletion happens afterward and remains best effort. Journaled source mutations add a separate staging/recovery boundary.
- The fixed restore staging/previous names are recovery generations, not retained user backups. Successful committed cleanup deletes them; a new attempt refuses unexplained leftovers.
- Archive export has one SQLite Backup API snapshot and atomic publication. Import validates integrity and format/application/schema identity and uses durable journal recovery, but bundles remain unsigned/unencrypted and filesystem durability still depends on the host filesystem honoring sync/rename guarantees.
- Duplicate and CSV identity is case-insensitive basename, not content identity. Duplicate apply does not support source-target rename cycles; users must choose independent final names.
- CSV cannot remove tags; it can only merge them. The shared tag invariant prevents creation of values that its tag field cannot encode.
- Source and thumbnail paths make bundles environment-dependent. The bundle is a profile-state backup, not a self-contained media archive.
- All application SQLite opens, including pooled queries and video resolution, participate in process-wide bundle maintenance. External tools that bypass the process remain outside this guarantee.

## Safe change and recovery checklist

1. Write down the intended commit point and exact ordering across DB, revision, source media, thumbnail files, cache invalidation, and frontend refresh. Do not label a workflow atomic unless all of those boundaries support it.
2. Preserve the lock order: normal workflows take a database lease before the scan mutex and thumbnail write lock; exclusive bundle work takes maintenance before scan and thumbnail. Never enter maintenance while holding either lower lock.
3. For deletion, preserve the distinctions between missing source, pre-commit failure, and post-commit cleanup pending. Verify returned counts and recovery paths against both SQLite and surviving files.
4. For rename, test filesystem-first failure, DB failure, compensation failure, thumbnail cleanup failure, and revision failure. Never assume the attempted rename-back succeeded.
5. Keep duplicate identity, stored `file_name_key`, CSV matching, rename, and scan-derived basenames aligned. A switch to content hashing is a contract and migration change, not a query-only refactor.
6. Keep the five CSV headers and parsing rules synchronized with Rust models, TypeScript types, UI messages, and the [IPC contract](../architecture/ipc-contract.md). Test missing/reordered/case-varied headers, quoting, malformed later rows, repeated rows, basename fan-out, no-ops, and delimiter-bearing tags.
7. Preserve complete CSV validation before the single transaction, and commit every asset field with its conditional revision bump. Test a malformed later row and transaction failure without partial metadata changes.
8. Preserve SQLite Backup API snapshots and temporary-file publication. Test concurrent metadata writes, export failure before publication, and preservation of an existing destination.
9. Preserve archive preflight limits and SQLite integrity, foreign-key, schema, and application checks before live mutation. Keep restore staging on the profile filesystem so install and rollback use same-filesystem renames.
10. Exercise failures at every rename in both move-aside and install phases. Preserve recovery generations until final initialization, revision bump, pool/query invalidation, and a verification read all succeed; surface rollback failure separately.
11. Define path-remapping policy before calling bundles portable across profiles or machines. Test absent media, stale absolute thumbnail paths, optional sidecars, no thumbnails, legacy schemas, and a DB with a future or foreign schema.
12. After an uncertain destructive or restore failure, make a byte-for-byte copy of all live, previous, staging, WAL, and SHM files before reopening or retrying. Reconcile the filesystem against DB rows, then restart and force a library/query refresh.

### Relevant existing tests

- `src-tauri/src/commands/scan.rs` tests invalid/valid roots and root removal with thumbnail cleanup.
- `src-tauri/src/commands/assets.rs` tests rename filename normalization; `src-tauri/src/db.rs` tests delete-path return values, duplicate basename groups, rename DB updates, thumbnail-reference clearing, root-prefix boundaries, and cleanup helpers.
- `src-tauri/src/services/thumb_service.rs` tests successful thumbnail-file counting and selected thumbnail lifecycle behavior.
- `src-tauri/src/services/csv_service.rs` tests headers, scalar parsing, complete pre-validation, basename fan-out, transaction rollback, and atomic export behavior. `src-tauri/src/utils/tags.rs` tests tag normalization, merge, CSV delimiters, and legacy separators.
- `src-tauri/src/services/backup_service.rs` tests path sanitization, standalone snapshot export without sidecars, nested thumbnails, rejection without a database, successful restore, and Windows-to-Linux root mapping with metadata preservation.
- `src-tauri/tests/backend_integration.rs` covers file-backed root-prefix deletion and clear-library DB state. `src-tauri/tests/backend_e2e.rs` covers a DB-level CSV-style tag merge, group metadata, and library clear workflow; it does not invoke the CSV command parser itself.
- Frontend settings tests cover confirmation gates, exclusive-runner sequencing, summary display, refresh/reset follow-ups, and structured duplicate-batch outcomes. They do not turn those frontend behaviors into backend atomicity guarantees.

There is currently no fault-injection coverage for suppressed filesystem deletion failures, rename compensation failure, a post-publication CSV directory-sync failure, inconsistent concurrent bundle export, move-aside failure, install rollback failure, post-install initialization failure, CSV or archive resource exhaustion, or every path-remapping collision and missing-file case. Successful Windows-to-Linux mapping is covered. These are test gaps; the validation and recovery mechanisms described above are implemented.
