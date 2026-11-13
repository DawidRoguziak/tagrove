use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::Context;
use rand::{distributions::Alphanumeric, Rng};

use crate::{
    db::{self, DbFileMutation, FileMutationAsset, PendingFileOperation},
    models::{
        DeleteAssetSummary, DeleteSourceStatus, DuplicateResolutionBatchInput,
        DuplicateResolutionBatchStatus, DuplicateResolutionChangeInput,
        DuplicateResolutionItemResult, DuplicateResolutionItemStatus, RenameAssetStatus,
        RenameAssetSummary,
    },
    services::thumb_service,
};

enum PreparedAction {
    Rename {
        new_file_name: String,
        new_path: String,
    },
    Delete,
}

struct PreparedMutation {
    asset: FileMutationAsset,
    action: PreparedAction,
    staging_path: Option<PathBuf>,
    source_missing: bool,
    finalized: bool,
    rollback_path: Option<PathBuf>,
}

pub fn delete_asset(
    conn: &rusqlite::Connection,
    thumbs_root: &Path,
    asset_id: i64,
) -> anyhow::Result<DeleteAssetSummary> {
    let asset = db::get_file_mutation_asset(conn, asset_id)?
        .ok_or_else(|| anyhow::anyhow!("Asset with id {asset_id} not found"))?;
    let changes = [DuplicateResolutionChangeInput::Delete {
        asset_id,
        expected_path: asset.path,
        expected_record_version: asset.record_version,
    }];
    let summary = apply_file_mutation_batch(conn, thumbs_root, &changes, None)?;
    let result = summary
        .results
        .into_iter()
        .next()
        .context("delete returned no item result")?;
    match summary.status {
        DuplicateResolutionBatchStatus::Committed
        | DuplicateResolutionBatchStatus::RecoveryRequired => {
            let source_status = match result.status {
                DuplicateResolutionItemStatus::Deleted => DeleteSourceStatus::Deleted,
                DuplicateResolutionItemStatus::SourceMissing => DeleteSourceStatus::Missing,
                DuplicateResolutionItemStatus::CleanupPending => DeleteSourceStatus::CleanupPending,
                _ => anyhow::bail!(
                    "delete did not commit; source metadata was preserved; recovery path: {}",
                    result.recovery_path.as_deref().unwrap_or("none")
                ),
            };
            Ok(DeleteAssetSummary {
                removed_assets: 1,
                removed_thumbnails: summary.removed_thumbnails,
                source_status,
                revision: summary.revision,
                recovery_path: result.recovery_path,
            })
        }
        DuplicateResolutionBatchStatus::RolledBack => {
            anyhow::bail!("delete failed and was rolled back; source metadata was preserved")
        }
    }
}

pub fn rename_asset(
    conn: &rusqlite::Connection,
    thumbs_root: &Path,
    asset_id: i64,
    new_file_name: String,
) -> anyhow::Result<RenameAssetSummary> {
    let asset = db::get_file_mutation_asset(conn, asset_id)?
        .ok_or_else(|| anyhow::anyhow!("Asset with id {asset_id} not found"))?;
    let old_path = asset.path.clone();
    let changes = [DuplicateResolutionChangeInput::Rename {
        asset_id,
        expected_path: asset.path,
        expected_record_version: asset.record_version,
        new_file_name,
    }];
    let summary = apply_file_mutation_batch(conn, thumbs_root, &changes, None)?;
    let result = summary
        .results
        .into_iter()
        .next()
        .context("rename returned no item result")?;
    let status = match result.status {
        DuplicateResolutionItemStatus::Renamed => RenameAssetStatus::Renamed,
        DuplicateResolutionItemStatus::CleanupPending
            if matches!(
                summary.status,
                DuplicateResolutionBatchStatus::RecoveryRequired
            ) =>
        {
            RenameAssetStatus::CleanupPending
        }
        _ => {
            anyhow::bail!(
                "rename did not commit; recovery path: {}",
                result.recovery_path.as_deref().unwrap_or("none")
            );
        }
    };
    Ok(RenameAssetSummary {
        asset_id,
        old_path,
        new_path: result.new_path.context("rename returned no destination")?,
        removed_thumbnails: summary.removed_thumbnails,
        revision: summary.revision,
        status,
        recovery_path: result.recovery_path,
    })
}

pub fn apply_duplicate_resolution_batch(
    conn: &rusqlite::Connection,
    thumbs_root: &Path,
    input: DuplicateResolutionBatchInput,
) -> anyhow::Result<crate::models::DuplicateResolutionBatchSummary> {
    apply_file_mutation_batch(conn, thumbs_root, &input.changes, Some(input.scan_revision))
}

