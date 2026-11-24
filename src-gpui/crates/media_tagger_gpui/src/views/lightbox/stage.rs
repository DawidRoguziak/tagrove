//! The media column of the lightbox (`LightboxMediaStage.tsx`,
//! `LightboxActionRail.tsx` and what `LightboxModal.tsx` floats over them): the stage
//! with the picture, the notices that take its place, the reopen and close buttons,
//! the failure alerts, the navigation notice and the action rail.
//!
//! An image is drawn from its decoded pixels at the rectangle the session's viewer
//! gives (fit, zoom and pan), clipped by the stage. A GIF is its first frame with a
//! chip, a video a placeholder with its thumbnail as a poster, the Video label and
//! its metadata: nothing animates or plays here (plan decision D8, epics E20 and E21).

use gpui::{
    canvas, div, img, prelude::*, px, AnyElement, ClickEvent, Context, Corners, MouseButton,
    MouseDownEvent, ObjectFit, Role, ScrollEvent, SharedString, Window,
};
use media_tagger_client::{
    controls::{AlertTone, ChipTone, SpinnerSize, THUMBNAIL_LOADER},
    gallery::layout::thumbnail_key,
    icons::IconName,
    lightbox::{
        image_controls::WHEEL_LINE,
        layout::{floating, rail, stage, video},
        media_summary, video_placeholder_lines, LightboxCommand, NavigationStatus, StageContent,
        WheelMode, GIF_PLACEHOLDER_KEY, VIDEO_PLACEHOLDER_KEY,
    },
    theme::{Rgba, RADII, TYPOGRAPHY},
};

use super::{
    measure_stage, Frame, LightboxView, DELETE_ID, DETAILS_ERROR_SELECTOR,
    DETAILS_LOADING_SELECTOR, DETAILS_NOTICE_SELECTOR, FAVORITE_FAILED_SELECTOR, FAVORITE_ID,
    FLOATING_CLOSE_ID, FULLSCREEN_ID, GIF_CHIP_SELECTOR, GROUP_FAILED_SELECTOR, IMAGE_SELECTOR,
    MEDIA_COLUMN_SELECTOR, MEDIA_ERROR_SELECTOR, MEDIA_LOADING_SELECTOR, NAVIGATION_RETRY_ID,
    NAVIGATION_SELECTOR, RAIL_SELECTOR, RESET_ZOOM_ID, RETRY_DETAILS_ID, STAGE_SELECTOR,
    SUMMARY_SELECTOR, TRIGGER_ID, VIDEO_PLACEHOLDER_SELECTOR, VIDEO_POSTER_SELECTOR,
};
use crate::{
    thumbs::TilePicture,
    ui::{
        alert::Alert,
        button::Button,
        chip::Chip,
        fonts,
        spinner::Spinner,
        theme::{box_shadows, color, weight},
    },
};

/// The light text on the video's dark surface, whatever the theme.
const VIDEO_TEXT: Rgba = Rgba::hex(0xf0f0e8);

impl LightboxView {
    /// The wheel over the stage zooms the picture at the pointer.
    fn stage_wheel(&mut self, event: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        // GPUI's delta is how far the content moves: the opposite of a DOM wheel
        // event's `deltaY`. Lines become pixels as the Solid viewer counts them.
        let delta = event.delta.pixel_delta(px(WHEEL_LINE as f32));
        let delta_y = -f64::from(f32::from(delta.y));
        session.input(now);
        let taken = session.controls_mut().wheel(
            delta_y,
            WheelMode::Pixels,
            f64::from(f32::from(event.position.x)),
            f64::from(f32::from(event.position.y)),
            now,
        );
        if taken {
            cx.stop_propagation();
        }
        self.arm_timer(window, cx);
        cx.notify();
    }

    /// A press on the stage starts the drag of a zoomed picture.
    fn stage_pressed(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        let started = session.controls_mut().pointer_down(
            true,
            f64::from(f32::from(event.position.x)),
            f64::from(f32::from(event.position.y)),
            now,
        );
        if started {
            cx.notify();
        }
    }

    /// A click on the picture zooms one step in at the pointer.
    fn image_clicked(&mut self, event: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let Some(session) = &mut self.session else {
            return;
        };
        let position = event.position();
        let zoomed = session.controls_mut().click(
            f64::from(f32::from(position.x)),
            f64::from(f32::from(position.y)),
            now,
        );
        if zoomed {
            cx.notify();
        }
    }

