//! The lightbox (`LightboxModal.tsx` and its parts): the selected asset over the
//! gallery, with the media stage, the action rail and the side panel.
//!
//! The view lives as long as the shell and draws nothing while no asset is selected.
//! What is selected, and every edit, is the runtime's lightbox model
//! ([`Lightbox`](media_tagger_client::lightbox::Lightbox)); this view only draws and
//! forwards. What an opening of the lightbox decides besides that (the side panel,
//! the fading controls, the delete confirmation, zoom and pan, Escape, the focus) is
//! a [`Session`] of the client crate, made when the lightbox opens and dropped when
//! it has faded out, as the Solid presence owner is. While it fades out the view
//! keeps drawing the last [`Shown`] values and takes no input.
//!
//! - `stage.rs`: the media column: the stage with the picture, the GIF's first
//!   frame or the video placeholder (plan decision D8), what floats over it, and the
//!   action rail.
//! - `sidebar.rs`: the side panel: header, media group, tags, Information and the
//!   inline delete confirmation.
//!
//! The picture is read and decoded off the UI thread by [`Originals`]. The dialog is
//! a modal layer of the layer stack through a [`ModalState`] with the lightbox's
//! motion; it draws itself, since its layout is not a dialog surface's.

mod sidebar;
mod stage;

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{
    actions, anchored, canvas, deferred, div, point, prelude::*, px, AnyElement, App, Bounds,
    ClickEvent, ClipboardItem, Context, DispatchPhase, Entity, FocusHandle, Focusable, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, PromptLevel, Role,
    SharedString, Subscription, Task, Window,
};
use media_tagger_client::{
    bulk::{parse_media_group_order, GroupOrderDraft},
    gallery::{layout::thumbnail_key, TileRequest},
    i18n::{I18n, Language, Params},
    lightbox::{
        is_narrow, layout, stored_dimensions, AddTag, DeleteNotice, Dismissal, FocusTarget,
        ImageRect, KeyOutcome, LightboxCommand, LightboxEvent, LoadState, MediaSource, Session,
        Shown, StageContent, StageKind, ViewerKey, Viewport, WindowFullscreen,
    },
    overlay::ModalMotion,
    runtime::RuntimeInput,
    scenario::ScenarioLightbox,
    shortcuts::{types_a_character, KeyContext, ShortcutCommand, SHORTCUTS},
    tagging::{SuggestionPlacement, TagEditorVariant},
    text::FieldChrome,
    theme::{ThemeName, RADII},
};

use super::tag_editor::{RemoveLabel, TagEditor, TagEditorEvent, TagEditorProps};
use crate::{
    media::Originals,
    runtime::Runtime,
    thumbs::Thumbnails,
    ui::{
        layers,
        modal::{self, ModalEvent, ModalState},
        text_input::{outside_text_input, TextInput, TextInputEvent},
        theme::{self, box_shadows, color},
    },
};

actions!(
    lightbox,
    [
        /// Arrow left: the previous asset of the result.
        Previous,
        /// Arrow right: the next asset of the result.
        Next,
        /// F: fullscreen on and off.
        ToggleFullscreen,
        /// 0: the whole picture again.
        ResetZoom,
        /// Plus or the equals sign: one zoom step in.
        ZoomIn,
        /// Minus: one zoom step out.
        ZoomOut,
    ]
);

/// Debug selectors and element ids (the `data-testid`s and ids of the Solid lightbox).
pub const BACKDROP_SELECTOR: &str = "lightbox-backdrop";
pub const DIALOG_SELECTOR: &str = "lightbox-dialog";
pub const MEDIA_COLUMN_SELECTOR: &str = "lightbox-media-column";
pub const STAGE_SELECTOR: &str = "lightbox-media-stage";
pub const IMAGE_SELECTOR: &str = "lightbox-image";
pub const GIF_CHIP_SELECTOR: &str = "lightbox-gif-chip";
pub const VIDEO_PLACEHOLDER_SELECTOR: &str = "lightbox-video-placeholder";
pub const VIDEO_POSTER_SELECTOR: &str = "lightbox-video-poster";
pub const DETAILS_LOADING_SELECTOR: &str = "lightbox-details-loading";
pub const DETAILS_ERROR_SELECTOR: &str = "lightbox-details-error";
pub const MEDIA_ERROR_SELECTOR: &str = "lightbox-media-error";
pub const MEDIA_LOADING_SELECTOR: &str = "lightbox-media-loading";
pub const DETAILS_NOTICE_SELECTOR: &str = "lightbox-details-notice";
pub const RETRY_DETAILS_ID: &str = "lightbox-retry-details";
pub const NAVIGATION_SELECTOR: &str = "lightbox-navigation-status";
pub const NAVIGATION_RETRY_ID: &str = "lightbox-navigation-retry";
pub const FAVORITE_FAILED_SELECTOR: &str = "lightbox-favorite-failed";
pub const GROUP_FAILED_SELECTOR: &str = "lightbox-group-failed";
pub const TRIGGER_ID: &str = "lightbox-sidebar-trigger-button";
pub const FLOATING_CLOSE_ID: &str = "lightbox-floating-close";
pub const RAIL_SELECTOR: &str = "lightbox-action-rail";
pub const SUMMARY_SELECTOR: &str = "lightbox-media-summary";
pub const FAVORITE_ID: &str = "lightbox-favorite";
pub const RESET_ZOOM_ID: &str = "lightbox-reset-zoom";
pub const FULLSCREEN_ID: &str = "lightbox-fullscreen";
pub const DELETE_ID: &str = "lightbox-delete-button";
pub const SIDEBAR_SELECTOR: &str = "lightbox-sidebar";
pub const SIDEBAR_SCRIM_SELECTOR: &str = "lightbox-sidebar-scrim";
pub const SIDEBAR_CLOSE_ID: &str = "lightbox-sidebar-close-button";
pub const CLOSE_ID: &str = "lightbox-close";
pub const SIDEBAR_UPPER_SELECTOR: &str = "lightbox-sidebar-upper";
pub const GROUP_PANEL_SELECTOR: &str = "lightbox-media-group-panel";
pub const GROUP_KEY_SELECTOR: &str = "lightbox-media-group-key-input";
pub const GROUP_ORDER_SELECTOR: &str = "lightbox-media-group-order-input";
pub const COPY_GROUP_ID: &str = "lightbox-copy-media-group";
pub const GENERATE_GROUP_ID: &str = "lightbox-generate-media-group";
pub const APPLY_GROUP_ID: &str = "lightbox-apply-media-group";
pub const TAG_PANEL_SELECTOR: &str = "lightbox-tag-panel";
pub const TAG_INVALID_SELECTOR: &str = "lightbox-tag-invalid";
pub const TAG_DETAILS_LOADING_SELECTOR: &str = "lightbox-tag-details-loading";
pub const TAG_DETAILS_FAILED_SELECTOR: &str = "lightbox-tag-details-failed";
pub const RETRY_TAG_DETAILS_ID: &str = "lightbox-retry-tag-details";
pub const TAG_SAVING_SELECTOR: &str = "lightbox-tag-saving";
pub const TAG_FAILED_SELECTOR: &str = "lightbox-tag-failed";
pub const RETRY_TAGS_ID: &str = "lightbox-retry-tags";
pub const INFO_TOGGLE_ID: &str = "lightbox-info-toggle";
pub const INFO_PANEL_SELECTOR: &str = "lightbox-info-panel";
pub const CONFIRM_SELECTOR: &str = "lightbox-delete-confirm-dialog";
pub const CONFIRM_INPUT_SELECTOR: &str = "lightbox-delete-confirm-input";
pub const CONFIRM_ERROR_SELECTOR: &str = "lightbox-delete-confirm-error";
pub const CONFIRM_CANCEL_ID: &str = "lightbox-delete-cancel";
pub const CONFIRM_SUBMIT_ID: &str = "lightbox-delete-confirm";

