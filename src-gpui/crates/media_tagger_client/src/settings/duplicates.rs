//! Duplicate resolution (`duplicateValidationService.ts`, `createDuplicateResolver.ts`):
//! the rules a staged batch must meet before it may be saved, and the form of the
//! resolver dialog.
//!
//! A duplicate group is a set of indexed files with the same file name. The resolver
//! stages at most one change per asset, a rename or a delete, and a group counts as
//! resolved once the names its files would end up with are valid and differ, ignoring
//! case. A batch may be saved when every staged change is well formed, names a known
//! asset, and every group a change touches is resolved; untouched groups are left
//! alone. The backend repeats and extends these checks before it changes a file
//! (`docs/subsystems/data-safety-and-portability.md`).

use std::collections::{BTreeMap, HashMap, HashSet};

use media_tagger_core::models::{
    DuplicateAsset, DuplicateGroup, DuplicateResolutionBatchInput, DuplicateResolutionBatchStatus,
    DuplicateResolutionBatchSummary, DuplicateResolutionChangeInput, DuplicateResolutionItemStatus,
    DuplicateScanSummary,
};

use crate::i18n::{I18n, Params};

const FILE_NAME_EMPTY: &str = "validation.fileNameCannotBeEmpty";
const FILE_NAME_INVALID: &str = "validation.fileNameInvalid";
const FILE_NAME_INVALID_CHARACTERS: &str = "validation.fileNameInvalidCharacters";
const PAYLOAD_INVALID: &str = "validation.duplicatePayloadInvalid";
const UNKNOWN_ASSET: &str = "validation.unknownAssetInChanges";
const GROUP_UNRESOLVED: &str = "validation.duplicateGroupStillUnresolved";

/// The device names Windows reserves, with or without an extension
/// (`WINDOWS_RESERVED_FILE_NAME`).
const RESERVED_STEMS: [&str; 4] = ["con", "prn", "aux", "nul"];
const RESERVED_NUMBERED: [&str; 2] = ["com", "lpt"];

/// What the resolver stages for one asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateAction {
    /// Rename the file to this name, inside its folder.
    Rename { next_file_name: String },
    /// Delete the file from disk and from the library.
    Delete,
}

/// One staged change (`DuplicateResolutionChange`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateChange {
    pub asset_id: i64,
    pub action: DuplicateAction,
}

impl DuplicateChange {
    pub fn rename(asset_id: i64, next_file_name: impl Into<String>) -> Self {
        Self {
            asset_id,
            action: DuplicateAction::Rename {
                next_file_name: next_file_name.into(),
            },
        }
    }

    pub fn delete(asset_id: i64) -> Self {
        Self {
            asset_id,
            action: DuplicateAction::Delete,
        }
    }

    pub fn is_delete(&self) -> bool {
        self.action == DuplicateAction::Delete
    }
}

/// Why a batch may not be saved: a locale key with its parameters, translated when it
/// is shown.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub key: &'static str,
    pub params: Params,
}

impl ValidationError {
    fn key(key: &'static str) -> Self {
        Self {
            key,
            params: Params::new(),
        }
    }

    fn unknown_asset(asset_id: i64) -> Self {
        Self {
            key: UNKNOWN_ASSET,
            params: Params::new().with("assetId", asset_id),
        }
    }

    /// The error in the language of `i18n`.
    pub fn resolve(&self, i18n: &I18n<'_>) -> String {
        i18n.t_with(self.key, &self.params)
    }
}

/// `DuplicateChangesValidationResult`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Validation {
    pub errors: Vec<ValidationError>,
}

impl Validation {
    pub fn valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn first_error(&self) -> Option<&ValidationError> {
        self.errors.first()
    }
}

/// `String.prototype.trim`: Unicode white space and the byte order mark.
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// `getFileNameFromPath`: what follows the last slash or backslash.
pub fn file_name_from_path(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn is_reserved_device_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower.split('.').next().unwrap_or_default();
    RESERVED_STEMS.contains(&stem)
        || RESERVED_NUMBERED.iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|digit| matches!(digit.as_bytes(), [b'1'..=b'9']))
        })
}

