//! One opening of the lightbox (`LightboxContent` of `LightboxModal.tsx`): what the
//! mounted view decides besides drawing. It owns the machines of
//! [`super::presentation`], the delete confirmation and the image viewer, keeps the
//! state that belongs to the selected asset (the Information panel, the tag draft)
//! and answers the questions that need several of them at once: what Escape and a
//! press on the backdrop do, whether the viewer's keys are on, where the focus goes
//! when the side panel opens or closes.
//!
//! A [`Session`] is made when the lightbox opens and dropped when it has faded out;
//! closing and reopening starts a new one, as the Solid presence owner does. It has
//! no timers and reads no clock: `now` is the duration since any fixed origin, the
//! same for every call. The view owns the focus handles and the window; the session
//! only names what should get the focus ([`FocusTarget`]).

use std::time::Duration;

use super::{
    delete_confirm::{DeleteConfirm, DeleteNotice},
    image_controls::{Fullscreen, ImageControls},
    media::StageKind,
    model::{DeletedAsset, LightboxView, NavigationStatus, SelectedAsset},
    presentation::{Activity, CopyRequest, GroupClipboard, Sidebar},
};
use crate::library::{is_valid_tag, normalize_tags, MutationError};

/// Everything the lightbox draws of the model, copied while it is open: the view
/// keeps showing the last copy while the closed lightbox fades out
/// (`createModalSnapshot`).
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub selected: SelectedAsset,
    pub selected_index: Option<usize>,
    pub navigation: NavigationStatus,
    pub tags: Vec<String>,
    pub tag_saving: bool,
    pub tag_failed: bool,
    pub tag_details_loading: bool,
    pub tag_details_failed: bool,
    pub tag_editing_disabled: bool,
    pub asset_details_failed: bool,
    pub favorite_pending: bool,
    pub favorite_failed: bool,
    pub group_pending: bool,
    pub group_failed: bool,
    pub group_key: String,
    pub group_order: String,
    pub group_valid: bool,
}

impl Shown {
    /// What an open lightbox shows; `None` while nothing is selected.
    pub fn of(view: &LightboxView<'_>) -> Option<Self> {
        let selected = view.selected()?.clone();
        Some(Self {
            selected,
            selected_index: view.selected_index(),
            navigation: view.navigation_status(),
            tags: view.tag_editor().to_vec(),
            tag_saving: view.tag_saving(),
            tag_failed: view.tag_failed(),
            tag_details_loading: view.tag_details_loading(),
            tag_details_failed: view.tag_details_failed(),
            tag_editing_disabled: view.tag_editing_disabled(),
            asset_details_failed: view.asset_details_failed(),
            favorite_pending: view.favorite_pending(),
            favorite_failed: view.favorite_failed(),
            group_pending: view.group_pending(),
            group_failed: view.group_failed(),
            group_key: view.media_group_key_editor().to_owned(),
            group_order: view.media_group_order_editor().to_owned(),
            group_valid: view.media_group_valid(),
        })
    }

    pub fn kind(&self) -> StageKind {
        StageKind::of(&self.selected.summary.kind)
    }
}

/// An element of the lightbox the focus is sent to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    /// The collapse arrow in the side panel's header (`lightbox-sidebar-close-button`).
    SidebarClose,
    /// The reopen button over the media (`lightbox-sidebar-trigger-button`).
    SidebarTrigger,
    /// The dialog itself.
    Shell,
    /// The field of the delete confirmation (`lightbox-delete-confirm-input`).
    ConfirmInput,
    /// The delete button of the action rail (`lightbox-delete-button`).
    DeleteButton,
    /// The tag field.
    TagInput,
}

/// What Escape did, or asks the view to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dismissal {
    /// Nothing: a deletion runs.
    Nothing,
    /// Fullscreen ends and the lightbox stays, as a browser's Escape ends the
    /// fullscreen of an element. The view toggles the window when `toggle_window`.
    LeftFullscreen { toggle_window: bool },
    /// The delete confirmation closed.
    ConfirmClosed,
    /// The drawer of a narrow window closed.
    SidebarClosed,
    /// The lightbox closes: send `LightboxCommand::Close`.
    Close,
}

/// A key of the viewer (`createImageControls`' key table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewerKey {
    /// Arrow left.
    Previous,
    /// Arrow right.
    Next,
    /// `f` and `F`.
    ToggleFullscreen,
    /// `0`.
    ResetZoom,
    /// `+` and `=`.
    ZoomIn,
    /// `-`.
    ZoomOut,
}

/// What a viewer key asks of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    /// The key is not the viewer's now; it goes on to the next binding.
    Ignored,
    /// The session took it; the view draws again.
    Handled,
    /// Send `LightboxCommand::Previous`.
    Previous,
    /// Send `LightboxCommand::Next`.
    Next,
    /// Toggle the window's fullscreen state, then draw again.
    ToggleWindowFullscreen,
}