/// The key context of the open dialog: a modal's and the lightbox's.
pub fn key_context() -> String {
    format!("{} {}", modal::KEY_CONTEXT, KeyContext::Lightbox.name())
}

/// The lightbox's entries of the shortcut registry as GPUI bindings. A key that types
/// a character is bound outside a text field only.
pub fn key_bindings() -> Vec<KeyBinding> {
    SHORTCUTS
        .iter()
        .filter(|shortcut| shortcut.context == KeyContext::Lightbox)
        .filter_map(|shortcut| {
            let name = shortcut.context.name();
            let context = if types_a_character(shortcut.keystroke) {
                outside_text_input(name)
            } else {
                name.to_owned()
            };
            let context = Some(context.as_str());
            let keystroke = shortcut.keystroke;
            Some(match shortcut.command {
                ShortcutCommand::LightboxPrevious => KeyBinding::new(keystroke, Previous, context),
                ShortcutCommand::LightboxNext => KeyBinding::new(keystroke, Next, context),
                ShortcutCommand::LightboxToggleFullscreen => {
                    KeyBinding::new(keystroke, ToggleFullscreen, context)
                }
                ShortcutCommand::LightboxResetZoom => {
                    KeyBinding::new(keystroke, ResetZoom, context)
                }
                ShortcutCommand::LightboxZoomIn => KeyBinding::new(keystroke, ZoomIn, context),
                ShortcutCommand::LightboxZoomOut => KeyBinding::new(keystroke, ZoomOut, context),
                _ => return None,
            })
        })
        .collect()
}

/// Binds the lightbox's keys. Part of `views::init`.
pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

/// Whether the order field takes `text` (`parseMediaGroupOrder` of the Solid field's
/// `onInput` and `onPaste`).
fn accepts_order(text: &str) -> bool {
    parse_media_group_order(text) != GroupOrderDraft::Invalid
}

/// What the tag editor was last given: its props are rebuilt only when one of these
/// changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct EditorStamp {
    tags: Vec<String>,
    disabled: bool,
    language: Option<Language>,
}

/// Everything one frame draws, read once.
struct Frame {
    theme: ThemeName,
    i18n: I18n<'static>,
    shown: Shown,
    kind: StageKind,
    active: bool,
    fullscreen: bool,
    narrow: bool,
    sidebar_open: bool,
    sidebar_occupied: bool,
    sidebar_shift: f32,
    media_inert: bool,
    rail_opacity: f32,
    controls_opacity: f32,
    /// The floating buttons are within the pointer's reach (`pointer-events`).
    controls_visible: bool,
    confirm_open: bool,
    deleting: bool,
    can_confirm: bool,
    confirm_error: Option<String>,
    info_open: bool,
    tag_invalid: bool,
    copy_confirmed: bool,
    can_copy: bool,
    copy_title: String,
    content: StageContent,
    image_rect: Option<ImageRect>,
    zoomed: bool,
    dragging: bool,
    viewport_width: f32,
    viewport_height: f32,
    shell_width: f32,
}

pub struct LightboxView {
    runtime: Entity<Runtime>,
    thumbnails: Entity<Thumbnails>,
    originals: Entity<Originals>,
    modal: Entity<ModalState>,
    session: Option<Session>,
    shown: Option<Shown>,
    /// What was asked of the window's fullscreen. It outlives the session: the
    /// window may answer after the lightbox closed.
    window_fullscreen: WindowFullscreen,
    editor: Entity<TagEditor>,
    editor_stamp: EditorStamp,
    /// The tag draft the editor's field was last known to show.
    seen_draft: String,
    group_key: Entity<TextInput>,
    group_order: Entity<TextInput>,
    confirm_input: Entity<TextInput>,
    sidebar_close_focus: FocusHandle,
    trigger_focus: FocusHandle,
    delete_focus: FocusHandle,
    /// The side panel as a whole: what it contains has the focus or not. No tab stop.
    sidebar_focus: FocusHandle,
    /// When the view was made, on the executor's clock: the origin of every `now`.
    started: Instant,
    /// Draws again at the session's next deadline.
    timer: Option<Task<()>>,
    timer_at: Option<Duration>,
    /// The last pointer press found the floating buttons over the media faded.
    pressed_faded: bool,
    /// Where the stage was last laid out in the window.
    stage_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The steps of a comparison scenario that are still to be made.
    script: Option<ScenarioLightbox>,
    /// A comparison scenario opened this lightbox: its controls never fade and
    /// nothing takes the focus, as a capture of the Tauri window shows it.
    scripted: bool,
    /// The notice of the last accepted deletion that had one.
    last_notice: Option<DeleteNotice>,
    language: Language,
    _subscriptions: Vec<Subscription>,
}

