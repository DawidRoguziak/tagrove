//! The overlay layer on GPUI's test platform (epic E05), ported from
//! `src/ui/overlays/__tests__/UiModal.test.tsx` and `UiLayerProvider.test.tsx`: Escape
//! and the backdrop reach only the top layer, a modal takes the focus, keeps Tab inside
//! and gives the focus back, and it stays drawn while it fades out.

use std::time::Duration;

use gpui::{
    div, point, prelude::*, px, Context, Entity, FocusHandle, KeyUpEvent, Keystroke, Modifiers,
    MouseButton, Subscription, TestAppContext, VisualTestContext, Window,
};
use media_tagger_client::overlay::Phase;
use media_tagger_gpui::ui::{
    self,
    button::Button,
    layers,
    modal::{Modal, ModalEvent, ModalState},
};

/// A window with a trigger button and two dialogs, each with two buttons. A dismissed
/// dialog closes, as `onClose` closes the Solid dialogs.
struct Shell {
    trigger: FocusHandle,
    lower: Entity<ModalState>,
    upper: Entity<ModalState>,
    lower_buttons: [FocusHandle; 2],
    upper_buttons: [FocusHandle; 2],
    dismissed: Vec<&'static str>,
    /// The dialog buttons that were activated, by prefix and index.
    presses: Vec<(&'static str, usize)>,
    close_on_dismiss: bool,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lower = cx.new(ModalState::new);
        let upper = cx.new(ModalState::new);
        let mut subscriptions = Vec::new();
        for (modal, name) in [(&lower, "lower"), (&upper, "upper")] {
            subscriptions.push(cx.subscribe_in(
                modal,
                window,
                move |shell, modal, event, window, cx| {
                    if *event == ModalEvent::Dismiss {
                        shell.dismissed.push(name);
                        if shell.close_on_dismiss {
                            modal.update(cx, |modal, cx| modal.set_open(false, window, cx));
                        }
                    }
                },
            ));
        }
        Self {
            trigger: cx.focus_handle().tab_stop(true),
            lower,
            upper,
            lower_buttons: [
                cx.focus_handle().tab_stop(true),
                cx.focus_handle().tab_stop(true),
            ],
            upper_buttons: [
                cx.focus_handle().tab_stop(true),
                cx.focus_handle().tab_stop(true),
            ],
            dismissed: Vec::new(),
            presses: Vec::new(),
            close_on_dismiss: true,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shell = cx.entity().downgrade();
        let buttons = |prefix: &'static str, handles: &[FocusHandle; 2]| {
            let button = |index: usize, label: &'static str| {
                let shell = shell.clone();
                Button::new((prefix, index), label)
                    .track_focus(&handles[index])
                    .on_click(move |_, _, cx| {
                        shell
                            .update(cx, |shell, _| shell.presses.push((prefix, index)))
                            .ok();
                    })
            };
            div()
                .flex()
                .gap_2()
                .child(button(0, "first"))
                .child(button(1, "last"))
        };
        layers::root(cx)
            .size_full()
            .child(Button::new("trigger", "open").track_focus(&self.trigger))
            .child(
                Modal::new("lower", &self.lower)
                    .label("lower")
                    .child(buttons("lower-button", &self.lower_buttons)),
            )
            .child(
                Modal::new("upper", &self.upper)
                    .label("upper")
                    .child(buttons("upper-button", &self.upper_buttons)),
            )
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Shell>, VisualTestContext) {
    cx.update(ui::init);
    let window = cx.add_window(Shell::new);
    let shell = window.root(cx).expect("the shell");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    // As a window does when it opens.
    cx.update(layers::focus_root);
    cx.run_until_parked();
    (shell, cx)
}

/// Delivers the next frame, as the platform does after drawing (`requestAnimationFrame`
/// in the Solid tests).
fn next_frame(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    cx.run_until_parked();
}

fn set_open(shell: &Entity<Shell>, which: &str, open: bool, cx: &mut VisualTestContext) {
    let modal = shell.read_with(cx, |shell, _| {
        if which == "lower" {
            shell.lower.clone()
        } else {
            shell.upper.clone()
        }
    });
    cx.update(|window, cx| modal.update(cx, |modal, cx| modal.set_open(open, window, cx)));
    cx.run_until_parked();
}