/// What the tag field's Enter or a picked suggestion does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddTag {
    /// Editing is disabled, the draft is blank or the tag is assigned already.
    Nothing,
    /// The tag holds a forbidden character: the draft stays and the notice shows.
    Invalid,
    /// Send `LightboxCommand::SaveTags` with this list; the draft was cleared and the
    /// field gets the focus back one frame later.
    Save(Vec<String>),
}

/// A value between 0 and 1 that moves to its target over a fixed time when the target
/// changes, as a CSS transition does (linearly here).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Transition {
    target: bool,
    from: f32,
    since: Duration,
}

impl Transition {
    fn new(target: bool) -> Self {
        Self {
            target,
            from: Self::goal(target),
            since: Duration::ZERO,
        }
    }

    fn goal(target: bool) -> f32 {
        if target {
            1.0
        } else {
            0.0
        }
    }

    fn value(&self, now: Duration, duration: Duration) -> f32 {
        let goal = Self::goal(self.target);
        if duration.is_zero() {
            return goal;
        }
        let progress = now.saturating_sub(self.since).as_secs_f32() / duration.as_secs_f32();
        if progress >= 1.0 {
            goal
        } else {
            self.from + (goal - self.from) * progress
        }
    }

    fn settled(&self, now: Duration, duration: Duration) -> bool {
        self.value(now, duration) == Self::goal(self.target)
    }

    fn set(&mut self, target: bool, now: Duration, duration: Duration) {
        if target != self.target {
            self.from = self.value(now, duration);
            self.target = target;
            self.since = now;
        }
    }
}

/// How long the action rail and the floating buttons fade
/// (`transition-opacity duration-200`).
pub const CONTROLS_FADE: Duration = Duration::from_millis(super::layout::rail::FADE_MS);

/// The state of one opening of the lightbox.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    sidebar: Sidebar,
    activity: Activity,
    clipboard: GroupClipboard,
    confirm: DeleteConfirm,
    controls: ImageControls,
    fullscreen: Fullscreen,
    active: bool,
    selected_id: Option<i64>,
    /// Counts the selected assets of this opening: what was started for one is not
    /// finished for the next.
    generation: u64,
    info: bool,
    draft: String,
    tag_invalid: bool,
    rail_hovered: bool,
    /// The controls never fade (a comparison scenario).
    pinned: bool,
    /// The side panel as the focus rule last saw it; `None` before the first look.
    was_visible: Option<bool>,
    restore_trigger_focus: bool,
    confirm_was_open: bool,
    confirm_focused: bool,
    reduced_motion: bool,
    rail_fade: Transition,
    controls_fade: Transition,
    sidebar_slide: Transition,
}

impl Session {
    /// The session of a lightbox that opens now, in a window that is `narrow` or not
    /// ([`super::is_narrow`]).
    pub fn new(narrow: bool, reduced_motion: bool, touch_device: bool, now: Duration) -> Self {
        let mut activity = Activity::new();
        activity.set_active(true, touch_device, now);
        let sidebar = Sidebar::new(narrow, reduced_motion);
        Self {
            reduced_motion,
            rail_fade: Transition::new(true),
            controls_fade: Transition::new(true),
            sidebar_slide: Transition::new(sidebar.open()),
            sidebar,
            activity,
            clipboard: GroupClipboard::new("", 0),
            confirm: DeleteConfirm::new(),
            controls: ImageControls::new(),
            fullscreen: Fullscreen::new(),
            active: true,
            selected_id: None,
            generation: 0,
            info: false,
            draft: String::new(),
            tag_invalid: false,
            rail_hovered: false,
            pinned: false,
            was_visible: None,
            restore_trigger_focus: true,
            confirm_was_open: false,
            confirm_focused: false,
        }
    }

    pub fn sidebar(&self) -> &Sidebar {
        &self.sidebar
    }

    pub fn confirm(&self) -> &DeleteConfirm {
        &self.confirm
    }

    pub fn controls(&self) -> &ImageControls {
        &self.controls
    }

    /// The image viewer, for the pointer and wheel events of the stage and for what
    /// the adapter measured and decoded.
    pub fn controls_mut(&mut self) -> &mut ImageControls {
        &mut self.controls
    }

    pub fn clipboard(&self) -> &GroupClipboard {
        &self.clipboard
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// `isFullscreen`.
    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen.is_on()
    }

    /// The Information panel is open.
    pub fn info_open(&self) -> bool {
        self.info
    }

    /// The tag field's text.
    pub fn draft(&self) -> &str {
        &self.draft
    }

    /// The notice under the tag field: the draft holds a forbidden character.
    pub fn tag_invalid(&self) -> bool {
        self.tag_invalid
    }

    /// Counts the selected assets of this opening.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The controls never fade from now on: a comparison scenario is captured at any
    /// time after its state is reached.
    pub fn pin_controls(&mut self) {
        self.pinned = true;
    }