/// `duplicateFileNameSchema`: the trimmed name, or the locale key of the first rule
/// it breaks. A name must not be empty, `.` or `..`, must not hold a control
/// character or one of ` / : * ? " < > |`, must not end with a dot or a space, and
/// must not be a device name Windows reserves.
pub fn validate_file_name(name: &str) -> Result<&str, &'static str> {
    let name = js_trim(name);
    if name.is_empty() {
        return Err(FILE_NAME_EMPTY);
    }
    if name == "." || name == ".." {
        return Err(FILE_NAME_INVALID);
    }
    let invalid_character = name.chars().any(|c| {
        c <= '\u{1f}' || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    });
    if invalid_character
        || name.ends_with('.')
        || name.ends_with(' ')
        || is_reserved_device_name(name)
    {
        return Err(FILE_NAME_INVALID_CHARACTERS);
    }
    Ok(name)
}

/// `normalizeFileNameForCompare`.
fn compare_key(name: &str) -> String {
    js_trim(name).to_lowercase()
}

/// `isDuplicateGroupResolved`: a change touches the group, and the names its files
/// keep or get are valid and differ, ignoring case. Deleted files are left out.
pub fn is_group_resolved(group: &DuplicateGroup, staged: &BTreeMap<i64, DuplicateAction>) -> bool {
    if !group
        .assets
        .iter()
        .any(|asset| staged.contains_key(&asset.id))
    {
        return false;
    }
    let mut seen = HashSet::new();
    for asset in &group.assets {
        let final_name = match staged.get(&asset.id) {
            Some(DuplicateAction::Delete) => continue,
            Some(DuplicateAction::Rename { next_file_name }) => next_file_name.as_str(),
            None => file_name_from_path(&asset.path),
        };
        if validate_file_name(final_name).is_err() || !seen.insert(compare_key(final_name)) {
            return false;
        }
    }
    true
}

/// `validateDuplicateResolutionChanges`. A change that is not well formed (an ID
/// that is not positive, a rename to an unusable name) is the one error reported;
/// otherwise every change of an unknown asset and every touched group that is still
/// unresolved are.
pub fn validate_changes(groups: &[DuplicateGroup], staged: &[DuplicateChange]) -> Validation {
    // Nothing staged touches no group and names no asset.
    if staged.is_empty() {
        return Validation::default();
    }
    for change in staged {
        if change.asset_id <= 0 {
            return Validation {
                errors: vec![ValidationError::key(PAYLOAD_INVALID)],
            };
        }
        if let DuplicateAction::Rename { next_file_name } = &change.action {
            if let Err(key) = validate_file_name(next_file_name) {
                return Validation {
                    errors: vec![ValidationError::key(key)],
                };
            }
        }
    }
    let known: HashSet<i64> = groups
        .iter()
        .flat_map(|group| group.assets.iter().map(|asset| asset.id))
        .collect();
    let mut errors: Vec<ValidationError> = staged
        .iter()
        .filter(|change| !known.contains(&change.asset_id))
        .map(|change| ValidationError::unknown_asset(change.asset_id))
        .collect();
    // A later change of the same asset replaces an earlier one, and the names are
    // compared trimmed, as the parsed payload holds them.
    let by_asset: BTreeMap<i64, DuplicateAction> = staged
        .iter()
        .map(|change| {
            let action = match &change.action {
                DuplicateAction::Rename { next_file_name } => DuplicateAction::Rename {
                    next_file_name: js_trim(next_file_name).to_owned(),
                },
                DuplicateAction::Delete => DuplicateAction::Delete,
            };
            (change.asset_id, action)
        })
        .collect();
    for group in groups {
        let touched = group
            .assets
            .iter()
            .any(|asset| by_asset.contains_key(&asset.id));
        if touched && !is_group_resolved(group, &by_asset) {
            errors.push(ValidationError {
                key: GROUP_UNRESOLVED,
                params: Params::new().with("fileName", &group.file_name),
            });
        }
    }
    Validation { errors }
}

