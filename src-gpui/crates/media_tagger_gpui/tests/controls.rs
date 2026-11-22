//! The shared controls on GPUI's test platform (epic E05), ported from
//! `src/ui/controls/__tests__/controls.test.tsx`: the slider and the segmented control
//! answer the keys of native range and radio inputs, ask their owner, and keep showing
//! the owner's value.

use gpui::{
    div, prelude::*, Context, Entity, Focusable, SharedString, Subscription, TestAppContext,
    VisualTestContext, Window,
};
use media_tagger_client::controls::SliderRange;
use media_tagger_gpui::ui::{
    self, layers,
    segmented::{SegmentedControl, SegmentedEvent, SegmentedOption},
    slider::{Slider, SliderEvent},
};

const OPTIONS: [(&str, &str); 3] = [("all", "All"), ("image", "Images"), ("video", "Video")];

fn options() -> Vec<SegmentedOption> {
    OPTIONS
        .iter()
        .map(|(value, label)| SegmentedOption::new(*value, *label))
        .collect()
}

/// An owner of a slider and two segmented groups. It accepts what they ask for unless
/// told to reject it.
struct Owner {
    slider: Entity<Slider>,
    groups: [Entity<SegmentedControl>; 2],
    inputs: Vec<f64>,
    changes: Vec<(usize, SharedString)>,
    accept: bool,
    _subscriptions: Vec<Subscription>,
}