    /// Brings the session in line with what the lightbox shows. Another selected
    /// asset starts the selection-scoped state over: the Information panel and the
    /// confirmation close, the draft and its notice go. A republished selection of
    /// the same asset (a tag or favorite write) keeps all of it.
    pub fn sync(&mut self, shown: &Shown, now: Duration) {
        let id = shown.selected.id();
        if self.selected_id != Some(id) {
            self.selected_id = Some(id);
            self.generation += 1;
            self.info = false;
            self.draft.clear();
            self.tag_invalid = false;
        }
        self.confirm.sync(Some(id));
        self.sync_lock(now);
        self.clipboard.sync(&shown.group_key, id);
        let summary = &shown.selected.summary;
        self.controls.select(
            id,
            !shown.kind().is_video(),
            summary.width,
            summary.height,
            now,
        );
    }

    fn sync_lock(&mut self, now: Duration) {
        self.sidebar.set_locked(self.confirm.locks_sidebar(), now);
    }

    /// The lightbox closed logically: it takes no input any more while it fades out.
    /// Returns whether the window's fullscreen has to be toggled back.
    pub fn deactivate(&mut self, window_fullscreen: bool, now: Duration) -> bool {
        if !self.active {
            return false;
        }
        self.active = false;
        self.activity.set_active(false, false, now);
        // The side panel keeps its choice: `LightboxContent` makes its sidebar with
        // `() => true`, so a collapsed panel stays collapsed while the lightbox fades
        // out, and the next opening starts a new session with no choice.
        self.controls.set_active(false, now);
        self.clipboard.dispose();
        self.rail_hovered = false;
        self.fullscreen.leave(window_fullscreen)
    }

    /// The window crossed the narrow breakpoint, or not.
    pub fn set_narrow(&mut self, narrow: bool, now: Duration) {
        self.sidebar.set_narrow(narrow, now);
    }

    pub fn set_reduced_motion(&mut self, reduced_motion: bool) {
        self.reduced_motion = reduced_motion;
        self.sidebar.set_reduced_motion(reduced_motion);
    }

    // ----- motion -----

    fn fade_time(&self) -> Duration {
        if self.reduced_motion {
            Duration::ZERO
        } else {
            CONTROLS_FADE
        }
    }

    fn slide_time(&self) -> Duration {
        if self.reduced_motion {
            Duration::ZERO
        } else {
            super::presentation::SIDEBAR_EXIT
        }
    }

    /// Starts the fades and the slide that what is shown now asks for. The view calls
    /// it with the time of every frame, before it reads the values below; it returns
    /// whether one of them still moves, so another frame is due.
    pub fn animate(&mut self, now: Duration) -> bool {
        let (fade, slide) = (self.fade_time(), self.slide_time());
        self.rail_fade.set(self.rail_visible(now), now, fade);
        self.controls_fade
            .set(self.controls_visible(now), now, fade);
        self.sidebar_slide.set(self.sidebar.open(), now, slide);
        !(self.rail_fade.settled(now, fade)
            && self.controls_fade.settled(now, fade)
            && self.sidebar_slide.settled(now, slide))
    }

    /// The opacity of the action rail.
    pub fn rail_opacity(&self, now: Duration) -> f32 {
        self.rail_fade.value(now, self.fade_time())
    }

    /// The opacity of the floating buttons over the media.
    pub fn controls_opacity(&self, now: Duration) -> f32 {
        self.controls_fade.value(now, self.fade_time())
    }

    /// How far the side panel is slid out to the right, as a share of its width: 0 in
    /// place, 1 out of sight (`translate-x-full`).
    pub fn sidebar_shift(&self, now: Duration) -> f32 {
        1.0 - self.sidebar_slide.value(now, self.slide_time())
    }

    // ----- activity -----

    /// The pointer is at `(x, y)` of the window.
    pub fn pointer_moved(&mut self, x: f32, y: f32, now: Duration) {
        self.activity.pointer_moved(x, y, now);
    }

    /// A pointer press anywhere in the window.
    pub fn pointer_pressed(&mut self, touch: bool, now: Duration) {
        self.activity.pointer_down(touch, now);
    }

    /// A wheel turn, a scroll or a key press anywhere in the window.
    pub fn input(&mut self, now: Duration) {
        self.activity.input(now);
    }

    /// The floating controls over the media (the reopen and close buttons) are shown.
    pub fn controls_visible(&self, now: Duration) -> bool {
        self.pinned || self.activity.visible(now)
    }

    /// The drawer covers the action rail.
    pub fn rail_covered(&self, now: Duration) -> bool {
        self.sidebar.narrow() && self.sidebar.occupied(now)
    }

    /// The pointer entered (`true`) or left the island of the action rail. Leaving
    /// counts as activity, so the rail does not vanish under the pointer's last
    /// position.
    pub fn rail_hover(&mut self, hovered: bool, now: Duration) {
        if hovered {
            if self.active && !self.rail_covered(now) {
                self.rail_hovered = true;
            }
        } else {
            self.activity.reveal(now);
            self.rail_hovered = false;
        }
    }

    /// The action rail is shown: after activity, and while the pointer rests on it.
    pub fn rail_visible(&self, now: Duration) -> bool {
        self.controls_visible(now) || (self.rail_hovered && !self.rail_covered(now))
    }

