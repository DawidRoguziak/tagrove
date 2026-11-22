//! The dock's own state (`BulkDock.tsx` and `GroupOrderModal.tsx`): what the Solid
//! components keep in signals and lose when the gallery view goes.
//!
//! [`Dock`] holds the active tab, the tag draft with its validation message and the
//! focus that returns to the field after a save, the inline order draft, and the
//! larger order view with its own draft. It reads the bulk model through a
//! [`BulkView`] and never changes it: commands for the model, the selection and the
//! view come back as [`DockEffect`]s. The view calls [`Dock::sync`] whenever the
//! runtime published something, with the model's answers since the last call.
//!
//! The inline board and the larger view are never shown together: while the larger
//! view is open ([`Dock::order_modal_open`]), the inline board's place stays empty.

use std::{
    collections::HashMap,
    hash::{BuildHasher, Hasher},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    group_order::normalize_group_key,
    model::{BulkCommand, BulkEvent, BulkView, GroupSaveResult, TagMode},
    tag_merge::normalize_bulk_tag,
};
use crate::{
    controls::ButtonVariant,
    gallery::selection::{Interaction, SelectionInput, SelectionNotice},
};

/// Request ids are unique in the process: a dock made after a visit to the settings
/// page never takes the answer to a request of the dock before it.
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

fn next_request() -> u64 {
    NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
}

/// The two tabs of the dock, in tablist order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DockTab {
    #[default]
    Tags,
    Group,
}

impl DockTab {
    pub const ALL: [Self; 2] = [Self::Tags, Self::Group];

    pub const fn index(self) -> usize {
        match self {
            Self::Tags => 0,
            Self::Group => 1,
        }
    }

    /// The locale key of the tab's label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Tags => "bulk.dock.tabTags",
            Self::Group => "bulk.dock.tabGroup",
        }
    }

    /// `data-testid` of the tab button.
    pub const fn test_id(self) -> &'static str {
        match self {
            Self::Tags => "bulk-tab-tags",
            Self::Group => "bulk-tab-group",
        }
    }
}

/// The keys that move through the tablist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKey {
    Left,
    Right,
    Home,
    End,
}

/// What an order draft belongs to: the selected IDs and the normalized group key
/// (`orderContext`). Another selection or key starts the inline draft again and
/// closes the larger view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderContext {
    pub ids: Arc<[i64]>,
    pub key: String,
}

/// What the dock asks for.
#[derive(Debug, PartialEq)]
pub enum DockEffect {
    /// A command for the bulk model.
    Bulk(BulkCommand),
    /// A command for the selection (the counter clears it).
    Selection(SelectionInput),
    /// Give the tag field the focus without scrolling, one frame later, unless the
    /// selection changed or the field is disabled by then.
    FocusTagInput,
    /// Give the tab button the focus (arrow keys in the tablist).
    FocusTab(DockTab),
}

/// The larger order view (`GroupOrderModal`).
#[derive(Debug, Default)]
struct OrderModal {
    order: Vec<i64>,
    saving: bool,
    result: Option<GroupSaveResult>,
    /// The save whose answer this opening of the view still waits for.
    save_request: Option<u64>,
}

#[derive(Debug, Default)]
pub struct Dock {
    tab: DockTab,
    /// Whether the selection was empty at the last sync: the tab returns to Tags when
    /// that becomes true, not for as long as it is.
    selection_empty: Option<bool>,
    tag_draft: String,
    tag_invalid: bool,
    /// The selection whose tag field gets the focus once it is enabled again.
    pending_tag_focus: Option<u64>,
    last_selection: Option<u64>,
    /// The tag adds that are not answered yet, with the selection they were made for.
    tag_requests: HashMap<u64, u64>,
    inline_order: Vec<i64>,
    /// The context and the model's order version the inline draft started from.
    inline_for: Option<(OrderContext, u64)>,
    /// The context the larger view was opened for.
    modal_context: Option<OrderContext>,
    modal_open: bool,
    modal: OrderModal,
}