/// Applies a journaled batch of source-file mutations.
///
/// `duplicate_snapshot_revision` is `Some` only for a duplicate-resolution batch: the batch was
/// built from a snapshot at that library revision, which must still be current, and it must not
/// leave a duplicate file name behind. Single-asset deletes and renames pass `None`; their guard
/// is the per-asset path/record-version compare-and-set, so an unrelated metadata write on
/// another asset cannot fail them (ADR 0014).
fn apply_file_mutation_batch(
    conn: &rusqlite::Connection,
    thumbs_root: &Path,
    changes: &[DuplicateResolutionChangeInput],
    duplicate_snapshot_revision: Option<i64>,
) -> anyhow::Result<crate::models::DuplicateResolutionBatchSummary> {
    let mut prepared = validate_and_prepare(conn, changes, duplicate_snapshot_revision)?;
    let operation_id: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .map(char::from)
        .collect();

    let journal = prepared
        .iter()
        .filter_map(|item| {
            item.staging_path
                .as_ref()
                .map(|staging_path| PendingFileOperation {
                    operation_id: operation_id.clone(),
                    asset_id: item.asset.id,
                    action: match &item.action {
                        PreparedAction::Rename { .. } => "rename",
                        PreparedAction::Delete => "delete",
                    }
                    .to_string(),
                    original_path: item.asset.path.clone(),
                    staging_path: staging_path.to_string_lossy().into_owned(),
                    final_path: match &item.action {
                        PreparedAction::Rename { new_path, .. } => Some(new_path.clone()),
                        PreparedAction::Delete => None,
                    },
                    committed: false,
                })
        })
        .collect::<Vec<_>>();
    db::insert_pending_file_operations(conn, &journal)?;

    for index in 0..prepared.len() {
        let Some(staging_path) = prepared[index].staging_path.as_ref() else {
            continue;
        };
        if let Err(error) = rename_no_replace(Path::new(&prepared[index].asset.path), staging_path)
        {
            let mut rollback_failures = rollback_files(&mut prepared);
            clear_restored_journal(conn, &operation_id, &mut prepared, &mut rollback_failures);
            return rollback_summary(
                conn,
                prepared,
                rollback_failures,
                format!("cannot stage source: {error}"),
            );
        }
    }

    for index in 0..prepared.len() {
        let PreparedAction::Rename { new_path, .. } = &prepared[index].action else {
            continue;
        };
        let staging_path = prepared[index]
            .staging_path
            .as_ref()
            .context("rename has no staging path")?;
        let moved = rename_no_replace(staging_path, Path::new(new_path));
        prepared[index].finalized = moved.as_ref().map_or_else(|error| error.moved, |_| true);
        if let Err(error) = moved {
            let mut rollback_failures = rollback_files(&mut prepared);
            clear_restored_journal(conn, &operation_id, &mut prepared, &mut rollback_failures);
            return rollback_summary(
                conn,
                prepared,
                rollback_failures,
                format!("cannot install rename target: {error}"),
            );
        }
        prepared[index].finalized = true;
    }

    let db_mutations = prepared
        .iter()
        .map(|item| match &item.action {
            PreparedAction::Rename {
                new_file_name,
                new_path,
            } => DbFileMutation::Rename {
                asset_id: item.asset.id,
                expected_path: &item.asset.path,
                expected_record_version: item.asset.record_version,
                new_path,
                new_file_name,
            },
            PreparedAction::Delete => DbFileMutation::Delete {
                asset_id: item.asset.id,
                expected_path: &item.asset.path,
                expected_record_version: item.asset.record_version,
            },
        })
        .collect::<Vec<_>>();

    let revision = match db::apply_file_mutations(conn, &operation_id, &db_mutations) {
        Ok(revision) => revision,
        Err(error) => {
            let mut rollback_failures = rollback_files(&mut prepared);
            clear_restored_journal(conn, &operation_id, &mut prepared, &mut rollback_failures);
            return rollback_summary(
                conn,
                prepared,
                rollback_failures,
                format!("database commit failed: {error}"),
            );
        }
    };

    let thumbnail_paths = prepared
        .iter()
        .filter_map(|item| item.asset.thumb_path.clone())
        .collect();
    let removed_thumbnails =
        thumb_service::delete_thumbnail_files_in_root(thumbs_root, thumbnail_paths);
    let mut recovery_required = false;
    let mut results = Vec::with_capacity(prepared.len());
    for item in prepared {
        let new_path = match &item.action {
            PreparedAction::Rename { new_path, .. } => Some(new_path.clone()),
            PreparedAction::Delete => None,
        };
        let (status, recovery_path) = match &item.action {
            PreparedAction::Rename { .. } => {
                if db::remove_pending_file_operation(conn, &operation_id, item.asset.id).is_ok() {
                    (DuplicateResolutionItemStatus::Renamed, None)
                } else {
                    recovery_required = true;
                    (
                        DuplicateResolutionItemStatus::CleanupPending,
                        new_path.clone(),
                    )
                }
            }
            PreparedAction::Delete if item.source_missing => {
                (DuplicateResolutionItemStatus::SourceMissing, None)
            }
            PreparedAction::Delete => {
                let staging_path = item
                    .staging_path
                    .as_ref()
                    .context("delete has no staging path")?;
                // The journal row is the only record of the staged file, so it is dropped only
                // once that file is verifiably gone. A parent folder that vanished after the
                // commit is not proof.
                if remove_staged_delete(staging_path).is_ok()
                    && db::remove_pending_file_operation(conn, &operation_id, item.asset.id).is_ok()
                {
                    (DuplicateResolutionItemStatus::Deleted, None)
                } else {
                    recovery_required = true;
                    (
                        DuplicateResolutionItemStatus::CleanupPending,
                        Some(staging_path.to_string_lossy().into_owned()),
                    )
                }
            }
        };
        results.push(DuplicateResolutionItemResult {
            asset_id: item.asset.id,
            status,
            old_path: item.asset.path,
            new_path,
            recovery_path,
        });
    }

    Ok(crate::models::DuplicateResolutionBatchSummary {
        status: if recovery_required {
            DuplicateResolutionBatchStatus::RecoveryRequired
        } else {
            DuplicateResolutionBatchStatus::Committed
        },
        revision,
        results,
        removed_thumbnails,
    })
}

fn validate_and_prepare(
    conn: &rusqlite::Connection,
    changes: &[DuplicateResolutionChangeInput],
    duplicate_snapshot_revision: Option<i64>,
) -> anyhow::Result<Vec<PreparedMutation>> {
    if changes.is_empty() {
        anyhow::bail!("duplicate resolution batch cannot be empty");
    }
    if let Some(scan_revision) = duplicate_snapshot_revision {
        if db::current_library_revision(conn)? != scan_revision {
            anyhow::bail!("duplicate snapshot is stale; rescan before applying changes");
        }
    }

    let mut ids = HashSet::new();
    let mut prepared = Vec::with_capacity(changes.len());
    let mut target_paths = HashSet::new();
    let source_paths = changes
        .iter()
        .map(|change| match change {
            DuplicateResolutionChangeInput::Rename { expected_path, .. }
            | DuplicateResolutionChangeInput::Delete { expected_path, .. } => {
                expected_path.to_lowercase()
            }
        })
        .collect::<HashSet<_>>();
    let changed_ids = changes
        .iter()
        .map(|change| match change {
            DuplicateResolutionChangeInput::Rename { asset_id, .. }
            | DuplicateResolutionChangeInput::Delete { asset_id, .. } => *asset_id,
        })
        .collect::<HashSet<_>>();

    for change in changes {
        let (asset_id, expected_path, expected_record_version) = match change {
            DuplicateResolutionChangeInput::Rename {
                asset_id,
                expected_path,
                expected_record_version,
                ..
            }
            | DuplicateResolutionChangeInput::Delete {
                asset_id,
                expected_path,
                expected_record_version,
            } => (*asset_id, expected_path, *expected_record_version),
        };
        if asset_id <= 0 || !ids.insert(asset_id) {
            anyhow::bail!("duplicate resolution contains an invalid or repeated asset id");
        }
        let asset = db::get_file_mutation_asset(conn, asset_id)?
            .ok_or_else(|| anyhow::anyhow!("Asset with id {asset_id} not found"))?;
        if asset.path != *expected_path || asset.record_version != expected_record_version {
            anyhow::bail!("asset {asset_id} changed since duplicate scan");
        }

        let source_state = match fs::symlink_metadata(&asset.path) {
            Ok(metadata) if metadata.file_type().is_file() => false,
            Ok(_) => anyhow::bail!("asset {asset_id} source is not a regular file"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => true,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot inspect source '{}'", asset.path))
            }
        };
        if !source_state {
            validate_source_within_assigned_root(
                asset.id,
                Path::new(&asset.path),
                &db::assigned_asset_roots(conn, asset.id)?,
            )?;
        }
        let action = match change {
            DuplicateResolutionChangeInput::Delete { .. } => PreparedAction::Delete,
            DuplicateResolutionChangeInput::Rename { new_file_name, .. } => {
                if source_state {
                    anyhow::bail!("cannot rename missing source for asset {asset_id}");
                }
                let normalized = validate_file_name(new_file_name)?;
                let parent = Path::new(&asset.path)
                    .parent()
                    .context("cannot resolve source parent")?;
                let new_path = parent.join(&normalized).to_string_lossy().into_owned();
                if asset.path.eq_ignore_ascii_case(&new_path) {
                    anyhow::bail!("new file name is the same as current one");
                }
                let target_key = new_path.to_lowercase();
                if !target_paths.insert(target_key.clone()) {
                    anyhow::bail!("duplicate resolution contains colliding targets");
                }
                if source_paths.contains(&target_key) {
                    anyhow::bail!("rename targets cannot be another source in the same batch");
                }
                if db::asset_paths_for_name(conn, &normalized.to_lowercase())?
                    .iter()
                    .any(|row| {
                        !changed_ids.contains(&row.id) && row.path.eq_ignore_ascii_case(&new_path)
                    })
                {
                    anyhow::bail!("target path is already indexed: {new_path}");
                }
                match fs::symlink_metadata(&new_path) {
                    Ok(_) => anyhow::bail!("target file already exists: {new_path}"),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("cannot inspect target '{new_path}'"))
                    }
                }
                PreparedAction::Rename {
                    new_file_name: normalized,
                    new_path,
                }
            }
        };
        let staging_path = if source_state {
            None
        } else {
            Some(unique_staging_path(Path::new(&asset.path)))
        };
        prepared.push(PreparedMutation {
            asset,
            action,
            staging_path,
            source_missing: source_state,
            finalized: false,
            rollback_path: None,
        });
    }

    validate_final_duplicate_names(conn, changes, duplicate_snapshot_revision.is_some())?;
    Ok(prepared)
}