    /// The earliest moment at which something this session shows changes by itself;
    /// the view draws again then. Always later than `now`.
    pub fn deadline(&self, now: Duration) -> Option<Duration> {
        let activity = if self.pinned {
            None
        } else {
            self.activity.deadline(now)
        };
        [
            self.sidebar.deadline(now),
            activity,
            self.clipboard.deadline(now),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    // ----- the side panel -----

    /// The side panel is drawn in place (not slid out) and takes input.
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar.open()
    }

    /// The media column and the action rail take no input: the drawer of a narrow
    /// window lies over them.
    pub fn media_inert(&self) -> bool {
        self.sidebar.narrow() && self.sidebar.open()
    }

    /// The reopen button over the media.
    pub fn open_sidebar(&mut self, now: Duration) {
        self.sidebar.show(now);
    }

    /// The collapse arrow, the scrim or Escape in a narrow window. `restore_focus`:
    /// the reopen button gets the focus (a keyboard collapse); otherwise the dialog.
    /// Nothing while the confirmation is open or a deletion runs.
    pub fn close_sidebar(&mut self, restore_focus: bool, now: Duration) {
        self.restore_trigger_focus = restore_focus;
        self.sidebar.hide(now);
    }

    // ----- the Information panel -----

    pub fn toggle_info(&mut self) {
        self.info = !self.info;
    }

    // ----- tags -----

    /// The tag field's text changed by typing.
    pub fn set_draft(&mut self, draft: impl Into<String>) {
        self.draft = draft.into();
        self.tag_invalid = false;
    }

    /// Enter in the tag field or a picked suggestion (`addTag`).
    pub fn add_tag(&mut self, value: &str, shown: &Shown) -> AddTag {
        if !self.active || shown.tag_editing_disabled {
            return AddTag::Nothing;
        }
        let tag = crate::search::text::js_trim(value).to_lowercase();
        if !is_valid_tag(&tag) {
            // The draft stays so that it can be corrected; an empty one stays silent.
            self.tag_invalid = !tag.is_empty();
            return if self.tag_invalid {
                AddTag::Invalid
            } else {
                AddTag::Nothing
            };
        }
        self.tag_invalid = false;
        if shown.tags.contains(&tag) {
            return AddTag::Nothing;
        }
        let mut tags = shown.tags.clone();
        tags.push(tag);
        self.draft.clear();
        AddTag::Save(normalize_tags(tags))
    }

    /// A chip's remove button: the list to save, unless editing is disabled.
    pub fn remove_tag(&self, tag: &str, shown: &Shown) -> Option<Vec<String>> {
        if !self.active || shown.tag_editing_disabled {
            return None;
        }
        Some(
            shown
                .tags
                .iter()
                .filter(|assigned| assigned.as_str() != tag)
                .cloned()
                .collect(),
        )
    }

    // ----- the group name -----

    /// The copy button: what to write to the clipboard.
    pub fn begin_copy(&self) -> Option<CopyRequest> {
        self.clipboard.begin_copy()
    }

    /// The outcome of the write; returns whether the copy is confirmed.
    pub fn finish_copy(&mut self, request: &CopyRequest, written: bool, now: Duration) -> bool {
        self.clipboard.finish_copy(request, written, now)
    }

    // ----- deletion -----

    /// The delete button of the action rail: the side panel opens with the
    /// confirmation in it and the Information panel closes.
    pub fn open_delete_confirm(&mut self, now: Duration) {
        if !self.active {
            return;
        }
        self.sidebar.show(now);
        self.confirm.open();
        self.info = false;
        self.sync_lock(now);
    }

    /// Cancel. Nothing while the deletion runs.
    pub fn close_delete_confirm(&mut self, now: Duration) -> bool {
        let closed = self.confirm.close();
        self.sync_lock(now);
        closed
    }

    /// The confirmation field's text.
    pub fn set_confirm_text(&mut self, text: impl Into<String>) {
        self.confirm.set_text(text);
    }

    /// Confirm: the request to send as `LightboxCommand::DeleteSelected`.
    pub fn submit_delete(&mut self, word: &str, now: Duration) -> Option<u64> {
        if !self.active {
            return None;
        }
        let request = self.confirm.submit(word);
        self.sync_lock(now);
        request
    }

    /// The answer to a deletion: the notice to show, if the accepted deletion has
    /// one. A refusal keeps the confirmation open with every exit usable again, and
    /// its field gets the focus back (`FocusTarget::ConfirmInput` of
    /// [`Self::focus_requests`]).
    pub fn delete_settled(
        &mut self,
        request: u64,
        result: &Result<Option<DeletedAsset>, MutationError>,
        now: Duration,
    ) -> Option<DeleteNotice> {
        let was_deleting = self.confirm.deleting();
        self.confirm.settled(request, result);
        if was_deleting && !self.confirm.deleting() && self.confirm.is_open() {
            // The disabled field gave the focus up; it takes it again.
            self.confirm_focused = false;
        }
        self.sync_lock(now);
        DeleteNotice::of(result)
    }

    // ----- exits -----

    /// The close buttons (`close`): not while a deletion runs.
    pub fn may_close(&self) -> bool {
        self.active && !self.confirm.deleting()
    }

    /// Escape, with this lightbox as the top layer.
    pub fn escape(&mut self, window_fullscreen: bool, now: Duration) -> Dismissal {
        if !self.active {
            return Dismissal::Nothing;
        }
        if self.fullscreen.is_on() {
            return Dismissal::LeftFullscreen {
                toggle_window: self.fullscreen.leave(window_fullscreen),
            };
        }
        if self.confirm.deleting() {
            return Dismissal::Nothing;
        }
        if self.confirm.is_open() {
            self.close_delete_confirm(now);
            return Dismissal::ConfirmClosed;
        }
        self.dismiss_windowed(now)
    }

    fn dismiss_windowed(&mut self, now: Duration) -> Dismissal {
        if self.media_inert() {
            self.close_sidebar(true, now);
            Dismissal::SidebarClosed
        } else {
            Dismissal::Close
        }
    }

    /// A press on the backdrop around the dialog. `top_layer`: no other layer lies
    /// above the lightbox.
    pub fn backdrop_pressed(&mut self, top_layer: bool, now: Duration) -> Dismissal {
        if !self.active || !top_layer || self.confirm.deleting() || self.confirm.is_open() {
            return Dismissal::Nothing;
        }
        if self.media_inert() {
            self.close_sidebar(true, now);
            return Dismissal::SidebarClosed;
        }
        if self.controls.may_close(now) {
            Dismissal::Close
        } else {
            Dismissal::Nothing
        }
    }

    // ----- keys and fullscreen -----

    /// `keyboardEnabled`: the viewer's keys are on. `top_layer`: no other layer lies
    /// above the lightbox.
    pub fn keyboard_enabled(&self, top_layer: bool) -> bool {
        self.active
            && top_layer
            && !self.confirm.is_open()
            && !self.confirm.deleting()
            && !self.media_inert()
    }

    /// A key of the viewer, pressed outside a text field.
    pub fn key(
        &mut self,
        key: ViewerKey,
        kind: StageKind,
        top_layer: bool,
        window_fullscreen: bool,
    ) -> KeyOutcome {
        if !self.keyboard_enabled(top_layer) {
            return KeyOutcome::Ignored;
        }
        match key {
            ViewerKey::Previous => KeyOutcome::Previous,
            ViewerKey::Next => KeyOutcome::Next,
            ViewerKey::ToggleFullscreen => self.toggle_fullscreen(kind, window_fullscreen),
            // The zoom keys are an image's and a GIF's.
            _ if kind.is_video() => KeyOutcome::Ignored,
            ViewerKey::ResetZoom => {
                self.controls.reset();
                KeyOutcome::Handled
            }
            ViewerKey::ZoomIn => {
                self.controls.zoom_in();
                KeyOutcome::Handled
            }
            ViewerKey::ZoomOut => {
                self.controls.zoom_out();
                KeyOutcome::Handled
            }
        }
    }

    /// The fullscreen button and the `f` key. An image and a GIF go fullscreen with
    /// the window, and the Information panel closes on the way in. A video's
    /// fullscreen is its player's, which the GPUI lightbox does not have yet (epic
    /// E21): the key is taken and nothing happens.
    pub fn toggle_fullscreen(&mut self, kind: StageKind, window_fullscreen: bool) -> KeyOutcome {
        if !self.active || kind.is_video() {
            return KeyOutcome::Handled;
        }
        let toggle = if self.fullscreen.is_on() {
            self.fullscreen.leave(window_fullscreen)
        } else {
            self.info = false;
            self.fullscreen.enter(window_fullscreen)
        };
        if toggle {
            KeyOutcome::ToggleWindowFullscreen
        } else {
            KeyOutcome::Handled
        }
    }

    /// The window's fullscreen state as it is now. A selected video leaves the
    /// lightbox's fullscreen (an image's fullscreen does not follow the selection to
    /// a video). Returns whether the window has to be toggled back.
    pub fn sync_fullscreen(&mut self, kind: StageKind, window_fullscreen: bool) -> bool {
        self.fullscreen.window_changed(window_fullscreen);
        if kind.is_video() && self.fullscreen.is_on() {
            return self.fullscreen.leave(window_fullscreen);
        }
        false
    }

    // ----- focus -----

    /// Where the focus should go after what happened since the last call, in order;
    /// the view focuses each one frame later, so the last one wins.
    ///
    /// - The side panel opened: the confirmation field when the confirmation is
    ///   open, else the collapse arrow.
    /// - The side panel closed: the reopen button after a keyboard collapse, else
    ///   the dialog; the floating controls show again.
    /// - The confirmation opened in a visible panel: its field, once per opening.
    /// - The confirmation closed: the delete button, or the collapse arrow in a
    ///   narrow window, where the drawer covers the rail.
    pub fn focus_requests(&mut self, now: Duration) -> Vec<FocusTarget> {
        let mut requests = Vec::new();
        if !self.active {
            return requests;
        }
        let open = self.sidebar.open();
        match self.was_visible {
            None => self.was_visible = Some(open),
            Some(previous) if previous != open => {
                self.was_visible = Some(open);
                if open {
                    requests.push(if self.confirm.is_open() {
                        FocusTarget::ConfirmInput
                    } else {
                        FocusTarget::SidebarClose
                    });
                } else {
                    requests.push(if self.restore_trigger_focus {
                        FocusTarget::SidebarTrigger
                    } else {
                        FocusTarget::Shell
                    });
                    self.activity.reveal(now);
                }
            }
            Some(_) => {}
        }
        if self.confirm.is_open() {
            self.confirm_was_open = true;
            if open && !self.confirm_focused && !self.confirm.deleting() {
                self.confirm_focused = true;
                if requests.last() != Some(&FocusTarget::ConfirmInput) {
                    requests.push(FocusTarget::ConfirmInput);
                }
            }
        } else {
            if self.confirm_was_open {
                requests.push(if self.sidebar.narrow() {
                    FocusTarget::SidebarClose
                } else {
                    FocusTarget::DeleteButton
                });
            }
            self.confirm_was_open = false;
            self.confirm_focused = false;
        }
        requests
    }
}

#[cfg(test)]
mod tests {
    use media_tagger_core::models::AssetSummary;

