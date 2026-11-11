# 0017: Export targets are validated before any folder is created, and the indexed-asset check reads assets only for an existing target

- Status: Accepted
- Date: 2026-11-11
- Areas: data safety (CSV export, bundle export), database queries
- Related: [data safety](../subsystems/data-safety-and-portability.md#export-target-validation), [IPC contract](../architecture/ipc-contract.md), [ADR 0016](0016-startup-sweep-of-abandoned-backup-temporaries.md)

## Context

`export_tags_csv` and `export_db_bundle` each had their own `validate_export_target`. Both called `fs::create_dir_all` on the requested parent before checking anything, so a target inside the active profile or an indexed root got its missing folders created and was rejected only afterwards. The empty folders stayed in the locations the checks protect.

Both then loaded every asset path into memory and called `Path::canonicalize` on each one to reject a target equal to an indexed file. That costs one `realpath` per asset (several `lstat`/`readlink` calls each) before any byte is written, and for the bundle export it runs while the maintenance gate, `scan_lock`, and the exclusive thumbnail lock are held. Libraries reach 1,000,000 assets.

## Options considered

1. **Keep the order, delete the created folders on rejection.** It needs to remember which folders were new, races with other writers, and still touches the protected location.
2. **Resolve the target without creating folders, check it, then create the folders.** No filesystem change happens before the decision.
3. **Skip the per-asset check when the canonical target is outside every canonical root.** O(1), but an asset reached through a symbolic link inside a root, or a row whose root is no longer registered, could then be overwritten. Rejected: it weakens the guarantee.
4. **Look up only `assets.path = target`.** O(log n), but it misses the same aliases as option 3. Rejected as the only check; kept as a fast path.
5. **Read assets only when the target exists, compare device and inode, and confirm with `canonicalize`.** Exact, and free for the common new-file export.

## Decision

Options 2 and 5, in one helper `src-tauri/src/services/export_target.rs` used by both exports:

- The parent is resolved one component at a time: an existing component is canonicalized (following a symbolic link, also one reached after a missing folder and `..`), a missing one is appended as a plain name, `..` drops the last resolved component, and a dangling symbolic link is an error. This matches the path the kernel resolves after the folders are created.
- The profile, indexed-root and indexed-asset checks run on that path. Indexed roots are resolved with the same function, so a registered root whose folder is missing still rejects targets below it. For a root, unlike a target, a dangling symbolic link is followed to its destination (up to 40 links) so the folder it will point to stays protected; the earlier code got that result only because it had already created the folders. Only then is `create_dir_all` called on the resolved parent, and the export fails if the parent then canonicalizes to anything else.
- The indexed-asset check returns false without reading the database when the target does not exist or is a symbolic link, because a canonical path never names either. Otherwise it tries an exact match through the unique index on `assets.path`, then streams `SELECT path FROM assets` and compares each asset's device and inode with the target's, canonicalizing only a matching candidate so a hard link stays accepted.

Existing error messages and the set of rejected targets do not change, with one deliberate exception: a target equal to a registered root whose folder is missing is now rejected (the earlier code skipped a root it could not canonicalize). New messages are added for a dangling symbolic link or a file used as a folder in the parent, and for a parent that no longer canonicalizes to the checked path after folder creation.

## Consequences

- A rejected export leaves no new folder. A path through a dangling symbolic link is rejected before anything is created.
- A new-file export no longer reads asset rows for validation. Overwriting an existing file is still linear: one `stat` per asset instead of one `realpath`, with no materialized list. Measured on 100,000 warm tmpfs assets in a release build: about 140 ms before, about 30 µs for a new file and about 40 ms for an existing file after. Not measured at 1,000,000 or on cold storage.
- `db::list_backup_asset_paths` was removed; `db::asset_path_exists` and `db::any_asset_path` were added. No schema, IPC, or profile change.
- Regression tests in `export_target.rs` compare the new check with the earlier per-asset loop on direct, aliased, hard-linked, symbolic-link, new and missing targets; `csv_service.rs` and `backup_service.rs` test that a rejected target creates no folder.
- The partial `.<name>.<pid>-<n>.tmp` archive that a hard kill leaves next to an accepted target (ADR 0016) is unchanged.