fn focused(handle: &FocusHandle, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| handle.is_focused(window))
}

#[gpui::test]
fn escape_closes_only_the_top_dialog(cx: &mut TestAppContext) {
    let (shell, mut cx) = open(cx);
    set_open(&shell, "lower", true, &mut cx);
    set_open(&shell, "upper", true, &mut cx);
    next_frame(&mut cx);

    cx.simulate_keystrokes("escape");
    assert_eq!(
        shell.read_with(&cx, |shell, _| shell.dismissed.clone()),
        ["upper"]
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(
        shell.read_with(&cx, |shell, _| shell.dismissed.clone()),
        ["upper", "lower"]
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(
        shell.read_with(&cx, |shell, _| shell.dismissed.len()),
        2,
        "no layer left"
    );
}

#[gpui::test]
fn a_locked_top_dialog_keeps_escape_and_the_backdrop_from_every_layer(cx: &mut TestAppContext) {
    let (shell, mut cx) = open(cx);
    set_open(&shell, "lower", true, &mut cx);
    let upper = shell.read_with(&cx, |shell, _| shell.upper.clone());
    upper.update(&mut cx, |modal, cx| {
        modal.set_close_on_escape(false, cx);
        modal.set_close_on_overlay_click(false);
    });
    set_open(&shell, "upper", true, &mut cx);
    next_frame(&mut cx);

    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_down(point(px(4.), px(4.)), MouseButton::Left, Modifiers::none());
    assert!(shell.read_with(&cx, |shell, _| shell.dismissed.is_empty()));
}

#[gpui::test]
fn the_backdrop_dismisses_the_top_dialog_and_a_press_inside_does_not(cx: &mut TestAppContext) {
    let (shell, mut cx) = open(cx);
    shell.update(&mut cx, |shell, _| shell.close_on_dismiss = false);
    set_open(&shell, "lower", true, &mut cx);
    set_open(&shell, "upper", true, &mut cx);
    next_frame(&mut cx);

    let viewport = cx.update(|window, _| window.viewport_size());
    let centre = point(viewport.width / 2., viewport.height / 2.);
    cx.simulate_mouse_down(centre, MouseButton::Left, Modifiers::none());
    assert!(shell.read_with(&cx, |shell, _| shell.dismissed.is_empty()));

    cx.simulate_mouse_down(point(px(4.), px(4.)), MouseButton::Left, Modifiers::none());
    assert_eq!(
        shell.read_with(&cx, |shell, _| shell.dismissed.clone()),
        ["upper"]
    );
}

#[gpui::test]
fn a_dialog_takes_the_focus_traps_tab_and_restores_its_trigger(cx: &mut TestAppContext) {
    let (shell, mut cx) = open(cx);
    let (trigger, buttons) = shell.read_with(&cx, |shell, _| {
        (shell.trigger.clone(), shell.lower_buttons.clone())
    });
    cx.update(|window, cx| window.focus(&trigger, cx));
    set_open(&shell, "lower", true, &mut cx);
    next_frame(&mut cx);
    assert!(
        focused(&buttons[0], &mut cx),
        "the first tab stop takes the focus"
    );

    cx.simulate_keystrokes("shift-tab");
    assert!(focused(&buttons[1], &mut cx), "Shift+Tab wraps to the last");
    cx.simulate_keystrokes("tab");
    assert!(focused(&buttons[0], &mut cx), "Tab wraps to the first");
    cx.simulate_keystrokes("tab");
    assert!(focused(&buttons[1], &mut cx));
    cx.simulate_keystrokes("tab");
    assert!(
        focused(&buttons[0], &mut cx),
        "the trigger outside is never reached"
    );

    cx.simulate_keystrokes("escape");
    next_frame(&mut cx);
    assert!(
        focused(&trigger, &mut cx),
        "the focus goes back to the trigger"
    );
}

#[gpui::test]
fn closing_the_upper_dialog_restores_the_nested_trigger_during_its_exit(cx: &mut TestAppContext) {
    let (shell, mut cx) = open(cx);
    let (upper, nested) = shell.read_with(&cx, |shell, _| {
        (shell.upper.clone(), shell.lower_buttons[1].clone())
    });
    upper.update(&mut cx, |modal, _| {
        modal.set_restore_focus(Some(nested.clone()))
    });
    set_open(&shell, "lower", true, &mut cx);
    set_open(&shell, "upper", true, &mut cx);
    next_frame(&mut cx);

    set_open(&shell, "upper", false, &mut cx);
    next_frame(&mut cx);
    assert!(focused(&nested, &mut cx));
    assert_eq!(
        upper.read_with(&cx, |modal, _| modal.phase()),
        Phase::Closing
    );
    assert_eq!(
        cx.update(|_, cx| layers::layer_ids(cx).len()),
        1,
        "only the lower layer"
    );
}

#[gpui::test]
fn a_closing_dialog_stays_drawn_for_the_exit_and_ignores_the_backdrop(cx: &mut TestAppContext) {
    let (shell, mut cx) = open(cx);
    shell.update(&mut cx, |shell, _| shell.close_on_dismiss = false);
    set_open(&shell, "lower", true, &mut cx);
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    let lower = shell.read_with(&cx, |shell, _| shell.lower.clone());
    assert_eq!(lower.read_with(&cx, |modal, _| modal.phase()), Phase::Open);

    set_open(&shell, "lower", false, &mut cx);
    assert_eq!(
        lower.read_with(&cx, |modal, _| modal.phase()),
        Phase::Closing
    );
    cx.simulate_mouse_down(point(px(4.), px(4.)), MouseButton::Left, Modifiers::none());
    assert!(shell.read_with(&cx, |shell, _| shell.dismissed.is_empty()));

    cx.executor().advance_clock(Duration::from_millis(99));
    cx.run_until_parked();
    assert!(lower.read_with(&cx, |modal, _| modal.is_present()));
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert!(!lower.read_with(&cx, |modal, _| modal.is_present()));
}

/// A full key press: GPUI turns Enter or Space into a click on the key's release.
fn press(key: &'static str, cx: &mut VisualTestContext) {
    cx.simulate_keystrokes(key);
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse(key).expect("a key"),
    });
    cx.run_until_parked();
}