fn validate_final_duplicate_names(
    conn: &rusqlite::Connection,
    input_changes: &[DuplicateResolutionChangeInput],
    enforce_duplicate_resolution: bool,
) -> anyhow::Result<()> {
    let changes = input_changes
        .iter()
        .map(|change| match change {
            DuplicateResolutionChangeInput::Rename {
                asset_id,
                new_file_name,
                ..
            } => (*asset_id, Some(new_file_name.trim().to_lowercase())),
            DuplicateResolutionChangeInput::Delete { asset_id, .. } => (*asset_id, None),
        })
        .collect::<HashMap<_, _>>();
    let renamed_keys = changes
        .values()
        .filter_map(Clone::clone)
        .collect::<HashSet<_>>();
    let mut touched_keys = renamed_keys.clone();
    if enforce_duplicate_resolution {
        for change in input_changes {
            let path = match change {
                DuplicateResolutionChangeInput::Rename { expected_path, .. }
                | DuplicateResolutionChangeInput::Delete { expected_path, .. } => expected_path,
            };
            touched_keys.insert(
                Path::new(path)
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or("")
                    .to_lowercase(),
            );
        }
    }
    for key in touched_keys {
        let existing = db::asset_paths_for_name(conn, &key)?;
        let remaining = existing
            .iter()
            .filter(|asset| !changes.contains_key(&asset.id))
            .count();
        let renamed = changes
            .values()
            .filter(|name| name.as_ref() == Some(&key))
            .count();
        if remaining + renamed > 1 {
            anyhow::bail!("duplicate resolution would leave or create another duplicate file name");
        }
    }
    Ok(())
}

fn validate_source_within_assigned_root(
    asset_id: i64,
    source: &Path,
    mappings: &[(i64, String)],
) -> anyhow::Result<()> {
    let canonical_source = source
        .canonicalize()
        .with_context(|| format!("cannot canonicalize source '{}'", source.display()))?;
    let mut has_assignment = false;
    for (_, root) in mappings.iter().filter(|(id, _)| *id == asset_id) {
        has_assignment = true;
        if Path::new(root)
            .canonicalize()
            .is_ok_and(|canonical_root| canonical_source.starts_with(canonical_root))
        {
            return Ok(());
        }
    }
    if !has_assignment {
        anyhow::bail!("asset {asset_id} has no assigned scan root");
    }
    anyhow::bail!("asset {asset_id} source escapes its assigned scan roots through a symbolic link")
}

fn rollback_files(prepared: &mut [PreparedMutation]) -> HashSet<i64> {
    let mut failures = HashSet::new();
    for item in prepared.iter_mut().rev() {
        let Some(staging_path) = item.staging_path.as_ref() else {
            continue;
        };
        let current = match &item.action {
            PreparedAction::Rename { new_path, .. } if item.finalized => Path::new(new_path),
            _ => staging_path,
        };
        match fs::symlink_metadata(current) {
            Ok(_) => {
                if let Err(error) = rename_no_replace(current, Path::new(&item.asset.path)) {
                    failures.insert(item.asset.id);
                    item.rollback_path = Some(if error.moved {
                        PathBuf::from(&item.asset.path)
                    } else {
                        current.to_path_buf()
                    });
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                failures.insert(item.asset.id);
                item.rollback_path = Some(current.to_path_buf());
            }
        }
        match fs::symlink_metadata(&item.asset.path) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            _ => {
                failures.insert(item.asset.id);
                item.rollback_path = Some(PathBuf::from(&item.asset.path));
            }
        }
        item.finalized = false;
    }
    failures
}

fn rollback_summary(
    conn: &rusqlite::Connection,
    prepared: Vec<PreparedMutation>,
    rollback_failures: HashSet<i64>,
    _reason: String,
) -> anyhow::Result<crate::models::DuplicateResolutionBatchSummary> {
    Ok(crate::models::DuplicateResolutionBatchSummary {
        status: if rollback_failures.is_empty() {
            DuplicateResolutionBatchStatus::RolledBack
        } else {
            DuplicateResolutionBatchStatus::RecoveryRequired
        },
        revision: db::current_library_revision(conn)?,
        results: prepared
            .into_iter()
            .map(|item| {
                let failed = rollback_failures.contains(&item.asset.id);
                let new_path = match &item.action {
                    PreparedAction::Rename { new_path, .. } => Some(new_path.clone()),
                    PreparedAction::Delete => None,
                };
                let recovery_path = if failed {
                    item.rollback_path
                        .map(|path| path.to_string_lossy().into_owned())
                } else {
                    None
                };
                DuplicateResolutionItemResult {
                    asset_id: item.asset.id,
                    status: if failed {
                        DuplicateResolutionItemStatus::RollbackFailed
                    } else {
                        DuplicateResolutionItemStatus::RolledBack
                    },
                    old_path: item.asset.path,
                    new_path,
                    recovery_path,
                }
            })
            .collect(),
        removed_thumbnails: 0,
    })
}