impl Owner {
    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let slider = cx.new(|cx| Slider::new(SliderRange::new(0.0, 100.0), 20.0, "Zoom", cx));
        let groups = ["First", "Second"].map(|label| {
            cx.new(|cx| SegmentedControl::new(label, options(), Some("all".into()), cx))
        });
        let mut subscriptions =
            vec![
                cx.subscribe(&slider, |owner: &mut Self, slider, event, cx| {
                    let SliderEvent::Input(value) = *event;
                    owner.inputs.push(value);
                    if owner.accept {
                        slider.update(cx, |slider, cx| slider.set_value(value, cx));
                    }
                }),
            ];
        for (index, group) in groups.iter().enumerate() {
            subscriptions.push(
                cx.subscribe(group, move |owner: &mut Self, group, event, cx| {
                    let SegmentedEvent::Change(value) = event.clone();
                    owner.changes.push((index, value.clone()));
                    if owner.accept {
                        group.update(cx, |group, cx| group.set_value(Some(value), cx));
                    }
                }),
            );
        }
        Self {
            slider,
            groups,
            inputs: Vec::new(),
            changes: Vec::new(),
            accept: true,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        layers::root(cx)
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .child(self.groups[0].clone())
            .child(self.groups[1].clone())
            .child(div().w_64().child(self.slider.clone()))
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Owner>, VisualTestContext) {
    cx.update(ui::init);
    let window = cx.add_window(Owner::new);
    let owner = window.root(cx).expect("the owner");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();
    // As a window does when it opens.
    cx.update(layers::focus_root);
    cx.run_until_parked();
    (owner, cx)
}

fn focus_slider(owner: &Entity<Owner>, cx: &mut VisualTestContext) -> Entity<Slider> {
    let slider = owner.read_with(cx, |owner, _| owner.slider.clone());
    let handle = slider.read_with(cx, |slider, cx| slider.focus_handle(cx));
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
    slider
}

fn slider_value(slider: &Entity<Slider>, cx: &mut VisualTestContext) -> f64 {
    slider.read_with(cx, |slider, _| slider.value())
}

#[gpui::test]
fn the_slider_answers_the_keys_of_a_range_input(cx: &mut TestAppContext) {
    let (owner, mut cx) = open(cx);
    let slider = focus_slider(&owner, &mut cx);

    for (keys, value) in [
        ("right", 21.0),
        ("up", 22.0),
        ("left", 21.0),
        ("down", 20.0),
        ("pageup", 30.0),
        ("pagedown", 20.0),
        ("end", 100.0),
        ("home", 0.0),
    ] {
        cx.simulate_keystrokes(keys);
        assert_eq!(slider_value(&slider, &mut cx), value, "{keys}");
    }
    cx.simulate_keystrokes("left");
    assert_eq!(
        owner.read_with(&cx, |owner, _| owner.inputs.len()),
        8,
        "no input at the start"
    );
}

#[gpui::test]
fn the_slider_keeps_the_committed_value_when_the_owner_rejects_an_input(cx: &mut TestAppContext) {
    let (owner, mut cx) = open(cx);
    owner.update(&mut cx, |owner, _| owner.accept = false);
    let slider = focus_slider(&owner, &mut cx);

    cx.simulate_keystrokes("end");
    assert_eq!(
        owner.read_with(&cx, |owner, _| owner.inputs.clone()),
        [100.0]
    );
    assert_eq!(slider_value(&slider, &mut cx), 20.0);

    slider.update(&mut cx, |slider, cx| slider.set_value(80.0, cx));
    assert_eq!(
        slider_value(&slider, &mut cx),
        80.0,
        "an outside change shows"
    );
}

fn group_value(
    owner: &Entity<Owner>,
    index: usize,
    cx: &mut VisualTestContext,
) -> Option<SharedString> {
    let group = owner.read_with(cx, |owner, _| owner.groups[index].clone());
    group.read_with(cx, |group, _| group.value().cloned())
}

fn segment_focused(
    owner: &Entity<Owner>,
    group: usize,
    segment: usize,
    cx: &mut VisualTestContext,
) -> bool {
    let group = owner.read_with(cx, |owner, _| owner.groups[group].clone());
    let handle = group.read_with(cx, |group, _| {
        group.focus_handle(segment).cloned().expect("segment")
    });
    cx.update(|window, _| handle.is_focused(window))
}

#[gpui::test]
fn segmented_groups_are_one_tab_stop_each_and_arrows_select(cx: &mut TestAppContext) {
    let (owner, mut cx) = open(cx);

    cx.simulate_keystrokes("tab");
    assert!(
        segment_focused(&owner, 0, 0, &mut cx),
        "the selected segment of the first group"
    );
    cx.simulate_keystrokes("right");
    assert_eq!(group_value(&owner, 0, &mut cx).as_deref(), Some("image"));
    assert!(segment_focused(&owner, 0, 1, &mut cx));
    assert_eq!(
        group_value(&owner, 1, &mut cx).as_deref(),
        Some("all"),
        "groups are independent"
    );

    cx.simulate_keystrokes("tab");
    assert!(
        segment_focused(&owner, 1, 0, &mut cx),
        "Tab leaves the group for the next one"
    );

    cx.simulate_keystrokes("left");
    assert_eq!(
        group_value(&owner, 1, &mut cx).as_deref(),
        Some("video"),
        "arrows wrap"
    );
    cx.simulate_keystrokes("down");
    assert_eq!(group_value(&owner, 1, &mut cx).as_deref(), Some("all"));
    assert_eq!(owner.read_with(&cx, |owner, _| owner.changes.len()), 3);
}

#[gpui::test]
fn a_segmented_group_waits_for_its_owner(cx: &mut TestAppContext) {
    let (owner, mut cx) = open(cx);
    owner.update(&mut cx, |owner, _| owner.accept = false);

    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("right");
    assert_eq!(
        owner.read_with(&cx, |owner, _| owner.changes.clone()),
        [(0, SharedString::from("image"))]
    );
    assert_eq!(
        group_value(&owner, 0, &mut cx).as_deref(),
        Some("all"),
        "still the owner's value"
    );
    assert!(segment_focused(&owner, 0, 1, &mut cx), "the focus moved");

    // From the focused segment, not the selected one.
    cx.simulate_keystrokes("right");
    assert_eq!(
        owner.read_with(&cx, |owner, _| owner.changes.last().cloned()),
        Some((0, SharedString::from("video")))
    );
    // Back onto the selected segment: no change, as a checked radio fires none.
    cx.simulate_keystrokes("right");
    assert_eq!(owner.read_with(&cx, |owner, _| owner.changes.len()), 2);
}

/// Ported from the same file: a button clicks once and a disabled one does not, the
/// switch asks its owner, a click on a segment asks once and waits for the owner, and
/// a press or drag on the slider's track asks for the value under the pointer. Also the
/// active theme as a global that themed text fields follow.
mod pointer_and_activation {
    use gpui::{
        div, point, prelude::*, px, Bounds, Context, Entity, FocusHandle, KeyUpEvent, Keystroke,
        Modifiers, MouseButton, MouseMoveEvent, Pixels, SharedString, Subscription, TestAppContext,
        VisualTestContext, Window,
    };
    use media_tagger_client::{controls::SliderRange, theme::ThemeName};
    use media_tagger_gpui::ui::{
        self,
        button::Button,
        layers,
        segmented::{SegmentedControl, SegmentedEvent},
        slider::{Slider, SliderEvent},
        switch::Switch,
        text_input::TextInput,
        theme,
    };

