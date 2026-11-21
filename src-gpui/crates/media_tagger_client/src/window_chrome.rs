//! The window's own decorations (`WindowControls.tsx`, plan decision D14): the
//! minimise, maximise/restore and close buttons, the eight resize edges and the drag
//! surface of the header. The Tauri window is frameless and draws these itself; the
//! GPUI window asks for client-side decorations and draws the same.
//!
//! This module decides what is shown and what a gesture asks for; the GPUI adapter
//! draws it and calls the window. Measures are CSS pixels of `src/styles.css`.

use crate::{
    icons::IconName,
    theme::{Rgba, ThemeTokens},
};

/// The accessible names of the controls (`window.*` in the locale files).
pub const CONTROLS_LABEL_KEY: &str = "window.controls";
pub const MINIMIZE_LABEL_KEY: &str = "window.minimize";
pub const MAXIMIZE_LABEL_KEY: &str = "window.maximize";
pub const RESTORE_LABEL_KEY: &str = "window.restore";
pub const CLOSE_LABEL_KEY: &str = "window.close";

/// `.window-controls > button`: 32 × 32 ghost buttons, 2 px apart, after a 10 px
/// padding and a soft border on the left.
pub const CONTROL_SIZE: f32 = 32.0;
pub const CONTROLS_GAP: f32 = 2.0;
pub const CONTROLS_PADDING_LEFT: f32 = 10.0;

/// The thickness of an edge and the size of a corner of the resize frame.
pub const EDGE_THICKNESS: f32 = 4.0;
pub const CORNER_SIZE: f32 = 8.0;

/// What the window looks like now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowState {
    pub maximized: bool,
    pub fullscreen: bool,
}

impl WindowState {
    /// The resize edges exist only for a window that can be resized by them: neither
    /// maximised nor fullscreen.
    pub fn shows_resize_edges(self) -> bool {
        !self.maximized && !self.fullscreen
    }

