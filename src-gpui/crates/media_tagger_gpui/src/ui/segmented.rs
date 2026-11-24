//! `UiSegmentedControl`: a radio group drawn as segments with a highlight that slides
//! to the selected one.
//!
//! Controlled like the Solid control: a click, Space on a focused segment or an arrow key
//! ask the owner with [`SegmentedEvent::Change`], and the group draws the owner's value
//! until [`SegmentedControl::set_value`]. The selected segment is the group's one tab
//! stop; the arrows move the focus around the group (wrapping) and ask for the segment
//! they land on.

use std::rc::Rc;

use gpui::{
    actions, div, prelude::*, px, Animation, AnimationExt as _, App, Bounds, Context, EventEmitter,
    FocusHandle, KeyBinding, Pixels, Role, SharedString, Window,
};
use media_tagger_client::{
    controls::{segmented_visuals, HighlightTrack, Measure, Rect, SegmentedKey},
    theme::{ThemeName, MOTION},
};

use super::theme::{self, box_shadows, color, focus_ring, weight, RingBorder};

/// The key context of the group.
pub const KEY_CONTEXT: &str = "UiSegmented";

actions!(
    ui_segmented,
    [
        /// Arrow right and arrow down.
        Next,
        /// Arrow left and arrow up.
        Previous,
    ]
);

pub fn key_bindings() -> Vec<KeyBinding> {
    let context = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("right", Next, context),
        KeyBinding::new("down", Next, context),
        KeyBinding::new("left", Previous, context),
        KeyBinding::new("up", Previous, context),
    ]
}

pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

/// One segment: the value the owner gets and the label it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentedOption {
    pub value: SharedString,
    pub label: SharedString,
}

impl SegmentedOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentedEvent {
    /// The value a click or a key asks for (`onChange`).
    Change(SharedString),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Highlight {
    Hidden,
    At(Rect),
    Sliding {
        from: Rect,
        to: Rect,
        generation: u64,
    },
}

pub struct SegmentedControl {
    options: Vec<SegmentedOption>,
    value: Option<SharedString>,
    label: SharedString,
    theme: Option<ThemeName>,
    handles: Vec<FocusHandle>,
    track: HighlightTrack,
    highlight: Highlight,
    /// The selection or the options changed since the last measurement.
    changed: bool,
    slides: u64,
}

impl EventEmitter<SegmentedEvent> for SegmentedControl {}

impl SegmentedControl {
    /// `label` names the group (its `legend`).
    pub fn new(
        label: impl Into<SharedString>,
        options: Vec<SegmentedOption>,
        value: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut control = Self {
            options: Vec::new(),
            value,
            label: label.into(),
            theme: None,
            handles: Vec::new(),
            track: HighlightTrack::new(),
            highlight: Highlight::Hidden,
            changed: true,
            slides: 0,
        };
        control.set_options(options, cx);
        control
    }

    pub fn value(&self) -> Option<&SharedString> {
        self.value.as_ref()
    }

    pub fn options(&self) -> &[SegmentedOption] {
        &self.options
    }

    /// The selected segment, if the value is one of the options.
    pub fn selected(&self) -> Option<usize> {
        let value = self.value.as_ref()?;
        self.options
            .iter()
            .position(|option| &option.value == value)
    }

    /// The focus handle of segment `index`.
    pub fn focus_handle(&self, index: usize) -> Option<&FocusHandle> {
        self.handles.get(index)
    }

    /// The owner's value; the highlight slides to it.
    pub fn set_value(&mut self, value: Option<SharedString>, cx: &mut Context<Self>) {
        if self.value != value {
            self.value = value;
            self.changed = true;
            cx.notify();
        }
    }

    pub fn set_options(&mut self, options: Vec<SegmentedOption>, cx: &mut Context<Self>) {
        self.handles
            .resize_with(options.len(), || cx.focus_handle());
        self.options = options;
        self.changed = true;
        cx.notify();
    }

    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.label = label.into();
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Option<ThemeName>, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Asks for segment `index` unless it is the selected one, as a radio only fires
    /// `change` when it becomes checked.
    fn request(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.selected() == Some(index) {
            return;
        }
        if let Some(option) = self.options.get(index) {
            cx.emit(SegmentedEvent::Change(option.value.clone()));
        }
    }

    fn arrow(&mut self, key: SegmentedKey, window: &mut Window, cx: &mut Context<Self>) {
        let from = self
            .handles
            .iter()
            .position(|handle| handle.is_focused(window))
            .or_else(|| self.selected())
            .unwrap_or(0);
        let Some(target) = key.target(from, self.options.len()) else {
            return;
        };
        window.focus(&self.handles[target], cx);
        self.request(target, cx);
        cx.notify();
    }