    /// A wheel turn over what floats beside the stage (the reopen and close buttons,
    /// the save-failure alerts, the navigation notice): it shows the controls, as
    /// every wheel turn does, and goes no further. In `LightboxModal.tsx` these are
    /// siblings of the stage, so its wheel listener never hears them; GPUI gives a
    /// wheel turn to every hitbox under the pointer, whatever blocks the other mouse
    /// events, until a listener stops it. The dialog's own wheel listener is behind
    /// that stop, so the activity is counted here.
    fn floating_wheel(&mut self, _: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.activity(window, cx);
        cx.stop_propagation();
    }

    fn rail_hover(&mut self, hovered: bool, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        if let Some(session) = &mut self.session {
            session.rail_hover(hovered, now);
        }
        self.arm_timer(window, cx);
        cx.notify();
    }

    /// The media column: the stage above the action rail, and what floats over both.
    pub(super) fn media_column(
        &mut self,
        frame: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = frame.theme.tokens();
        let colors = &tokens.colors;
        let i18n = &frame.i18n;
        let is_video = frame.kind.is_video();
        let surface = if is_video {
            video::SURFACE
        } else {
            colors.surface_muted
        };
        let stage = self.stage(frame, surface, window, cx);
        let rail = self.rail(frame, cx);

        let mut column = div()
            .debug_selector(|| MEDIA_COLUMN_SELECTOR.to_owned())
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(color(surface))
            // The strip that keeps the floating buttons clear of a video's surface.
            .when(is_video && !frame.fullscreen, |column| {
                column.pt(px(video::TOP_RESERVE))
            })
            .child(stage)
            .child(rail);

        if !frame.sidebar_open {
            column = column.child(
                div()
                    .absolute()
                    .right(px(floating::CONTROLS_RIGHT))
                    .top(px(floating::CONTROLS_TOP))
                    .flex()
                    .gap(px(floating::CONTROLS_GAP))
                    .opacity(frame.controls_opacity)
                    // What floats over the stage is not the stage's child in the DOM:
                    // a press or a click on it is not the picture's as well. GPUI
                    // gives a click to every hitbox under the pointer down to the
                    // first that blocks it, and a wheel turn to all of them until
                    // a listener stops it. The faded buttons block nothing
                    // (`pointer-events: none`).
                    .when(frame.controls_visible, |controls| {
                        controls
                            .block_mouse_except_scroll()
                            .on_scroll(cx.listener(Self::floating_wheel))
                    })
                    .child(
                        Button::icon(
                            TRIGGER_ID,
                            IconName::ArrowLeft,
                            i18n.t("lightbox.openPanel"),
                        )
                        .size(floating::CONTROL_SIZE)
                        .track_focus(&self.trigger_focus)
                        .debug_selector(TRIGGER_ID)
                        .theme(frame.theme)
                        .on_click(cx.listener(
                            |this, event, window, cx| {
                                if this.floating_click_counts(event) {
                                    this.update_session(window, cx, |session, now| {
                                        session.open_sidebar(now)
                                    })
                                }
                            },
                        )),
                    )
                    .child(
                        Button::icon(
                            FLOATING_CLOSE_ID,
                            IconName::Close,
                            i18n.t("lightbox.closePreview"),
                        )
                        .size(floating::CONTROL_SIZE)
                        .debug_selector(FLOATING_CLOSE_ID)
                        .theme(frame.theme)
                        .disabled(frame.deleting)
                        .on_click(cx.listener(|this, event, _, cx| {
                            if this.floating_click_counts(event) {
                                this.close(cx)
                            }
                        })),
                    ),
            );
        }

        let shown = &frame.shown;
        if shown.favorite_failed || shown.group_failed {
            let column_width = self
                .stage_bounds
                .get()
                .map(|bounds| f32::from(bounds.size.width))
                .unwrap_or(frame.shell_width);
            let alert = |selector: &'static str, message: &str| {
                div()
                    .rounded(px(RADII.control))
                    .shadow(box_shadows(tokens.shadows.floating))
                    .child(
                        Alert::new(selector, i18n.t(message))
                            .tone(AlertTone::Error)
                            .title(i18n.t("lightbox.saveFailedTitle"))
                            .theme(frame.theme)
                            .debug_selector(selector),
                    )
            };
            column = column.child(
                div()
                    .absolute()
                    .left(px(floating::ALERTS_INSET))
                    .top(px(floating::ALERTS_INSET))
                    .flex()
                    .flex_col()
                    .gap(px(floating::ALERTS_GAP))
                    .max_w(px(floating::alerts_max_width(column_width)))
                    .block_mouse_except_scroll()
                    .on_scroll(cx.listener(Self::floating_wheel))
                    .when(shown.favorite_failed, |alerts| {
                        alerts.child(alert(
                            FAVORITE_FAILED_SELECTOR,
                            "lightbox.favoriteSaveFailed",
                        ))
                    })
                    .when(shown.group_failed, |alerts| {
                        alerts.child(alert(
                            GROUP_FAILED_SELECTOR,
                            "lightbox.mediaGroupSaveFailed",
                        ))
                    }),
            );
        }