    /// The middle button: restore a maximised window, else maximise it. A fullscreen
    /// window that is not maximised still offers to maximise, as in Tauri.
    pub fn maximize_control(self) -> MaximizeControl {
        if self.maximized {
            MaximizeControl {
                label_key: RESTORE_LABEL_KEY,
                icon: IconName::WindowRestore,
            }
        } else {
            MaximizeControl {
                label_key: MAXIMIZE_LABEL_KEY,
                icon: IconName::WindowMaximize,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaximizeControl {
    pub label_key: &'static str,
    pub icon: IconName,
}

/// The three buttons, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowControl {
    Minimize,
    ToggleMaximize,
    Close,
}

impl WindowControl {
    pub const ALL: [Self; 3] = [Self::Minimize, Self::ToggleMaximize, Self::Close];

    pub fn label_key(self, state: WindowState) -> &'static str {
        match self {
            Self::Minimize => MINIMIZE_LABEL_KEY,
            Self::ToggleMaximize => state.maximize_control().label_key,
            Self::Close => CLOSE_LABEL_KEY,
        }
    }

    pub fn icon(self, state: WindowState) -> IconName {
        match self {
            Self::Minimize => IconName::WindowMinimize,
            Self::ToggleMaximize => state.maximize_control().icon,
            Self::Close => IconName::Close,
        }
    }

    /// Background and icon colour: a ghost button, except that close turns red under
    /// the pointer (`.window-close:hover`).
    pub fn colors(self, tokens: &ThemeTokens, hovered: bool) -> ControlColors {
        let colors = &tokens.colors;
        match (self, hovered) {
            (Self::Close, true) => ControlColors {
                background: colors.error,
                foreground: colors.error_content,
            },
            (_, true) => ControlColors {
                background: colors.base_content.with_alpha_percent(10),
                foreground: colors.base_content,
            },
            (_, false) => ControlColors {
                background: Rgba::TRANSPARENT,
                foreground: colors.text_muted,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlColors {
    pub background: Rgba,
    pub foreground: Rgba,
}

/// Which edge or corner a resize grip pulls (`data-resize-direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeDirection {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

/// Where a grip lies, as the CSS insets of `.window-resize-edges`: a side set to
/// `Some` is that far from the window's side, a size set to `Some` is fixed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeRect {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl ResizeDirection {
    /// In the order of `WindowControls.tsx`; the corners are drawn last, above the
    /// edges.
    pub const ALL: [Self; 8] = [
        Self::North,
        Self::South,
        Self::East,
        Self::West,
        Self::NorthEast,
        Self::NorthWest,
        Self::SouthEast,
        Self::SouthWest,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::North => "North",
            Self::South => "South",
            Self::East => "East",
            Self::West => "West",
            Self::NorthEast => "NorthEast",
            Self::NorthWest => "NorthWest",
            Self::SouthEast => "SouthEast",
            Self::SouthWest => "SouthWest",
        }
    }

    pub fn rect(self) -> EdgeRect {
        let (edge, corner) = (Some(EDGE_THICKNESS), Some(CORNER_SIZE));
        let zero = Some(0.0);
        match self {
            Self::North => EdgeRect {
                top: zero,
                left: corner,
                right: corner,
                height: edge,
                ..EdgeRect::default()
            },
            Self::South => EdgeRect {
                bottom: zero,
                left: corner,
                right: corner,
                height: edge,
                ..EdgeRect::default()
            },
            Self::East => EdgeRect {
                right: zero,
                top: corner,
                bottom: corner,
                width: edge,
                ..EdgeRect::default()
            },
            Self::West => EdgeRect {
                left: zero,
                top: corner,
                bottom: corner,
                width: edge,
                ..EdgeRect::default()
            },
            Self::NorthEast => EdgeRect {
                top: zero,
                right: zero,
                width: corner,
                height: corner,
                ..EdgeRect::default()
            },
            Self::NorthWest => EdgeRect {
                top: zero,
                left: zero,
                width: corner,
                height: corner,
                ..EdgeRect::default()
            },
            Self::SouthEast => EdgeRect {
                bottom: zero,
                right: zero,
                width: corner,
                height: corner,
                ..EdgeRect::default()
            },
            Self::SouthWest => EdgeRect {
                bottom: zero,
                left: zero,
                width: corner,
                height: corner,
                ..EdgeRect::default()
            },
        }
    }

    /// The cursor over the grip (`n-resize` … `sw-resize`).
    pub fn cursor(self) -> ResizeCursor {
        match self {
            Self::North | Self::South => ResizeCursor::UpDown,
            Self::East | Self::West => ResizeCursor::LeftRight,
            Self::NorthWest | Self::SouthEast => ResizeCursor::UpLeftDownRight,
            Self::NorthEast | Self::SouthWest => ResizeCursor::UpRightDownLeft,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeCursor {
    UpDown,
    LeftRight,
    UpLeftDownRight,
    UpRightDownLeft,
}

/// The grips to draw in `state`.
pub fn resize_edges(state: WindowState) -> &'static [ResizeDirection] {
    if state.shows_resize_edges() {
        &ResizeDirection::ALL
    } else {
        &[]
    }
}

/// What a gesture on the drag surface asks the window for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragAction {
    /// The window manager moves the window from here on.
    StartMove,
    /// A double click maximises or restores, as on a native title bar.
    ToggleMaximize,
}

/// The drag surface of a header (`data-tauri-drag-region`). The move starts when the
/// pointer moves with the primary button held, not on the press itself, so a click
/// (and a double click) on the surface stays a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DragSurface {
    armed: bool,
}

impl DragSurface {
    /// The primary button went down on the surface; `click_count` counts the clicks of
    /// a multi-click.
    pub fn press(&mut self, click_count: usize) -> Option<DragAction> {
        if click_count >= 2 {
            self.armed = false;
            return Some(DragAction::ToggleMaximize);
        }
        self.armed = true;
        None
    }

    /// The pointer moved; `primary_held` says whether the primary button is still down.
    pub fn moved(&mut self, primary_held: bool) -> Option<DragAction> {
        if !std::mem::take(&mut self.armed) {
            return None;
        }
        primary_held.then_some(DragAction::StartMove)
    }

    pub fn release(&mut self) {
        self.armed = false;
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }
}

#[cfg(test)]
mod tests {
    use super::{
        resize_edges, DragAction, DragSurface, ResizeCursor, ResizeDirection, WindowControl,
        WindowState,
    };
    use crate::{icons::IconName, theme::ThemeName};

    const NORMAL: WindowState = WindowState {
        maximized: false,
        fullscreen: false,
    };
    const MAXIMIZED: WindowState = WindowState {
        maximized: true,
        fullscreen: false,
    };
    const FULLSCREEN: WindowState = WindowState {
        maximized: false,
        fullscreen: true,
    };

    #[test]
    fn the_maximize_button_follows_the_window_state() {
        assert_eq!(
            WindowControl::ToggleMaximize.label_key(NORMAL),
            "window.maximize"
        );
        assert_eq!(
            WindowControl::ToggleMaximize.icon(NORMAL),
            IconName::WindowMaximize
        );
        assert_eq!(
            WindowControl::ToggleMaximize.label_key(MAXIMIZED),
            "window.restore"
        );
        assert_eq!(
            WindowControl::ToggleMaximize.icon(MAXIMIZED),
            IconName::WindowRestore
        );
        assert_eq!(
            WindowControl::ToggleMaximize.label_key(FULLSCREEN),
            "window.maximize"
        );
        assert_eq!(
            WindowControl::Minimize.label_key(MAXIMIZED),
            "window.minimize"
        );
        assert_eq!(WindowControl::Close.icon(NORMAL), IconName::Close);
    }

    #[test]
    fn resize_edges_exist_only_for_a_normal_window() {
        assert_eq!(resize_edges(NORMAL).len(), 8);
        assert!(resize_edges(MAXIMIZED).is_empty());
        assert!(resize_edges(FULLSCREEN).is_empty());
    }

    #[test]
    fn grips_follow_the_stylesheet() {
        let north = ResizeDirection::North.rect();
        assert_eq!(
            (north.top, north.left, north.right, north.height),
            (Some(0.0), Some(8.0), Some(8.0), Some(4.0))
        );
        let south_east = ResizeDirection::SouthEast.rect();
        assert_eq!(
            (
                south_east.bottom,
                south_east.right,
                south_east.width,
                south_east.height
            ),
            (Some(0.0), Some(0.0), Some(8.0), Some(8.0))
        );
        assert_eq!(ResizeDirection::West.cursor(), ResizeCursor::LeftRight);
        assert_eq!(
            ResizeDirection::NorthEast.cursor(),
            ResizeCursor::UpRightDownLeft
        );
        let names: Vec<_> = ResizeDirection::ALL.iter().map(|d| d.name()).collect();
        assert_eq!(
            names,
            [
                "North",
                "South",
                "East",
                "West",
                "NorthEast",
                "NorthWest",
                "SouthEast",
                "SouthWest"
            ]
        );
    }

    #[test]
    fn grips_match_the_stylesheet_rules() {
        let css = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../src/styles.css"),
        )
        .expect("read styles.css");
        for direction in ResizeDirection::ALL {
            let selector = format!(
                ".window-resize-edges [data-resize-direction=\"{}\"] {{",
                direction.name()
            );
            let start = css.find(&selector).expect("a rule per direction") + selector.len();
            let rule = &css[start..start + css[start..].find('}').expect("rule end")];
            let rect = direction.rect();
            for (property, value) in [
                ("top", rect.top),
                ("right", rect.right),
                ("bottom", rect.bottom),
                ("left", rect.left),
                ("width", rect.width),
                ("height", rect.height),
            ] {
                let declared = rule.split(';').find_map(|declaration| {
                    let (name, value) = declaration.split_once(':')?;
                    (name.trim() == property)
                        .then(|| value.trim().trim_end_matches("px").to_owned())
                });
                assert_eq!(
                    declared.map(|text| text.parse::<f32>().expect("a length")),
                    value,
                    "{} {property}",
                    direction.name()
                );
            }
        }
    }

    #[test]
    fn close_turns_red_only_under_the_pointer() {
        let tokens = ThemeName::Dark.tokens();
        let close = WindowControl::Close.colors(tokens, true);
        assert_eq!(close.background, tokens.colors.error);
        assert_eq!(close.foreground, tokens.colors.error_content);
        let minimize = WindowControl::Minimize.colors(tokens, true);
        assert_ne!(minimize.background, tokens.colors.error);
        assert_eq!(
            WindowControl::Close.colors(tokens, false),
            WindowControl::Minimize.colors(tokens, false)
        );
    }

    #[test]
    fn a_press_and_a_move_start_the_window_move_once() {
        let mut surface = DragSurface::default();
        assert_eq!(surface.moved(true), None, "no press, no move");

        assert_eq!(surface.press(1), None);
        assert_eq!(surface.moved(true), Some(DragAction::StartMove));
        assert_eq!(
            surface.moved(true),
            None,
            "the window manager owns the move now"
        );

        surface.press(1);
        surface.release();
        assert_eq!(surface.moved(true), None, "a click is no move");

        surface.press(1);
        assert_eq!(
            surface.moved(false),
            None,
            "the button was let go elsewhere"
        );
    }

    #[test]
    fn a_double_click_toggles_maximize() {
        let mut surface = DragSurface::default();
        surface.press(1);
        assert_eq!(surface.press(2), Some(DragAction::ToggleMaximize));
        assert!(!surface.is_armed());
        assert_eq!(surface.moved(true), None);
    }
}