impl Dock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tab(&self) -> DockTab {
        self.tab
    }

    /// The text of the tag field as the dock decided it: emptied by a saved tag and
    /// by a new selection, replaced by the normalized tag while it is saved.
    pub fn tag_draft(&self) -> &str {
        &self.tag_draft
    }

    /// The draft is no valid tag: the message under the header row is shown.
    pub fn tag_invalid(&self) -> bool {
        self.tag_invalid
    }

    /// The order of the inline board.
    pub fn inline_order(&self) -> &[i64] {
        &self.inline_order
    }

    /// The order of the larger view.
    pub fn modal_order(&self) -> &[i64] {
        &self.modal.order
    }

    /// The larger view sent its save and waits for the answer.
    pub fn modal_saving(&self) -> bool {
        self.modal.saving
    }

    /// How the last save of the larger view ended, while it stays open.
    pub fn modal_result(&self) -> Option<GroupSaveResult> {
        self.modal.result
    }

    /// `expanded`: something is selected, or a notice says why nothing is any more.
    pub fn expanded(view: &BulkView<'_>) -> bool {
        view.selected_count() > 0 || view.selection_notice().is_some()
    }

    /// The dock has the fixed tall height of the group tab.
    pub fn tall(&self, view: &BulkView<'_>) -> bool {
        Self::expanded(view) && self.tab == DockTab::Group
    }

    /// The chips of the tag field: the single asset's tags, or what was added to
    /// this multi-selection.
    pub fn displayed_tags<'a>(view: &BulkView<'a>) -> &'a [String] {
        match view.tag_mode() {
            TagMode::Single => view.single_asset_tags(),
            _ => view.applied_bulk_tags(),
        }
    }

    pub fn controls_disabled(view: &BulkView<'_>) -> bool {
        view.selected_count() == 0 || view.selection_busy()
    }

    pub fn tag_controls_disabled(view: &BulkView<'_>) -> bool {
        Self::controls_disabled(view)
            || view.tag_details_loading()
            || view.tag_details_failed()
            || view.tag_applying()
    }

    /// The favorite heart takes no click.
    pub fn favorite_disabled(view: &BulkView<'_>) -> bool {
        Self::controls_disabled(view) || view.favorite_applying()
    }

    /// The group key field and the UUID button take no input.
    pub fn group_controls_disabled(view: &BulkView<'_>) -> bool {
        Self::controls_disabled(view) || view.group_applying()
    }

    /// Apply group takes no click.
    pub fn apply_group_disabled(view: &BulkView<'_>) -> bool {
        Self::group_controls_disabled(view) || view.metadata_loading() || view.metadata_failed()
    }

    pub fn normalized_group_key<'a>(view: &BulkView<'a>) -> &'a str {
        normalize_group_key(view.group_key_draft())
    }

    pub fn order_context(view: &BulkView<'_>) -> OrderContext {
        OrderContext {
            ids: view.selected_asset_ids(),
            key: Self::normalized_group_key(view).to_owned(),
        }
    }

    /// The order board is offered: a group key is set and at least two of the
    /// ordered assets have their record.
    pub fn has_order_panel(view: &BulkView<'_>) -> bool {
        if Self::normalized_group_key(view).is_empty() {
            return false;
        }
        let summaries = view.bulk.summaries();
        view.ordered_asset_ids()
            .iter()
            .filter(|id| summaries.record(**id).is_some())
            .nth(1)
            .is_some()
    }

    /// The larger view is open, and the inline board is not drawn.
    pub fn order_modal_open(&self, view: &BulkView<'_>) -> bool {
        Self::has_order_panel(view)
            && self
                .modal_context
                .as_ref()
                .is_some_and(|context| *context == Self::order_context(view))
    }

    /// The boards take no reordering: a range read or a group write is on its way.
    pub fn order_locked(view: &BulkView<'_>) -> bool {
        view.selection_busy() || view.group_applying()
    }

    /// "Sort in larger view" takes no click.
    pub fn open_order_modal_disabled(view: &BulkView<'_>) -> bool {
        Self::order_locked(view) || view.metadata_loading() || view.metadata_failed()
    }

    /// The larger view takes no reordering, no Cancel and no dismissal.
    pub fn modal_locked(&self, view: &BulkView<'_>) -> bool {
        self.modal.saving || view.group_applying()
    }

    /// "Save order" takes no click.
    pub fn modal_save_disabled(&self, view: &BulkView<'_>) -> bool {
        self.modal_locked(view)
            || view.selection_busy()
            || view.metadata_loading()
            || view.metadata_failed()
    }

    /// The locale key of the Apply button's label.
    pub fn apply_group_label_key(view: &BulkView<'_>) -> &'static str {
        if view.group_applying() {
            "bulk.groupModal.applyInProgress"
        } else if !Self::normalized_group_key(view).is_empty() {
            "bulk.groupModal.applyGroup"
        } else if view.bulk.summaries().loaded_count() == 1 {
            "bulk.panel.removeGroup"
        } else {
            "bulk.panel.removeGroups"
        }
    }

    /// Primary with a key, danger for the removal an empty key means.
    pub fn apply_group_variant(view: &BulkView<'_>) -> ButtonVariant {
        if Self::normalized_group_key(view).is_empty() {
            ButtonVariant::Danger
        } else {
            ButtonVariant::Primary
        }
    }

    /// The locale key of the heart's name and tooltip.
    pub fn favorite_label_key(view: &BulkView<'_>) -> &'static str {
        if view.all_selected_favorites() {
            "bulk.favorite.remove"
        } else {
            "bulk.favorite.toggle"
        }
    }

    /// The locale key of the description on the tags tab.
    pub fn tags_description_key(view: &BulkView<'_>) -> &'static str {
        if view.tag_mode() == TagMode::Multiple {
            "bulk.panel.tagsMultipleDescription"
        } else {
            "bulk.panel.tagsSingleDescription"
        }
    }

    /// The locale key of the text that says why the selection was cleared.
    pub fn notice_description_key(notice: SelectionNotice) -> &'static str {
        match notice {
            SelectionNotice::RangeExpired => "bulk.panel.selectionExpiredDescription",
            SelectionNotice::RangeCleared => "bulk.panel.selectionClearedDescription",
        }
    }

    /// Brings the dock in line with the model: applies the model's answers, starts
    /// the drafts again for a new selection or order context, opens and closes the
    /// larger view, and gives the tag field its focus back when it is due.
    pub fn sync(&mut self, view: &BulkView<'_>, events: &[BulkEvent]) -> Vec<DockEffect> {
        let mut effects = Vec::new();
        let key = view.selection_key();
        for event in events {
            match event {
                BulkEvent::TagSettled { request, saved } => {
                    let Some(selection) = self.tag_requests.remove(request) else {
                        continue;
                    };
                    // An obsolete selection neither empties the new draft nor takes
                    // the focus.
                    if selection != key {
                        continue;
                    }
                    if *saved {
                        self.tag_draft.clear();
                    }
                    self.pending_tag_focus = Some(selection);
                }
                BulkEvent::GroupSettled { request, result } => {
                    if self.modal.save_request != Some(*request) {
                        continue;
                    }
                    self.modal.save_request = None;
                    self.modal.saving = false;
                    self.modal.result = Some(*result);
                    if *result == GroupSaveResult::Saved {
                        self.modal_context = None;
                    }
                }
            }
        }
        if self.last_selection != Some(key) {
            self.last_selection = Some(key);
            self.tag_draft.clear();
            self.tag_invalid = false;
            self.pending_tag_focus = None;
        }
        // The tab returns to Tags when the selection becomes empty (the effect on
        // `selectedCount === 0` runs when that value changes). While it stays empty and
        // a notice keeps the dock open, the tab the user picks stays.
        let empty = view.selected_count() == 0;
        if self.selection_empty != Some(empty) {
            self.selection_empty = Some(empty);
            if empty {
                self.tab = DockTab::Tags;
            }
        }
        let context = Self::order_context(view);
        if self
            .modal_context
            .as_ref()
            .is_some_and(|open| *open != context)
        {
            self.modal_context = None;
        }
        // The inline draft restarts from the model's order for each selection, key
        // and save.
        let version = view.order_version();
        if !self
            .inline_for
            .as_ref()
            .is_some_and(|(for_context, for_version)| {
                *for_context == context && *for_version == version
            })
        {
            self.inline_order = view.ordered_asset_ids().to_vec();
            self.inline_for = Some((context, version));
        }
        let open = self.order_modal_open(view);
        if open != self.modal_open {
            self.modal_open = open;
            // The answer to a save of the view as it was open before is not its own.
            self.modal.save_request = None;
            self.modal.saving = false;
            if open {
                self.modal.order = self.inline_order.clone();
                self.modal.result = None;
            }
        }
        if self.pending_tag_focus == Some(key) && !Self::tag_controls_disabled(view) {
            self.pending_tag_focus = None;
            effects.push(DockEffect::FocusTagInput);
        }
        effects
    }

    /// The tag field's text changed by typing.
    pub fn set_tag_draft(&mut self, value: String) {
        self.tag_draft = value;
        self.tag_invalid = false;
    }

    /// Enter in the tag field or a picked suggestion (`addTagAndRestoreFocus`). An
    /// invalid draft stays for correction instead of being cleared silently.
    pub fn add_tag(&mut self, tag: &str, view: &BulkView<'_>) -> Vec<DockEffect> {
        let attempted = normalize_bulk_tag(tag);
        if attempted.is_empty() {
            self.tag_invalid = !tag.trim().is_empty();
            return Vec::new();
        }
        self.tag_invalid = false;
        self.tag_draft = attempted.clone();
        let request = next_request();
        self.tag_requests.insert(request, view.selection_key());
        vec![DockEffect::Bulk(BulkCommand::AddTag {
            request,
            tag: attempted,
        })]
    }

    /// A chip's remove button; offered for a single asset only.
    pub fn remove_tag(&mut self, tag: &str) -> Vec<DockEffect> {
        vec![DockEffect::Bulk(BulkCommand::RemoveTag {
            request: next_request(),
            tag: tag.to_owned(),
        })]
    }

    /// The counter: clears the selection and keeps selection mode on.
    pub fn clear_selection(&self) -> Vec<DockEffect> {
        vec![DockEffect::Selection(SelectionInput::Interact(
            Interaction::Clear,
        ))]
    }

    /// A click on a tab.
    pub fn select_tab(&mut self, tab: DockTab) {
        self.tab = tab;
    }

    /// An arrow, Home or End key on the tab `from`: the neighbour (wrapping), the
    /// first or the last tab is selected and gets the focus.
    pub fn tab_key(&mut self, from: DockTab, key: TabKey) -> Vec<DockEffect> {
        let count = DockTab::ALL.len();
        let next = match key {
            TabKey::Right => DockTab::ALL[(from.index() + 1) % count],
            TabKey::Left => DockTab::ALL[(from.index() + count - 1) % count],
            TabKey::Home => DockTab::ALL[0],
            TabKey::End => DockTab::ALL[count - 1],
        };
        self.tab = next;
        vec![DockEffect::FocusTab(next)]
    }

    /// The group key field's text changed.
    pub fn set_group_key(&self, value: String) -> Vec<DockEffect> {
        vec![DockEffect::Bulk(BulkCommand::SetGroupKeyDraft(value))]
    }

    /// "Generate UUID": a fresh random key as the group draft.
    pub fn generate_group_key(&self) -> Vec<DockEffect> {
        self.set_group_key(uuid_v4(random_bits()))
    }

    /// Apply group: the inline order goes with it only when the user changed it; the
    /// model owns the default.
    pub fn apply_group(&mut self, view: &BulkView<'_>) -> Vec<DockEffect> {
        let order = (Self::has_order_panel(view) && self.inline_order != view.ordered_asset_ids())
            .then(|| self.inline_order.clone());
        vec![DockEffect::Bulk(BulkCommand::ApplyGroup {
            request: next_request(),
            order,
        })]
    }

    /// The inline board reported its next order.
    pub fn set_inline_order(&mut self, order: Vec<i64>) {
        self.inline_order = order;
    }

    /// "Sort in larger view": the larger view opens for the current context and
    /// starts from the inline draft.
    pub fn open_order_modal(&mut self, view: &BulkView<'_>) {
        self.modal_context = Some(Self::order_context(view));
    }

    /// Cancel, Escape or the backdrop of the larger view; refused while it saves.
    /// The draft is dropped.
    pub fn close_order_modal(&mut self, view: &BulkView<'_>) {
        if !self.modal_locked(view) {
            self.modal_context = None;
        }
    }

    /// The larger view's board reported its next order.
    pub fn set_modal_order(&mut self, order: Vec<i64>) {
        self.modal.order = order;
    }

    /// "Save order": the group is applied with exactly the larger view's order. A
    /// second click while the save runs does nothing. The view closes when
    /// everything was saved and keeps its draft otherwise.
    pub fn save_order_modal(&mut self, view: &BulkView<'_>) -> Vec<DockEffect> {
        if self.modal.save_request.is_some() || self.modal_locked(view) {
            return Vec::new();
        }
        let request = next_request();
        self.modal.save_request = Some(request);
        self.modal.saving = true;
        self.modal.result = None;
        vec![DockEffect::Bulk(BulkCommand::ApplyGroup {
            request,
            order: Some(self.modal.order.clone()),
        })]
    }
}

/// A version 4 UUID from 128 random bits, in the lowercase 8-4-4-4-12 form of
/// `crypto.randomUUID()`.
pub fn uuid_v4(random: u128) -> String {
    // Version 4 in the high nibble of byte 6, variant 10 in the top bits of byte 8.
    let value = (random & !(0xf << 76) & !(0x3 << 62)) | (0x4 << 76) | (0x2 << 62);
    let hex = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// 128 bits that differ from call to call and from process to process: two hashes
/// under the standard library's randomly seeded hasher keys, over the time and a
/// counter. Good for a group key nobody else chose; not for secrets.
pub fn random_bits() -> u128 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let half = |salt: u64| {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(nanos);
        hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        hasher.write_u64(salt);
        u128::from(hasher.finish())
    };
    (half(1) << 64) | half(2)
}

#[cfg(test)]
mod tests {
    use super::{random_bits, uuid_v4, Dock, DockEffect, DockTab, TabKey};

    #[test]
    fn a_generated_key_is_a_version_4_uuid() {
        assert_eq!(uuid_v4(0), "00000000-0000-4000-8000-000000000000");
        assert_eq!(uuid_v4(u128::MAX), "ffffffff-ffff-4fff-bfff-ffffffffffff");
        let first = uuid_v4(random_bits());
        let second = uuid_v4(random_bits());
        assert_ne!(first, second);
        for key in [first, second] {
            let parts: Vec<&str> = key.split('-').collect();
            assert_eq!(
                parts.iter().map(|part| part.len()).collect::<Vec<_>>(),
                [8, 4, 4, 4, 12]
            );
            assert!(parts[2].starts_with('4'));
            assert!(matches!(parts[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b'));
            assert!(key.bytes().all(|byte| byte == b'-'
                || byte.is_ascii_digit()
                || (b'a'..=b'f').contains(&byte)));
        }
    }

    #[test]
    fn the_tablist_keys_wrap_and_jump_to_the_ends() {
        let mut dock = Dock::new();
        assert_eq!(dock.tab(), DockTab::Tags);
        assert_eq!(
            dock.tab_key(DockTab::Tags, TabKey::Right),
            [DockEffect::FocusTab(DockTab::Group)]
        );
        assert_eq!(dock.tab(), DockTab::Group);
        dock.tab_key(DockTab::Group, TabKey::Right);
        assert_eq!(dock.tab(), DockTab::Tags);
        dock.tab_key(DockTab::Tags, TabKey::Left);
        assert_eq!(dock.tab(), DockTab::Group);
        dock.tab_key(DockTab::Group, TabKey::Home);
        assert_eq!(dock.tab(), DockTab::Tags);
        dock.tab_key(DockTab::Tags, TabKey::End);
        assert_eq!(dock.tab(), DockTab::Group);
        dock.select_tab(DockTab::Tags);
        assert_eq!(dock.tab(), DockTab::Tags);
    }
}