impl LightboxView {
    pub fn new(runtime: Entity<Runtime>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (i18n, language, thumbnails) = {
            let runtime = runtime.read(cx);
            (
                runtime.i18n(),
                runtime.model().language(),
                runtime.thumbnails().clone(),
            )
        };
        let originals = cx.new(|_| Originals::new());
        let modal = cx.new(|cx| ModalState::with_motion(ModalMotion::LIGHTBOX, cx));
        let editor = cx.new(|cx| {
            TagEditor::new(
                runtime.clone(),
                TagEditorProps {
                    variant: TagEditorVariant::Lightbox,
                    placement: SuggestionPlacement::default(),
                    disabled: true,
                    ..TagEditorProps::default()
                },
                window,
                cx,
            )
        });
        let field = |cx: &mut Context<Self>, height: f32, window: &mut Window| {
            cx.new(|cx| {
                let mut input = TextInput::themed(window, cx);
                input.set_chrome(FieldChrome::Boxed, cx);
                input.set_height(Some(height), cx);
                input
            })
        };
        let group_key = field(cx, layout::sidebar::FIELD_HEIGHT, window);
        let group_order = field(cx, layout::sidebar::FIELD_HEIGHT, window);
        group_order.update(cx, |input, _| input.set_accept(Some(accepts_order)));
        let confirm_input = field(cx, layout::confirm::FIELD_HEIGHT, window);
        // `input input-sm … border border-base-content/20 bg-base-100/70`: a bordered
        // field on the dialog's own background.
        confirm_input.update(cx, |input, cx| input.set_chrome(FieldChrome::Outlined, cx));
        let sidebar_focus = cx.focus_handle();
        let dialog_focus = modal.focus_handle(cx);

        let subscriptions = vec![
            cx.on_focus_in(&sidebar_focus, window, |this, window, cx| {
                this.sidebar_focused(window, cx)
            }),
            cx.on_focus_in(&dialog_focus, window, |this, window, cx| {
                this.dialog_focused(window, cx)
            }),
            cx.observe_in(&runtime, window, |this, _, window, cx| {
                this.sync(window, cx)
            }),
            // The picture arrived or failed.
            cx.observe_in(&originals, window, |this, _, window, cx| {
                this.apply(window, cx);
                this.after_change(window, cx);
            }),
            // The poster of a video arrived.
            cx.observe(&thumbnails, |this, _, cx| {
                if this.session.is_some() {
                    cx.notify();
                }
            }),
            cx.observe(&modal, |_, _, cx| cx.notify()),
            cx.subscribe_in(&modal, window, |this, _, event, window, cx| {
                if *event == ModalEvent::Dismiss {
                    this.escape(window, cx);
                }
            }),
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| {
                this.editor_event(event, window, cx)
            }),
            cx.subscribe_in(&group_key, window, |this, input, event, _, cx| {
                if *event != TextInputEvent::Changed {
                    return;
                }
                let text = input.read(cx).text().to_owned();
                if this
                    .shown
                    .as_ref()
                    .is_some_and(|shown| shown.group_key != text)
                {
                    this.command(LightboxCommand::SetMediaGroupKeyDraft(text), cx);
                }
            }),
            cx.subscribe_in(&group_order, window, |this, input, event, _, cx| {
                if *event != TextInputEvent::Changed {
                    return;
                }
                let text = input.read(cx).text().to_owned();
                if this
                    .shown
                    .as_ref()
                    .is_some_and(|shown| shown.group_order != text)
                {
                    this.command(LightboxCommand::SetMediaGroupOrderDraft(text), cx);
                }
            }),
            cx.subscribe_in(&confirm_input, window, |this, input, event, window, cx| {
                if *event != TextInputEvent::Changed {
                    return;
                }
                let text = input.read(cx).text().to_owned();
                if let Some(session) = &mut this.session {
                    session.set_confirm_text(text);
                }
                this.after_change(window, cx);
            }),
            cx.observe_window_bounds(window, |this, window, cx| this.window_changed(window, cx)),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.pointer_released(cx);
                }
            }),
            // Every key of this window shows the controls, before anything takes it
            // (the capturing `keydown` listener of `createLightboxActivity`). A key
            // listener of the dialog would miss the keys a binding handles: GPUI runs
            // no key listener once an action took the keystroke.
            {
                let view = cx.weak_entity();
                let own_window = window.window_handle();
                cx.intercept_keystrokes(move |_, window, cx| {
                    if window.window_handle() == own_window {
                        view.update(cx, |view, cx| view.activity(window, cx)).ok();
                    }
                })
            },
        ];
        // A view that goes away with its window shows no poster any more.
        cx.on_release(|this: &mut Self, cx| {
            this.thumbnails
                .update(cx, |thumbnails, _| thumbnails.sync_poster(None));
        })
        .detach();
        let mut view = Self {
            runtime,
            thumbnails,
            originals,
            modal,
            session: None,
            shown: None,
            window_fullscreen: WindowFullscreen::new(),
            editor,
            editor_stamp: EditorStamp::default(),
            seen_draft: String::new(),
            group_key,
            group_order,
            confirm_input,
            sidebar_close_focus: cx.focus_handle().tab_stop(true),
            trigger_focus: cx.focus_handle().tab_stop(true),
            delete_focus: cx.focus_handle().tab_stop(true),
            sidebar_focus,
            started: cx.background_executor().now(),
            timer: None,
            timer_at: None,
            pressed_faded: false,
            stage_bounds: Rc::default(),
            script: None,
            scripted: false,
            last_notice: None,
            language,
            _subscriptions: subscriptions,
        };
        view.apply_texts(&i18n, cx);
        view.sync(window, cx);
        view
    }

    /// The session of the open lightbox; also while it fades out.
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// What the lightbox draws of the model; the last values while it fades out.
    pub fn shown(&self) -> Option<&Shown> {
        self.shown.as_ref()
    }

    /// Whether an asset is selected and the lightbox takes input.
    pub fn is_open(&self) -> bool {
        self.session.as_ref().is_some_and(Session::is_active)
    }

    /// The state of the dialog's layer and fade.
    pub fn modal(&self) -> &Entity<ModalState> {
        &self.modal
    }

    /// The loader of the shown picture.
    pub fn originals(&self) -> &Entity<Originals> {
        &self.originals
    }

    /// The tag editor of the side panel.
    pub fn editor(&self) -> &Entity<TagEditor> {
        &self.editor
    }

    pub fn group_key_input(&self) -> &Entity<TextInput> {
        &self.group_key
    }

    pub fn group_order_input(&self) -> &Entity<TextInput> {
        &self.group_order
    }

    /// The field of the delete confirmation.
    pub fn confirm_input(&self) -> &Entity<TextInput> {
        &self.confirm_input
    }

    /// The focus handle of an element the session sends the focus to.
    pub fn focus_handle_of(&self, target: FocusTarget, cx: &App) -> FocusHandle {
        match target {
            FocusTarget::SidebarClose => self.sidebar_close_focus.clone(),
            FocusTarget::SidebarTrigger => self.trigger_focus.clone(),
            FocusTarget::Shell => self.modal.focus_handle(cx),
            FocusTarget::ConfirmInput => self.confirm_input.focus_handle(cx),
            FocusTarget::DeleteButton => self.delete_focus.clone(),
            FocusTarget::TagInput => self.editor.focus_handle(cx),
        }
    }

    /// Whether the focus is on something inside the side panel.
    pub fn sidebar_has_focus(&self, window: &Window, cx: &App) -> bool {
        self.sidebar_focus.contains_focused(window, cx)
    }

    /// The focus came into the side panel. A panel that slides out takes none
    /// (`inert` on the closed `LightboxToolbar`): its controls are drawn until the
    /// slide has ended, and Tab would stop on them. The focus goes on to the reopen
    /// button, where a collapse with the keyboard puts it.
    fn sidebar_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let closed = self
            .session
            .as_ref()
            .is_some_and(|session| session.is_active() && !session.sidebar_visible());
        if closed {
            window.focus(&self.trigger_focus, cx);
        }
    }

    /// The focus came into the dialog. A lightbox that fades out takes none (`inert`
    /// on the closing shell): its controls are drawn until the fade has ended, and
    /// Tab would stop on them. The focus goes back to the window's root, where the
    /// closing dialog left it.
    fn dialog_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fading = self
            .session
            .as_ref()
            .is_some_and(|session| !session.is_active());
        if fading {
            layers::focus_root(window, cx);
        }
    }

    /// The focused element left the window's frame, and the shell let the focus fall
    /// back to the window's root. When that element was a control of this dialog
    /// that is disabled or gone by now (the heart while its write runs, Apply, the
    /// confirmation's field during a deletion), the root is the wrong place: it is
    /// outside the dialog's key context, so the arrows and the viewer's keys would
    /// reach nothing until Tab brought the focus back. The keys of the Solid
    /// lightbox are the window's, whatever is focused; here the dialog takes the
    /// focus while the lightbox is the top layer.
    pub fn focus_lost(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_open() && self.is_top(cx) {
            let dialog = self.modal.focus_handle(cx);
            window.focus(&dialog, cx);
        }
    }

    /// The notice of the last accepted deletion that had one (a missing source, a
    /// pending cleanup), as it was announced.
    pub fn last_notice(&self) -> Option<&DeleteNotice> {
        self.last_notice.as_ref()
    }

    /// The clock every call of the session is made with: the time since the view was
    /// made, on the executor's clock. Public so that tests can ask the session what it
    /// shows now.
    pub fn now(&self, cx: &App) -> Duration {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.started)
    }

    fn command(&self, command: LightboxCommand, cx: &mut Context<Self>) {
        self.runtime.update(cx, |runtime, cx| {
            runtime.send(RuntimeInput::lightbox(command), cx)
        });
    }

    fn is_top(&self, cx: &App) -> bool {
        self.modal
            .read(cx)
            .layer()
            .is_some_and(|layer| layers::is_top(layer, cx))
    }

    /// Brings the view in line with the runtime: the answers to its deletions, then
    /// what is selected.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        self.open_scripted(cx);
        let events = self
            .runtime
            .update(cx, |runtime, _| runtime.take_lightbox_events());
        for event in events {
            let LightboxEvent::DeleteSettled { request, result } = event;
            let notice = match &mut self.session {
                Some(session) => session.delete_settled(request, &result, now),
                None => DeleteNotice::of(&result),
            };
            if let Some(notice) = notice {
                self.announce(notice, window, cx);
            }
        }
        let shown = Shown::of(&self.runtime.read(cx).model().lightbox());
        match shown {
            Some(shown) => {
                if !self.is_open() {
                    self.open_session(window, cx);
                }
                if let Some(session) = &mut self.session {
                    session.sync(&shown, now);
                    // An image's fullscreen does not follow the selection to a video.
                    if session.sync_fullscreen(shown.kind(), window.is_fullscreen()) {
                        self.request_window_fullscreen(window, cx);
                    }
                }
                self.shown = Some(shown);
            }
            None => self.close_session(window, cx),
        }
        self.apply(window, cx);
        self.after_change(window, cx);
    }

    /// The click of a comparison scenario on its tile, once the tile's row is there.
    fn open_scripted(&mut self, cx: &mut Context<Self>) {
        if self.scripted {
            return;
        }
        let Some(script) = self
            .runtime
            .update(cx, |runtime, _| runtime.take_scripted_lightbox())
        else {
            return;
        };
        let asset = self
            .runtime
            .read(cx)
            .model()
            .query()
            .asset_at(script.index)
            .cloned();
        let Some(asset) = asset else {
            return;
        };
        self.scripted = true;
        self.script = Some(script);
        self.command(
            LightboxCommand::Select {
                asset: Box::new(asset),
                index: Some(script.index),
            },
            cx,
        );
    }

    fn open_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let width = f32::from(window.viewport_size().width);
        let mut session = Session::new(is_narrow(width), cx.reduce_motion(), false, now);
        if self.scripted {
            session.pin_controls();
        }
        self.session = Some(session);
        // The new session's viewer knows no viewport: a lightbox reopened while the
        // old one still fades out has its stage where it was, so the stage reports
        // its bounds again.
        self.stage_bounds.set(None);
        self.seen_draft.clear();
        self.editor
            .update(cx, |editor, cx| editor.set_draft("", window, cx));
        self.confirm_input
            .update(cx, |input, cx| input.set_text("", cx));
        self.modal
            .update(cx, |modal, cx| modal.set_open(true, window, cx));
    }

    /// The lightbox closed logically: it fades out showing what it showed last.
    fn close_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        if !session.is_active() {
            return;
        }
        if session.deactivate(window.is_fullscreen(), now) {
            self.request_window_fullscreen(window, cx);
        }
        self.thumbnails
            .update(cx, |thumbnails, _| thumbnails.sync_poster(None));
        self.modal
            .update(cx, |modal, cx| modal.set_open(false, window, cx));
    }

    /// The fade-out has ended: the session, the shown values and the picture go.
    fn retire(&mut self, cx: &mut Context<Self>) {
        if self.session.is_none() && self.shown.is_none() {
            return;
        }
        self.session = None;
        self.shown = None;
        self.timer = None;
        self.timer_at = None;
        self.stage_bounds.set(None);
        self.originals
            .update(cx, |originals, cx| originals.show(None, None, cx));
    }

    /// Announces what an accepted deletion has to tell (`window.alert` in the Solid
    /// lightbox): the window's own prompt with one button.
    fn announce(&mut self, notice: DeleteNotice, window: &mut Window, cx: &mut Context<Self>) {
        let i18n = self.runtime.read(cx).i18n();
        let text = notice.text(&i18n);
        let close = i18n.t("common.close");
        self.last_notice = Some(notice);
        // The answer is not needed: the prompt closes with its button.
        drop(window.prompt(PromptLevel::Info, &text, None, &[close.as_str()], cx));
    }

    /// The fixed texts of the fields, in the runtime's language.
    fn apply_texts(&mut self, i18n: &I18n<'_>, cx: &mut Context<Self>) {
        self.group_key.update(cx, |input, cx| {
            input.set_placeholder(i18n.t("lightbox.mediaGroupKeyPlaceholder"), cx);
            input.set_label(i18n.t("lightbox.mediaGroupKeyAria"), cx);
        });
        self.group_order.update(cx, |input, cx| {
            input.set_placeholder(i18n.t("lightbox.mediaGroupOrderPlaceholder"), cx);
            input.set_label(i18n.t("lightbox.mediaGroupOrderAria"), cx);
        });
        let word = Params::new().with("value", i18n.t("lightbox.deleteConfirm.confirmWord"));
        self.confirm_input.update(cx, |input, cx| {
            input.set_placeholder(
                i18n.t_with("lightbox.deleteConfirm.typeYesPlaceholder", &word),
                cx,
            );
            input.set_label(
                i18n.t_with("lightbox.deleteConfirm.typeYesLabel", &word),
                cx,
            );
        });
    }

    /// Gives the picture loader, the thumbnails, the viewer, the tag editor and the
    /// fields what the model and the session say now.
    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (i18n, language) = {
            let runtime = self.runtime.read(cx);
            (runtime.i18n(), runtime.model().language())
        };
        if std::mem::replace(&mut self.language, language) != language {
            self.apply_texts(&i18n, cx);
        }
        let (Some(shown), Some(session)) = (self.shown.clone(), self.session.as_mut()) else {
            return;
        };
        let active = session.is_active();
        let kind = shown.kind();
        let summary = &shown.selected.summary;

        if active {
            let source = MediaSource::of(&shown.selected);
            let stored = stored_dimensions(summary.width, summary.height);
            self.originals
                .update(cx, |originals, cx| originals.show(source, stored, cx));
            let poster = kind.is_video().then(|| TileRequest {
                key: thumbnail_key(summary),
                visible: true,
            });
            self.thumbnails
                .update(cx, |thumbnails, _| thumbnails.sync_poster(poster));
        }
        let content = self.stage_content(&shown, cx);
        if let Some(session) = &mut self.session {
            let controls = session.controls_mut();
            controls.set_mounted(content.is_picture());
            if content.is_picture() {
                if let Some((width, height)) = self.originals.read(cx).size() {
                    controls.loaded(f64::from(width), f64::from(height));
                }
            }
        }
        self.run_script(&shown, content, cx);

        let Some(session) = self.session.as_ref() else {
            return;
        };
        let stamp = EditorStamp {
            tags: shown.tags.clone(),
            disabled: shown.tag_editing_disabled || !active,
            language: Some(language),
        };
        if stamp != self.editor_stamp {
            let remove_i18n = i18n;
            let props = TagEditorProps {
                variant: TagEditorVariant::Lightbox,
                tags: stamp.tags.clone(),
                empty_text: None,
                loading_text: None,
                disabled: stamp.disabled,
                input_label: i18n.t("lightbox.addTag").into(),
                placeholder: i18n.t("lightbox.tagInputPlaceholder").into(),
                remove_label: Some(Rc::new(move |tag: &str| {
                    SharedString::from(
                        remove_i18n
                            .t_with("lightbox.removeTagAria", &Params::new().with("tag", tag)),
                    )
                }) as RemoveLabel),
                placement: SuggestionPlacement::default(),
            };
            self.editor_stamp = stamp;
            self.editor
                .update(cx, |editor, cx| editor.set_props(props, window, cx));
        }
        if session.draft() != self.seen_draft {
            self.seen_draft = session.draft().to_owned();
            let draft = self.seen_draft.clone();
            self.editor
                .update(cx, |editor, cx| editor.set_draft(&draft, window, cx));
        }
        let deleting = session.confirm().deleting();
        let confirm_text = session.confirm().text().to_owned();
        self.group_key.update(cx, |input, cx| {
            if input.text() != shown.group_key {
                input.set_text(&shown.group_key, cx);
            }
            input.set_disabled(!active, window, cx);
        });
        self.group_order.update(cx, |input, cx| {
            if input.text() != shown.group_order {
                input.set_text(&shown.group_order, cx);
            }
            input.set_disabled(!active, window, cx);
        });
        self.confirm_input.update(cx, |input, cx| {
            if input.text() != confirm_text {
                input.set_text(&confirm_text, cx);
            }
            input.set_disabled(deleting || !active, window, cx);
        });
    }

    /// What the stage draws for `shown` now.
    fn stage_content(&self, shown: &Shown, cx: &App) -> StageContent {
        let originals = self.originals.read(cx);
        let source = MediaSource::of(&shown.selected);
        // A picture that belongs to another source is not this asset's.
        let load = if originals.source() == source.as_ref() {
            originals.state()
        } else {
            LoadState::Loading
        };
        StageContent::of(
            shown.kind(),
            source.is_some(),
            shown.asset_details_failed,
            load,
        )
    }

    /// The steps of a comparison scenario after its click, made as its Tauri steps
    /// make them: once the details are read and the picture is on the stage.
    fn run_script(&mut self, shown: &Shown, content: StageContent, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let (Some(script), Some(session)) = (self.script, self.session.as_mut()) else {
            return;
        };
        if !session.is_active() || shown.tag_editing_disabled {
            return;
        }
        let zooms = script.zoom_in > 0 || script.drag != (0, 0);
        if zooms {
            let Some(viewport) = session.controls().viewport() else {
                return;
            };
            if !content.is_picture() {
                return;
            }
            let controls = session.controls_mut();
            for _ in 0..script.zoom_in {
                controls.zoom_in();
            }
            if script.drag != (0, 0) {
                let x = viewport.left + viewport.width / 2.0;
                let y = viewport.top + viewport.height / 2.0;
                if controls.pointer_down(true, x, y, now) {
                    controls
                        .pointer_move(x + f64::from(script.drag.0), y + f64::from(script.drag.1));
                    controls.pointer_up(now);
                }
            }
        } else if !shown.kind().is_video() && !content.is_picture() {
            return;
        }
        if let Some(word) = script.delete_confirm {
            session.open_delete_confirm(now);
            session.set_confirm_text(word);
            // The scenario gives nothing the focus.
            session.focus_requests(now);
        }
        self.script = None;
        cx.notify();
    }

    /// After anything changed: the focus the session asks for, the timer for its next
    /// deadline, and a redraw.
    fn after_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        // The bounds observer can run before the window knows its new state.
        self.settle_window_fullscreen(window, cx);
        if let Some(session) = &mut self.session {
            let requests = session.focus_requests(now);
            if !requests.is_empty() && !self.scripted {
                let generation = session.generation();
                cx.on_next_frame(window, move |this, window, cx| {
                    let current = this.session.as_ref().is_some_and(|session| {
                        session.is_active() && session.generation() == generation
                    });
                    if !current {
                        return;
                    }
                    for target in &requests {
                        let handle = this.focus_handle_of(*target, cx);
                        window.focus(&handle, cx);
                    }
                });
            }
        }
        self.arm_timer(window, cx);
        cx.notify();
    }

    /// Sets one timer for the session's next deadline, unless one is set that fires
    /// no later.
    fn arm_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let deadline = self
            .session
            .as_ref()
            .and_then(|session| session.deadline(now));
        let Some(deadline) = deadline else {
            self.timer = None;
            self.timer_at = None;
            return;
        };
        if self.timer_at.is_some_and(|at| at > now && at <= deadline) {
            return;
        }
        self.timer_at = Some(deadline);
        let wait = deadline.saturating_sub(now);
        self.timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update_in(cx, |this, window, cx| {
                this.timer_at = None;
                this.after_change(window, cx);
            })
            .ok();
        }));
    }

    /// The session asked for the window to be toggled: the window is asked for what
    /// the lightbox shows now, unless it still owes the answer to an earlier request;
    /// then [`Self::settle_window_fullscreen`] asks once it has answered.
    fn request_window_fullscreen(&mut self, window: &mut Window, cx: &App) {
        let now = self.now(cx);
        let wanted = self.session.as_ref().is_some_and(Session::is_fullscreen);
        if self
            .window_fullscreen
            .request(wanted, window.is_fullscreen(), now)
        {
            window.toggle_fullscreen();
        }
    }

    /// The window answered a request the lightbox has left behind (it went
    /// fullscreen after the lightbox stopped being so, or the other way round): it is
    /// toggled once more. Also with no session: the lightbox may have closed since.
    /// Not for ever: a request the window has not answered within
    /// `WINDOW_ANSWER_TIMEOUT` is forgotten, so a window the user makes fullscreen
    /// later stays so.
    fn settle_window_fullscreen(&mut self, window: &mut Window, cx: &App) {
        let now = self.now(cx);
        if self
            .window_fullscreen
            .window_changed(window.is_fullscreen(), now)
        {
            window.toggle_fullscreen();
        }
    }

    fn window_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        self.settle_window_fullscreen(window, cx);
        let kind = self.shown.as_ref().map(Shown::kind);
        if let (Some(session), Some(kind)) = (&mut self.session, kind) {
            if session.is_active() {
                session.set_narrow(is_narrow(f32::from(window.viewport_size().width)), now);
                session.set_reduced_motion(cx.reduce_motion());
                if session.sync_fullscreen(kind, window.is_fullscreen()) {
                    self.request_window_fullscreen(window, cx);
                }
            }
            self.after_change(window, cx);
        }
    }

    /// The stage was laid out at `bounds`.
    fn stage_measured(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = &mut self.session {
            session.controls_mut().set_viewport(Viewport {
                left: f64::from(f32::from(bounds.origin.x)),
                top: f64::from(f32::from(bounds.origin.y)),
                width: f64::from(f32::from(bounds.size.width)),
                height: f64::from(f32::from(bounds.size.height)),
            });
        }
        self.apply(window, cx);
        cx.notify();
    }

    // ----- exits -----

    /// Escape, with the lightbox as the top layer.
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        match session.escape(window.is_fullscreen(), now) {
            Dismissal::LeftFullscreen { toggle_window } => {
                if toggle_window {
                    self.request_window_fullscreen(window, cx);
                }
            }
            Dismissal::Close => self.command(LightboxCommand::Close, cx),
            Dismissal::Nothing | Dismissal::ConfirmClosed | Dismissal::SidebarClosed => {}
        }
        self.apply(window, cx);
        self.after_change(window, cx);
    }

    fn backdrop_pressed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let top = self.is_top(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        if session.backdrop_pressed(top, now) == Dismissal::Close {
            self.command(LightboxCommand::Close, cx);
        }
        self.after_change(window, cx);
    }

    /// The close buttons.
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.session.as_ref().is_some_and(Session::may_close) {
            self.command(LightboxCommand::Close, cx);
        }
    }

    // ----- the session's inputs -----

    /// Runs `change` on the active session and brings the view in line with it.
    fn update_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Session, Duration),
    ) {
        let now = self.now(cx);
        if let Some(session) = self.session.as_mut().filter(|session| session.is_active()) {
            change(session, now);
        }
        self.apply(window, cx);
        self.after_change(window, cx);
    }

    fn key(&mut self, key: ViewerKey, window: &mut Window, cx: &mut Context<Self>) {
        let top = self.is_top(cx);
        let kind = self.shown.as_ref().map(Shown::kind);
        let (Some(session), Some(kind)) = (&mut self.session, kind) else {
            cx.propagate();
            return;
        };
        match session.key(key, kind, top, window.is_fullscreen()) {
            KeyOutcome::Ignored => {
                cx.propagate();
                return;
            }
            KeyOutcome::Handled => {}
            KeyOutcome::Previous => self.command(LightboxCommand::Previous, cx),
            KeyOutcome::Next => self.command(LightboxCommand::Next, cx),
            KeyOutcome::ToggleWindowFullscreen => self.request_window_fullscreen(window, cx),
        }
        self.after_change(window, cx);
    }

    /// The fullscreen button of the action rail.
    fn toggle_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let kind = self.shown.as_ref().map(Shown::kind);
        let (Some(session), Some(kind)) = (&mut self.session, kind) else {
            return;
        };
        if session.toggle_fullscreen(kind, window.is_fullscreen())
            == KeyOutcome::ToggleWindowFullscreen
        {
            self.request_window_fullscreen(window, cx);
        }
        self.after_change(window, cx);
    }

    fn editor_event(
        &mut self,
        event: &TagEditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(session), Some(shown)) = (&mut self.session, &self.shown) else {
            return;
        };
        match event {
            TagEditorEvent::DraftChanged(text) => {
                self.seen_draft.clone_from(text);
                session.set_draft(text.clone());
            }
            TagEditorEvent::AddTag(tag) => {
                if let AddTag::Save(tags) = session.add_tag(tag, shown) {
                    let generation = session.generation();
                    self.command(LightboxCommand::SaveTags(tags), cx);
                    if !self.scripted {
                        // One frame later, when the field shows the cleared draft; not
                        // for another asset and not once the lightbox closed.
                        cx.on_next_frame(window, move |this, window, cx| {
                            let current = this.session.as_ref().is_some_and(|session| {
                                session.is_active() && session.generation() == generation
                            });
                            if current && !this.editor.read(cx).editing_disabled() {
                                let focus = this.editor.focus_handle(cx);
                                window.focus(&focus, cx);
                            }
                        });
                    }
                }
            }
            TagEditorEvent::RemoveTag(tag) => {
                if let Some(tags) = session.remove_tag(tag, shown) {
                    self.command(LightboxCommand::SaveTags(tags), cx);
                }
            }
        }
        self.apply(window, cx);
        self.after_change(window, cx);
    }

    /// The copy button of the group name.
    fn copy_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        if let Some(request) = session.begin_copy() {
            cx.write_to_clipboard(ClipboardItem::new_string(request.text.clone()));
            session.finish_copy(&request, true, now);
        }
        self.after_change(window, cx);
    }

    /// Confirm of the delete confirmation.
    fn submit_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let word = self
            .runtime
            .read(cx)
            .i18n()
            .t("lightbox.deleteConfirm.confirmWord");
        let request = self
            .session
            .as_mut()
            .and_then(|session| session.submit_delete(&word, now));
        if let Some(request) = request {
            self.command(LightboxCommand::DeleteSelected { request }, cx);
        }
        self.apply(window, cx);
        self.after_change(window, cx);
    }

    // ----- pointer and activity -----

    fn activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        let before = session.controls_visible(now);
        session.input(now);
        if before != session.controls_visible(now) {
            cx.notify();
        }
        self.arm_timer(window, cx);
    }

    fn pointer_pressed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        let before = session.controls_visible(now);
        self.pressed_faded = !before;
        session.pointer_pressed(false, now);
        if before != session.controls_visible(now) {
            cx.notify();
        }
        self.arm_timer(window, cx);
    }

    /// Whether a click on a floating button over the media (reopen, close) counts.
    /// The faded buttons are out of the pointer's reach (`pointer-events: none` in
    /// `LightboxModal.tsx`): a press that finds them faded shows them, as any press
    /// does, and its click is not theirs. The keyboard activates a focused one
    /// whether or not it is shown.
    fn floating_click_counts(&self, event: &ClickEvent) -> bool {
        matches!(event, ClickEvent::Keyboard(_)) || !self.pressed_faded
    }

    fn pointer_moved(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        let before = session.controls_visible(now);
        session.pointer_moved(x, y, now);
        let dragged = session
            .controls_mut()
            .pointer_move(f64::from(x), f64::from(y));
        if dragged || before != session.controls_visible(now) {
            cx.notify();
        }
        self.arm_timer(window, cx);
    }

    fn pointer_released(&mut self, cx: &mut Context<Self>) {
        let now = self.now(cx);
        if let Some(session) = &mut self.session {
            if session.controls().is_dragging() {
                session.controls_mut().pointer_up(now);
                cx.notify();
            }
        }
    }

    /// Listens to the pointer of the whole window while the lightbox is drawn: its
    /// movement and presses show the controls, and a drag of the picture goes on and
    /// ends outside the stage.
    fn pointer_listeners(&self, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let moved = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        moved
                            .update(cx, |view, cx| view.pointer_moved(event, window, cx))
                            .ok();
                    }
                });
                let pressed = view.clone();
                window.on_mouse_event(move |_: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        pressed
                            .update(cx, |view, cx| view.pointer_pressed(window, cx))
                            .ok();
                    }
                });
                let released = view.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        released
                            .update(cx, |view, cx| view.pointer_released(cx))
                            .ok();
                    }
                });
            },
        )
        .absolute()
        .size_0()
    }

    /// Reads everything one frame draws.
    fn frame(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Frame> {
        let now = self.now(cx);
        let shown = self.shown.clone()?;
        let content = self.stage_content(&shown, cx);
        let i18n = self.runtime.read(cx).i18n();
        let fading = self.modal.read(cx).is_fading(cx);
        let viewport = window.viewport_size();
        let (viewport_width, viewport_height) =
            (f32::from(viewport.width), f32::from(viewport.height));
        let session = self.session.as_mut()?;
        if session.animate(now) || fading {
            window.request_animation_frame();
        }
        let fullscreen = session.is_fullscreen();
        let shell_width = if fullscreen {
            viewport_width
        } else {
            layout::shell::size(viewport_width, viewport_height).0
        };
        let word = i18n.t("lightbox.deleteConfirm.confirmWord");
        Some(Frame {
            theme: theme::active(cx),
            kind: shown.kind(),
            active: session.is_active(),
            fullscreen,
            narrow: session.sidebar().narrow(),
            sidebar_open: session.sidebar().open(),
            sidebar_occupied: session.sidebar().occupied(now),
            sidebar_shift: session.sidebar_shift(now),
            media_inert: session.media_inert(),
            rail_opacity: session.rail_opacity(now),
            controls_opacity: session.controls_opacity(now),
            controls_visible: session.controls_visible(now),
            confirm_open: session.confirm().is_open(),
            deleting: session.confirm().deleting(),
            can_confirm: session.confirm().can_confirm(&word),
            confirm_error: session.confirm().error_text(&i18n),
            info_open: session.info_open(),
            tag_invalid: session.tag_invalid(),
            copy_confirmed: session.clipboard().confirmed(now),
            can_copy: session.clipboard().can_copy(),
            copy_title: i18n.t(session.clipboard().title_key(now)),
            content,
            image_rect: session.controls().image_rect(),
            zoomed: session.controls().is_zoomed(),
            dragging: session.controls().is_dragging(),
            viewport_width,
            viewport_height,
            shell_width,
            i18n,
            shown,
        })
    }
}

