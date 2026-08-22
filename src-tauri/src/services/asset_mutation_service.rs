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
    let input = DuplicateResolutionBatchInput {
        scan_revision: db::current_library_revision(conn)?,
        changes: vec![DuplicateResolutionChangeInput::Delete {
            asset_id,
            expected_path: asset.path,
            expected_record_version: asset.record_version,
        }],
    };
    let summary = apply_file_mutation_batch(conn, thumbs_root, input, false)?;
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
    let input = DuplicateResolutionBatchInput {
        scan_revision: db::current_library_revision(conn)?,
        changes: vec![DuplicateResolutionChangeInput::Rename {
            asset_id,
            expected_path: asset.path,
            expected_record_version: asset.record_version,
            new_file_name,
        }],
    };
    let summary = apply_file_mutation_batch(conn, thumbs_root, input, false)?;
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
    apply_file_mutation_batch(conn, thumbs_root, input, true)
}

fn apply_file_mutation_batch(
    conn: &rusqlite::Connection,
    thumbs_root: &Path,
    input: DuplicateResolutionBatchInput,
    enforce_duplicate_resolution: bool,
) -> anyhow::Result<crate::models::DuplicateResolutionBatchSummary> {
    let mut prepared = validate_and_prepare(conn, &input, enforce_duplicate_resolution)?;
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
        if let Err(error) = rename_no_replace(staging_path, Path::new(new_path)) {
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
                match fs::remove_file(staging_path) {
                    Ok(()) => {
                        if db::remove_pending_file_operation(conn, &operation_id, item.asset.id)
                            .is_ok()
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
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        if db::remove_pending_file_operation(conn, &operation_id, item.asset.id)
                            .is_ok()
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
                    Err(_) => {
                        recovery_required = true;
                        (
                            DuplicateResolutionItemStatus::CleanupPending,
                            Some(staging_path.to_string_lossy().into_owned()),
                        )
                    }
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
    input: &DuplicateResolutionBatchInput,
    enforce_duplicate_resolution: bool,
) -> anyhow::Result<Vec<PreparedMutation>> {
    if input.changes.is_empty() {
        anyhow::bail!("duplicate resolution batch cannot be empty");
    }
    let revision = db::current_library_revision(conn)?;
    if revision != input.scan_revision {
        anyhow::bail!("duplicate snapshot is stale; rescan before applying changes");
    }

    let mut ids = HashSet::new();
    let mut prepared = Vec::with_capacity(input.changes.len());
    let mut target_paths = HashSet::new();
    let source_paths = input
        .changes
        .iter()
        .map(|change| match change {
            DuplicateResolutionChangeInput::Rename { expected_path, .. }
            | DuplicateResolutionChangeInput::Delete { expected_path, .. } => {
                expected_path.to_lowercase()
            }
        })
        .collect::<HashSet<_>>();
    let changed_ids = input
        .changes
        .iter()
        .map(|change| match change {
            DuplicateResolutionChangeInput::Rename { asset_id, .. }
            | DuplicateResolutionChangeInput::Delete { asset_id, .. } => *asset_id,
        })
        .collect::<HashSet<_>>();
    let database_paths = db::list_asset_paths(conn)?;

    for change in &input.changes {
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
                if database_paths.iter().any(|row| {
                    !changed_ids.contains(&row.id) && row.path.eq_ignore_ascii_case(&new_path)
                }) {
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

    validate_final_duplicate_names(conn, input, enforce_duplicate_resolution)?;
    Ok(prepared)
}

fn validate_final_duplicate_names(
    conn: &rusqlite::Connection,
    input: &DuplicateResolutionBatchInput,
    enforce_duplicate_resolution: bool,
) -> anyhow::Result<()> {
    let changes = input
        .changes
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
    if enforce_duplicate_resolution {
        for group in db::list_duplicate_groups(conn)? {
            if !group
                .assets
                .iter()
                .any(|asset| changes.contains_key(&asset.id))
            {
                continue;
            }
            let mut names = HashSet::new();
            for asset in group.assets {
                let name = match changes.get(&asset.id) {
                    Some(None) => continue,
                    Some(Some(name)) => name.clone(),
                    None => Path::new(&asset.path)
                        .file_name()
                        .and_then(OsStr::to_str)
                        .unwrap_or("")
                        .to_lowercase(),
                };
                if !names.insert(name) {
                    anyhow::bail!("duplicate group remains unresolved after the batch");
                }
            }
        }
    }
    let mut renamed_key_counts = HashMap::<String, usize>::new();
    for asset in db::list_asset_paths(conn)? {
        let final_name = match changes.get(&asset.id) {
            Some(None) => continue,
            Some(Some(name)) => name.clone(),
            None => Path::new(&asset.path)
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("")
                .to_lowercase(),
        };
        if renamed_keys.contains(&final_name) {
            *renamed_key_counts.entry(final_name).or_default() += 1;
        }
    }
    if renamed_key_counts.values().any(|count| *count > 1) {
        anyhow::bail!("duplicate resolution would create another duplicate file name");
    }
    Ok(())
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
            Ok(_) if rename_no_replace(current, Path::new(&item.asset.path)).is_err() => {
                failures.insert(item.asset.id);
                item.rollback_path = Some(current.to_path_buf());
            }
            Ok(_) => {}
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
        if failures.contains(&item.asset.id) {
            continue;
        }
        let original_restored = fs::symlink_metadata(&item.asset.path)
            .is_ok_and(|metadata| metadata.file_type().is_file());
        let staging_absent = match item.staging_path.as_ref() {
            None => true,
            Some(path) => {
                matches!(fs::symlink_metadata(path), Err(ref error) if error.kind() == io::ErrorKind::NotFound)
            }
        };
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
    for operation in db::list_pending_file_operations(conn)? {
        let original = Path::new(&operation.original_path);
        let staging = Path::new(&operation.staging_path);
        let final_path = operation.final_path.as_deref().map(Path::new);
        let db_committed = operation.committed;

        if db_committed {
            if operation.action == "delete" {
                match fs::remove_file(staging) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(_) => continue,
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
                matches!(fs::symlink_metadata(staging), Err(ref error) if error.kind() == io::ErrorKind::NotFound)
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

#[cfg(target_os = "linux")]
fn rename_no_replace(source: &Path, target: &Path) -> io::Result<()> {
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

#[cfg(windows)]
fn rename_no_replace(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{MoveFileExW, MOVE_FILE_FLAGS},
    };
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVE_FILE_FLAGS(0),
        )
    }
    .map_err(|error| io::Error::new(io::ErrorKind::Other, error))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn rename_no_replace(_source: &Path, _target: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic no-clobber rename is unsupported on this platform",
    ))
}