/// The request of one batch (`runSaveDuplicateChanges`): every change with the path
/// and record version the scan saw, under the scan's revision. A rename to an empty
/// name and a change of an asset the scan does not know are refused here, whatever the
/// dialog checked before.
pub fn build_batch(
    scan_revision: i64,
    groups: &[DuplicateGroup],
    changes: &[DuplicateChange],
) -> Result<DuplicateResolutionBatchInput, ValidationError> {
    let assets: HashMap<i64, &DuplicateAsset> = groups
        .iter()
        .flat_map(|group| group.assets.iter().map(|asset| (asset.id, asset)))
        .collect();
    let changes = changes
        .iter()
        .map(|change| {
            let new_file_name = match &change.action {
                DuplicateAction::Rename { next_file_name } => {
                    let name = js_trim(next_file_name);
                    if name.is_empty() {
                        return Err(ValidationError::key(FILE_NAME_EMPTY));
                    }
                    Some(name.to_owned())
                }
                DuplicateAction::Delete => None,
            };
            let asset = assets
                .get(&change.asset_id)
                .ok_or_else(|| ValidationError::unknown_asset(change.asset_id))?;
            Ok(match new_file_name {
                Some(new_file_name) => DuplicateResolutionChangeInput::Rename {
                    asset_id: asset.id,
                    expected_path: asset.path.clone(),
                    expected_record_version: asset.record_version,
                    new_file_name,
                },
                None => DuplicateResolutionChangeInput::Delete {
                    asset_id: asset.id,
                    expected_path: asset.path.clone(),
                    expected_record_version: asset.record_version,
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DuplicateResolutionBatchInput {
        scan_revision,
        changes,
    })
}

/// How many files the groups hold (`duplicateAssetCount`).
pub fn duplicate_asset_count(groups: &[DuplicateGroup]) -> usize {
    groups.iter().map(|group| group.assets.len()).sum()
}

/// A row of the resolver's list: a group, or a run of the files of a group that has
/// more files than one row holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolverRow {
    /// The index of the group in the scan.
    pub group: usize,
    /// The files of the group the row shows: `first..end`.
    pub first: usize,
    pub end: usize,
    /// Whether the row is the last one of its group.
    pub last: bool,
}

impl ResolverRow {
    /// Whether the row is the first one of its group, the one with the group's name.
    pub fn is_first(&self) -> bool {
        self.first == 0
    }
}

/// The rows of the resolver's list: every group in the order of the scan, at most
/// `per_row` files in a row. A list that draws only the rows near its visible part then
/// builds a bounded number of cards whatever the size of a group; a library can hold
/// one file name thousands of times.
pub fn resolver_rows(groups: &[DuplicateGroup], per_row: usize) -> Vec<ResolverRow> {
    let per_row = per_row.max(1);
    let mut rows = Vec::with_capacity(groups.len());
    for (group, files) in groups.iter().map(|group| group.assets.len()).enumerate() {
        let mut first = 0;
        loop {
            let end = files.min(first + per_row);
            let last = end == files;
            rows.push(ResolverRow {
                group,
                first,
                end,
                last,
            });
            if last {
                break;
            }
            first = end;
        }
    }
    rows
}

/// The form of the resolver dialog (`createDuplicateResolver`): the rename drafts and
/// the staged changes, one per asset.
///
/// The form belongs to the dialog, not to the application: it starts over whenever the
/// dialog opens or a scan replaces the groups ([`Self::reset`]), and it goes with the
/// settings page.
#[derive(Debug, Clone, Default)]
pub struct DuplicateResolver {
    names: HashMap<i64, String>,
    staged: BTreeMap<i64, DuplicateAction>,
    /// See [`Self::revision`].
    revision: u64,
}

/// Two forms are equal when they hold the same drafts and the same staged changes,
/// however they got there.
impl PartialEq for DuplicateResolver {
    fn eq(&self, other: &Self) -> bool {
        self.names == other.names && self.staged == other.staged
    }
}

impl DuplicateResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// `resetForm`: every draft is the file's current name and nothing is staged.
    pub fn reset(&mut self, groups: &[DuplicateGroup]) {
        self.names = groups
            .iter()
            .flat_map(|group| &group.assets)
            .map(|asset| (asset.id, file_name_from_path(&asset.path).to_owned()))
            .collect();
        self.staged.clear();
        self.revision += 1;
    }

    /// Advances whenever what is staged may have changed, and with every reset. What
    /// was worked out from the staged changes ([`Self::validation`],
    /// [`Self::is_group_resolved`]) holds for as long as this and the groups stay the
    /// same, so a view works it out once per revision and not once per frame (the
    /// `createMemo` of `createDuplicateResolver`).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The rename draft of an asset: what was typed, else its current name.
    pub fn rename_value<'a>(&'a self, asset: &'a DuplicateAsset) -> &'a str {
        self.names
            .get(&asset.id)
            .map_or_else(|| file_name_from_path(&asset.path), String::as_str)
    }

    /// The rename draft the form holds for an asset; `None` before a reset knew it.
    pub fn draft(&self, asset_id: i64) -> Option<&str> {
        self.names.get(&asset_id).map(String::as_str)
    }

    /// The staged change of an asset.
    pub fn staged(&self, asset_id: i64) -> Option<&DuplicateAction> {
        self.staged.get(&asset_id)
    }

    /// Every staged change by ascending asset ID (the order of `Object.values` over
    /// the IDs).
    pub fn staged_changes(&self) -> Vec<DuplicateChange> {
        self.staged
            .iter()
            .map(|(asset_id, action)| DuplicateChange {
                asset_id: *asset_id,
                action: action.clone(),
            })
            .collect()
    }

    pub fn staged_count(&self) -> usize {
        self.staged.len()
    }

    pub fn has_changes(&self) -> bool {
        !self.staged.is_empty()
    }

    pub fn validation(&self, groups: &[DuplicateGroup]) -> Validation {
        validate_changes(groups, &self.staged_changes())
    }

    /// Whether the group shows its "Ready" badge.
    pub fn is_group_resolved(&self, group: &DuplicateGroup) -> bool {
        is_group_resolved(group, &self.staged)
    }

    /// "Save all" is enabled: no operation runs, something is staged and it is valid.
    pub fn can_save(&self, groups: &[DuplicateGroup], locked: bool) -> bool {
        !locked && self.has_changes() && self.validation(groups).valid()
    }

    /// Typing into the rename field: the draft, and the staged rename when there is
    /// one, trimmed.
    pub fn rename(&mut self, asset_id: i64, value: &str) {
        self.names.insert(asset_id, value.to_owned());
        if let Some(DuplicateAction::Rename { next_file_name }) = self.staged.get_mut(&asset_id) {
            js_trim(value).clone_into(next_file_name);
            self.revision += 1;
        }
    }

    /// "Generate UUID": the draft becomes `uuid` with the extension of the file's
    /// current name. Nothing is staged and a staged rename keeps its name. Returns the
    /// new draft.
    pub fn generate_name(&mut self, asset_id: i64, current_name: &str, uuid: &str) -> String {
        let extension = match current_name.rfind('.') {
            Some(dot) if dot > 0 => &current_name[dot..],
            _ => "",
        };
        let name = format!("{uuid}{extension}");
        self.names.insert(asset_id, name.clone());
        name
    }

    /// "Queue rename": stages the trimmed draft, unless it is empty.
    pub fn queue_rename(&mut self, asset_id: i64) {
        let Some(name) = self
            .names
            .get(&asset_id)
            .map(|name| js_trim(name))
            .filter(|name| !name.is_empty())
        else {
            return;
        };
        let action = DuplicateAction::Rename {
            next_file_name: name.to_owned(),
        };
        self.staged.insert(asset_id, action);
        self.revision += 1;
    }

    /// "Queue delete" and "Unmark delete": a staged delete is taken back, anything else
    /// is replaced by a delete.
    pub fn toggle_delete(&mut self, asset_id: i64) {
        if self.staged.get(&asset_id) == Some(&DuplicateAction::Delete) {
            self.staged.remove(&asset_id);
        } else {
            self.staged.insert(asset_id, DuplicateAction::Delete);
        }
        self.revision += 1;
    }

    /// "Clear action".
    pub fn clear(&mut self, asset_id: i64) {
        self.staged.remove(&asset_id);
        self.revision += 1;
    }
}

/// The result of a duplicate scan (`DuplicateScanSummary`): the groups, their counts,
/// and the library revision a batch built from it must name.
#[derive(Debug, Clone, Default)]
pub struct DuplicateScan {
    pub groups: Vec<DuplicateGroup>,
    pub duplicate_groups: usize,
    pub duplicate_assets: usize,
    pub revision: i64,
}

impl From<DuplicateScanSummary> for DuplicateScan {
    fn from(summary: DuplicateScanSummary) -> Self {
        Self {
            groups: summary.groups,
            duplicate_groups: summary.duplicate_groups,
            duplicate_assets: summary.duplicate_assets,
            revision: summary.revision,
        }
    }
}

/// How a batch ended (`DuplicateResolutionBatchStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliedStatus {
    Committed,
    /// Every file change was undone and no metadata changed.
    RolledBack,
    /// The batch could not be undone completely.
    RecoveryRequired,
}

/// What the interface reads of a batch result (`DuplicateResolutionBatchSummary`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedDuplicates {
    pub status: AppliedStatus,
    /// The assets with a reported recovery path, as `(asset ID, path)`.
    pub recovery: Vec<(i64, String)>,
    /// How many files were already missing; their stale records were removed.
    pub source_missing: usize,
}