    use super::options;

    struct Board {
        action: FocusHandle,
        action_disabled: bool,
        clicks: usize,
        checked: bool,
        switch_disabled: bool,
        switch_requests: Vec<bool>,
        slider: Entity<Slider>,
        inputs: Vec<f64>,
        group: Entity<SegmentedControl>,
        changes: Vec<SharedString>,
        accept: bool,
        _subscriptions: Vec<Subscription>,
    }

    impl Board {
        fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
            let slider = cx.new(|cx| Slider::new(SliderRange::new(0.0, 100.0), 20.0, "Zoom", cx));
            let group =
                cx.new(|cx| SegmentedControl::new("Media type", options(), Some("all".into()), cx));
            let subscriptions = vec![
                cx.subscribe(&slider, |board: &mut Self, slider, event, cx| {
                    let SliderEvent::Input(value) = *event;
                    board.inputs.push(value);
                    if board.accept {
                        slider.update(cx, |slider, cx| slider.set_value(value, cx));
                    }
                }),
                cx.subscribe(&group, |board: &mut Self, group, event, cx| {
                    let SegmentedEvent::Change(value) = event.clone();
                    board.changes.push(value.clone());
                    if board.accept {
                        group.update(cx, |group, cx| group.set_value(Some(value), cx));
                    }
                }),
            ];
            Self {
                action: cx.focus_handle().tab_stop(true),
                action_disabled: false,
                clicks: 0,
                checked: false,
                switch_disabled: false,
                switch_requests: Vec::new(),
                slider,
                inputs: Vec::new(),
                group,
                changes: Vec::new(),
                accept: true,
                _subscriptions: subscriptions,
            }
        }
    }