fn clear_restored_journal(
    conn: &rusqlite::Connection,
    operation_id: &str,
    prepared: &mut [PreparedMutation],
    failures: &mut HashSet<i64>,
) {
    for item in prepared {
        // A missing-source delete was never journaled or moved, so it has nothing to restore
        // and no recovery path: its row is untouched stale-record cleanup that did not run.
        let Some(staging_path) = item.staging_path.as_ref() else {
            continue;
        };
        if failures.contains(&item.asset.id) {
            continue;
        }
        let original_restored = fs::symlink_metadata(&item.asset.path)
            .is_ok_and(|metadata| metadata.file_type().is_file());
        let staging_absent = matches!(fs::symlink_metadata(staging_path), Err(ref error) if error.kind() == io::ErrorKind::NotFound);
        let final_absent = match &item.action {
            PreparedAction::Rename { new_path, .. } => {
                matches!(fs::symlink_metadata(new_path), Err(ref error) if error.kind() == io::ErrorKind::NotFound)
            }
            PreparedAction::Delete => true,
        };
        if !original_restored
            || !staging_absent
            || !final_absent
            || db::remove_pending_file_operation(conn, operation_id, item.asset.id).is_err()
        {
            failures.insert(item.asset.id);
            item.rollback_path = Some(PathBuf::from(&item.asset.path));
        }
    }
}

pub fn recover_pending_file_operations(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let scan_roots = db::list_scan_roots(conn)?;
    for operation in db::list_pending_file_operations(conn)? {
        let original = Path::new(&operation.original_path);
        let staging = Path::new(&operation.staging_path);
        let final_path = operation.final_path.as_deref().map(Path::new);
        let db_committed = operation.committed;

        if db_committed {
            if operation.action == "delete" {
                // Cleanup-only work: the delete is committed and the asset row is gone, so no
                // failure here may stop startup. The row is kept and retried on a later start.
                if let Err(error) = finish_committed_delete(original, staging, &scan_roots) {
                    eprintln!(
                        "Kept pending delete cleanup {} for asset {} for a later start: {error:#}",
                        operation.operation_id, operation.asset_id
                    );
                    continue;
                }
            }
        } else {
            let current = if fs::symlink_metadata(staging).is_ok() {
                Some(staging)
            } else {
                final_path.filter(|path| fs::symlink_metadata(path).is_ok())
            };
            if let Some(current) = current {
                if fs::symlink_metadata(original).is_ok() {
                    anyhow::bail!(
                        "pending operation {} found both recovery source and occupied original path",
                        operation.operation_id
                    );
                }
                rename_no_replace(current, original).with_context(|| {
                    format!("cannot restore pending source '{}'", original.display())
                })?;
            }
        }

        let reconciled = if db_committed {
            if operation.action == "rename" {
                final_path.is_some_and(|path| {
                    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
                })
            } else {
                staged_file_verifiably_absent(staging)
            }
        } else {
            let original_ok =
                fs::symlink_metadata(original).is_ok_and(|metadata| metadata.file_type().is_file());
            let staging_absent = matches!(fs::symlink_metadata(staging), Err(ref error) if error.kind() == io::ErrorKind::NotFound);
            let final_absent = match final_path {
                None => true,
                Some(path) => {
                    matches!(fs::symlink_metadata(path), Err(ref error) if error.kind() == io::ErrorKind::NotFound)
                }
            };
            original_ok && staging_absent && final_absent
        };
        if !reconciled {
            anyhow::bail!(
                "pending file operation {} for asset {} requires manual recovery",
                operation.operation_id,
                operation.asset_id
            );
        }
        db::remove_pending_file_operation(conn, &operation.operation_id, operation.asset_id)?;
    }
    Ok(())
}

/// Finishes a committed delete at startup by removing its staging file.
///
/// A staging file that still exists is removed only when it canonicalizes below a current scan
/// root that contains the journaled original path. `Ok` means the file is verifiably gone and
/// the journal row can be dropped; every error means "keep the row and retry later".
fn finish_committed_delete(
    original: &Path,
    staging: &Path,
    scan_roots: &[String],
) -> anyhow::Result<()> {
    match fs::symlink_metadata(staging) {
        Ok(_) => validate_recovery_staging_path(original, staging, scan_roots)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot inspect staging file '{}'", staging.display()))
        }
    }
    remove_staged_delete(staging).with_context(|| {
        format!(
            "cannot confirm removal of staging file '{}'",
            staging.display()
        )
    })
}

/// Removes a committed delete's staging file and reports success only when the file is
/// verifiably gone: removed now or already absent, inside a parent directory that can be
/// reached and synced. An unreachable parent (renamed folder, unmounted volume whose folder is
/// gone) is an error, because the file may reappear with the deleted content.
fn remove_staged_delete(staging: &Path) -> io::Result<()> {
    remove_file_and_sync_parent(staging)?;
    if staged_file_verifiably_absent(staging) {
        Ok(())
    } else {
        Err(io::Error::other("the staging file is not verifiably gone"))
    }
}

/// `true` only when `lstat` reports the staging file as missing while its parent directory is
/// reachable. A missing parent also yields `NotFound` for the file, which proves nothing.
///
/// Known limit (ADR 0013): the journal records no volume identity, so a mount-point folder left
/// behind as an empty directory by an unmounted volume passes as a reachable parent.
fn staged_file_verifiably_absent(staging: &Path) -> bool {
    matches!(fs::symlink_metadata(staging), Err(ref error) if error.kind() == io::ErrorKind::NotFound)
        && staging
            .parent()
            .is_some_and(|parent| fs::metadata(parent).is_ok_and(|metadata| metadata.is_dir()))
}

fn validate_recovery_staging_path(
    original: &Path,
    staging: &Path,
    scan_roots: &[String],
) -> anyhow::Result<()> {
    let canonical_staging = staging
        .canonicalize()
        .with_context(|| format!("cannot canonicalize recovery path '{}'", staging.display()))?;
    let confined = scan_roots.iter().any(|root| {
        let root = Path::new(root);
        original.starts_with(root)
            && root
                .canonicalize()
                .is_ok_and(|canonical_root| canonical_staging.starts_with(canonical_root))
    });
    if !confined {
        anyhow::bail!(
            "pending delete recovery path '{}' escapes the source's scan roots",
            staging.display()
        );
    }
    Ok(())
}