/// `inert` of the closing `UiModal` content: a dialog that fades out keeps no focus, so
/// Enter or Space during its exit activates none of its buttons, even when there is no
/// restore target to take the focus (the dialog opened while only the root had it).
#[gpui::test]
fn a_closing_dialog_takes_no_keys_while_it_fades_out(cx: &mut TestAppContext) {
    let (shell, mut cx) = open(cx);
    let buttons = shell.read_with(&cx, |shell, _| shell.lower_buttons.clone());
    set_open(&shell, "lower", true, &mut cx);
    next_frame(&mut cx);
    assert!(focused(&buttons[0], &mut cx), "the first tab stop");

    set_open(&shell, "lower", false, &mut cx);
    next_frame(&mut cx);
    let lower = shell.read_with(&cx, |shell, _| shell.lower.clone());
    assert_eq!(
        lower.read_with(&cx, |modal, _| modal.phase()),
        Phase::Closing
    );
    assert!(
        !focused(&buttons[0], &mut cx),
        "the focus left the closing dialog"
    );
    let root = cx.update(|_, cx| layers::root_focus_handle(cx));
    assert!(focused(&root, &mut cx), "the window's root keeps the keys");
    press("enter", &mut cx);
    press("space", &mut cx);
    assert!(
        shell.read_with(&cx, |shell, _| shell.presses.is_empty()),
        "no button of the closing dialog was activated"
    );
}

