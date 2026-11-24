//! The duplicate section of the settings page (`DuplicatesSection.tsx`), the resolver
//! dialog with its group and asset cards (`DuplicateResolverDialog.tsx`,
//! `DuplicateGroupCard.tsx`, `DuplicateAssetCard.tsx`) and the confirmation of a batch
//! that deletes files (`DuplicateDeleteConfirmDialog.tsx`).
//!
//! The scan, its groups, the open dialog and the pending confirmation live in the
//! settings model. The page keeps the resolver's form
//! (`media_tagger_client::settings::DuplicateResolver`): the rename drafts and the
//! staged changes, which start over whenever the dialog opens or a scan replaces the
//! groups, and go with the page.
//!
//! The groups are a virtual list (GPUI's `list`): only the rows near the visible part
//! are drawn, each measured as it is drawn, so a library with thousands of duplicate
//! names asks for the previews and builds the fields of a few rows only. A row is a
//! group, or a run of at most `ROW_ASSETS` files of a larger group
//! (`settings::duplicates::resolver_rows`); the rows of one group join into one card.
//! One file name can be in a library thousands of times, and its group is still drawn
//! a few rows at a time. The previews are thumbnails of the runtime's pipeline, asked
//! for while their card is drawn.
//!
//! The page is drawn again on every frame while a preview shows its loader, so nothing
//! here walks all groups per frame: the validation of the staged changes, the count of
//! files and whether a group is resolved are worked out once per scan and per change
//! of what is staged ([`Checked`]).

use std::collections::{HashMap, HashSet};

use gpui::{
    div, img, list, prelude::*, px, relative, Animation, AnimationExt as _, AnyElement, App,
    Context, ElementId, Entity, Focusable as _, ListAlignment, ListState, ObjectFit, SharedString,
    Subscription, Window,
};
use media_tagger_client::{
    controls::{ButtonVariant, FocusRing, SpinnerSize, THUMBNAIL_LOADER},
    gallery::TileRequest,
    i18n::{format_count, I18n, Params},
    overlay::ModalSize,
    runtime::RuntimeInput,
    settings::{
        duplicates::{
            duplicate_asset_count, file_name_from_path, resolver_rows, ResolverRow, Validation,
        },
        layout::{self, resolver as measures},
        DuplicateAction, DuplicateResolver, OperationSection, SettingsCommand,
    },
    text::FieldChrome,
    theme::{ThemeName, RADII, TYPOGRAPHY},
    thumbs::ThumbnailKey,
};
use media_tagger_core::models::DuplicateAsset;

use super::{
    card, confirm_buttons, confirm_text, dialog_heading, heading, operation_row, status_block,
    SettingsView,
};
use crate::{
    thumbs::TilePicture,
    ui::{
        button::Button,
        modal::Modal,
        spinner::Spinner,
        text_input::{TextInput, TextInputEvent},
        theme::{self, box_shadows, color, focus_ring, weight, RingBorder},
    },
};

/// The duplicate card and its loader.
pub const DUPLICATES_SECTION_SELECTOR: &str = "duplicates-settings-section";
pub const DUPLICATES_LOADER_SELECTOR: &str = "duplicates-section-loader";
pub const START_DUPLICATES_ID: &str = "settings-duplicates-start";
/// The line with the result of the last scan that found groups.
pub const DUPLICATES_LAST_RESULT_SELECTOR: &str = "settings-duplicates-last-result";

/// The resolver: its loader, summary, hint line, list and "no groups" box.
pub const RESOLVER_LOADER_SELECTOR: &str = "duplicates-modal-loader";
pub const RESOLVER_SUMMARY_SELECTOR: &str = "duplicates-resolver-summary";
pub const RESOLVER_HINT_SELECTOR: &str = "duplicates-resolver-hint";
pub const RESOLVER_LIST_SELECTOR: &str = "duplicates-resolver-list";
pub const RESOLVER_EMPTY_SELECTOR: &str = "duplicates-resolver-empty";
/// Element ids of the resolver's four actions.
pub const RESOLVER_SAVE_ID: &str = "duplicates-resolver-save";
pub const RESOLVER_DISCARD_ID: &str = "duplicates-resolver-discard";
pub const RESOLVER_RESCAN_ID: &str = "duplicates-resolver-rescan";
pub const RESOLVER_CLOSE_ID: &str = "duplicates-resolver-close";
/// Element ids of the delete confirmation's buttons.
pub const DUPLICATE_DELETE_CONFIRM_ID: &str = "duplicates-delete-confirm";
pub const DUPLICATE_DELETE_CANCEL_ID: &str = "duplicates-delete-cancel";