    fn measured(
        &mut self,
        children: &[Bounds<Pixels>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Children: the anchor at the group's origin, the highlight, then the segments.
        let Some(anchor) = children.first() else {
            return;
        };
        let rects: Vec<Rect> = children
            .iter()
            .skip(2)
            .map(|bounds| Rect {
                x: f32::from(bounds.origin.x - anchor.origin.x),
                y: f32::from(bounds.origin.y - anchor.origin.y),
                width: f32::from(bounds.size.width),
                height: f32::from(bounds.size.height),
            })
            .collect();
        let labels: Vec<&str> = self
            .options
            .iter()
            .map(|option| option.label.as_ref())
            .collect();
        let selected = self.selected();
        let animate = std::mem::take(&mut self.changed);
        let next = match self.track.measure(&rects, &labels, selected, animate) {
            Measure::Unchanged => return,
            Measure::Hidden => Highlight::Hidden,
            Measure::Snap(rect) => Highlight::At(rect),
            Measure::Slide { from, to } => {
                self.slides += 1;
                Highlight::Sliding {
                    from,
                    to,
                    generation: self.slides,
                }
            }
        };
        if next != self.highlight {
            self.highlight = next;
            // Measured while the frame is prepainted, where a notify does not mark the
            // window dirty (`WindowInvalidator::invalidate_view` outside
            // `DrawPhase::None`): draw the new highlight with the next frame.
            let view = cx.entity_id();
            window.on_next_frame(move |_, cx| cx.notify(view));
        }
    }
}

fn place(element: gpui::Div, rect: Rect) -> gpui::Div {
    element
        .left(px(rect.x))
        .top(px(rect.y))
        .w(px(rect.width))
        .h(px(rect.height))
}

impl Render for SegmentedControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let visuals = segmented_visuals(theme.tokens());
        let selected = self.selected();
        let tab_stop = selected.unwrap_or(0);
        for (index, handle) in self.handles.iter_mut().enumerate() {
            *handle = handle.clone().tab_stop(index == tab_stop);
        }
        let keyboard = window.last_input_was_keyboard();

        let highlight_base = div()
            .debug_selector(|| "segmented-highlight".into())
            .absolute()
            .rounded(px(visuals.label_radius))
            .bg(color(visuals.highlight));
        let highlight = match self.highlight {
            Highlight::Hidden => div().absolute().size_0().into_any_element(),
            Highlight::At(rect) => place(highlight_base, rect).into_any_element(),
            Highlight::Sliding {
                from,
                to,
                generation,
            } => place(highlight_base, to)
                .with_animation(
                    ("segmented-slide", generation),
                    Animation::new(MOTION.entrance).with_easing(|t| MOTION.ease.sample(t)),
                    move |highlight, progress| place(highlight, from.lerp(to, progress)),
                )
                .into_any_element(),
        };

        let segments = self.options.iter().enumerate().map(|(index, option)| {
            let checked = selected == Some(index);
            let handle = &self.handles[index];
            let focus_visible = keyboard && handle.is_focused(window);
            div()
                .id(("segment", index))
                .role(Role::RadioButton)
                .aria_label(option.label.clone())
                .aria_toggled(if checked {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .aria_position_in_set(index + 1)
                .aria_size_of_set(self.options.len())
                .track_focus(handle)
                .relative()
                .flex_none()
                .px(px(visuals.label_padding_x))
                .py(px(visuals.label_padding_y))
                .rounded(px(visuals.label_radius))
                .cursor_pointer()
                .text_size(px(visuals.font_size))
                .line_height(px(visuals.line_height))
                .when(!checked, |segment| {
                    segment.hover(|style| style.bg(color(visuals.hover_background)))
                })
                // The hidden semibold copy reserves the width of the selected look.
                .child(
                    div()
                        .invisible()
                        .font_weight(weight(visuals.selected_weight))
                        .child(option.label.clone()),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(color(if checked {
                            visuals.selected_text
                        } else {
                            visuals.text
                        }))
                        .when(checked, |label| {
                            label.font_weight(weight(visuals.selected_weight))
                        })
                        .child(option.label.clone()),
                )
                .when(focus_visible, |segment| {
                    segment.child(focus_ring(
                        visuals.focus_ring,
                        visuals.label_radius,
                        RingBorder::NONE,
                    ))
                })
                .on_click(cx.listener(move |this, _, _, cx| this.request(index, cx)))
        });

        let entity = cx.entity().downgrade();
        let measured = Rc::new(
            move |children: Vec<Bounds<Pixels>>, window: &mut Window, cx: &mut App| {
                entity
                    .update(cx, |this, cx| this.measured(&children, window, cx))
                    .ok();
            },
        );
        div()
            .on_children_prepainted(move |children, window, cx| measured(children, window, cx))
            .id("segmented")
            .role(Role::RadioGroup)
            .aria_label(self.label.clone())
            .key_context(KEY_CONTEXT)
            .relative()
            .flex()
            .flex_wrap()
            .min_w_0()
            .gap(px(visuals.gap))
            .p(px(visuals.padding))
            .rounded(px(visuals.radius))
            .bg(color(visuals.background))
            .shadow(box_shadows(&[media_tagger_client::theme::Shadow::ring(
                true,
                1.0,
                visuals.ring,
            )]))
            .on_action(
                cx.listener(|this, _: &Next, window, cx| {
                    this.arrow(SegmentedKey::Next, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &Previous, window, cx| {
                this.arrow(SegmentedKey::Previous, window, cx)
            }))
            .child(div().absolute().top_0().left_0().size_0())
            .child(highlight)
            .children(segments)
    }
}