    impl Render for Board {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let clicked = cx.entity().downgrade();
            let switched = clicked.clone();
            layers::root(cx)
                .size_full()
                .flex()
                .flex_col()
                .gap_4()
                .child(
                    div().flex().child(
                        div().debug_selector(|| "action".into()).child(
                            Button::new("action", "Action")
                                .track_focus(&self.action)
                                .disabled(self.action_disabled)
                                .on_click(move |_, _, cx| {
                                    clicked
                                        .update(cx, |board, cx| {
                                            board.clicks += 1;
                                            cx.notify();
                                        })
                                        .ok();
                                }),
                        ),
                    ),
                )
                .child(
                    div().flex().child(
                        div().debug_selector(|| "switch".into()).child(
                            Switch::new("switch", self.checked, "Switch")
                                .text("Text")
                                .disabled(self.switch_disabled)
                                .on_change(move |checked, _, cx| {
                                    switched
                                        .update(cx, |board, cx| {
                                            board.switch_requests.push(checked);
                                            if board.accept {
                                                board.checked = checked;
                                            }
                                            cx.notify();
                                        })
                                        .ok();
                                }),
                        ),
                    ),
                )
                .child(
                    div().flex().child(
                        div()
                            .debug_selector(|| "group".into())
                            .child(self.group.clone()),
                    ),
                )
                .child(
                    div()
                        .w_64()
                        .debug_selector(|| "slider".into())
                        .child(self.slider.clone()),
                )
        }
    }

    fn open(cx: &mut TestAppContext) -> (Entity<Board>, VisualTestContext) {
        cx.update(ui::init);
        let window = cx.add_window(Board::new);
        let board = window.root(cx).expect("the board");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        cx.update(layers::focus_root);
        cx.run_until_parked();
        (board, cx)
    }

    fn bounds(selector: &'static str, cx: &mut VisualTestContext) -> Bounds<Pixels> {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("no bounds for {selector}"))
    }

    /// A full key press: GPUI turns Enter or Space into a click on the key's release.
    fn press(key: &'static str, cx: &mut VisualTestContext) {
        cx.simulate_keystrokes(key);
        cx.simulate_event(KeyUpEvent {
            keystroke: Keystroke::parse(key).expect("a key"),
        });
        cx.run_until_parked();
    }

    fn focused(handle: &FocusHandle, cx: &mut VisualTestContext) -> bool {
        cx.update(|window, _| handle.is_focused(window))
    }

    #[gpui::test]
    fn a_button_clicks_once_per_press_and_a_disabled_one_neither_clicks_nor_takes_the_focus(
        cx: &mut TestAppContext,
    ) {
        let (board, mut cx) = open(cx);
        let action = board.read_with(&cx, |board, _| board.action.clone());

        let button = bounds("action", &mut cx);
        cx.simulate_click(button.center(), Modifiers::none());
        assert_eq!(board.read_with(&cx, |board, _| board.clicks), 1, "a click");

        cx.update(|window, cx| window.focus(&action, cx));
        cx.run_until_parked();
        press("enter", &mut cx);
        assert_eq!(board.read_with(&cx, |board, _| board.clicks), 2, "Enter");
        press("space", &mut cx);
        assert_eq!(board.read_with(&cx, |board, _| board.clicks), 3, "Space");

        board.update(&mut cx, |board, cx| {
            board.action_disabled = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.simulate_click(button.center(), Modifiers::none());
        press("enter", &mut cx);
        assert_eq!(
            board.read_with(&cx, |board, _| board.clicks),
            3,
            "a disabled button does not click"
        );

        cx.update(layers::focus_root);
        cx.run_until_parked();
        cx.simulate_keystrokes("tab");
        assert!(
            !focused(&action, &mut cx),
            "a disabled button is no tab stop"
        );
    }

    #[gpui::test]
    fn the_switch_asks_its_owner_on_a_click_or_space_and_not_while_disabled(
        cx: &mut TestAppContext,
    ) {
        let (board, mut cx) = open(cx);
        board.update(&mut cx, |board, _| board.accept = false);

        let switch = bounds("switch", &mut cx);
        cx.simulate_click(switch.center(), Modifiers::none());
        assert_eq!(
            board.read_with(&cx, |board, _| board.switch_requests.clone()),
            [true]
        );
        assert!(
            !board.read_with(&cx, |board, _| board.checked),
            "the owner decides"
        );

        // The button is the first tab stop, the switch the second.
        cx.update(layers::focus_root);
        cx.run_until_parked();
        cx.simulate_keystrokes("tab tab");
        press("space", &mut cx);
        assert_eq!(
            board.read_with(&cx, |board, _| board.switch_requests.clone()),
            [true, true]
        );

        board.update(&mut cx, |board, cx| {
            board.accept = true;
            cx.notify();
        });
        cx.simulate_click(switch.center(), Modifiers::none());
        assert!(board.read_with(&cx, |board, _| board.checked));
        cx.simulate_click(switch.center(), Modifiers::none());
        assert_eq!(
            board.read_with(&cx, |board, _| board.switch_requests.clone()),
            [true, true, true, false],
            "a checked switch asks to uncheck"
        );

        board.update(&mut cx, |board, cx| {
            board.switch_disabled = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.simulate_click(switch.center(), Modifiers::none());
        assert_eq!(
            board.read_with(&cx, |board, _| board.switch_requests.len()),
            4,
            "a disabled switch asks nothing"
        );
    }

    /// Port of "names radio groups and waits for controlled selection".
    #[gpui::test]
    fn a_click_on_a_segment_asks_once_and_the_group_shows_the_owner_value(cx: &mut TestAppContext) {
        let (board, mut cx) = open(cx);
        board.update(&mut cx, |board, _| board.accept = false);
        let group = board.read_with(&cx, |board, _| board.group.clone());
        let value =
            |cx: &mut VisualTestContext| group.read_with(cx, |group, _| group.value().cloned());

        let bounds = bounds("group", &mut cx);
        let y = bounds.center().y;
        let last = point(bounds.right() - px(8.), y);
        let first = point(bounds.left() + px(8.), y);

        cx.simulate_click(last, Modifiers::none());
        assert_eq!(
            board.read_with(&cx, |board, _| board.changes.clone()),
            [SharedString::from("video")]
        );
        assert_eq!(
            value(&mut cx).as_deref(),
            Some("all"),
            "still the owner's value"
        );

        cx.simulate_click(first, Modifiers::none());
        assert_eq!(
            board.read_with(&cx, |board, _| board.changes.len()),
            1,
            "the checked segment asks nothing"
        );

        group.update(&mut cx, |group, cx| {
            group.set_value(Some("video".into()), cx)
        });
        assert_eq!(value(&mut cx).as_deref(), Some("video"));
        group.update(&mut cx, |group, cx| {
            group.set_value(Some("image".into()), cx)
        });
        assert_eq!(value(&mut cx).as_deref(), Some("image"));
        assert_eq!(
            board.read_with(&cx, |board, _| board.changes.len()),
            1,
            "an outside change asks nothing"
        );
        assert_eq!(group.read_with(&cx, |group, _| group.selected()), Some(1));
    }

    #[gpui::test]
    fn space_on_a_focused_segment_asks_for_it(cx: &mut TestAppContext) {
        let (board, mut cx) = open(cx);
        board.update(&mut cx, |board, _| board.accept = false);
        let group = board.read_with(&cx, |board, _| board.group.clone());
        let second = group.read_with(&cx, |group, _| {
            group.focus_handle(1).cloned().expect("segment")
        });

        cx.update(|window, cx| window.focus(&second, cx));
        cx.run_until_parked();
        press("space", &mut cx);
        assert_eq!(
            board.read_with(&cx, |board, _| board.changes.clone()),
            [SharedString::from("image")]
        );
    }

    #[gpui::test]
    fn a_press_and_a_drag_on_the_track_ask_for_the_value_under_the_pointer(
        cx: &mut TestAppContext,
    ) {
        let (board, mut cx) = open(cx);
        let slider = board.read_with(&cx, |board, _| board.slider.clone());
        let track = bounds("slider", &mut cx);
        let y = track.center().y;

        cx.simulate_mouse_down(track.center(), MouseButton::Left, Modifiers::none());
        assert_eq!(
            board.read_with(&cx, |board, _| board.inputs.clone()),
            [50.0]
        );
        assert_eq!(slider.read_with(&cx, |slider, _| slider.value()), 50.0);

        // The drag goes on outside the slider and is clamped to the range.
        cx.simulate_event(MouseMoveEvent {
            position: point(track.right() + px(40.), y + px(30.)),
            modifiers: Modifiers::none(),
            pressed_button: Some(MouseButton::Left),
        });
        assert_eq!(
            board.read_with(&cx, |board, _| board.inputs.clone()),
            [50.0, 100.0]
        );
        cx.simulate_mouse_up(
            point(track.right() + px(40.), y),
            MouseButton::Left,
            Modifiers::none(),
        );
        cx.simulate_event(MouseMoveEvent {
            position: point(track.left(), y),
            modifiers: Modifiers::none(),
            pressed_button: Some(MouseButton::Left),
        });
        assert_eq!(
            board.read_with(&cx, |board, _| board.inputs.len()),
            2,
            "the drag ended with the release"
        );

        cx.simulate_mouse_down(point(track.left(), y), MouseButton::Left, Modifiers::none());
        assert_eq!(slider.read_with(&cx, |slider, _| slider.value()), 0.0);
    }

    #[gpui::test]
    fn a_disabled_slider_takes_no_input(cx: &mut TestAppContext) {
        let (board, mut cx) = open(cx);
        let slider = board.read_with(&cx, |board, _| board.slider.clone());
        slider.update(&mut cx, |slider, cx| slider.set_disabled(true, cx));
        cx.run_until_parked();
        let track = bounds("slider", &mut cx);

        cx.simulate_mouse_down(track.center(), MouseButton::Left, Modifiers::none());
        let handle = slider.read_with(&cx, gpui::Focusable::focus_handle);
        cx.update(|window, cx| window.focus(&handle, cx));
        cx.run_until_parked();
        cx.simulate_keystrokes("end");
        assert!(board.read_with(&cx, |board, _| board.inputs.is_empty()));
        assert_eq!(slider.read_with(&cx, |slider, _| slider.value()), 20.0);
    }

    #[gpui::test]
    fn the_active_theme_is_a_global_that_themed_fields_follow(cx: &mut TestAppContext) {
        let (_, mut cx) = open(cx);
        assert_eq!(
            cx.update(|_, cx| theme::active(cx)),
            ThemeName::Dark,
            "dark by default"
        );
        let (themed, fixed) = cx.update(|window, cx| {
            (
                cx.new(|cx| TextInput::themed(window, cx)),
                cx.new(|cx| TextInput::new(ThemeName::Dark, window, cx)),
            )
        });
        assert_eq!(
            themed.read_with(&cx, |input, _| input.theme()),
            ThemeName::Dark
        );

        cx.update(|_, cx| theme::set_active(ThemeName::Light, cx));
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| theme::active(cx)), ThemeName::Light);
        assert_eq!(
            themed.read_with(&cx, |input, _| input.theme()),
            ThemeName::Light
        );
        assert_eq!(
            fixed.read_with(&cx, |input, _| input.theme()),
            ThemeName::Dark,
            "a field given a theme keeps it"
        );

        cx.update(|_, cx| ui::init(cx));
        assert_eq!(
            cx.update(|_, cx| theme::active(cx)),
            ThemeName::Light,
            "init keeps a theme that is set"
        );
    }
}

/// The highlight is measured while a frame is prepainted, where a notify does not redraw
/// the window: it must still be drawn without waiting for an unrelated event, when the
/// group first shows and after a key selects another segment.
mod segmented_highlight {
    use std::time::Duration;

    use gpui::{
        prelude::*, Bounds, Context, Entity, Pixels, Subscription, TestAppContext,
        VisualTestContext, Window,
    };
    use media_tagger_gpui::ui::{
        self, layers,
        segmented::{SegmentedControl, SegmentedEvent},
    };

    use super::options;

    struct Group {
        group: Entity<SegmentedControl>,
        _subscription: Subscription,
    }

    impl Group {
        /// Focuses the window's root while it is built, as a window does before its
        /// first frame, so no later focus change redraws it.
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let group =
                cx.new(|cx| SegmentedControl::new("Media type", options(), Some("all".into()), cx));
            let subscription = cx.subscribe(&group, |_, group, event, cx| {
                let SegmentedEvent::Change(value) = event.clone();
                group.update(cx, |group, cx| group.set_value(Some(value), cx));
            });
            layers::focus_root(window, cx);
            Self {
                group,
                _subscription: subscription,
            }
        }
    }

    impl Render for Group {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            layers::root(cx).size_full().child(self.group.clone())
        }
    }

    /// Delivers the next frame, as the platform does after drawing.
    fn next_frame(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }

    fn highlight(cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
        cx.debug_bounds("segmented-highlight")
    }

    #[gpui::test]
    fn the_highlight_is_drawn_on_the_next_frame_and_follows_a_key(cx: &mut TestAppContext) {
        cx.update(ui::init);
        let window = cx.add_window(Group::new);
        let view = window.root(cx).expect("the view");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        next_frame(&mut cx);
        let first = highlight(&mut cx).expect("the highlight is drawn");
        assert!(f32::from(first.size.width) > 0.0);

        let group = view.read_with(&cx, |view, _| view.group.clone());
        let handle = group.read_with(&cx, |group, _| {
            group.focus_handle(0).expect("a segment").clone()
        });
        cx.update(|window, cx| window.focus(&handle, cx));
        cx.run_until_parked();
        cx.simulate_keystrokes("right");
        assert_eq!(
            group.read_with(&cx, |group, _| group.selected()),
            Some(1),
            "the owner took the segment"
        );
        next_frame(&mut cx);
        cx.executor().advance_clock(Duration::from_millis(200));
        next_frame(&mut cx);
        let second = highlight(&mut cx).expect("the highlight is drawn");
        assert!(
            second.left() > first.right() - gpui::px(1.),
            "the highlight moved to the second segment: {first:?} -> {second:?}"
        );
    }
}