    use super::*;
    use crate::library::TagStateError;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn shown(id: i64, kind: &str) -> Shown {
        Shown {
            selected: SelectedAsset {
                summary: AssetSummary {
                    id,
                    file_name: format!("asset-{id}.png"),
                    preview_path: None,
                    kind: kind.into(),
                    modified_at: 0,
                    width: Some(800),
                    height: Some(600),
                    duration_ms: None,
                    thumb_version: "0123456789abcdef".into(),
                    is_favorite: false,
                    media_group_key: None,
                    media_group_order: None,
                },
                path: Some(format!("/media/asset-{id}.png")),
                size_bytes: Some(10),
                tags: vec!["cat".into()],
            },
            selected_index: Some(0),
            navigation: NavigationStatus::Ready,
            tags: vec!["cat".into()],
            tag_saving: false,
            tag_failed: false,
            tag_details_loading: false,
            tag_details_failed: false,
            tag_editing_disabled: false,
            asset_details_failed: false,
            favorite_pending: false,
            favorite_failed: false,
            group_pending: false,
            group_failed: false,
            group_key: String::new(),
            group_order: String::new(),
            group_valid: true,
        }
    }

    fn session(narrow: bool) -> Session {
        let mut session = Session::new(narrow, false, false, MS(0));
        session.sync(&shown(1, "image"), MS(0));
        assert!(session.focus_requests(MS(0)).is_empty());
        session
    }