/// `inert` of the closing `UiModal` content, for Tab: while a dialog fades out, Tab
/// from the window's root passes over its buttons and reaches the page's own tab stop,
/// in both directions; once the dialog opens again its buttons are tab stops again.
#[gpui::test]
fn tab_passes_over_a_dialog_that_fades_out(cx: &mut TestAppContext) {
    let (shell, mut cx) = open(cx);
    let (trigger, buttons) = shell.read_with(&cx, |shell, _| {
        (shell.trigger.clone(), shell.lower_buttons.clone())
    });
    // Opened with the focus on the root: closing has no trigger to give the focus to.
    set_open(&shell, "lower", true, &mut cx);
    next_frame(&mut cx);
    assert!(focused(&buttons[0], &mut cx), "the first tab stop");

    set_open(&shell, "lower", false, &mut cx);
    next_frame(&mut cx);
    let lower = shell.read_with(&cx, |shell, _| shell.lower.clone());
    assert_eq!(
        lower.read_with(&cx, |modal, _| modal.phase()),
        Phase::Closing
    );
    let root = cx.update(|_, cx| layers::root_focus_handle(cx));
    assert!(focused(&root, &mut cx));

    cx.simulate_keystrokes("tab");
    assert!(
        focused(&trigger, &mut cx),
        "Tab reaches the page, not the fading dialog"
    );
    cx.simulate_keystrokes("tab");
    assert!(
        focused(&trigger, &mut cx),
        "the page's only tab stop keeps the focus"
    );
    cx.simulate_keystrokes("shift-tab");
    assert!(focused(&trigger, &mut cx), "and backwards");
    for button in &buttons {
        assert!(!focused(button, &mut cx));
    }

    // Opened again before the fade ended: a modal with its own tab stops.
    set_open(&shell, "lower", true, &mut cx);
    next_frame(&mut cx);
    assert!(focused(&buttons[0], &mut cx));
    cx.simulate_keystrokes("tab");
    assert!(focused(&buttons[1], &mut cx));

    // Once the dialog is gone nothing of it is remembered.
    set_open(&shell, "lower", false, &mut cx);
    next_frame(&mut cx);
    cx.executor().advance_clock(Duration::from_millis(500));
    next_frame(&mut cx);
    assert_eq!(
        lower.read_with(&cx, |modal, _| modal.phase()),
        Phase::Hidden
    );
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    assert!(focused(&trigger, &mut cx));
}

/// Ported from `UiLayerProvider.test.tsx` (with the settings page and the bulk mode as
/// nonmodal layers, as in `App.tsx`) and the rest of `UiModal.test.tsx`.
mod nonmodal_layers {
    use gpui::{
        actions, div, prelude::*, Context, Entity, FocusHandle, KeyBinding, Subscription,
        TestAppContext, VisualTestContext, Window,
    };
    use media_tagger_client::overlay::{LayerId, Phase};
    use media_tagger_gpui::ui::{
        self,
        button::Button,
        layers::{self, LayerOptions},
        modal::{Modal, ModalEvent, ModalState},
        text_input::TextInput,
    };

    use super::{focused, next_frame};

    actions!(nonmodal_layers, [OuterEscape]);

    #[derive(Clone)]
    enum Content {
        /// Two buttons; the second is the designated initial focus (`data-initial-focus`).
        Buttons,
        Field(Entity<TextInput>),
        /// No tab stop at all.
        Empty,
    }

    /// The application shell of the Solid test: an opener, the settings page (a
    /// nonmodal layer that Escape closes and that gives the focus back to the opener),
    /// the gallery with a tile (a nonmodal layer while the bulk mode is on), and a
    /// newer modal. Outside the layer root, a binding records the Escape no layer took.
    struct Page {
        opener: FocusHandle,
        back: FocusHandle,
        tile: FocusHandle,
        settings_open: bool,
        settings: Option<LayerId>,
        bulk_enabled: bool,
        bulk: Option<LayerId>,
        /// The gallery stays drawn under the settings page (else it is its fallback).
        gallery_always: bool,
        modal: Option<Entity<ModalState>>,
        content: Content,
        modal_first: FocusHandle,
        modal_action: FocusHandle,
        dismissed: usize,
        outer_escapes: usize,
        _subscriptions: Vec<Subscription>,
    }

    impl Page {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            // As every window does: focus that is lost falls back to the root.
            let focus_lost =
                cx.on_focus_lost(window, |_, window, cx| layers::focus_root(window, cx));
            Self {
                opener: cx.focus_handle().tab_stop(true),
                back: cx.focus_handle().tab_stop(true),
                tile: cx.focus_handle().tab_stop(true),
                settings_open: false,
                settings: None,
                bulk_enabled: false,
                bulk: None,
                gallery_always: false,
                modal: None,
                content: Content::Buttons,
                modal_first: cx.focus_handle().tab_stop(true),
                modal_action: cx.focus_handle().tab_stop(true),
                dismissed: 0,
                outer_escapes: 0,
                _subscriptions: vec![focus_lost],
            }
        }