/// `UiAlert` lays its message out as a block that fills the alert, so a long message
/// wraps inside it. The alert's body is a column that keeps its children at their own
/// width (a Retry button under the message); the message must still wrap at the alert's
/// inner width in every place an alert is used: as the child of a plain box (the search
/// header's validation message), in a column with a gap (the kit, the dock and the
/// larger order view), and with a title and a button (the dock's failed tag read).
mod alert_layout {
    use gpui::{
        div, prelude::*, px, Bounds, Context, Pixels, TestAppContext, VisualTestContext, Window,
    };
    use media_tagger_client::{
        controls::{alert_visuals, AlertTone},
        theme::ThemeName,
    };
    use media_tagger_gpui::ui::{self, alert::Alert, button::Button, layers, theme::app_root};

    const WIDTH: f32 = 220.0;
    const MESSAGE: &str = "The selection expired before it could be used, so nothing was \
                           written; select the items again and repeat the last step.";

    struct Alerts;

    impl Render for Alerts {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = ThemeName::Dark;
            let visuals = alert_visuals(theme.tokens(), AlertTone::Error);
            app_root(layers::root(cx), theme)
                .flex()
                .flex_col()
                .items_start()
                .gap_4()
                .child(
                    div().w(px(WIDTH)).child(
                        Alert::new("in-a-box", MESSAGE)
                            .tone(AlertTone::Error)
                            .theme(theme)
                            .debug_selector("in-a-box"),
                    ),
                )
                .child(
                    div()
                        .w(px(WIDTH))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(
                            Alert::new("in-a-column", MESSAGE)
                                .tone(AlertTone::Error)
                                .theme(theme)
                                .debug_selector("in-a-column"),
                        )
                        .child(
                            Alert::new("with-a-button", MESSAGE)
                                .tone(AlertTone::Error)
                                .title("Title")
                                .theme(theme)
                                .debug_selector("with-a-button")
                                .child(Button::new("retry", "Retry").debug_selector("retry")),
                        ),
                )
                // The message alone, in a box as wide as the alert's content.
                .child(
                    div()
                        .debug_selector(|| "reference".into())
                        .w(px(WIDTH - 2. * visuals.padding_x - 2.))
                        .text_size(px(visuals.font_size))
                        .line_height(px(visuals.line_height))
                        .child(MESSAGE),
                )
        }
    }

    fn bounds(selector: &'static str, cx: &mut VisualTestContext) -> Bounds<Pixels> {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("no bounds for {selector}"))
    }

    #[gpui::test]
    fn a_long_message_wraps_at_the_inner_width_of_a_narrow_alert(cx: &mut TestAppContext) {
        cx.update(ui::init);
        let window = cx.add_window(|_, _| Alerts);
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();

        let visuals = alert_visuals(ThemeName::Dark.tokens(), AlertTone::Error);
        let frame = 2. * visuals.padding_y + 2.;
        let message = f32::from(bounds("reference", &mut cx).size.height);
        assert!(
            message >= 3. * visuals.line_height,
            "the message needs several lines at this width: {message}"
        );

        for selector in ["in-a-box", "in-a-column"] {
            let alert = bounds(selector, &mut cx);
            assert_eq!(f32::from(alert.size.width), WIDTH, "{selector}");
            assert_eq!(f32::from(alert.size.height), message + frame, "{selector}");
        }

        let alert = bounds("with-a-button", &mut cx);
        let retry = bounds("retry", &mut cx);
        assert_eq!(f32::from(alert.size.width), WIDTH);
        // The title's line, the gap under it, the message, the gap, the button.
        assert_eq!(
            f32::from(alert.size.height),
            visuals.line_height
                + visuals.title_gap
                + message
                + 8.
                + f32::from(retry.size.height)
                + frame
        );
        // The button keeps its own width at the start of the body, inside the alert.
        assert_eq!(
            f32::from(retry.left() - alert.left()),
            visuals.padding_x + 1.
        );
        assert!(retry.size.width < px(WIDTH / 2.), "{retry:?}");
        assert!(retry.bottom() <= alert.bottom() - px(visuals.padding_y));
    }
}