impl From<&DuplicateResolutionBatchSummary> for AppliedDuplicates {
    fn from(summary: &DuplicateResolutionBatchSummary) -> Self {
        Self {
            status: match summary.status {
                DuplicateResolutionBatchStatus::Committed => AppliedStatus::Committed,
                DuplicateResolutionBatchStatus::RolledBack => AppliedStatus::RolledBack,
                DuplicateResolutionBatchStatus::RecoveryRequired => AppliedStatus::RecoveryRequired,
            },
            recovery: summary
                .results
                .iter()
                .filter_map(|item| {
                    item.recovery_path
                        .as_ref()
                        .filter(|path| !path.is_empty())
                        .map(|path| (item.asset_id, path.clone()))
                })
                .collect(),
            source_missing: summary
                .results
                .iter()
                .filter(|item| matches!(item.status, DuplicateResolutionItemStatus::SourceMissing))
                .count(),
        }
    }
}

/// The batch the delete confirmation asks about (`PendingDuplicateDeleteConfirm`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDuplicateDelete {
    pub changes: Vec<DuplicateChange>,
    pub delete_count: usize,
}

/// A version 4 UUID from 16 random bytes, in the form of `crypto.randomUUID`.
pub fn uuid_v4_from(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A random version 4 UUID for "Generate UUID". The bytes come from the standard
/// library's per-process hash keys, which the operating system seeds, so two calls
/// never give the same name; a file name needs no more than that.
pub fn random_uuid() -> String {
    use std::{
        collections::hash_map::RandomState,
        hash::{BuildHasher, Hasher},
        time::{SystemTime, UNIX_EPOCH},
    };
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut bytes = [0; 16];
    for (index, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(nanos);
        hasher.write_usize(index);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    uuid_v4_from(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    fn asset(id: i64, path: &str) -> DuplicateAsset {
        DuplicateAsset {
            id,
            path: path.to_owned(),
            record_version: id + 10,
            size_bytes: 1,
            thumb_version: format!("{id:016x}"),
        }
    }

    fn group(file_name: &str, assets: Vec<DuplicateAsset>) -> DuplicateGroup {
        DuplicateGroup {
            file_name: file_name.to_owned(),
            assets,
        }
    }

    fn pair() -> Vec<DuplicateGroup> {
        vec![group(
            "same.jpg",
            vec![asset(1, "/a/same.jpg"), asset(2, "/b/same.jpg")],
        )]
    }

    #[test]
    fn a_file_name_breaks_the_first_rule_that_applies() {
        assert_eq!(validate_file_name("  new.jpg "), Ok("new.jpg"));
        assert_eq!(validate_file_name("   "), Err(FILE_NAME_EMPTY));
        for dots in [".", ".."] {
            assert_eq!(validate_file_name(dots), Err(FILE_NAME_INVALID));
        }
        for name in [
            "a/b.jpg", "a\\b.jpg", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "a\u{1}b",
            "name.", "CON", "nul.txt", "Com1.jpg", "lpt9",
        ] {
            assert_eq!(
                validate_file_name(name),
                Err(FILE_NAME_INVALID_CHARACTERS),
                "{name:?}"
            );
        }
        for name in ["console.jpg", "com0.jpg", "com10.jpg", "lpt.jpg", ".hidden"] {
            assert!(validate_file_name(name).is_ok(), "{name:?}");
        }
    }

    #[test]
    fn a_touched_group_must_end_with_names_that_differ_ignoring_case() {
        let groups = pair();
        assert!(validate_changes(&groups, &[]).valid());
        assert!(validate_changes(&groups, &[DuplicateChange::rename(1, "other.jpg")]).valid());
        assert!(validate_changes(&groups, &[DuplicateChange::delete(2)]).valid());

        let same = validate_changes(&groups, &[DuplicateChange::rename(1, "SAME.JPG")]);
        assert_eq!(
            same.first_error()
                .map(|error| error.resolve(&I18n::new(Language::En))),
            Some(
                "Group \"same.jpg\": queued changes still leave duplicate file names. Rename or \
                 delete more items in this block."
                    .to_owned()
            )
        );
    }

    #[test]
    fn an_unknown_asset_and_a_malformed_change_are_refused() {
        let groups = pair();
        let unknown = validate_changes(&groups, &[DuplicateChange::delete(9)]);
        assert_eq!(
            unknown.errors,
            [ValidationError::unknown_asset(9)],
            "an untouched group is not reported"
        );
        let malformed = validate_changes(
            &groups,
            &[DuplicateChange::delete(9), DuplicateChange::rename(1, " ")],
        );
        assert_eq!(malformed.errors, [ValidationError::key(FILE_NAME_EMPTY)]);
        assert_eq!(
            validate_changes(&groups, &[DuplicateChange::delete(0)]).errors,
            [ValidationError::key(PAYLOAD_INVALID)]
        );
    }

    #[test]
    fn the_form_stages_one_change_per_asset() {
        let groups = pair();
        let mut form = DuplicateResolver::new();
        form.reset(&groups);
        assert_eq!(form.rename_value(&groups[0].assets[0]), "same.jpg");
        assert!(!form.can_save(&groups, false));

        form.rename(1, "  renamed.jpg ");
        assert!(!form.has_changes(), "typing stages nothing");
        form.queue_rename(1);
        assert_eq!(
            form.staged_changes(),
            [DuplicateChange::rename(1, "renamed.jpg")]
        );
        form.rename(1, " again.jpg");
        assert_eq!(form.rename_value(&groups[0].assets[0]), " again.jpg");
        assert_eq!(
            form.staged_changes(),
            [DuplicateChange::rename(1, "again.jpg")]
        );
        assert!(form.is_group_resolved(&groups[0]));
        assert!(form.can_save(&groups, false) && !form.can_save(&groups, true));

        form.toggle_delete(1);
        assert_eq!(form.staged_changes(), [DuplicateChange::delete(1)]);
        form.toggle_delete(1);
        assert!(!form.has_changes());
        form.toggle_delete(2);
        form.clear(2);
        assert!(!form.has_changes());

        form.rename(2, "   ");
        form.queue_rename(2);
        assert!(!form.has_changes(), "an empty draft is not staged");

        form.reset(&groups);
        assert_eq!(form.rename_value(&groups[0].assets[0]), "same.jpg");
    }

    #[test]
    fn the_revision_advances_with_what_is_staged_and_not_with_a_draft() {
        let groups = pair();
        let mut form = DuplicateResolver::new();
        form.reset(&groups);
        let mut seen = form.revision();
        let mut advanced = |form: &DuplicateResolver| {
            let advanced = form.revision() != seen;
            seen = form.revision();
            advanced
        };

        form.rename(1, "draft.jpg");
        form.generate_name(2, "same.jpg", "u");
        assert!(!advanced(&form), "a draft stages nothing");
        form.queue_rename(1);
        assert!(advanced(&form));
        form.rename(1, "other.jpg");
        assert!(advanced(&form), "the staged rename follows the draft");
        form.toggle_delete(2);
        assert!(advanced(&form));
        form.toggle_delete(2);
        assert!(advanced(&form));
        form.toggle_delete(2);
        form.clear(2);
        assert!(advanced(&form));
        form.reset(&groups);
        assert!(advanced(&form));

        // Equal forms are forms with the same drafts and staged changes.
        let mut other = DuplicateResolver::new();
        other.reset(&groups);
        assert_ne!(other.revision(), form.revision());
        assert_eq!(other, form);
    }

    #[test]
    fn a_large_group_is_several_rows_and_a_small_one_is_one() {
        let many: Vec<DuplicateAsset> = (1..=20)
            .map(|id| asset(id, &format!("/media/{id}/cover.jpg")))
            .collect();
        let groups = vec![
            group("cover.jpg", many),
            group(
                "same.jpg",
                vec![asset(21, "/a/same.jpg"), asset(22, "/b/same.jpg")],
            ),
            group("gone.jpg", Vec::new()),
        ];
        let row = |group, first, end, last| ResolverRow {
            group,
            first,
            end,
            last,
        };
        let rows = resolver_rows(&groups, 8);
        assert_eq!(
            rows,
            [
                row(0, 0, 8, false),
                row(0, 8, 16, false),
                row(0, 16, 20, true),
                row(1, 0, 2, true),
                row(2, 0, 0, true),
            ]
        );
        assert!(rows[0].is_first() && !rows[1].is_first() && rows[3].is_first());
        // A group that fills its row exactly has no empty row after it.
        assert_eq!(resolver_rows(&groups[1..2], 2), [row(0, 0, 2, true)]);
        // Every file is in exactly one row.
        for (index, group) in groups.iter().enumerate() {
            let shown: usize = rows
                .iter()
                .filter(|row| row.group == index)
                .map(|row| row.end - row.first)
                .sum();
            assert_eq!(shown, group.assets.len());
        }
        assert!(resolver_rows(&[], 8).is_empty());
    }

    #[test]
    fn a_generated_name_keeps_the_extension_and_stages_nothing() {
        let groups = pair();
        let mut form = DuplicateResolver::new();
        form.reset(&groups);
        assert_eq!(form.generate_name(1, "same.jpg", "u"), "u.jpg");
        assert_eq!(form.generate_name(2, ".hidden", "u"), "u");
        assert_eq!(form.generate_name(2, "plain", "u"), "u");
        assert!(!form.has_changes());
        assert_eq!(form.rename_value(&groups[0].assets[0]), "u.jpg");
    }

    #[test]
    fn a_batch_carries_what_the_scan_saw() {
        let groups = pair();
        let batch = build_batch(
            7,
            &groups,
            &[
                DuplicateChange::rename(1, " renamed.jpg "),
                DuplicateChange::delete(2),
            ],
        )
        .expect("a batch");
        assert_eq!(batch.scan_revision, 7);
        assert_eq!(
            batch.changes,
            [
                DuplicateResolutionChangeInput::Rename {
                    asset_id: 1,
                    expected_path: "/a/same.jpg".into(),
                    expected_record_version: 11,
                    new_file_name: "renamed.jpg".into(),
                },
                DuplicateResolutionChangeInput::Delete {
                    asset_id: 2,
                    expected_path: "/b/same.jpg".into(),
                    expected_record_version: 12,
                },
            ]
        );
        assert_eq!(
            build_batch(7, &groups, &[DuplicateChange::rename(1, "  ")]),
            Err(ValidationError::key(FILE_NAME_EMPTY))
        );
        assert_eq!(
            build_batch(7, &groups, &[DuplicateChange::delete(5)]),
            Err(ValidationError::unknown_asset(5))
        );
    }

    #[test]
    fn a_uuid_has_the_form_of_version_four() {
        assert_eq!(
            uuid_v4_from([0xff; 16]),
            "ffffffff-ffff-4fff-bfff-ffffffffffff"
        );
        assert_eq!(
            uuid_v4_from([0; 16]),
            "00000000-0000-4000-8000-000000000000"
        );
        let (a, b) = (random_uuid(), random_uuid());
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
    }

    #[test]
    fn file_names_come_from_unix_and_windows_paths() {
        assert_eq!(file_name_from_path("/media/a/photo.jpg"), "photo.jpg");
        assert_eq!(file_name_from_path("C:\\media\\photo.jpg"), "photo.jpg");
        assert_eq!(file_name_from_path("photo.jpg"), "photo.jpg");
    }
}