    #[test]
    fn a_collapsed_panel_stays_collapsed_while_the_lightbox_fades_out() {
        let mut session = session(false);
        session.close_sidebar(false, MS(0));
        // The frame of the collapse starts the slide.
        session.animate(MS(0));
        assert!(!session.sidebar().occupied(MS(200)));
        session.deactivate(false, MS(300));
        session.animate(MS(300));
        assert!(!session.sidebar().open(), "the choice outlives the close");
        assert!(!session.sidebar().occupied(MS(300)));
        assert_eq!(session.sidebar_shift(MS(300)), 1.0);
    }

    #[test]
    fn another_asset_starts_the_selection_scoped_state_over() {
        let mut session = session(false);
        session.toggle_info();
        session.set_draft("do");
        session.open_delete_confirm(MS(0));
        session.set_confirm_text("ye");
        let generation = session.generation();

        // The same asset, republished by a tag write: everything stays.
        session.toggle_info();
        session.sync(&shown(1, "image"), MS(10));
        assert!(session.info_open() && session.confirm().is_open());
        assert_eq!((session.draft(), session.confirm().text()), ("do", "ye"));
        assert_eq!(session.generation(), generation);

        session.sync(&shown(2, "image"), MS(20));
        assert!(!session.info_open() && !session.confirm().is_open());
        assert_eq!((session.draft(), session.confirm().text()), ("", ""));
        assert_eq!(session.generation(), generation + 1);
    }

    #[test]
    fn adding_a_tag_validates_normalizes_and_clears_the_draft() {
        let mut session = session(false);
        let shown = shown(1, "image");
        session.set_draft("  Dog ");
        assert_eq!(
            session.add_tag("  Dog ", &shown),
            AddTag::Save(vec!["cat".into(), "dog".into()])
        );
        assert_eq!(session.draft(), "");

        session.set_draft("a,b");
        assert_eq!(session.add_tag("a,b", &shown), AddTag::Invalid);
        assert!(session.tag_invalid());
        assert_eq!(session.draft(), "a,b", "the draft stays to be corrected");
        session.set_draft("a");
        assert!(!session.tag_invalid(), "typing clears the notice");

        assert_eq!(session.add_tag("   ", &shown), AddTag::Nothing);
        assert!(!session.tag_invalid(), "an empty submit stays silent");
        assert_eq!(session.add_tag("CAT", &shown), AddTag::Nothing);

        let mut disabled = shown.clone();
        disabled.tag_editing_disabled = true;
        assert_eq!(session.add_tag("dog", &disabled), AddTag::Nothing);
        assert_eq!(session.remove_tag("cat", &disabled), None);
        assert_eq!(session.remove_tag("cat", &shown), Some(Vec::new()));
    }