/// The three ways a button's box is set: `size` alone (a square icon button: the tag
/// editor's remove control, the search header's heart and clear button, the tag list
/// launcher), `height` alone (a text button: the search header's submit) and both (the
/// dock's heart, `h-[50px]! w-10!`: `size` wide and `height` tall).
mod button_sizes {
    use gpui::{div, prelude::*, Context, Pixels, Size, TestAppContext, VisualTestContext, Window};
    use media_tagger_client::{controls::ICON_BUTTON_SIZE, icons::IconName, theme::ThemeName};
    use media_tagger_gpui::ui::{self, button::Button, layers, theme::app_root};

    struct Buttons;

    impl Render for Buttons {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            app_root(layers::root(cx), ThemeName::Dark).child(
                div()
                    .flex()
                    .items_start()
                    .gap_4()
                    .child(Button::icon("plain", IconName::Reset, "Plain").debug_selector("plain"))
                    .child(
                        Button::icon("square", IconName::Close, "Square")
                            .size(20.)
                            .debug_selector("square"),
                    )
                    .child(
                        Button::icon("both", IconName::Heart, "Both")
                            .size(40.)
                            .height(50.)
                            .debug_selector("both"),
                    )
                    .child(
                        Button::new("text", "Text")
                            .height(28.)
                            .debug_selector("text"),
                    ),
            )
        }
    }

    #[gpui::test]
    fn size_is_the_width_and_the_height_unless_a_height_is_given(cx: &mut TestAppContext) {
        cx.update(ui::init);
        let window = cx.add_window(|_, _| Buttons);
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.run_until_parked();
        let mut size = |selector: &'static str| -> Size<Pixels> {
            cx.debug_bounds(selector)
                .unwrap_or_else(|| panic!("no bounds for {selector}"))
                .size
        };
        let of = |size: Size<Pixels>| (f32::from(size.width), f32::from(size.height));
        assert_eq!(of(size("plain")), (ICON_BUTTON_SIZE, ICON_BUTTON_SIZE));
        assert_eq!(of(size("square")), (20., 20.));
        assert_eq!(of(size("both")), (40., 50.));
        assert_eq!(f32::from(size("text").height), 28.);
    }
}