pub fn validate_file_name(raw: &str) -> anyhow::Result<String> {
    let value = raw.trim();
    if value.is_empty() {
        anyhow::bail!("File name cannot be empty");
    }
    if value == "." || value == ".." {
        anyhow::bail!("File name is invalid");
    }
    if value.contains(['/', '\\']) {
        anyhow::bail!("File name cannot contain directory separators");
    }
    if value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|character| character <= '\u{1f}' || ":*?\"<>|".contains(character))
    {
        anyhow::bail!("File name contains invalid characters");
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
    {
        anyhow::bail!("file name is reserved on Windows");
    }
    Ok(value.to_string())
}

fn unique_staging_path(source: &Path) -> PathBuf {
    let token: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();
    source
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".mediatagger-{token}.pending"))
}

fn platform_rename_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains NUL"))?;
    let target = CString::new(target.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "target path contains NUL"))?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[derive(Debug)]
struct MoveError {
    moved: bool,
    error: io::Error,
}
impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for MoveError {}

fn rename_no_replace(source: &Path, target: &Path) -> Result<(), MoveError> {
    platform_rename_no_replace(source, target).map_err(|error| MoveError {
        moved: false,
        error,
    })?;
    sync_parent_directory(source).map_err(|error| MoveError { moved: true, error })?;
    if source.parent() != target.parent() {
        sync_parent_directory(target).map_err(|error| MoveError { moved: true, error })?;
    }
    Ok(())
}