    #[test]
    fn escape_closes_only_the_innermost_thing() {
        let mut session = session(false);
        session.open_delete_confirm(MS(0));
        assert_eq!(session.escape(false, MS(1)), Dismissal::ConfirmClosed);
        assert!(!session.confirm().is_open());
        assert_eq!(session.escape(false, MS(2)), Dismissal::Close);

        // A narrow window: the drawer first, then the lightbox.
        let mut narrow = self::session(true);
        narrow.open_sidebar(MS(0));
        assert_eq!(narrow.escape(false, MS(1)), Dismissal::SidebarClosed);
        assert!(!narrow.sidebar().open());
        assert_eq!(narrow.escape(false, MS(2)), Dismissal::Close);

        // Fullscreen ends before anything closes.
        let mut full = self::session(false);
        assert_eq!(
            full.toggle_fullscreen(StageKind::Image, false),
            KeyOutcome::ToggleWindowFullscreen
        );
        assert!(full.is_fullscreen());
        assert_eq!(
            full.escape(true, MS(1)),
            Dismissal::LeftFullscreen {
                toggle_window: true
            }
        );
        assert!(!full.is_fullscreen());
    }

    #[test]
    fn a_running_deletion_locks_every_exit_and_a_refusal_unlocks_them() {
        let mut session = session(false);
        session.open_delete_confirm(MS(0));
        session.focus_requests(MS(0));
        session.set_confirm_text("yes");
        let request = session.submit_delete("Yes", MS(1)).expect("a request");
        assert!(!session.may_close());
        assert_eq!(session.escape(false, MS(2)), Dismissal::Nothing);
        assert_eq!(session.backdrop_pressed(true, MS(2)), Dismissal::Nothing);
        assert!(!session.close_delete_confirm(MS(2)));
        assert!(!session.keyboard_enabled(true));
        session.close_sidebar(true, MS(2));
        assert!(session.sidebar().open(), "the panel is locked");

        let refused = Err(MutationError::Refused(TagStateError::Unavailable));
        assert_eq!(session.delete_settled(request, &refused, MS(3)), None);
        assert!(session.may_close() && session.confirm().is_open());
        assert_eq!(session.focus_requests(MS(3)), [FocusTarget::ConfirmInput]);
        assert_eq!(session.escape(false, MS(4)), Dismissal::ConfirmClosed);
        assert_eq!(session.focus_requests(MS(4)), [FocusTarget::DeleteButton]);
    }

    #[test]
    fn the_backdrop_closes_unless_something_is_in_the_way() {
        let mut session = session(false);
        assert_eq!(session.backdrop_pressed(false, MS(0)), Dismissal::Nothing);
        assert_eq!(session.backdrop_pressed(true, MS(0)), Dismissal::Close);
        session.open_delete_confirm(MS(0));
        assert_eq!(session.backdrop_pressed(true, MS(0)), Dismissal::Nothing);
        session.close_delete_confirm(MS(0));

        // Right after a zooming click the press is the gesture's, not a close.
        session
            .controls_mut()
            .set_viewport(super::super::image_controls::Viewport {
                left: 0.0,
                top: 0.0,
                width: 800.0,
                height: 600.0,
            });
        session.controls_mut().set_mounted(true);
        assert!(session.controls_mut().click(400.0, 300.0, MS(1000)));
        assert_eq!(session.backdrop_pressed(true, MS(1100)), Dismissal::Nothing);
        assert_eq!(session.backdrop_pressed(true, MS(1360)), Dismissal::Close);

        let mut narrow = self::session(true);
        narrow.open_sidebar(MS(0));
        assert_eq!(
            narrow.backdrop_pressed(true, MS(1)),
            Dismissal::SidebarClosed
        );
    }

    #[test]
    fn the_viewer_keys_follow_the_layer_the_drawer_and_the_kind() {
        let mut session = session(false);
        let image = StageKind::Image;
        assert_eq!(
            session.key(ViewerKey::Next, image, true, false),
            KeyOutcome::Next
        );
        assert_eq!(
            session.key(ViewerKey::Previous, image, true, false),
            KeyOutcome::Previous
        );
        assert_eq!(
            session.key(ViewerKey::Next, image, false, false),
            KeyOutcome::Ignored
        );
        assert_eq!(
            session.key(ViewerKey::ZoomIn, image, true, false),
            KeyOutcome::Handled
        );
        // A video keeps the arrows and takes the fullscreen key without an effect.
        let video = StageKind::Video;
        assert_eq!(
            session.key(ViewerKey::Next, video, true, false),
            KeyOutcome::Next
        );
        assert_eq!(
            session.key(ViewerKey::ZoomIn, video, true, false),
            KeyOutcome::Ignored
        );
        assert_eq!(
            session.key(ViewerKey::ToggleFullscreen, video, true, false),
            KeyOutcome::Handled
        );
        assert!(!session.is_fullscreen());

        session.open_delete_confirm(MS(0));
        assert_eq!(
            session.key(ViewerKey::Next, image, true, false),
            KeyOutcome::Ignored
        );

        let mut narrow = self::session(true);
        assert_eq!(
            narrow.key(ViewerKey::Next, image, true, false),
            KeyOutcome::Next
        );
        narrow.open_sidebar(MS(0));
        assert_eq!(
            narrow.key(ViewerKey::Next, image, true, false),
            KeyOutcome::Ignored
        );
    }