/// Debug selector of the card of the group at `index` of the scan, and of its "Ready"
/// badge. Of a group drawn as several rows it is the first row, the one with the
/// group's name.
pub fn group_selector(index: usize) -> String {
    format!("duplicates-group-{index}")
}
pub fn group_ready_selector(index: usize) -> String {
    format!("duplicates-group-{index}-ready")
}

/// A control of an asset's card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetControl {
    /// The card itself.
    Card,
    /// The preview's box.
    Preview,
    /// The line that says what is staged.
    Pending,
    /// The rename field.
    Name,
    GenerateUuid,
    QueueRename,
    /// "Queue delete" or "Unmark delete".
    Delete,
    ClearAction,
}

/// Element id and debug selector of a control of the card of asset `asset_id`.
pub fn asset_control_id(control: AssetControl, asset_id: i64) -> SharedString {
    let name = match control {
        AssetControl::Card => "card",
        AssetControl::Preview => "preview",
        AssetControl::Pending => "pending",
        AssetControl::Name => "name",
        AssetControl::GenerateUuid => "generate-uuid",
        AssetControl::QueueRename => "queue-rename",
        AssetControl::Delete => "delete",
        AssetControl::ClearAction => "clear-action",
    };
    format!("duplicates-asset-{asset_id}-{name}").into()
}

/// Debug selector of the outline a rename field shows while it has the focus.
pub fn rename_outline_selector(asset_id: i64) -> String {
    format!("duplicates-asset-{asset_id}-name-outline")
}

/// What the page keeps of the resolver.
pub(super) struct ResolverState {
    form: DuplicateResolver,
    /// The scan generation the form was started over for; `None` while the dialog is
    /// closed, so that opening it starts the form over.
    form_for: Option<u64>,
    /// The rows of the list, made when the form starts over.
    rows: Vec<ResolverRow>,
    checked: Checked,
    list: ListState,
    /// The rename fields of the cards the last frame drew, by asset ID. A field whose
    /// row left the list is dropped with its window listeners; its draft stays in the
    /// form and the field is made again from it when the row comes back.
    inputs: HashMap<i64, (Entity<TextInput>, Subscription)>,
    /// The previews the frame being drawn shows.
    tiles: Vec<TileRequest>,
    flush_scheduled: bool,
}

impl ResolverState {
    pub(super) fn new() -> Self {
        Self {
            form: DuplicateResolver::new(),
            form_for: None,
            rows: Vec::new(),
            checked: Checked::default(),
            list: ListState::new(
                0,
                ListAlignment::Top,
                px(measures::ESTIMATED_ROW * measures::OVERSCAN_ROWS),
            ),
            inputs: HashMap::new(),
            tiles: Vec::new(),
            flush_scheduled: false,
        }
    }
}

/// What follows from the groups of a scan and the staged changes. Working it out walks
/// every group, so it is kept until either changes (the `createMemo`s of
/// `createDuplicateResolver`).
#[derive(Default)]
struct Checked {
    /// The scan generation and the revision of the form this was worked out for.
    key: Option<(u64, u64)>,
    validation: Validation,
    /// The files in all groups.
    assets: usize,
    /// Whether a group is resolved, by its index; worked out when a row of the group
    /// is first drawn.
    resolved: HashMap<usize, bool>,
}