        if let Some(key) = shown.navigation.notice_key() {
            column = column.child(
                div()
                    .id(NAVIGATION_SELECTOR)
                    .debug_selector(|| NAVIGATION_SELECTOR.to_owned())
                    .role(Role::Status)
                    .block_mouse_except_scroll()
                    .on_scroll(cx.listener(Self::floating_wheel))
                    .absolute()
                    .left(px(floating::NOTICE_INSET))
                    .bottom(px(floating::NOTICE_INSET))
                    .flex()
                    .items_center()
                    .gap(px(floating::NOTICE_BUTTON_MARGIN))
                    .rounded(px(floating::NOTICE_RADIUS))
                    .bg(color(colors.base_100))
                    .p(px(floating::NOTICE_PADDING))
                    .shadow(box_shadows(floating::NOTICE_SHADOW))
                    .text_color(color(colors.base_content))
                    .child(i18n.t(key))
                    .when(shown.navigation == NavigationStatus::Failed, |notice| {
                        notice.child(
                            Button::new(NAVIGATION_RETRY_ID, i18n.t("gallery.retry"))
                                .debug_selector(NAVIGATION_RETRY_ID)
                                .theme(frame.theme)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.command(LightboxCommand::RetryNavigation, cx)
                                })),
                        )
                    }),
            );
        }
        column.into_any_element()
    }

    /// The stage: the picture or what stands in for it.
    fn stage(
        &mut self,
        frame: &Frame,
        surface: Rgba,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = frame.theme.tokens();
        let colors = &tokens.colors;
        let i18n = &frame.i18n;
        let shown = &frame.shown;
        let is_video = frame.kind.is_video();

        let mut stage_element = div()
            .id(STAGE_SELECTOR)
            .debug_selector(|| STAGE_SELECTOR.to_owned())
            // After the floating buttons in the Tab order (`LightboxView::render`).
            .tab_group()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(color(surface))
            .child(measure_stage(
                self.stage_bounds.clone(),
                cx.entity().downgrade(),
            ));
        if frame.active && !is_video {
            stage_element = stage_element
                .on_scroll(cx.listener(Self::stage_wheel))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::stage_pressed));
            if frame.zoomed {
                stage_element = if frame.dragging {
                    stage_element.cursor_grabbing()
                } else {
                    stage_element.cursor_grab()
                };
            }
        }

        let centered = || {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
        };
        let notice = |selector: &'static str, edge: Rgba| {
            div()
                .id(selector)
                .debug_selector(move || selector.to_owned())
                .role(Role::Alert)
                .flex()
                .flex_col()
                .items_center()
                .gap(px(stage::NOTICE_GAP))
                .rounded(px(RADII.control))
                .border_1()
                .border_color(color(edge.with_alpha_percent(stage::NOTICE_BORDER_PERCENT)))
                .bg(color(
                    edge.with_alpha_percent(stage::NOTICE_BACKGROUND_PERCENT),
                ))
                .px(px(stage::NOTICE_PADDING_X))
                .py(px(stage::NOTICE_PADDING_Y))
                .text_size(px(TYPOGRAPHY.sm.size))
                .line_height(px(TYPOGRAPHY.sm.line_height_px()))
                .font_weight(weight(TYPOGRAPHY.weight_semibold))
                .text_color(color(colors.base_content))
        };

        stage_element = match frame.content {
            StageContent::DetailsLoading => stage_element.child(
                centered().child(
                    div()
                        .debug_selector(|| DETAILS_LOADING_SELECTOR.to_owned())
                        .child(
                            Spinner::new("lightbox-details-spinner", SpinnerSize::Stage)
                                .theme(frame.theme),
                        ),
                ),
            ),
            StageContent::DetailsFailed => stage_element.child(
                centered().p(px(stage::NOTICE_MARGIN)).child(
                    notice(DETAILS_ERROR_SELECTOR, colors.warning)
                        .child(i18n.t("lightbox.assetDetailsLoadFailed"))
                        .child(
                            Button::new(RETRY_DETAILS_ID, i18n.t("lightbox.retryAssetDetails"))
                                .debug_selector(RETRY_DETAILS_ID)
                                .theme(frame.theme)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.command(LightboxCommand::RetryDetails, cx)
                                })),
                        ),
                ),
            ),
            StageContent::MediaFailed => stage_element.child(
                centered().p(px(stage::NOTICE_MARGIN)).child(
                    notice(MEDIA_ERROR_SELECTOR, colors.error)
                        .child(i18n.t("lightbox.mediaError.image")),
                ),
            ),
            // A picture that is there at once never shows it: the ring appears after
            // a delay, as a tile's does.
            StageContent::Pending => stage_element.child(
                centered().child(
                    div()
                        .debug_selector(|| MEDIA_LOADING_SELECTOR.to_owned())
                        .child(
                            Spinner::new("lightbox-media-spinner", SpinnerSize::Stage)
                                .reveal(THUMBNAIL_LOADER)
                                .theme(frame.theme),
                        ),
                ),
            ),
            StageContent::Image | StageContent::Gif => {
                let picture = self.originals.read(cx).picture().cloned();
                let stage_element = match (frame.image_rect, picture) {
                    (Some(rect), Some(picture)) => {
                        let originals = self.originals.downgrade();
                        let image = div()
                            .id(IMAGE_SELECTOR)
                            .debug_selector(|| IMAGE_SELECTOR.to_owned())
                            .role(Role::Image)
                            .aria_label(SharedString::from(
                                shown.selected.summary.file_name.clone(),
                            ))
                            .absolute()
                            .left(px(rect.left as f32))
                            .top(px(rect.top as f32))
                            .w(px(rect.width as f32))
                            .h(px(rect.height as f32))
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, cx| {
                                        let drawn = window.paint_image(
                                            bounds,
                                            bounds,
                                            Corners::default(),
                                            picture.clone(),
                                            0,
                                            false,
                                        );
                                        if drawn.is_err() {
                                            // The GPU takes no texture of this size.
                                            let originals = originals.clone();
                                            window.defer(cx, move |_, cx| {
                                                originals
                                                    .update(cx, |originals, cx| {
                                                        originals.draw_failed(cx)
                                                    })
                                                    .ok();
                                            });
                                        }
                                    },
                                )
                                .size_full(),
                            );
                        let image = if frame.active {
                            let image = image.on_click(cx.listener(Self::image_clicked));
                            if frame.zoomed {
                                image
                            } else {
                                // `cursor-zoom-in` has no GPUI cursor; the hand says
                                // that a click does something.
                                image.cursor_pointer()
                            }
                        } else {
                            image
                        };
                        stage_element.child(image)
                    }
                    _ => stage_element,
                };
                if frame.content == StageContent::Gif {
                    stage_element.child(
                        div()
                            .id(GIF_CHIP_SELECTOR)
                            .debug_selector(|| GIF_CHIP_SELECTOR.to_owned())
                            .aria_label(SharedString::from(i18n.t(GIF_PLACEHOLDER_KEY)))
                            .absolute()
                            .left(px(stage::CHIP_INSET))
                            .top(px(stage::CHIP_INSET))
                            .child(
                                Chip::new(i18n.t("gallery.gifChip"))
                                    .tone(ChipTone::Gif)
                                    .theme(frame.theme),
                            ),
                    )
                } else {
                    stage_element
                }
            }
            StageContent::Video => {
                let summary = &shown.selected.summary;
                let (stage_width, stage_height) = self
                    .stage_bounds
                    .get()
                    .map(|bounds| (f32::from(bounds.size.width), f32::from(bounds.size.height)))
                    .unwrap_or_default();
                let (poster_width, poster_height) =
                    stage::poster_size(summary.width, summary.height, stage_width, stage_height);
                let poster = div()
                    .debug_selector(|| VIDEO_POSTER_SELECTOR.to_owned())
                    .flex_none()
                    .w(px(poster_width))
                    .h(px(poster_height))
                    .rounded(px(RADII.tile))
                    .overflow_hidden()
                    .bg(color(VIDEO_TEXT.with_alpha_percent(6)));
                let poster = match self.thumbnails.read(cx).picture(&thumbnail_key(summary)) {
                    TilePicture::Ready(picture) => {
                        poster.child(img(picture).size_full().object_fit(ObjectFit::Contain))
                    }
                    TilePicture::Loading | TilePicture::Empty => poster,
                };
                let muted = color(VIDEO_TEXT.with_alpha_percent(stage::PLACEHOLDER_TEXT_PERCENT));
                stage_element.child(
                    centered().child(
                        div()
                            .id(VIDEO_PLACEHOLDER_SELECTOR)
                            .debug_selector(|| VIDEO_PLACEHOLDER_SELECTOR.to_owned())
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(stage::PLACEHOLDER_GAP))
                            .child(poster)
                            .child(
                                Chip::new(i18n.t("lightbox.kind.video"))
                                    .tone(ChipTone::Video)
                                    .theme(frame.theme),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(stage::PLACEHOLDER_LINE_GAP))
                                    .font_family(fonts::MONO)
                                    .text_size(px(TYPOGRAPHY.xs.size))
                                    .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                                    .text_color(color(VIDEO_TEXT))
                                    .children(
                                        video_placeholder_lines(&shown.selected, i18n)
                                            .into_iter()
                                            .map(|line| div().child(line)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(TYPOGRAPHY.xs.size))
                                    .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                                    .text_color(muted)
                                    .child(i18n.t(VIDEO_PLACEHOLDER_KEY)),
                            ),
                    ),
                )
            }
        };

        // The details failed beside a medium that is shown from its summary.
        if shown.selected.path.is_some() && shown.asset_details_failed {
            stage_element = stage_element.child(
                div()
                    .id(DETAILS_NOTICE_SELECTOR)
                    .debug_selector(|| DETAILS_NOTICE_SELECTOR.to_owned())
                    .role(Role::Alert)
                    // Beside the picture in the DOM, not over it: its Retry is not a
                    // click on the picture. It is inside the stage, though, whose
                    // wheel listener hears a wheel turn over it: that one zooms.
                    .block_mouse_except_scroll()
                    .absolute()
                    .left(px(floating::DETAILS_NOTICE_INSET))
                    .bottom(px(if is_video {
                        video::DETAILS_NOTICE_BOTTOM
                    } else {
                        floating::DETAILS_NOTICE_INSET
                    }))
                    .flex()
                    .items_center()
                    .gap(px(floating::DETAILS_NOTICE_GAP))
                    .rounded(px(RADII.control))
                    .border_1()
                    .border_color(color(
                        colors
                            .warning
                            .with_alpha_percent(floating::DETAILS_NOTICE_BORDER_PERCENT),
                    ))
                    .bg(color(colors.base_100.with_alpha_percent(
                        floating::DETAILS_NOTICE_BACKGROUND_PERCENT,
                    )))
                    .px(px(floating::DETAILS_NOTICE_PADDING_X))
                    .py(px(floating::DETAILS_NOTICE_PADDING_Y))
                    .text_size(px(TYPOGRAPHY.xs.size))
                    .text_color(color(colors.base_content))
                    .shadow(box_shadows(tokens.shadows.floating))
                    .child(i18n.t("lightbox.assetDetailsLoadFailed"))
                    .child(
                        Button::new(RETRY_DETAILS_ID, i18n.t("lightbox.retryAssetDetails"))
                            .debug_selector(RETRY_DETAILS_ID)
                            .theme(frame.theme)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.command(LightboxCommand::RetryDetails, cx)
                            })),
                    ),
            );
        }
        stage_element.into_any_element()
    }

    /// The action rail under the stage: favorite, reset zoom, the summary, fullscreen
    /// and delete on one island.
    fn rail(&mut self, frame: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let tokens = frame.theme.tokens();
        let colors = &tokens.colors;
        let i18n = &frame.i18n;
        let shown = &frame.shown;
        let compact = rail::compact(frame.viewport_width);
        let favorite = shown.selected.summary.is_favorite;

        let summary = div()
            .debug_selector(|| SUMMARY_SELECTOR.to_owned())
            .flex()
            .flex_wrap()
            .justify_center()
            .gap(px(rail::SUMMARY_GAP))
            .px(px(rail::SUMMARY_PADDING_X))
            .py(px(rail::SUMMARY_PADDING_Y))
            .rounded(px(rail::SUMMARY_RADIUS))
            .bg(color(colors.surface_muted))
            .text_color(color(colors.text_muted))
            .font_family(fonts::MONO)
            .text_size(px(rail::SUMMARY_FONT_SIZE))
            .when(compact, |summary| summary.w_full())
            .children(
                media_summary(&shown.selected, i18n)
                    .into_iter()
                    .enumerate()
                    .map(|(index, part)| {
                        div()
                            .when(index == 0, |part| {
                                part.text_color(color(colors.primary_text))
                            })
                            .child(part)
                    }),
            );

        let heart = Button::icon(
            FAVORITE_ID,
            IconName::Heart,
            i18n.t(if favorite {
                "lightbox.favorite.remove"
            } else {
                "lightbox.favorite.add"
            }),
        )
        .size(rail::BUTTON_SIZE)
        .active(favorite)
        .inert(frame.media_inert)
        .debug_selector(FAVORITE_ID)
        .theme(frame.theme)
        .disabled(shown.favorite_pending)
        .on_click(cx.listener(|this, _, _, cx| this.command(LightboxCommand::ToggleFavorite, cx)));
        let reset = Button::icon(RESET_ZOOM_ID, IconName::Reset, i18n.t("lightbox.resetZoom"))
            .size(rail::BUTTON_SIZE)
            .inert(frame.media_inert)
            .debug_selector(RESET_ZOOM_ID)
            .theme(frame.theme)
            .disabled(frame.kind.is_video())
            .on_click(cx.listener(|this, _, window, cx| {
                this.update_session(window, cx, |session, _| session.controls_mut().reset())
            }));
        let fullscreen = Button::icon(
            FULLSCREEN_ID,
            IconName::Fullscreen,
            i18n.t("lightbox.toggleFullscreen"),
        )
        .size(rail::BUTTON_SIZE)
        .active(frame.fullscreen)
        .inert(frame.media_inert)
        .debug_selector(FULLSCREEN_ID)
        .theme(frame.theme)
        .on_click(cx.listener(|this, _, window, cx| this.toggle_fullscreen(window, cx)));
        let delete = Button::icon(DELETE_ID, IconName::Trash, i18n.t("lightbox.deleteMedia"))
            .size(rail::BUTTON_SIZE)
            .danger(true)
            .inert(frame.media_inert)
            .track_focus(&self.delete_focus)
            .debug_selector(DELETE_ID)
            .theme(frame.theme)
            .on_click(cx.listener(|this, _, window, cx| {
                this.update_session(window, cx, |session, now| session.open_delete_confirm(now))
            }));

        let island = div()
            .id(RAIL_SELECTOR)
            .debug_selector(|| RAIL_SELECTOR.to_owned())
            // After the floating buttons in the Tab order (`LightboxView::render`).
            .tab_group()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_center()
            .max_w_full()
            .gap(px(if compact {
                rail::COMPACT_ISLAND_GAP
            } else {
                rail::ISLAND_GAP
            }))
            .px(px(if compact {
                rail::COMPACT_ISLAND_PADDING
            } else {
                rail::ISLAND_PADDING_X
            }))
            .py(px(if compact {
                rail::COMPACT_ISLAND_PADDING
            } else {
                rail::ISLAND_PADDING_Y
            }))
            .border_1()
            .border_color(color(colors.border_soft))
            .rounded(px(RADII.panel))
            .bg(color(colors.surface_raised))
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                this.rail_hover(*hovered, window, cx)
            }));
        // In a compact rail the summary takes the first row (`order: -1`).
        let island = if compact {
            island
                .child(summary)
                .child(heart)
                .child(reset)
                .child(fullscreen)
                .child(delete)
        } else {
            island
                .child(heart)
                .child(reset)
                .child(summary)
                .child(fullscreen)
                .child(delete)
        };

        let row = div()
            .flex()
            .flex_none()
            .justify_center()
            .opacity(frame.rail_opacity);
        let row = if compact {
            row.p(px(rail::COMPACT_ROW_PADDING))
        } else {
            row.pt(px(rail::ROW_PADDING_TOP))
                .px(px(rail::ROW_PADDING_X))
                .pb(px(rail::ROW_PADDING_BOTTOM))
        };
        row.child(island).into_any_element()
    }
}