    #[test]
    fn fullscreen_closes_the_information_panel_and_ends_with_the_lightbox() {
        let mut session = session(false);
        session.toggle_info();
        assert_eq!(
            session.key(ViewerKey::ToggleFullscreen, StageKind::Image, true, false),
            KeyOutcome::ToggleWindowFullscreen
        );
        assert!(session.is_fullscreen() && !session.info_open());
        // A selected video leaves it.
        assert!(session.sync_fullscreen(StageKind::Video, true));
        assert!(!session.is_fullscreen());

        session.toggle_fullscreen(StageKind::Image, false);
        assert!(
            session.deactivate(true, MS(5)),
            "the window is toggled back"
        );
        assert!(!session.is_fullscreen() && !session.is_active());
        assert!(!session.deactivate(true, MS(6)));
        assert_eq!(session.escape(false, MS(7)), Dismissal::Nothing);
        assert!(!session.may_close());
    }

    #[test]
    fn the_focus_follows_the_side_panel_and_the_confirmation() {
        let mut session = session(true);
        session.open_sidebar(MS(0));
        assert_eq!(session.focus_requests(MS(0)), [FocusTarget::SidebarClose]);
        assert!(session.focus_requests(MS(1)).is_empty());
        session.close_sidebar(true, MS(2));
        assert_eq!(session.focus_requests(MS(2)), [FocusTarget::SidebarTrigger]);
        session.open_sidebar(MS(3));
        session.focus_requests(MS(3));
        session.close_sidebar(false, MS(4));
        assert_eq!(session.focus_requests(MS(4)), [FocusTarget::Shell]);

        // The delete button opens the drawer with the confirmation: its field.
        session.open_delete_confirm(MS(5));
        assert_eq!(session.focus_requests(MS(5)), [FocusTarget::ConfirmInput]);
        // A republished selection asks for nothing again.
        session.sync(&shown(1, "image"), MS(6));
        assert!(session.focus_requests(MS(6)).is_empty());
        // Closing it in a narrow window: the collapse arrow; the panel stays open by
        // the choice the delete button made.
        session.close_delete_confirm(MS(7));
        assert_eq!(session.focus_requests(MS(7)), [FocusTarget::SidebarClose]);
    }

    #[test]
    fn fades_and_the_slide_take_their_time_and_reduced_motion_takes_none() {
        let mut session = session(false);
        assert!(!session.animate(MS(0)));
        assert_eq!(
            (session.rail_opacity(MS(0)), session.sidebar_shift(MS(0))),
            (1.0, 0.0)
        );
        // The controls hide three seconds after the lightbox opened.
        assert!(session.animate(MS(3000)));
        assert_eq!(session.rail_opacity(MS(3000)), 1.0);
        assert_eq!(session.rail_opacity(MS(3100)), 0.5);
        assert_eq!(session.controls_opacity(MS(3100)), 0.5);
        assert!(session.animate(MS(3100)));
        assert!(!session.animate(MS(3200)));
        assert_eq!(session.rail_opacity(MS(3200)), 0.0);
        // Activity fades them in again.
        session.input(MS(4000));
        assert!(session.animate(MS(4000)));
        assert_eq!(session.rail_opacity(MS(4050)), 0.25);

        session.close_sidebar(false, MS(5000));
        assert!(session.animate(MS(5000)));
        assert_eq!(session.sidebar_shift(MS(5100)), 0.5);
        assert!(session.sidebar().occupied(MS(5100)));
        assert_eq!(session.sidebar_shift(MS(5200)), 1.0);
        assert!(!session.sidebar().occupied(MS(5200)));

        session.set_reduced_motion(true);
        session.open_sidebar(MS(6000));
        session.animate(MS(6000));
        assert_eq!(session.sidebar_shift(MS(6000)), 0.0);
    }

    #[test]
    fn the_rail_stays_under_the_pointer_and_pinned_controls_never_fade() {
        let mut session = session(false);
        assert!(session.rail_visible(MS(0)));
        assert_eq!(session.deadline(MS(0)), Some(MS(3000)));
        session.rail_hover(true, MS(100));
        assert!(!session.controls_visible(MS(3000)));
        assert!(session.rail_visible(MS(3000)));
        session.rail_hover(false, MS(3100));
        assert!(
            session.controls_visible(MS(3100)),
            "leaving counts as activity"
        );
        assert!(!session.rail_visible(MS(6100)));
        session.pointer_moved(5.0, 5.0, MS(7000));
        assert!(session.rail_visible(MS(7000)));

        session.pin_controls();
        assert!(session.controls_visible(MS(60_000)));
        assert_eq!(session.deadline(MS(7000)), None);
    }
}