impl Render for LightboxView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Textures of pictures dropped since the last frame.
        self.originals
            .update(cx, |originals, cx| originals.release_textures(window, cx));
        if !self.modal.read(cx).is_present() {
            self.retire(cx);
            return div().into_any_element();
        }
        let Some(frame) = self.frame(window, cx) else {
            return div().into_any_element();
        };
        let tokens = frame.theme.tokens();
        let colors = &tokens.colors;
        let opacity = self.modal.read(cx).opacity(cx);
        let focus_handle = self.modal.focus_handle(cx);
        let video = frame.kind.is_video();
        let name = frame.shown.selected.summary.file_name.clone();

        let media_column = self.media_column(&frame, window, cx);
        let sidebar = self.sidebar(&frame, window, cx);

        let layout_row = div()
            .relative()
            .size_full()
            .flex()
            .overflow_hidden()
            .child(media_column)
            .children(sidebar);

        let (shell_width, shell_height) =
            layout::shell::size(frame.viewport_width, frame.viewport_height);
        let dialog = div()
            .id(DIALOG_SELECTOR)
            .debug_selector(|| DIALOG_SELECTOR.to_owned())
            .relative()
            .overflow_hidden()
            .key_context(key_context().as_str())
            .track_focus(&focus_handle)
            // A press inside the dialog never reaches the backdrop.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .map(|dialog| {
                if frame.fullscreen {
                    dialog.size_full().bg(color(colors.base_100))
                } else {
                    dialog
                        .w(px(shell_width))
                        .h(px(shell_height))
                        .rounded(px(if frame.viewport_width >= layout::BREAKPOINT_SM {
                            RADII.panel
                        } else {
                            RADII.surface
                        }))
                        .border_1()
                        .shadow(box_shadows(tokens.shadows.modal))
                        .map(|dialog| {
                            if video {
                                dialog
                                    .border_color(color(layout::video::SHELL_BORDER))
                                    .bg(color(layout::video::SURFACE))
                            } else {
                                dialog
                                    .border_color(color(colors.border_soft))
                                    .bg(color(colors.surface_solid))
                            }
                        })
                }
            })
            .when(frame.active, |dialog| {
                dialog
                    .role(Role::Dialog)
                    .aria_label(SharedString::from(frame.i18n.t_with(
                        "lightbox.previewDialog",
                        &Params::new().with("name", &name),
                    )))
                    .on_scroll(cx.listener(|this, _, window, cx| this.activity(window, cx)))
                    .on_action(cx.listener(|this, _: &Previous, window, cx| {
                        this.key(ViewerKey::Previous, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &Next, window, cx| {
                        this.key(ViewerKey::Next, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ToggleFullscreen, window, cx| {
                        this.key(ViewerKey::ToggleFullscreen, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ResetZoom, window, cx| {
                        this.key(ViewerKey::ResetZoom, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ZoomIn, window, cx| {
                        this.key(ViewerKey::ZoomIn, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ZoomOut, window, cx| {
                        this.key(ViewerKey::ZoomOut, window, cx)
                    }))
            })
            .child(layout_row)
            .when(frame.active, |dialog| {
                dialog.child(self.pointer_listeners(cx))
            })
            // Fading out: still drawn, no longer interactive (`inert`).
            .when(!frame.active, |dialog| {
                dialog.child(div().id("lightbox-inert").absolute().inset_0().occlude())
            });

        let overlay = div()
            .id(BACKDROP_SELECTOR)
            .debug_selector(|| BACKDROP_SELECTOR.to_owned())
            .occlude()
            .w(px(frame.viewport_width))
            .h(px(frame.viewport_height))
            .flex()
            .items_center()
            .justify_center()
            .p(px(if frame.fullscreen {
                0.0
            } else {
                layout::shell::backdrop_padding(frame.kind, frame.viewport_width)
            }))
            .bg(color(
                colors
                    .neutral
                    .with_alpha_percent(layout::shell::BACKDROP_PERCENT),
            ))
            .opacity(opacity)
            .when(frame.active, |overlay| {
                overlay.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.backdrop_pressed(window, cx)),
                )
            })
            .child(dialog);
        deferred(anchored().position(point(px(0.), px(0.))).child(overlay))
            .with_priority(1)
            .into_any_element()
    }
}

/// A canvas that reports where the stage was laid out.
fn measure_stage(
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    view: gpui::WeakEntity<LightboxView>,
) -> AnyElement {
    let known = bounds.get();
    canvas(
        move |laid_out, window, cx| {
            if known != Some(laid_out) {
                bounds.set(Some(laid_out));
                window.defer(cx, move |window, cx| {
                    view.update(cx, |view, cx| view.stage_measured(laid_out, window, cx))
                        .ok();
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}
