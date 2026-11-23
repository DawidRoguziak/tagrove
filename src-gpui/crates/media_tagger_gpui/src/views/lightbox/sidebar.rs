//! The side panel of the lightbox (`LightboxToolbar.tsx` with `MediaGroupSetter.tsx`,
//! `LightboxTagPanel.tsx`, `LightboxInfoPanel.tsx` and
//! `LightboxDeleteConfirmDialog.tsx`): the header with the file name and the collapse
//! and close buttons, the media group fields, the tag editor with its feedback, the
//! Information panel, and the inline delete confirmation at the bottom.
//!
//! In a wide window the panel is a column beside the media; in a narrow one it is a
//! drawer over the media with a scrim behind it. Either way it slides out to the
//! right when it closes and keeps its place until the slide has ended.

use gpui::{
    div, prelude::*, px, relative, AnyElement, ClickEvent, Context, ElementId, MouseButton, Role,
    SharedString, Window,
};
use media_tagger_client::{
    bulk::dock::{random_bits, uuid_v4},
    controls::{AlertTone, ButtonVariant, FocusRing},
    i18n::Params,
    icons::IconName,
    lightbox::{
        info_rows,
        layout::{confirm, info, sidebar},
        LightboxCommand,
    },
    theme::{TextStep, ThemeColors, RADII, TYPOGRAPHY},
};

use super::{
    Frame, LightboxView, APPLY_GROUP_ID, CLOSE_ID, CONFIRM_CANCEL_ID, CONFIRM_ERROR_SELECTOR,
    CONFIRM_INPUT_SELECTOR, CONFIRM_SELECTOR, CONFIRM_SUBMIT_ID, COPY_GROUP_ID, GENERATE_GROUP_ID,
    GROUP_KEY_SELECTOR, GROUP_ORDER_SELECTOR, GROUP_PANEL_SELECTOR, INFO_PANEL_SELECTOR,
    INFO_TOGGLE_ID, RETRY_TAGS_ID, RETRY_TAG_DETAILS_ID, SIDEBAR_CLOSE_ID, SIDEBAR_SCRIM_SELECTOR,
    SIDEBAR_SELECTOR, SIDEBAR_UPPER_SELECTOR, TAG_DETAILS_FAILED_SELECTOR,
    TAG_DETAILS_LOADING_SELECTOR, TAG_FAILED_SELECTOR, TAG_INVALID_SELECTOR, TAG_PANEL_SELECTOR,
    TAG_SAVING_SELECTOR,
};
use crate::ui::{
    alert::Alert,
    button::{keyed_focus_handle, Button},
    fonts,
    theme::{color, focus_ring, weight},
};

/// A section heading: `h3` at the size and line height its utility class gives it.
fn heading(text: String, step: TextStep, colors: &ThemeColors) -> gpui::Div {
    div()
        .flex_none()
        .text_size(px(step.size))
        .line_height(px(step.line_height_px()))
        .font_weight(weight(TYPOGRAPHY.heading_weight))
        .text_color(color(colors.base_content))
        .child(text)
}