fn remove_file_and_sync_parent(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent_directory(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => sync_parent_directory(path),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> io::Result<()> {
    #[cfg(test)]
    if SYNC_FAILURE_AFTER.with(|remaining| {
        let count = remaining.get();
        if count > 0 {
            remaining.set(count - 1);
        }
        count == 1
    }) {
        return Err(io::Error::other("injected directory sync failure"));
    }
    fs::File::open(
        path.parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?,
    )?
    .sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
thread_local! { static SYNC_FAILURE_AFTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use std::sync::atomic::Ordering;

    use super::{
        apply_duplicate_resolution_batch, delete_asset, recover_pending_file_operations,
        rename_asset, validate_recovery_staging_path,
    };
    use crate::{
        db::{
            self, test_support::commit_revision_bump_before_every_statement, PendingFileOperation,
        },
        models::{
            DeleteSourceStatus, DuplicateResolutionBatchInput, DuplicateResolutionBatchStatus,
            DuplicateResolutionChangeInput, DuplicateResolutionItemStatus, NewAsset,
            RenameAssetStatus,
        },
    };

    /// A disposable library: a scan root and a WAL database file below a temp dir. The file lets
    /// a test open a second connection as a concurrent writer.
    struct JournalFixture {
        _tmp: tempfile::TempDir,
        root: std::path::PathBuf,
        db_path: std::path::PathBuf,
        conn: rusqlite::Connection,
    }

    fn journal_fixture() -> JournalFixture {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("library");
        fs::create_dir_all(&root).expect("create root");
        let db_path = tmp.path().join("media.db");
        let conn = db::open_connection(&db_path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        db::add_scan_root(&conn, &root.to_string_lossy()).expect("add scan root");
        JournalFixture {
            _tmp: tmp,
            root,
            db_path,
            conn,
        }
    }

    fn remove_concurrent_writer(conn: &mut rusqlite::Connection) {
        conn.authorizer(
            None::<fn(rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization>,
        )
        .expect("remove concurrent writer");
    }

    fn thumbs_dir(fixture: &JournalFixture) -> std::path::PathBuf {
        fixture.root.parent().expect("temp dir").join("thumbs")
    }

    fn index_asset(fixture: &JournalFixture, path: &std::path::Path) -> i64 {
        db::upsert_scanned_asset(
            &fixture.conn,
            &NewAsset {
                path: path.to_string_lossy().into_owned(),
                kind: "image".to_string(),
                size_bytes: 6,
                modified_at: 1,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            },
            1,
            &fixture.root.to_string_lossy(),
            1,
        )
        .expect("index asset");
        fixture
            .conn
            .query_row(
                "SELECT id FROM assets WHERE path = ?1",
                [path.to_string_lossy()],
                |row| row.get(0),
            )
            .expect("asset id")
    }

    /// Writes one journal row the way an interrupted mutation leaves it. `asset_id` only has to
    /// be unique per operation; a committed delete has no asset row any more.
    fn journal_row(
        conn: &rusqlite::Connection,
        asset_id: i64,
        action: &str,
        original: &std::path::Path,
        staging: &std::path::Path,
        final_path: Option<&std::path::Path>,
        committed: bool,
    ) {
        db::insert_pending_file_operations(
            conn,
            &[PendingFileOperation {
                operation_id: format!("operation-{asset_id}"),
                asset_id,
                action: action.to_string(),
                original_path: original.to_string_lossy().into_owned(),
                staging_path: staging.to_string_lossy().into_owned(),
                final_path: final_path.map(|path| path.to_string_lossy().into_owned()),
                committed,
            }],
        )
        .expect("insert journal row");
    }

    fn journal_len(conn: &rusqlite::Connection) -> usize {
        db::list_pending_file_operations(conn)
            .expect("list journal")
            .len()
    }

    fn asset_path(conn: &rusqlite::Connection, asset_id: i64) -> Option<String> {
        db::get_file_mutation_asset(conn, asset_id)
            .expect("asset query")
            .map(|asset| asset.path)
    }

    #[test]
    fn recovery_restores_an_uncommitted_delete_from_staging() {
        let fixture = journal_fixture();
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join(".mediatagger-delete.pending");
        fs::write(&original, b"source").expect("write source");
        let asset_id = index_asset(&fixture, &original);
        fs::rename(&original, &staging).expect("stage source");
        journal_row(
            &fixture.conn,
            asset_id,
            "delete",
            &original,
            &staging,
            None,
            false,
        );

        recover_pending_file_operations(&fixture.conn).expect("recovery");

        assert_eq!(fs::read(&original).expect("restored source"), b"source");
        assert!(!staging.exists());
        assert_eq!(
            asset_path(&fixture.conn, asset_id).as_deref(),
            Some(original.to_string_lossy().as_ref())
        );
        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[test]
    fn recovery_restores_an_uncommitted_rename_from_staging_or_final_path() {
        for moved_to_final in [false, true] {
            let fixture = journal_fixture();
            let original = fixture.root.join("photo.jpg");
            let staging = fixture.root.join(".mediatagger-rename.pending");
            let target = fixture.root.join("renamed.jpg");
            fs::write(&original, b"source").expect("write source");
            let asset_id = index_asset(&fixture, &original);
            let interrupted_at = if moved_to_final { &target } else { &staging };
            fs::rename(&original, interrupted_at).expect("move source");
            journal_row(
                &fixture.conn,
                asset_id,
                "rename",
                &original,
                &staging,
                Some(&target),
                false,
            );

            recover_pending_file_operations(&fixture.conn).expect("recovery");

            assert_eq!(fs::read(&original).expect("restored source"), b"source");
            assert!(!staging.exists(), "moved_to_final={moved_to_final}");
            assert!(!target.exists(), "moved_to_final={moved_to_final}");
            assert_eq!(
                asset_path(&fixture.conn, asset_id).as_deref(),
                Some(original.to_string_lossy().as_ref())
            );
            assert_eq!(journal_len(&fixture.conn), 0);
        }
    }

    #[test]
    fn recovery_drops_an_uncommitted_row_whose_source_never_moved() {
        let fixture = journal_fixture();
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join(".mediatagger-rename.pending");
        let target = fixture.root.join("renamed.jpg");
        fs::write(&original, b"source").expect("write source");
        let asset_id = index_asset(&fixture, &original);
        journal_row(
            &fixture.conn,
            asset_id,
            "rename",
            &original,
            &staging,
            Some(&target),
            false,
        );

        recover_pending_file_operations(&fixture.conn).expect("recovery");

        assert_eq!(fs::read(&original).expect("source"), b"source");
        assert!(!target.exists());
        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[test]
    fn recovery_stops_when_an_uncommitted_source_and_its_original_path_both_exist() {
        let fixture = journal_fixture();
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join(".mediatagger-delete.pending");
        fs::write(&original, b"newer file").expect("write occupant");
        fs::write(&staging, b"source").expect("write staged source");
        journal_row(&fixture.conn, 1, "delete", &original, &staging, None, false);

        let error = recover_pending_file_operations(&fixture.conn)
            .expect_err("an occupied original path must stop recovery");

        assert!(error
            .to_string()
            .contains("found both recovery source and occupied original path"));
        assert_eq!(fs::read(&original).expect("occupant"), b"newer file");
        assert_eq!(fs::read(&staging).expect("staged source"), b"source");
        assert_eq!(journal_len(&fixture.conn), 1);
    }

    #[test]
    fn recovery_stops_when_an_uncommitted_source_cannot_be_found() {
        let fixture = journal_fixture();
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join(".mediatagger-delete.pending");
        journal_row(&fixture.conn, 1, "delete", &original, &staging, None, false);

        let error = recover_pending_file_operations(&fixture.conn)
            .expect_err("a lost source must stop recovery");

        assert!(error.to_string().contains("requires manual recovery"));
        assert_eq!(journal_len(&fixture.conn), 1);
    }

    #[test]
    fn recovery_accepts_a_committed_rename_only_with_its_final_file() {
        let fixture = journal_fixture();
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join(".mediatagger-rename.pending");
        let target = fixture.root.join("renamed.jpg");
        journal_row(
            &fixture.conn,
            1,
            "rename",
            &original,
            &staging,
            Some(&target),
            true,
        );

        let error = recover_pending_file_operations(&fixture.conn)
            .expect_err("a committed rename without its final file must stop recovery");
        assert!(error.to_string().contains("requires manual recovery"));
        assert_eq!(journal_len(&fixture.conn), 1);

        fs::write(&target, b"source").expect("write final file");
        recover_pending_file_operations(&fixture.conn).expect("recovery");

        assert_eq!(fs::read(&target).expect("final file"), b"source");
        assert!(!original.exists());
        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[test]
    fn recovery_finishes_a_committed_delete_inside_its_scan_root() {
        let fixture = journal_fixture();
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join(".mediatagger-delete.pending");
        let neighbour = fixture.root.join("keep.jpg");
        fs::write(&staging, b"source").expect("write staged source");
        fs::write(&neighbour, b"keep").expect("write neighbour");
        journal_row(&fixture.conn, 1, "delete", &original, &staging, None, true);

        recover_pending_file_operations(&fixture.conn).expect("recovery");

        assert!(!staging.exists());
        assert!(!original.exists());
        assert_eq!(fs::read(&neighbour).expect("neighbour"), b"keep");
        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[test]
    fn recovery_drops_a_committed_delete_whose_staging_file_is_already_gone() {
        let fixture = journal_fixture();
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join(".mediatagger-delete.pending");
        journal_row(&fixture.conn, 1, "delete", &original, &staging, None, true);

        recover_pending_file_operations(&fixture.conn).expect("recovery");

        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[test]
    fn recovery_keeps_a_committed_delete_without_a_covering_scan_root_and_still_starts() {
        let fixture = journal_fixture();
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join(".mediatagger-delete.pending");
        fs::write(&staging, b"source").expect("write staged source");
        journal_row(&fixture.conn, 1, "delete", &original, &staging, None, true);
        let root = fixture.root.to_string_lossy().into_owned();
        db::remove_scan_root(&fixture.conn, &root).expect("remove scan root");

        recover_pending_file_operations(&fixture.conn)
            .expect("a cleanup-only failure must not stop startup");

        assert_eq!(fs::read(&staging).expect("staged source"), b"source");
        assert_eq!(journal_len(&fixture.conn), 1);

        db::add_scan_root(&fixture.conn, &root).expect("add scan root again");
        recover_pending_file_operations(&fixture.conn).expect("recovery with the root back");

        assert!(!staging.exists());
        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[cfg(unix)]
    #[test]
    fn recovery_keeps_a_committed_delete_that_escapes_through_a_symlink() {
        use std::os::unix::fs::symlink;

        let fixture = journal_fixture();
        let outside = fixture.root.parent().expect("temp dir").join("outside");
        fs::create_dir_all(&outside).expect("create outside");
        fs::write(outside.join("staged.jpg"), b"outside").expect("write outside file");
        symlink(&outside, fixture.root.join("escape")).expect("symlink");
        let original = fixture.root.join("photo.jpg");
        let staging = fixture.root.join("escape/staged.jpg");
        journal_row(&fixture.conn, 1, "delete", &original, &staging, None, true);

        recover_pending_file_operations(&fixture.conn)
            .expect("a confinement failure must not stop startup");

        assert_eq!(
            fs::read(outside.join("staged.jpg")).expect("outside file"),
            b"outside"
        );
        assert_eq!(journal_len(&fixture.conn), 1);
    }

    #[test]
    fn recovery_keeps_a_committed_delete_while_its_staging_folder_is_unreachable() {
        let fixture = journal_fixture();
        let folder = fixture.root.join("album");
        let hidden = fixture.root.parent().expect("temp dir").join("unmounted");
        fs::create_dir_all(&folder).expect("create folder");
        let original = folder.join("photo.jpg");
        let staging = folder.join(".mediatagger-delete.pending");
        fs::write(&staging, b"source").expect("write staged source");
        journal_row(&fixture.conn, 1, "delete", &original, &staging, None, true);
        fs::rename(&folder, &hidden).expect("make the folder unreachable");

        recover_pending_file_operations(&fixture.conn)
            .expect("an unreachable folder must not stop startup");

        assert_eq!(
            fs::read(hidden.join(".mediatagger-delete.pending")).expect("staged source"),
            b"source"
        );
        assert_eq!(journal_len(&fixture.conn), 1);

        fs::rename(&hidden, &folder).expect("make the folder reachable again");
        recover_pending_file_operations(&fixture.conn).expect("recovery with the folder back");

        assert!(!staging.exists());
        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[test]
    fn recovery_continues_past_a_kept_committed_delete() {
        let fixture = journal_fixture();
        let outside = fixture.root.parent().expect("temp dir").join("outside");
        fs::create_dir_all(&outside).expect("create outside");
        let kept_staging = outside.join(".mediatagger-kept.pending");
        fs::write(&kept_staging, b"outside").expect("write unconfined staged file");
        journal_row(
            &fixture.conn,
            1,
            "delete",
            &outside.join("photo.jpg"),
            &kept_staging,
            None,
            true,
        );
        let cleaned_staging = fixture.root.join(".mediatagger-cleaned.pending");
        fs::write(&cleaned_staging, b"source").expect("write staged source");
        journal_row(
            &fixture.conn,
            2,
            "delete",
            &fixture.root.join("second.jpg"),
            &cleaned_staging,
            None,
            true,
        );
        let original = fixture.root.join("third.jpg");
        let restore_staging = fixture.root.join(".mediatagger-restore.pending");
        fs::write(&restore_staging, b"third").expect("write uncommitted staged source");
        journal_row(
            &fixture.conn,
            3,
            "delete",
            &original,
            &restore_staging,
            None,
            false,
        );

        recover_pending_file_operations(&fixture.conn).expect("recovery");

        assert_eq!(fs::read(&kept_staging).expect("kept file"), b"outside");
        assert!(!cleaned_staging.exists());
        assert_eq!(fs::read(&original).expect("restored source"), b"third");
        let remaining = db::list_pending_file_operations(&fixture.conn).expect("list journal");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].asset_id, 1);
    }

    #[test]
    fn staged_delete_is_not_cleaned_while_its_folder_is_unreachable() {
        let tmp = tempdir().expect("tempdir");
        let folder = tmp.path().join("album");
        fs::create_dir_all(&folder).expect("create folder");
        let staging = folder.join(".mediatagger-delete.pending");
        fs::write(&staging, b"source").expect("write staged source");

        fs::rename(&folder, tmp.path().join("unmounted")).expect("make the folder unreachable");
        assert!(super::remove_staged_delete(&staging).is_err());

        fs::rename(tmp.path().join("unmounted"), &folder).expect("make the folder reachable");
        assert!(staging.exists());
        super::remove_staged_delete(&staging).expect("cleanup");
        assert!(!staging.exists());
        super::remove_staged_delete(&staging).expect("cleanup of an already removed file");
    }

    #[test]
    fn delete_cleanup_sync_failure_keeps_the_journal_until_startup_recovery() {
        let fixture = journal_fixture();
        let source = fixture.root.join("photo.jpg");
        fs::write(&source, b"source").expect("write source");
        let asset_id = index_asset(&fixture, &source);
        let thumbs = fixture.root.parent().expect("temp dir").join("thumbs");

        // Sync 1 follows staging; sync 2 follows the removal of the staged file after commit.
        super::SYNC_FAILURE_AFTER.with(|remaining| remaining.set(2));
        let summary = delete_asset(&fixture.conn, &thumbs, asset_id).expect("delete commits");

        assert!(matches!(
            summary.source_status,
            DeleteSourceStatus::CleanupPending
        ));
        assert!(summary.recovery_path.is_some());
        assert_eq!(asset_path(&fixture.conn, asset_id), None);
        let pending = db::list_pending_file_operations(&fixture.conn).expect("list journal");
        assert_eq!(pending.len(), 1);
        assert!(pending[0].committed);
        assert_eq!(pending[0].action, "delete");

        recover_pending_file_operations(&fixture.conn).expect("recovery");

        assert!(!source.exists());
        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[cfg(unix)]
    #[test]
    fn delete_rejects_source_reached_through_symlinked_parent_escape() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("root");
        let outside = tmp.path().join("outside");
        let thumbs = tmp.path().join("thumbs");
        fs::create_dir_all(&root).expect("root");
        fs::create_dir_all(&outside).expect("outside");
        fs::create_dir_all(&thumbs).expect("thumbs");
        fs::write(outside.join("asset.jpg"), b"source").expect("source");
        symlink(&outside, root.join("escape")).expect("symlink");
        let escaped_path = root.join("escape/asset.jpg");

        let conn = rusqlite::Connection::open_in_memory().expect("db");
        db::init_schema(&conn).expect("schema");
        db::add_scan_root(&conn, &root.to_string_lossy()).expect("scan root");
        db::upsert_scanned_asset(
            &conn,
            &NewAsset {
                path: escaped_path.to_string_lossy().into_owned(),
                kind: "image".to_string(),
                size_bytes: 6,
                modified_at: 1,
                width: None,
                height: None,
                duration_ms: None,
                thumb_path: None,
            },
            1,
            &root.to_string_lossy(),
            1,
        )
        .expect("asset");

        let error = delete_asset(&conn, &thumbs, 1).expect_err("escape must be rejected");
        assert!(error
            .to_string()
            .contains("escapes its assigned scan roots"));
        assert!(outside.join("asset.jpg").exists());
        assert!(db::get_file_mutation_asset(&conn, 1)
            .expect("asset query")
            .is_some());
    }

    #[cfg(unix)]
    #[test]
    fn recovery_rejects_staging_path_reached_through_symlink_escape() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().join("root");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&root).expect("root");
        fs::create_dir_all(&outside).expect("outside");
        fs::write(outside.join("staged.jpg"), b"source").expect("staged source");
        symlink(&outside, root.join("escape")).expect("symlink");

        let error = validate_recovery_staging_path(
            &root.join("asset.jpg"),
            &root.join("escape/staged.jpg"),
            &[root.to_string_lossy().into_owned()],
        )
        .expect_err("escape must be rejected");

        assert!(error
            .to_string()
            .contains("escapes the source's scan roots"));
        assert!(outside.join("staged.jpg").exists());
    }
    #[test]
    fn staging_and_final_move_sync_failures_restore_source_and_metadata() {
        for fail_after in [1, 2] {
            let dir = tempdir().unwrap();
            let source = dir.path().join("source.jpg");
            fs::write(&source, b"source").unwrap();
            let conn = db::open_connection(&dir.path().join("media.db")).unwrap();
            conn.pragma_update(None, "synchronous", "FULL").unwrap();
            db::init_schema(&conn).unwrap();
            db::add_scan_root(&conn, dir.path().to_str().unwrap()).unwrap();
            db::upsert_scanned_asset(
                &conn,
                &NewAsset {
                    path: source.to_string_lossy().into(),
                    kind: "image".into(),
                    size_bytes: 6,
                    modified_at: 1,
                    width: None,
                    height: None,
                    duration_ms: None,
                    thumb_path: None,
                },
                1,
                dir.path().to_str().unwrap(),
                1,
            )
            .unwrap();
            super::SYNC_FAILURE_AFTER.with(|remaining| remaining.set(fail_after));
            assert!(
                super::rename_asset(&conn, &dir.path().join("thumbs"), 1, "target.jpg".into())
                    .is_err()
            );
            assert_eq!(fs::read(&source).unwrap(), b"source");
            assert!(!dir.path().join("target.jpg").exists());
            assert_eq!(
                db::get_file_mutation_asset(&conn, 1).unwrap().unwrap().path,
                source.to_string_lossy()
            );
            assert!(db::list_pending_file_operations(&conn).unwrap().is_empty());
            super::recover_pending_file_operations(&conn).unwrap();
        }
    }

    #[test]
    fn single_asset_mutations_ignore_metadata_writes_committed_between_their_statements() {
        for rename in [false, true] {
            let mut fixture = journal_fixture();
            let source = fixture.root.join("photo.jpg");
            fs::write(&source, b"source").expect("write source");
            let asset_id = index_asset(&fixture, &source);
            let thumbs = thumbs_dir(&fixture);

            // A tag or favorite save on another asset takes neither the scan nor the thumbnail
            // lock, so its revision bump can land between any two statements of the mutation.
            let bumps =
                commit_revision_bump_before_every_statement(&mut fixture.conn, &fixture.db_path);
            if rename {
                let summary = rename_asset(&fixture.conn, &thumbs, asset_id, "renamed.jpg".into())
                    .expect("a concurrent metadata write must not fail a rename");
                assert!(matches!(summary.status, RenameAssetStatus::Renamed));
            } else {
                let summary = delete_asset(&fixture.conn, &thumbs, asset_id)
                    .expect("a concurrent metadata write must not fail a delete");
                assert!(matches!(summary.source_status, DeleteSourceStatus::Deleted));
            }
            remove_concurrent_writer(&mut fixture.conn);

            assert!(
                bumps.committed.load(Ordering::SeqCst) > 1,
                "rename={rename}"
            );
            assert!(!source.exists(), "rename={rename}");
            assert_eq!(fixture.root.join("renamed.jpg").exists(), rename);
            assert_eq!(
                asset_path(&fixture.conn, asset_id),
                rename.then(|| fixture
                    .root
                    .join("renamed.jpg")
                    .to_string_lossy()
                    .into_owned())
            );
            assert_eq!(journal_len(&fixture.conn), 0, "rename={rename}");
        }
    }

    #[test]
    fn duplicate_batch_still_rejects_a_stale_snapshot_before_touching_files() {
        let fixture = journal_fixture();
        let first = fixture.root.join("a/dup.jpg");
        let second = fixture.root.join("b/DUP.jpg");
        for path in [&first, &second] {
            fs::create_dir_all(path.parent().expect("parent")).expect("create folder");
            fs::write(path, b"source").expect("write source");
        }
        let first_id = index_asset(&fixture, &first);
        index_asset(&fixture, &second);
        let snapshot = db::current_library_revision(&fixture.conn).expect("revision");
        db::bump_library_revision(&fixture.conn).expect("concurrent write");

        let error = apply_duplicate_resolution_batch(
            &fixture.conn,
            &thumbs_dir(&fixture),
            DuplicateResolutionBatchInput {
                scan_revision: snapshot,
                changes: vec![DuplicateResolutionChangeInput::Delete {
                    asset_id: first_id,
                    expected_path: first.to_string_lossy().into_owned(),
                    expected_record_version: 1,
                }],
            },
        )
        .expect_err("a stale duplicate snapshot must be rejected");

        assert!(error.to_string().contains("duplicate snapshot is stale"));
        assert_eq!(fs::read(&first).expect("untouched source"), b"source");
        assert!(asset_path(&fixture.conn, first_id).is_some());
        assert_eq!(journal_len(&fixture.conn), 0);
    }

    #[test]
    fn rolled_back_batch_reports_a_missing_source_delete_as_rolled_back_without_recovery_path() {
        let fixture = journal_fixture();
        let missing = fixture.root.join("a/dup.jpg");
        let kept = fixture.root.join("b/dup.jpg");
        for path in [&missing, &kept] {
            fs::create_dir_all(path.parent().expect("parent")).expect("create folder");
            fs::write(path, b"source").expect("write source");
        }
        let missing_id = index_asset(&fixture, &missing);
        let kept_id = index_asset(&fixture, &kept);
        fs::remove_file(&missing).expect("source disappears without a rescan");
        let revision = db::current_library_revision(&fixture.conn).expect("revision");

        // Sync 1 follows staging the rename source, so the batch fails before its commit.
        super::SYNC_FAILURE_AFTER.with(|remaining| remaining.set(1));
        let summary = apply_duplicate_resolution_batch(
            &fixture.conn,
            &thumbs_dir(&fixture),
            DuplicateResolutionBatchInput {
                scan_revision: revision,
                changes: vec![
                    DuplicateResolutionChangeInput::Delete {
                        asset_id: missing_id,
                        expected_path: missing.to_string_lossy().into_owned(),
                        expected_record_version: 1,
                    },
                    DuplicateResolutionChangeInput::Rename {
                        asset_id: kept_id,
                        expected_path: kept.to_string_lossy().into_owned(),
                        expected_record_version: 1,
                        new_file_name: "unique.jpg".into(),
                    },
                ],
            },
        )
        .expect("a pre-commit failure returns a summary");

        assert!(matches!(
            summary.status,
            DuplicateResolutionBatchStatus::RolledBack
        ));
        for item in &summary.results {
            assert!(
                matches!(item.status, DuplicateResolutionItemStatus::RolledBack),
                "asset {}",
                item.asset_id
            );
            assert_eq!(item.recovery_path, None, "asset {}", item.asset_id);
        }
        assert_eq!(summary.revision, revision);
        assert_eq!(fs::read(&kept).expect("restored source"), b"source");
        assert!(!fixture.root.join("b/unique.jpg").exists());
        assert!(!missing.exists());
        assert_eq!(
            asset_path(&fixture.conn, missing_id).as_deref(),
            Some(missing.to_string_lossy().as_ref())
        );
        assert_eq!(
            asset_path(&fixture.conn, kept_id).as_deref(),
            Some(kept.to_string_lossy().as_ref())
        );
        assert_eq!(journal_len(&fixture.conn), 0);
    }
}