        /// The bulk layer exists while the gallery is drawn and the bulk mode is on.
        fn sync_gallery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let wanted = self.bulk_enabled && (self.gallery_always || !self.settings_open);
            match (wanted, self.bulk) {
                (true, None) => {
                    self.bulk = Some(layers::register(
                        LayerOptions::nonmodal(|_, _| {}),
                        window,
                        cx,
                    ));
                }
                (false, Some(id)) => {
                    self.bulk = None;
                    layers::release(id, None, window, cx);
                }
                _ => {}
            }
            cx.notify();
        }

        fn set_bulk(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
            self.bulk_enabled = enabled;
            self.sync_gallery(window, cx);
        }

        fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            self.settings_open = true;
            self.sync_gallery(window, cx);
            let page = cx.entity().downgrade();
            self.settings = Some(layers::register(
                LayerOptions::nonmodal(move |window, cx| {
                    page.update(cx, |page, cx| page.close_settings(window, cx))
                        .ok();
                }),
                window,
                cx,
            ));
            cx.notify();
        }

        fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if let Some(id) = self.settings.take() {
                layers::release(id, Some(self.opener.clone()), window, cx);
            }
            self.settings_open = false;
            self.sync_gallery(window, cx);
        }

        /// Mounts the modal owner (closed) with `content`.
        fn mount_modal(&mut self, content: Content, window: &mut Window, cx: &mut Context<Self>) {
            let modal = cx.new(ModalState::new);
            if matches!(content, Content::Buttons) {
                let action = self.modal_action.clone();
                modal.update(cx, |modal, _| modal.set_initial_focus(Some(action)));
            }
            self._subscriptions.push(cx.subscribe_in(
                &modal,
                window,
                |page, modal, event, window, cx| {
                    if *event == ModalEvent::Dismiss {
                        page.dismissed += 1;
                        modal.update(cx, |modal, cx| modal.set_open(false, window, cx));
                    }
                },
            ));
            self.content = content;
            self.modal = Some(modal);
            cx.notify();
        }

        fn set_modal_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
            if self.modal.is_none() {
                self.mount_modal(Content::Buttons, window, cx);
            }
            let modal = self.modal.clone().expect("mounted");
            modal.update(cx, |modal, cx| modal.set_open(open, window, cx));
        }
    }

    impl Render for Page {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let content = match &self.content {
                Content::Buttons => div()
                    .flex()
                    .gap_2()
                    .child(Button::new("modal-first", "first").track_focus(&self.modal_first))
                    .child(Button::new("modal-action", "action").track_focus(&self.modal_action))
                    .into_any_element(),
                Content::Field(field) => div().w_64().child(field.clone()).into_any_element(),
                Content::Empty => div().child("text").into_any_element(),
            };
            div()
                .size_full()
                .key_context("Outer")
                .on_action(cx.listener(|page, _: &OuterEscape, _, cx| {
                    page.outer_escapes += 1;
                    cx.notify();
                }))
                .child(
                    layers::root(cx)
                        .size_full()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(Button::new("opener", "open settings").track_focus(&self.opener))
                        .when(self.settings_open, |page| {
                            page.child(Button::new("back", "back").track_focus(&self.back))
                        })
                        .when(self.gallery_always || !self.settings_open, |page| {
                            page.child(Button::new("tile", "tile").track_focus(&self.tile))
                        })
                        .when_some(self.modal.clone(), |page, modal| {
                            page.child(Modal::new("newer", &modal).label("modal").child(content))
                        }),
                )
        }
    }

    fn open(cx: &mut TestAppContext) -> (Entity<Page>, VisualTestContext) {
        cx.update(|cx| {
            ui::init(cx);
            cx.bind_keys([KeyBinding::new("escape", OuterEscape, Some("Outer"))]);
        });
        let window = cx.add_window(Page::new);
        let page = window.root(cx).expect("the page");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(layers::focus_root);
        cx.run_until_parked();
        (page, cx)
    }

    fn with_page(
        page: &Entity<Page>,
        cx: &mut VisualTestContext,
        f: impl FnOnce(&mut Page, &mut Window, &mut Context<Page>),
    ) {
        cx.update(|window, cx| page.update(cx, |page, cx| f(page, window, cx)));
        cx.run_until_parked();
    }

    fn focus(handle: &FocusHandle, cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.focus(handle, cx));
        cx.run_until_parked();
    }

    fn handles(page: &Entity<Page>, cx: &mut VisualTestContext) -> [FocusHandle; 4] {
        page.read_with(cx, |page, _| {
            [
                page.opener.clone(),
                page.back.clone(),
                page.tile.clone(),
                page.modal_action.clone(),
            ]
        })
    }

    /// The `shell` of the Solid test: the opener has the focus, the settings page
    /// opens (unmounting the gallery and its bulk layer), and its Back button takes the
    /// focus.
    fn shell(bulk: bool, cx: &mut TestAppContext) -> (Entity<Page>, VisualTestContext) {
        let (page, mut cx) = open(cx);
        let [opener, back, ..] = handles(&page, &mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_bulk(bulk, window, cx)
        });
        focus(&opener, &mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.open_settings(window, cx)
        });
        next_frame(&mut cx);
        focus(&back, &mut cx);
        (page, cx)
    }

    fn restores_the_settings_opener_on_close(bulk: bool, cx: &mut TestAppContext) {
        let (page, mut cx) = shell(bulk, cx);
        let [opener, back, ..] = handles(&page, &mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.close_settings(window, cx)
        });
        next_frame(&mut cx);
        assert!(!focused(&back, &mut cx));
        assert!(focused(&opener, &mut cx), "bulk mode {bulk}");
        assert_eq!(
            cx.update(|_, cx| layers::layer_ids(cx).len()),
            usize::from(bulk),
            "only the bulk layer is left"
        );
    }

    #[gpui::test]
    fn restores_the_settings_opener_on_close_without_bulk_mode(cx: &mut TestAppContext) {
        restores_the_settings_opener_on_close(false, cx);
    }

    #[gpui::test]
    fn restores_the_settings_opener_on_close_in_bulk_mode(cx: &mut TestAppContext) {
        restores_the_settings_opener_on_close(true, cx);
    }

    #[gpui::test]
    fn escape_closes_the_settings_page_and_gives_the_focus_back(cx: &mut TestAppContext) {
        let (page, mut cx) = shell(false, cx);
        let [opener, ..] = handles(&page, &mut cx);
        cx.simulate_keystrokes("escape");
        assert!(!page.read_with(&cx, |page, _| page.settings_open));
        assert_eq!(
            page.read_with(&cx, |page, _| page.outer_escapes),
            0,
            "the layer took it"
        );
        next_frame(&mut cx);
        assert!(focused(&opener, &mut cx));
    }

    #[gpui::test]
    fn restores_when_a_nonmodal_layer_registers_before_the_settings_layer_releases(
        cx: &mut TestAppContext,
    ) {
        let (page, mut cx) = open(cx);
        let [opener, back, ..] = handles(&page, &mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.gallery_always = true;
            page.open_settings(window, cx);
        });
        next_frame(&mut cx);
        focus(&back, &mut cx);

        with_page(&page, &mut cx, |page, window, cx| {
            page.set_bulk(true, window, cx)
        });
        with_page(&page, &mut cx, |page, window, cx| {
            page.close_settings(window, cx)
        });
        next_frame(&mut cx);
        assert!(focused(&opener, &mut cx));
    }

    #[gpui::test]
    fn does_not_steal_focus_that_stayed_below_a_nonmodal_layer(cx: &mut TestAppContext) {
        let (page, mut cx) = open(cx);
        let [opener, _, tile, _] = handles(&page, &mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.gallery_always = true;
            page.open_settings(window, cx);
        });
        next_frame(&mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_bulk(true, window, cx)
        });
        focus(&tile, &mut cx);

        with_page(&page, &mut cx, |page, window, cx| {
            page.close_settings(window, cx)
        });
        next_frame(&mut cx);
        assert!(focused(&tile, &mut cx));
        assert!(!focused(&opener, &mut cx));
    }

    #[gpui::test]
    fn skips_the_restore_when_a_newer_modal_is_on_top(cx: &mut TestAppContext) {
        let (page, mut cx) = shell(true, cx);
        let [opener, _, _, action] = handles(&page, &mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.mount_modal(Content::Buttons, window, cx)
        });
        with_page(&page, &mut cx, |page, window, cx| {
            page.close_settings(window, cx);
            page.set_modal_open(true, window, cx);
        });
        next_frame(&mut cx);
        assert!(
            focused(&action, &mut cx),
            "the designated element of the modal"
        );
        assert!(!focused(&opener, &mut cx));
    }

    #[gpui::test]
    fn does_not_restore_a_layer_released_beneath_an_open_modal(cx: &mut TestAppContext) {
        let (page, mut cx) = shell(false, cx);
        let [_, _, _, action] = handles(&page, &mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        next_frame(&mut cx);
        assert!(focused(&action, &mut cx));

        cx.simulate_keystrokes("escape");
        assert_eq!(
            page.read_with(&cx, |page, _| page.dismissed),
            1,
            "the modal is on top"
        );
        assert!(
            page.read_with(&cx, |page, _| page.settings_open),
            "the settings page below stays"
        );

        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        next_frame(&mut cx);
        assert!(focused(&action, &mut cx));
        with_page(&page, &mut cx, |page, window, cx| {
            page.close_settings(window, cx)
        });
        next_frame(&mut cx);
        assert!(focused(&action, &mut cx));
    }

    /// Port of "does not render closed" and "supports Escape ... and removes the
    /// listener": a closed modal has no layer, and a dropped one leaves none behind.
    #[gpui::test]
    fn a_closed_or_dropped_modal_holds_no_layer_and_escape_goes_on(cx: &mut TestAppContext) {
        let (page, mut cx) = open(cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.mount_modal(Content::Buttons, window, cx)
        });
        let modal = page
            .read_with(&cx, |page, _| page.modal.clone())
            .expect("mounted");
        assert!(!modal.read_with(&cx, |modal, _| modal.is_present()));
        assert!(cx.update(|_, cx| layers::layer_ids(cx).is_empty()));
        cx.simulate_keystrokes("escape");
        assert_eq!(page.read_with(&cx, |page, _| page.dismissed), 0);
        assert_eq!(
            page.read_with(&cx, |page, _| page.outer_escapes),
            1,
            "Escape that closes nothing goes on"
        );

        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        assert_eq!(cx.update(|_, cx| layers::layer_ids(cx).len()), 1);
        drop(modal);
        with_page(&page, &mut cx, |page, _, cx| {
            page.modal = None;
            page._subscriptions.truncate(1);
            cx.notify();
        });
        next_frame(&mut cx);
        assert!(
            cx.update(|_, cx| layers::layer_ids(cx).is_empty()),
            "the dropped modal released its layer"
        );
        cx.simulate_keystrokes("escape");
        assert_eq!(page.read_with(&cx, |page, _| page.dismissed), 0);
        assert_eq!(page.read_with(&cx, |page, _| page.outer_escapes), 2);
    }

    #[gpui::test]
    fn escape_ignored_by_a_locked_top_dialog_goes_on_but_closes_no_layer(cx: &mut TestAppContext) {
        let (page, mut cx) = shell(false, cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.mount_modal(Content::Buttons, window, cx)
        });
        let modal = page
            .read_with(&cx, |page, _| page.modal.clone())
            .expect("mounted");
        modal.update(&mut cx, |modal, cx| modal.set_close_on_escape(false, cx));
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        next_frame(&mut cx);

        cx.simulate_keystrokes("escape");
        assert_eq!(page.read_with(&cx, |page, _| page.dismissed), 0);
        assert!(
            page.read_with(&cx, |page, _| page.settings_open),
            "the settings page below does not close"
        );
        assert_eq!(page.read_with(&cx, |page, _| page.outer_escapes), 1);

        modal.update(&mut cx, |modal, cx| modal.set_close_on_escape(true, cx));
        cx.simulate_keystrokes("escape");
        assert_eq!(
            page.read_with(&cx, |page, _| page.dismissed),
            1,
            "closeOnEscape is read when the key arrives"
        );
    }

    #[gpui::test]
    fn escape_in_a_text_field_of_a_dialog_closes_the_dialog(cx: &mut TestAppContext) {
        let (page, mut cx) = open(cx);
        let field = cx.update(|window, cx| cx.new(|cx| TextInput::themed(window, cx)));
        let content = Content::Field(field.clone());
        with_page(&page, &mut cx, |page, window, cx| {
            page.mount_modal(content, window, cx)
        });
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        next_frame(&mut cx);
        let field_focus = field.read_with(&cx, gpui::Focusable::focus_handle);
        assert!(
            focused(&field_focus, &mut cx),
            "the field is the first tab stop of the dialog"
        );

        cx.simulate_input("draft");
        cx.simulate_keystrokes("enter");
        assert_eq!(
            page.read_with(&cx, |page, _| page.dismissed),
            0,
            "Enter is the field's"
        );
        cx.simulate_keystrokes("escape");
        assert_eq!(page.read_with(&cx, |page, _| page.dismissed), 1);
        assert_eq!(page.read_with(&cx, |page, _| page.outer_escapes), 0);
    }

    #[gpui::test]
    fn a_dialog_without_a_tab_stop_keeps_the_focus_on_itself(cx: &mut TestAppContext) {
        let (page, mut cx) = open(cx);
        let [opener, ..] = handles(&page, &mut cx);
        focus(&opener, &mut cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.mount_modal(Content::Empty, window, cx)
        });
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        next_frame(&mut cx);
        let modal = page
            .read_with(&cx, |page, _| page.modal.clone())
            .expect("mounted");
        let dialog = modal.read_with(&cx, gpui::Focusable::focus_handle);
        assert!(focused(&dialog, &mut cx));

        cx.simulate_keystrokes("tab");
        assert!(focused(&dialog, &mut cx), "Tab stays in the dialog");
        cx.simulate_keystrokes("shift-tab");
        assert!(focused(&dialog, &mut cx));
        assert!(!focused(&opener, &mut cx));
    }

    /// Port of "honors reduced motion": no fade, so the dialog is open or gone at once.
    #[gpui::test]
    fn reduced_motion_opens_and_closes_at_once(cx: &mut TestAppContext) {
        let (page, mut cx) = open(cx);
        cx.update(|_, cx| cx.set_reduce_motion(true));
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        let modal = page
            .read_with(&cx, |page, _| page.modal.clone())
            .expect("mounted");
        assert_eq!(modal.read_with(&cx, |modal, _| modal.phase()), Phase::Open);
        assert_eq!(cx.update(|_, cx| modal.read(cx).opacity(cx)), 1.0);

        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(false, window, cx)
        });
        assert!(!modal.read_with(&cx, |modal, _| modal.is_present()));

        cx.update(|_, cx| cx.set_reduce_motion(false));
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        assert_eq!(
            modal.read_with(&cx, |modal, _| modal.phase()),
            Phase::Opening,
            "a live change of the preference applies to the next fade"
        );
    }

    /// Port of "retains visible values for 100 ms" for the opacity: the fade runs on
    /// the opacity alone, from 0 to 1 over the entrance and back over the exit.
    #[gpui::test]
    fn a_dialog_fades_its_opacity_in_and_out(cx: &mut TestAppContext) {
        let (page, mut cx) = open(cx);
        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(true, window, cx)
        });
        let modal = page
            .read_with(&cx, |page, _| page.modal.clone())
            .expect("mounted");
        let opacity = |cx: &mut VisualTestContext| cx.update(|_, cx| modal.read(cx).opacity(cx));
        assert_eq!(opacity(&mut cx), 0.0);
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(75));
        let midway = opacity(&mut cx);
        assert!(midway > 0.0 && midway < 1.0, "{midway}");
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(75));
        cx.run_until_parked();
        assert_eq!(opacity(&mut cx), 1.0);
        assert_eq!(modal.read_with(&cx, |modal, _| modal.phase()), Phase::Open);

        with_page(&page, &mut cx, |page, window, cx| {
            page.set_modal_open(false, window, cx)
        });
        assert_eq!(
            opacity(&mut cx),
            1.0,
            "the exit starts from the open opacity"
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        let fading = opacity(&mut cx);
        assert!(fading > 0.0 && fading < 1.0, "{fading}");
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        assert!(!modal.read_with(&cx, |modal, _| modal.is_present()));
    }
}