impl LightboxView {
    /// The side panel in its place: nothing once it has slid out; in a narrow window
    /// the scrim and the drawer over the media, otherwise a column beside it.
    pub(super) fn sidebar(
        &mut self,
        frame: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !frame.sidebar_occupied {
            return Vec::new();
        }
        let colors = &frame.theme.tokens().colors;
        let width = if frame.narrow {
            sidebar::width(frame.shell_width)
        } else {
            sidebar::WIDTH
        };
        let panel = self.sidebar_panel(frame, window, cx);
        // The panel keeps its width while its slot clips what has slid out.
        let shifted = div()
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-frame.sidebar_shift * width))
            .w(px(width))
            .child(panel);
        if !frame.narrow {
            return vec![div()
                .relative()
                .flex_none()
                .w(px(width))
                .h_full()
                .overflow_hidden()
                .child(shifted)
                .into_any_element()];
        }
        let mut layers = Vec::new();
        if frame.sidebar_open {
            layers.push(
                div()
                    .id(SIDEBAR_SCRIM_SELECTOR)
                    .debug_selector(|| SIDEBAR_SCRIM_SELECTOR.to_owned())
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(color(
                        colors.neutral.with_alpha_percent(sidebar::SCRIM_PERCENT),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.update_session(window, cx, |session, now| {
                                session.close_sidebar(true, now)
                            });
                        }),
                    )
                    .into_any_element(),
            );
        }
        layers.push(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(width))
                .overflow_hidden()
                .child(shifted)
                .into_any_element(),
        );
        layers
    }

    fn sidebar_panel(
        &mut self,
        frame: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = frame.theme.tokens();
        let colors = &tokens.colors;
        let i18n = &frame.i18n;
        let shown = &frame.shown;
        let file_name = shown.selected.summary.file_name.clone();

        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(px(sidebar::HEADER_GAP))
            .pb(px(sidebar::HEADER_PADDING_BOTTOM))
            .border_b_1()
            .border_color(color(colors.border_soft))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TYPOGRAPHY.sm.size))
                    .line_height(px(TYPOGRAPHY.sm.line_height_px()))
                    .font_weight(weight(TYPOGRAPHY.weight_semibold))
                    .text_color(color(colors.base_content))
                    .child(file_name),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(sidebar::HEADER_BUTTON_GAP))
                    .child(
                        Button::icon(
                            SIDEBAR_CLOSE_ID,
                            IconName::ArrowLeft,
                            i18n.t("lightbox.closePanel"),
                        )
                        .icon_half_turn(true)
                        .size(sidebar::BUTTON_SIZE)
                        .track_focus(&self.sidebar_close_focus)
                        .debug_selector(SIDEBAR_CLOSE_ID)
                        .theme(frame.theme)
                        .disabled(frame.confirm_open || frame.deleting)
                        .on_click(cx.listener(
                            |this, event: &ClickEvent, window, cx| {
                                // A keyboard collapse gives the reopen button the focus.
                                let keyboard = matches!(event, ClickEvent::Keyboard(_));
                                this.update_session(window, cx, |session, now| {
                                    session.close_sidebar(keyboard, now)
                                })
                            },
                        )),
                    )
                    .child(
                        Button::icon(CLOSE_ID, IconName::Close, i18n.t("lightbox.closePreview"))
                            .size(sidebar::BUTTON_SIZE)
                            .debug_selector(CLOSE_ID)
                            .theme(frame.theme)
                            .disabled(frame.deleting)
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    ),
            );

        let group = div()
            .debug_selector(|| GROUP_PANEL_SELECTOR.to_owned())
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(sidebar::GROUP_GAP))
            .child(heading(
                i18n.t("lightbox.mediaGroup"),
                TYPOGRAPHY.xs,
                colors,
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(sidebar::GROUP_GAP))
                    .child(
                        div()
                            .debug_selector(|| GROUP_KEY_SELECTOR.to_owned())
                            .flex_1()
                            .min_w_0()
                            .child(self.group_key.clone()),
                    )
                    .child(
                        Button::icon(
                            COPY_GROUP_ID,
                            if frame.copy_confirmed {
                                IconName::CheckSquare
                            } else {
                                IconName::Copy
                            },
                            frame.copy_title.clone(),
                        )
                        .size(sidebar::BUTTON_SIZE)
                        .active(frame.copy_confirmed)
                        .debug_selector(COPY_GROUP_ID)
                        .theme(frame.theme)
                        .disabled(!frame.can_copy)
                        .on_click(cx.listener(|this, _, window, cx| this.copy_group(window, cx))),
                    )
                    .child(
                        Button::icon(
                            GENERATE_GROUP_ID,
                            IconName::Reset,
                            i18n.t("lightbox.generateGroupAria"),
                        )
                        .size(sidebar::BUTTON_SIZE)
                        .debug_selector(GENERATE_GROUP_ID)
                        .theme(frame.theme)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.command(
                                LightboxCommand::SetMediaGroupKeyDraft(uuid_v4(random_bits())),
                                cx,
                            )
                        })),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| GROUP_ORDER_SELECTOR.to_owned())
                    .child(self.group_order.clone()),
            )
            .child(
                Button::new(APPLY_GROUP_ID, i18n.t("common.apply"))
                    .variant(ButtonVariant::Primary)
                    .height(sidebar::FIELD_HEIGHT)
                    .text_size(TYPOGRAPHY.xs.size)
                    .full_width(true)
                    .debug_selector(APPLY_GROUP_ID)
                    .theme(frame.theme)
                    .disabled(shown.group_pending || !shown.group_valid)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.command(LightboxCommand::SaveMediaGroup, cx)
                    })),
            );

        let status = |selector: &'static str, text: String| {
            div()
                .id(selector)
                .debug_selector(move || selector.to_owned())
                .role(Role::Status)
                .text_size(px(TYPOGRAPHY.xs.size))
                .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                .text_color(color(colors.text_muted))
                .child(text)
        };
        let mut tags = div()
            .debug_selector(|| TAG_PANEL_SELECTOR.to_owned())
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(sidebar::TAG_GAP))
            .pt(px(sidebar::TAG_PADDING_TOP))
            .border_t_1()
            .border_color(color(colors.border_soft))
            .child(heading(
                i18n.t("lightbox.taggingHeading"),
                TYPOGRAPHY.xs,
                colors,
            ))
            .child(self.editor.clone());
        if frame.tag_invalid {
            tags = tags.child(
                Alert::new(
                    TAG_INVALID_SELECTOR,
                    i18n.t("search.errors.tagInvalidCharacters"),
                )
                .tone(AlertTone::Error)
                .theme(frame.theme)
                .debug_selector(TAG_INVALID_SELECTOR),
            );
        }
        if shown.tag_details_loading {
            tags = tags.child(status(
                TAG_DETAILS_LOADING_SELECTOR,
                i18n.t("lightbox.tagDetailsLoading"),
            ));
        }
        if shown.tag_details_failed {
            tags = tags.child(
                Alert::new(
                    TAG_DETAILS_FAILED_SELECTOR,
                    i18n.t("lightbox.tagDetailsLoadFailed"),
                )
                .tone(AlertTone::Error)
                .title(i18n.t("lightbox.tagDetailsLoadFailedTitle"))
                .theme(frame.theme)
                .debug_selector(TAG_DETAILS_FAILED_SELECTOR)
                .child(
                    Button::new(RETRY_TAG_DETAILS_ID, i18n.t("lightbox.retryTagDetails"))
                        .debug_selector(RETRY_TAG_DETAILS_ID)
                        .theme(frame.theme)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.command(LightboxCommand::RetryDetails, cx)
                        })),
                ),
            );
        }
        if shown.tag_saving {
            tags = tags.child(status(TAG_SAVING_SELECTOR, i18n.t("lightbox.tagSaving")));
        }
        if shown.tag_failed {
            tags = tags.child(
                Alert::new(TAG_FAILED_SELECTOR, i18n.t("bulk.panel.tagSaveFailed"))
                    .tone(AlertTone::Error)
                    .title(i18n.t("bulk.panel.saveFailedTitle"))
                    .theme(frame.theme)
                    .debug_selector(TAG_FAILED_SELECTOR)
                    .child(
                        Button::new(RETRY_TAGS_ID, i18n.t("lightbox.retryTagSave"))
                            .debug_selector(RETRY_TAGS_ID)
                            .theme(frame.theme)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.command(LightboxCommand::RetryTags, cx)
                            })),
                    ),
            );
        }

        let info_toggle = {
            let id = ElementId::from(INFO_TOGGLE_ID);
            let focus = keyed_focus_handle(&id, window, cx);
            let focus_visible = focus.is_focused(window) && window.last_input_was_keyboard();
            div()
                .id(id)
                .debug_selector(|| INFO_TOGGLE_ID.to_owned())
                .role(Role::Button)
                .aria_expanded(frame.info_open)
                .track_focus(&focus)
                .relative()
                .flex()
                .flex_none()
                .items_center()
                .justify_between()
                .py(px(sidebar::INFO_TOGGLE_PADDING_Y))
                .border_t_1()
                .border_color(color(colors.border_soft))
                .text_size(px(TYPOGRAPHY.xs.size))
                .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                .text_color(color(colors.text_muted))
                .cursor_pointer()
                .when(focus_visible, |toggle| {
                    toggle.child(focus_ring(FocusRing::base(tokens), RADII.field))
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.update_session(window, cx, |session, _| session.toggle_info())
                }))
                .child(i18n.t("lightbox.infoHeading"))
                .child(if frame.info_open { "\u{2212}" } else { "+" })
        };

        let mut upper = div()
            .id(SIDEBAR_UPPER_SELECTOR)
            .debug_selector(|| SIDEBAR_UPPER_SELECTOR.to_owned())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap(px(sidebar::SECTION_GAP))
            .pr(px(sidebar::SCROLL_PADDING_RIGHT))
            .child(group)
            .child(tags)
            .child(info_toggle);
        if frame.info_open {
            upper = upper.child(
                div()
                    .debug_selector(|| INFO_PANEL_SELECTOR.to_owned())
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap(px(info::GAP))
                    .rounded(px(RADII.control))
                    .border_1()
                    .border_color(color(colors.border_soft))
                    .bg(color(colors.surface_muted))
                    .p(px(info::PADDING))
                    .text_size(px(TYPOGRAPHY.xs.size))
                    .text_color(color(colors.text_muted))
                    .child(heading(
                        i18n.t("lightbox.infoHeading"),
                        TYPOGRAPHY.xs,
                        colors,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(info::ROW_GAP))
                            .font_family(fonts::MONO)
                            .children(info_rows(&shown.selected, i18n).into_iter().map(|row| {
                                if row.path {
                                    div()
                                        .line_height(relative(info::PATH_LINE_HEIGHT))
                                        .child(format!("{}: {}", row.label, row.value))
                                } else {
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(TYPOGRAPHY.xs.size / 2.0))
                                        .child(
                                            div()
                                                .font_weight(weight(TYPOGRAPHY.weight_semibold))
                                                .text_color(color(colors.base_content))
                                                .child(format!("{}:", row.label)),
                                        )
                                        .child(row.value)
                                }
                            })),
                    ),
            );
        }

        let mut panel = div()
            .id(SIDEBAR_SELECTOR)
            .debug_selector(|| SIDEBAR_SELECTOR.to_owned())
            .aria_label(SharedString::from(i18n.t("lightbox.sidebarAria")))
            // What the panel contains has the focus or not (`sidebar_focused`).
            .track_focus(&self.sidebar_focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(sidebar::GAP))
            .p(px(sidebar::PADDING))
            .border_l_1()
            .border_color(color(colors.border_soft))
            .bg(color(colors.surface_solid))
            .text_color(color(colors.base_content))
            // A press on the drawer is none on the scrim or the media beneath it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(upper);

        if frame.confirm_open {
            let word = i18n.t("lightbox.deleteConfirm.confirmWord");
            let label = i18n.t_with(
                "lightbox.deleteConfirm.typeYesLabel",
                &Params::new().with("value", &word),
            );
            let mut section = div()
                .id(CONFIRM_SELECTOR)
                .debug_selector(|| CONFIRM_SELECTOR.to_owned())
                .aria_label(SharedString::from(i18n.t("lightbox.deleteConfirm.heading")))
                .flex()
                .flex_col()
                .flex_none()
                .overflow_y_scroll()
                .max_h(px((frame.viewport_height
                    - sidebar::CONFIRM_VIEWPORT_INSET)
                    .max(0.0)))
                .gap(px(confirm::GAP))
                .rounded(px(RADII.control))
                .border_1()
                .border_color(color(
                    colors.error.with_alpha_percent(confirm::BORDER_PERCENT),
                ))
                .bg(color(
                    colors
                        .base_200
                        .with_alpha_percent(confirm::BACKGROUND_PERCENT),
                ))
                .p(px(confirm::PADDING))
                .child(heading(
                    i18n.t("lightbox.deleteConfirm.heading"),
                    TYPOGRAPHY.sm,
                    colors,
                ))
                .child(
                    div()
                        .text_size(px(confirm::DESCRIPTION_SIZE))
                        .line_height(relative(confirm::DESCRIPTION_LINE_HEIGHT))
                        .text_color(color(colors.text_muted))
                        .child(i18n.t("lightbox.deleteConfirm.description")),
                );
            if let Some(error) = frame.confirm_error.clone() {
                section = section.child(
                    Alert::new(CONFIRM_ERROR_SELECTOR, error)
                        .tone(AlertTone::Error)
                        .theme(frame.theme)
                        .debug_selector(CONFIRM_ERROR_SELECTOR),
                );
            }
            section = section
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(confirm::FIELD_GAP))
                        .child(
                            div()
                                .text_size(px(TYPOGRAPHY.xs.size))
                                .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                                .text_color(color(colors.text_muted))
                                .child(label),
                        )
                        .child(
                            div()
                                .debug_selector(|| CONFIRM_INPUT_SELECTOR.to_owned())
                                .child(self.confirm_input.clone()),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(confirm::BUTTON_GAP))
                        .child(
                            Button::new(CONFIRM_CANCEL_ID, i18n.t("common.cancel"))
                                .debug_selector(CONFIRM_CANCEL_ID)
                                .theme(frame.theme)
                                .disabled(frame.deleting)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.update_session(window, cx, |session, now| {
                                        session.close_delete_confirm(now);
                                    })
                                })),
                        )
                        .child(
                            Button::new(
                                CONFIRM_SUBMIT_ID,
                                i18n.t("lightbox.deleteConfirm.confirm"),
                            )
                            .variant(ButtonVariant::Danger)
                            .debug_selector(CONFIRM_SUBMIT_ID)
                            .theme(frame.theme)
                            .disabled(frame.deleting || !frame.can_confirm)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.submit_delete(window, cx)),
                            ),
                        ),
                );
            panel = panel.child(section);
        }
        // Sliding out: still drawn, no longer interactive (`inert`).
        if !frame.sidebar_open {
            panel = panel.child(
                div()
                    .id("lightbox-sidebar-inert")
                    .absolute()
                    .inset_0()
                    .occlude(),
            );
        }
        panel.into_any_element()
    }
}