fn count(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn small_muted(theme: ThemeName) -> gpui::Div {
    div()
        .text_size(px(measures::SMALL_SIZE))
        .line_height(px(TYPOGRAPHY.xs.line_height_px()))
        .text_color(color(theme.tokens().colors.text_muted))
}

/// The preview of an asset, covering its box, or the delayed loader while it is on
/// its way.
fn preview(asset_id: i64, picture: TilePicture, theme: ThemeName) -> AnyElement {
    let colors = &theme.tokens().colors;
    match picture {
        TilePicture::Ready(picture) => img(picture)
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        TilePicture::Loading => {
            let loader =
                ElementId::NamedInteger("duplicates-preview-loader".into(), asset_id as u64);
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(color(
                    colors
                        .base_100
                        .with_alpha_percent(measures::LOADER_VEIL_PERCENT),
                ))
                .child(Spinner::new(loader.clone(), SpinnerSize::Tile).theme(theme))
                .with_animation(
                    (loader, "veil"),
                    Animation::new(THUMBNAIL_LOADER.total()),
                    |veil, progress| veil.opacity(THUMBNAIL_LOADER.opacity_at_progress(progress)),
                )
                .into_any_element()
        }
        TilePicture::Empty => div().into_any_element(),
    }
}

impl SettingsView {
    /// The resolver's form: the rename drafts and the staged changes.
    pub fn duplicate_form(&self) -> &DuplicateResolver {
        &self.resolver.form
    }

    /// The rename field of an asset whose card the last frame drew.
    pub fn duplicate_name_input(&self, asset_id: i64) -> Option<&Entity<TextInput>> {
        self.resolver.inputs.get(&asset_id).map(|(input, _)| input)
    }

    /// The list of the resolver's groups.
    pub fn duplicate_list(&self) -> &ListState {
        &self.resolver.list
    }

    /// The counts the delete confirmation shows, as `(changes, deletes)`: those of the
    /// batch it asks about, kept while the closed dialog fades out. `(0, 0)` before the
    /// first question.
    pub fn duplicate_delete_counts(&self) -> (usize, usize) {
        *self.delete_snapshot.clone().update(false, || (0, 0))
    }

    /// Starts the form over when the dialog opened or a scan replaced the groups
    /// (`createEffect` of `createDuplicateResolver`).
    pub(super) fn sync_resolver_form(&mut self, cx: &mut Context<Self>) {
        let settings = self.runtime.read(cx).model().settings();
        if !settings.duplicate_dialog_open() {
            self.resolver.form_for = None;
            return;
        }
        let generation = settings.duplicate_scan_generation();
        if self.resolver.form_for == Some(generation) {
            return;
        }
        let groups = settings.duplicate_groups();
        self.resolver.form.reset(groups);
        self.resolver.rows = resolver_rows(groups, measures::ROW_ASSETS);
        self.resolver
            .list
            .reset_with_uniform_height(self.resolver.rows.len(), px(measures::ESTIMATED_ROW));
        self.resolver.inputs.clear();
        self.resolver.form_for = Some(generation);
    }

    /// Brings [`Checked`] up to date with the scan and the form. It costs a comparison
    /// unless a scan replaced the groups or what is staged changed.
    fn sync_checked(&mut self, cx: &App) {
        let settings = self.runtime.read(cx).model().settings();
        let key = (
            settings.duplicate_scan_generation(),
            self.resolver.form.revision(),
        );
        if self.resolver.checked.key == Some(key) {
            return;
        }
        let groups = settings.duplicate_groups();
        self.resolver.checked = Checked {
            key: Some(key),
            validation: self.resolver.form.validation(groups),
            assets: duplicate_asset_count(groups),
            resolved: HashMap::new(),
        };
    }

    /// "Discard changes": every draft back to the file's name, nothing staged.
    fn discard_duplicate_changes(&mut self, cx: &mut Context<Self>) {
        let groups = self.runtime.read(cx).model().settings().duplicate_groups();
        self.resolver.form.reset(groups);
        // The fields are made again with the names of the files.
        self.resolver.inputs.clear();
        cx.notify();
    }

    /// "Save all": the staged changes go to the model while they may be saved.
    fn save_duplicate_changes(&mut self, cx: &mut Context<Self>) {
        let settings = self.runtime.read(cx).model().settings();
        if !self
            .resolver
            .form
            .can_save(settings.duplicate_groups(), settings.is_locked())
        {
            return;
        }
        let changes = self.resolver.form.staged_changes();
        self.command(SettingsCommand::SaveDuplicateChanges(changes), cx);
    }

    /// "Generate UUID": a new draft with the extension of the file's name.
    fn generate_duplicate_name(
        &mut self,
        asset_id: i64,
        current_name: &str,
        cx: &mut Context<Self>,
    ) {
        let uuid = media_tagger_client::settings::duplicates::random_uuid();
        let name = self
            .resolver
            .form
            .generate_name(asset_id, current_name, &uuid);
        if let Some((input, _)) = self.resolver.inputs.get(&asset_id) {
            input.update(cx, |input, cx| input.set_text(&name, cx));
        }
        cx.notify();
    }

    /// Once per frame of the open resolver, after it is drawn: hands the thumbnails the
    /// previews the frame drew, and drops the rename fields of the cards it did not
    /// draw. The dialog asks for it, not its rows, so a frame that draws no card (a
    /// rescan that found nothing, rows of a scan that has been replaced) asks for no
    /// preview and keeps none pinned.
    fn schedule_preview_flush(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.resolver.flush_scheduled, true) {
            return;
        }
        let view = cx.entity().downgrade();
        window.defer(cx, move |_, cx| {
            view.update(cx, |view, cx| {
                view.resolver.flush_scheduled = false;
                let tiles = std::mem::take(&mut view.resolver.tiles);
                // Every card the frame drew is in `tiles`. The fields of the others go,
                // so their number follows the rows near the visible part and not the
                // files scrolled past. A focused field whose row left the frame has
                // lost the focus by now: the window gave it back to its root when the
                // frame ended (`on_focus_lost`), as a page does for an input that is
                // unmounted.
                let drawn: HashSet<i64> = tiles.iter().map(|tile| tile.key.asset_id).collect();
                view.resolver
                    .inputs
                    .retain(|asset_id, _| drawn.contains(asset_id));
                view.thumbnails
                    .update(cx, |thumbnails, _| thumbnails.sync_resolver(tiles));
            })
            .ok();
        });
    }

    /// The rename field of an asset, made with the form's draft when its card is drawn
    /// and has none.
    fn duplicate_input(
        &mut self,
        asset: &DuplicateAsset,
        placeholder: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        if let Some((input, _)) = self.resolver.inputs.get(&asset.id) {
            return input.clone();
        }
        let asset_id = asset.id;
        let text = self.resolver.form.rename_value(asset).to_owned();
        let input = cx.new(|cx| {
            let mut input = TextInput::themed(window, cx);
            // The card draws the field's frame ([`Self::rename_field`]).
            input.set_chrome(FieldChrome::Framed, cx);
            input.set_height(Some(measures::INPUT_HEIGHT - 2.0), cx);
            input.set_placeholder(placeholder.clone(), cx);
            input.set_label(placeholder, cx);
            input.set_text(&text, cx);
            input
        });
        let subscription = cx.subscribe(&input, move |view, input, event, cx| {
            if *event != TextInputEvent::Changed {
                return;
            }
            let text = input.read(cx).text().to_owned();
            // A draft the form already holds was set from here ("Generate UUID").
            if view.resolver.form.draft(asset_id) != Some(text.as_str()) {
                view.resolver.form.rename(asset_id, &text);
                cx.notify();
            }
        });
        self.resolver
            .inputs
            .insert(asset_id, (input.clone(), subscription));
        input
    }

    /// The frame of a rename field with the field in it: the border, the fill and the
    /// 12 px text of daisyUI's `.input.input-sm` under the card's utility classes, and
    /// its outline while the field has the focus (`layout::resolver::INPUT_*`). The
    /// frame of the shared field is the stylesheet's own `input`, which these classes
    /// replace.
    fn rename_field(
        &self,
        asset_id: i64,
        input: Entity<TextInput>,
        theme: ThemeName,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let colors = &theme.tokens().colors;
        let handle = input.focus_handle(cx);
        let disabled = input.read(cx).is_disabled();
        let focused = !disabled && handle.is_focused(window);
        let selector = asset_control_id(AssetControl::Name, asset_id);
        div()
            .id(selector.clone())
            .debug_selector(move || selector.to_string())
            .relative()
            .flex()
            .items_center()
            .h(px(measures::INPUT_HEIGHT))
            .rounded(px(RADII.control))
            .border_1()
            .border_color(color(
                colors
                    .base_content
                    .with_alpha_percent(measures::INPUT_BORDER_PERCENT),
            ))
            .bg(color(
                colors
                    .base_100
                    .with_alpha_percent(measures::INPUT_FILL_PERCENT),
            ))
            // The field brings its own padding: together they are the frame's.
            .px(px(measures::INPUT_PADDING_X
                - 1.0
                - media_tagger_client::text::field_visuals(
                    theme.tokens(),
                    FieldChrome::Framed,
                    media_tagger_client::text::FieldState::default(),
                )
                .padding_x))
            .text_size(px(measures::INPUT_FONT_SIZE))
            .line_height(px(TYPOGRAPHY.xs.line_height_px()))
            .when(disabled, |frame| {
                // `input:disabled` dims the whole element once, the frame and the
                // text; the field inside only takes the text colour of daisyUI's
                // `.input:disabled` (`FieldChrome::Framed`).
                frame.opacity(measures::INPUT_DISABLED_OPACITY)
            })
            .when(!disabled, |frame| {
                // A press on the frame's padding focuses the field; one on the field
                // focused it already.
                frame.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    if window.default_prevented() {
                        return;
                    }
                    window.focus(&handle, cx);
                    window.prevent_default();
                })
            })
            .child(input)
            .when(focused, |frame| {
                frame.child(
                    focus_ring(
                        FocusRing {
                            color: colors.base_content,
                            width: measures::INPUT_OUTLINE_WIDTH,
                            offset: measures::INPUT_OUTLINE_OFFSET,
                        },
                        RADII.control,
                        RingBorder::all(1.0),
                    )
                    .debug_selector(move || rename_outline_selector(asset_id)),
                )
            })
    }

    pub(super) fn duplicates_section(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.runtime.read(cx).model().settings();
        let locked = settings.is_locked();
        let groups = settings.duplicate_groups().len();
        let assets = settings.duplicate_asset_count();
        let language = i18n.language();
        let counted = |key: &str, value: usize| {
            i18n.t_with(
                key,
                &Params::new()
                    .with_count(count(value))
                    .with("formattedCount", format_count(language, count(value))),
            )
        };
        let last_result = (groups > 0).then(|| {
            div()
                .debug_selector(|| DUPLICATES_LAST_RESULT_SELECTOR.to_owned())
                .text_size(px(layout::data::RESULT_SIZE))
                .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                .text_color(color(theme.tokens().colors.text_muted))
                .child(
                    i18n.t_with(
                        "settings.duplicates.lastResult",
                        &Params::new()
                            .with(
                                "groups",
                                counted("settings.duplicates.lastResultGroups", groups),
                            )
                            .with(
                                "assets",
                                counted("settings.duplicates.lastResultAssets", assets),
                            ),
                    ),
                )
        });
        card(theme)
            .debug_selector(|| DUPLICATES_SECTION_SELECTOR.to_owned())
            .gap(px(layout::data::GAP))
            .child(heading(i18n.t("settings.duplicates.heading")))
            .child(
                operation_row(theme, i18n.t("settings.duplicates.description")).child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(layout::data::START_GAP))
                        .child(
                            Button::new(START_DUPLICATES_ID, i18n.t("settings.duplicates.start"))
                                .debug_selector(START_DUPLICATES_ID)
                                .variant(ButtonVariant::Primary)
                                .disabled(locked)
                                .on_click(self.send_on_click(|| {
                                    RuntimeInput::settings(SettingsCommand::StartDuplicates)
                                })),
                        )
                        .children(last_result),
                ),
            )
            .child(status_block(
                "settings-duplicates-status",
                DUPLICATES_LOADER_SELECTOR,
                settings.status(OperationSection::Duplicates),
                i18n,
                theme,
            ))
            .into_any_element()
    }

    /// The row at `index` of the resolver's list: the card of a group, or the part of
    /// it with one run of the files of a group that takes several rows. The first row
    /// of a group has its name and the top of the card, the last one the bottom, and
    /// the rows lie against each other, so a group reads as one card either way.
    fn duplicate_group_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_checked(cx);
        // Only the files of this row are copied: a group can hold thousands.
        let (shown, locked, i18n): (Option<_>, bool, I18n<'static>) = {
            let runtime = self.runtime.read(cx);
            let settings = runtime.model().settings();
            let mut shown = None;
            if let Some(row) = self.resolver.rows.get(index).copied() {
                if let Some(group) = settings.duplicate_groups().get(row.group) {
                    if let Some(assets) = group.assets.get(row.first..row.end) {
                        let resolved = match self.resolver.checked.resolved.get(&row.group) {
                            Some(resolved) => *resolved,
                            None => {
                                let resolved = self.resolver.form.is_group_resolved(group);
                                self.resolver.checked.resolved.insert(row.group, resolved);
                                resolved
                            }
                        };
                        shown = Some((row, group.file_name.clone(), assets.to_vec(), resolved));
                    }
                }
            }
            (shown, settings.is_locked(), runtime.i18n())
        };
        // A row of a scan that has been replaced since the list was laid out.
        let Some((row, file_name, shown_assets, resolved)) = shown else {
            return div().into_any_element();
        };
        let (index, first, last) = (row.group, row.is_first(), row.last);
        let theme = theme::active(cx);
        let colors = &theme.tokens().colors;
        let two_columns = f32::from(window.viewport_size().width) >= measures::TWO_COLUMNS_FROM;

        let assets: Vec<AnyElement> = shown_assets
            .iter()
            .map(|asset| {
                self.duplicate_asset_card(asset, locked, two_columns, &i18n, theme, window, cx)
            })
            .collect();

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(measures::GROUP_GAP))
            .child(
                div()
                    .min_w_0()
                    .text_size(px(measures::GROUP_NAME_SIZE))
                    .line_height(px(TYPOGRAPHY.sm.line_height_px()))
                    .child(file_name),
            )
            .when(resolved, |header| {
                header.child(
                    div()
                        .debug_selector(move || group_ready_selector(index))
                        .flex_none()
                        .rounded(px(RADII.control))
                        .border_1()
                        .border_color(color(
                            colors
                                .primary
                                .with_alpha_percent(measures::RESOLVED_BORDER_PERCENT),
                        ))
                        .bg(color(
                            colors
                                .primary
                                .with_alpha_percent(measures::BADGE_FILL_PERCENT),
                        ))
                        .px(px(measures::BADGE_PADDING_X))
                        .py(px(measures::BADGE_PADDING_Y))
                        .text_size(px(measures::BADGE_SIZE))
                        .line_height(relative(measures::BADGE_LINE_HEIGHT))
                        .font_weight(weight(TYPOGRAPHY.weight_semibold))
                        .text_color(color(colors.primary_text))
                        .child(i18n.t("settings.duplicates.readyBadge").to_uppercase()),
                )
            });
        let (border, fill) = if resolved {
            (
                colors
                    .primary
                    .with_alpha_percent(measures::RESOLVED_BORDER_PERCENT),
                colors
                    .primary
                    .with_alpha_percent(measures::RESOLVED_FILL_PERCENT),
            )
        } else {
            (
                colors
                    .base_content
                    .with_alpha_percent(measures::GROUP_BORDER_PERCENT),
                colors
                    .base_100
                    .with_alpha_percent(measures::GROUP_FILL_PERCENT),
            )
        };
        // The card's border, corners and padding, each on the rows that have that edge
        // of the card; a group in one row has all of them. Between two rows of a group
        // lies the gap that is between two files.
        div()
            .pr(px(measures::LIST_PADDING_RIGHT))
            .when(last, |row| row.pb(px(measures::ROW_GAP)))
            .child(
                div()
                    .when(first, |card| {
                        card.debug_selector(move || group_selector(index))
                    })
                    .flex()
                    .flex_col()
                    .gap(px(measures::GROUP_GAP))
                    .border_color(color(border))
                    .bg(color(fill))
                    .border_x_1()
                    .px(px(measures::GROUP_PADDING))
                    .when(first, |card| {
                        card.rounded_t(px(RADII.control))
                            .border_t_1()
                            .pt(px(measures::GROUP_PADDING))
                            .child(header)
                    })
                    .when(!first, |card| card.pt(px(measures::GROUP_GAP)))
                    .when(last, |card| {
                        card.rounded_b(px(RADII.control))
                            .border_b_1()
                            .pb(px(measures::GROUP_PADDING))
                    })
                    .children(assets),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn duplicate_asset_card(
        &mut self,
        asset: &DuplicateAsset,
        locked: bool,
        two_columns: bool,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.tokens().colors;
        let asset_id = asset.id;
        let staged = self.resolver.form.staged(asset_id).cloned();
        let deleting = staged == Some(DuplicateAction::Delete);
        let current_name = file_name_from_path(&asset.path).to_owned();

        let key = ThumbnailKey::new(asset.id, asset.thumb_version.clone());
        let picture = self.thumbnails.read(cx).picture(&key);
        // The list may draw a row twice in a frame. The frame's previews are those of
        // the rows near the visible part, at most `ROW_ASSETS` each, so this stays short.
        if !self.resolver.tiles.iter().any(|tile| tile.key == key) {
            self.resolver.tiles.push(TileRequest { key, visible: true });
        }

        let input = self.duplicate_input(
            asset,
            i18n.t("settings.duplicates.renamePlaceholder"),
            window,
            cx,
        );
        input.update(cx, |input, cx| {
            input.set_disabled(locked || deleting, window, cx)
        });
        let draft_empty = input.read(cx).text().trim().is_empty();

        let pending = match &staged {
            Some(DuplicateAction::Rename { next_file_name }) => i18n.t_with(
                "settings.duplicates.pendingRename",
                &Params::new().with("name", next_file_name),
            ),
            Some(DuplicateAction::Delete) => i18n.t("settings.duplicates.pendingDelete"),
            None => i18n.t("settings.duplicates.pendingNone"),
        };
        let id = |control| asset_control_id(control, asset_id);
        let button = |control: AssetControl, label: &str| {
            Button::new(id(control), i18n.t(label))
                .debug_selector(id(control))
                .disabled(locked)
        };
        let buttons = div()
            .flex()
            .flex_wrap()
            .gap(px(measures::BUTTONS_GAP))
            .child(
                button(
                    AssetControl::GenerateUuid,
                    "settings.duplicates.generateUuid",
                )
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.generate_duplicate_name(asset_id, &current_name, cx)
                })),
            )
            .child(
                button(AssetControl::QueueRename, "settings.duplicates.queueRename")
                    .disabled(locked || draft_empty)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.resolver.form.queue_rename(asset_id);
                        cx.notify();
                    })),
            )
            .child(
                button(
                    AssetControl::Delete,
                    if deleting {
                        "settings.duplicates.unmarkDelete"
                    } else {
                        "settings.duplicates.queueDelete"
                    },
                )
                .variant(if deleting {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Danger
                })
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.resolver.form.toggle_delete(asset_id);
                    cx.notify();
                })),
            )
            .when(staged.is_some(), |buttons| {
                buttons.child(
                    button(AssetControl::ClearAction, "settings.duplicates.clearAction").on_click(
                        cx.listener(move |view, _, _, cx| {
                            view.resolver.form.clear(asset_id);
                            cx.notify();
                        }),
                    ),
                )
            });

        let preview_box = div()
            .debug_selector(move || id(AssetControl::Preview).to_string())
            .relative()
            .flex_none()
            .size(px(measures::PREVIEW_SIZE))
            .overflow_hidden()
            .rounded(px(RADII.control))
            .border_1()
            .border_color(color(
                colors
                    .base_content
                    .with_alpha_percent(measures::ASSET_BORDER_PERCENT),
            ))
            .bg(color(
                colors
                    .base_100
                    .with_alpha_percent(measures::PREVIEW_FILL_PERCENT),
            ))
            .shadow(box_shadows(theme.tokens().shadows.tile))
            .child(preview(asset_id, picture, theme));
        let details = div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(measures::DETAILS_GAP))
            .when(two_columns, |details| details.flex_1())
            .when(!two_columns, |details| details.w_full())
            .child(
                small_muted(theme)
                    .font_weight(weight(TYPOGRAPHY.weight_semibold))
                    .child(i18n.t("settings.duplicates.resolve")),
            )
            .child(small_muted(theme).child(asset.path.clone()))
            .child(
                small_muted(theme)
                    .debug_selector(move || id(AssetControl::Pending).to_string())
                    .child(pending),
            )
            .child(self.rename_field(asset_id, input, theme, window, cx))
            .child(buttons);
        div()
            .debug_selector(move || id(AssetControl::Card).to_string())
            .flex()
            .when(two_columns, |card| card.flex_row())
            .when(!two_columns, |card| card.flex_col().items_center())
            .gap(px(measures::ASSET_GAP))
            .rounded(px(RADII.control))
            .border_1()
            .border_color(color(
                colors
                    .base_content
                    .with_alpha_percent(measures::ASSET_BORDER_PERCENT),
            ))
            .bg(color(
                colors
                    .base_200
                    .with_alpha_percent(measures::ASSET_FILL_PERCENT),
            ))
            .p(px(measures::ASSET_PADDING))
            .child(preview_box)
            .child(details)
            .into_any_element()
    }

    /// The resolver dialog.
    pub(super) fn duplicate_resolver_dialog(
        &mut self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Textures of previews the cache dropped since the last frame; the gallery,
        // which releases them otherwise, is not shown.
        self.thumbnails
            .update(cx, |thumbnails, cx| thumbnails.release_textures(window, cx));
        if !self.resolver_modal.read(cx).is_present() {
            // A closed resolver shows no preview.
            self.thumbnails
                .update(cx, |thumbnails, _| thumbnails.sync_resolver(Vec::new()));
            return div().into_any_element();
        }
        self.sync_checked(cx);
        // Before the list builds its rows, which collect this frame's previews.
        self.schedule_preview_flush(window, cx);
        let colors = &theme.tokens().colors;
        let settings = self.runtime.read(cx).model().settings();
        let locked = settings.is_locked();
        let groups = settings.duplicate_groups();
        let status = settings.status(OperationSection::Duplicates);
        let form = &self.resolver.form;
        let validation = &self.resolver.checked.validation;
        let can_save = !locked && form.has_changes() && validation.valid();
        let hint = if !form.has_changes() {
            i18n.t("settings.duplicates.queueHint")
        } else if let Some(error) = validation.first_error() {
            error.resolve(i18n)
        } else {
            i18n.t_with(
                "settings.duplicates.queuedReady",
                &Params::new().with_count(count(form.staged_count())),
            )
        };
        let summary = i18n.t_with(
            "settings.duplicates.resolverSummary",
            &Params::new()
                .with("groups", groups.len())
                .with("assets", self.resolver.checked.assets),
        );
        let empty = groups.is_empty() && !status.loading;

        let actions = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(measures::ACTIONS_GAP))
            .child(
                Button::new(RESOLVER_SAVE_ID, i18n.t("settings.duplicates.saveAll"))
                    .debug_selector(RESOLVER_SAVE_ID)
                    .disabled(!can_save)
                    .on_click(cx.listener(|view, _, _, cx| view.save_duplicate_changes(cx))),
            )
            .child(
                Button::new(
                    RESOLVER_DISCARD_ID,
                    i18n.t("settings.duplicates.discardChanges"),
                )
                .debug_selector(RESOLVER_DISCARD_ID)
                .disabled(locked)
                .on_click(cx.listener(|view, _, _, cx| view.discard_duplicate_changes(cx))),
            )
            .child(
                Button::new(RESOLVER_RESCAN_ID, i18n.t("settings.scan.rescan"))
                    .debug_selector(RESOLVER_RESCAN_ID)
                    .disabled(locked)
                    .on_click(self.send_on_click(|| {
                        RuntimeInput::settings(SettingsCommand::RescanDuplicates)
                    })),
            )
            .child(
                Button::new(RESOLVER_CLOSE_ID, i18n.t("common.close"))
                    .debug_selector(RESOLVER_CLOSE_ID)
                    .disabled(locked)
                    .on_click(self.send_on_click(|| {
                        RuntimeInput::settings(SettingsCommand::CloseDuplicateDialog)
                    })),
            );
        let header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(measures::ACTIONS_GAP))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(measures::TITLE_GAP))
                    .child(dialog_heading(
                        i18n.t("settings.duplicates.resolverHeading"),
                    ))
                    .child(
                        small_muted(theme)
                            .debug_selector(|| RESOLVER_SUMMARY_SELECTOR.to_owned())
                            .child(summary),
                    ),
            )
            .child(actions);
        let status = status_block(
            "settings-duplicates-modal-status",
            RESOLVER_LOADER_SELECTOR,
            status,
            i18n,
            theme,
        );
        let rows = list(
            self.resolver.list.clone(),
            cx.processor(|view: &mut Self, index: usize, window, cx| {
                view.duplicate_group_row(index, window, cx)
            }),
        )
        .size_full();
        let body = div()
            .id(RESOLVER_LIST_SELECTOR)
            .debug_selector(|| RESOLVER_LIST_SELECTOR.to_owned())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when(empty, |body| {
                body.child(
                    div()
                        .debug_selector(|| RESOLVER_EMPTY_SELECTOR.to_owned())
                        .flex_none()
                        .rounded(px(RADII.control))
                        .border_1()
                        .border_color(color(
                            colors
                                .base_content
                                .with_alpha_percent(measures::EMPTY_BORDER_PERCENT),
                        ))
                        .bg(color(
                            colors
                                .base_200
                                .with_alpha_percent(measures::EMPTY_FILL_PERCENT),
                        ))
                        .p(px(measures::EMPTY_PADDING))
                        .text_size(px(TYPOGRAPHY.sm.size))
                        .line_height(px(TYPOGRAPHY.sm.line_height_px()))
                        .text_color(color(colors.text_muted))
                        .child(i18n.t("settings.duplicates.noGroups")),
                )
            })
            .child(div().flex_1().min_h_0().child(rows));
        let height = measures::height(f32::from(window.viewport_size().height));
        Modal::new("settings-duplicate-resolver-dialog", &self.resolver_modal)
            .label(i18n.t("settings.duplicates.resolverHeading"))
            .size(ModalSize::Large)
            .height(height)
            .theme(theme)
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap(px(measures::GAP))
                    .child(header)
                    .child(status)
                    .child(
                        small_muted(theme)
                            .debug_selector(|| RESOLVER_HINT_SELECTOR.to_owned())
                            .child(hint),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    /// The confirmation of a batch that deletes files. It lies over the resolver.
    pub(super) fn duplicate_delete_dialog(
        &mut self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.runtime.read(cx).model().settings();
        let locked = settings.is_locked();
        let pending = settings.pending_duplicate_delete();
        let current = pending.map_or((0, 0), |pending| {
            (pending.changes.len(), pending.delete_count)
        });
        // The counts stay while the closed dialog fades out.
        let (changes, deletes) = *self.delete_snapshot.update(pending.is_some(), || current);
        let line = |key: &str, value: usize| {
            confirm_text(
                theme,
                i18n.t_with(key, &Params::new().with_count(count(value))),
            )
        };
        Modal::new(
            "settings-duplicate-delete-dialog",
            &self.duplicate_delete_modal,
        )
        .label(i18n.t("common.confirm"))
        .size(ModalSize::Small)
        .theme(theme)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(layout::confirm::GAP))
                .child(dialog_heading(i18n.t("common.confirm")))
                .child(
                    // Two paragraphs with an empty line between them.
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(layout::confirm::DESCRIPTION_SIZE
                            * layout::confirm::DESCRIPTION_LINE_HEIGHT))
                        .child(line(
                            "settings.actions.dialogs.confirmDuplicateChanges",
                            changes,
                        ))
                        .child(line(
                            "settings.actions.dialogs.confirmDuplicateDeletes",
                            deletes,
                        )),
                )
                .child(
                    confirm_buttons()
                        .child(
                            Button::new(DUPLICATE_DELETE_CANCEL_ID, i18n.t("common.cancel"))
                                .debug_selector(DUPLICATE_DELETE_CANCEL_ID)
                                .disabled(locked)
                                .on_click(self.send_on_click(|| {
                                    RuntimeInput::settings(SettingsCommand::CancelDuplicateDelete)
                                })),
                        )
                        .child(
                            Button::new(DUPLICATE_DELETE_CONFIRM_ID, i18n.t("common.confirm"))
                                .debug_selector(DUPLICATE_DELETE_CONFIRM_ID)
                                .variant(ButtonVariant::Danger)
                                .disabled(locked)
                                .on_click(self.send_on_click(|| {
                                    RuntimeInput::settings(SettingsCommand::ConfirmDuplicateDelete)
                                })),
                        ),
                ),
        )
        .into_any_element()
    }
}

/// Tells the thumbnails that no resolver shows a preview any more: the page went away.
pub(super) fn forget_previews(view: &mut SettingsView, cx: &mut App) {
    view.thumbnails
        .update(cx, |thumbnails, _| thumbnails.sync_resolver(Vec::new()));
}
