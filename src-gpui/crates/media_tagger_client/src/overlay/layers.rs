//! The layer stack of `UiLayerProvider` (`createLayerManager`).
//!
//! Every open overlay registers a layer: a modal dialog, or a page that Escape closes
//! (settings) and the bulk mode, which are not modal. The newest layer is on top:
//!
//! - **Escape** goes to the top layer only. A top layer that does not close on Escape
//!   lets the key go on; no layer below it closes.
//! - **Initial focus**: a modal that is still on top one frame after it registered gets
//!   the focus (its designated element, else its first tab stop, else itself).
//! - **Focus restore**: one frame after a layer is released the focus goes back to its
//!   restore target (the element focused when it opened, unless the owner names
//!   another), unless a modal above it was open when it was released, a modal
//!   registered since then has the focus, or it was not on top and the focus is still
//!   somewhere. A target that is gone or covered by a modal is replaced by the initial
//!   focus of the new top layer.
//!
//! The stack holds no focus handles; the adapter keeps them per layer and asks
//! [`LayerStack::resolve`] what to do.

/// A registered layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(u64);

impl LayerId {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layer {
    id: LayerId,
    modal: bool,
    close_on_escape: bool,
}

/// What Escape does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escape {
    /// No layer is open: the key goes on.
    NoLayer,
    /// The top layer does not close on Escape: the key goes on, nothing closes.
    Ignored(LayerId),
    /// Close this layer (its owner's `onEscape`) and stop the key.
    Dismiss(LayerId),
}

/// What releasing a layer left to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Release {
    /// The layer was not registered.
    Unknown,
    /// A modal above it owns the focus: nothing to restore.
    Covered,
    /// Decide the restore one frame later with [`LayerStack::resolve`].
    Pending(PendingRestore),
}

/// A focus restore waiting for the next frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingRestore {
    pub layer: LayerId,
    /// The released layer was the top one.
    pub was_top: bool,
    /// The top layer right after the release.
    pub next: Option<LayerId>,
    /// Layers with an id from here on registered after the release.
    registered_from: u64,
}

/// Where the focus goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreAction {
    /// Leave it.
    Nothing,
    /// Focus the restore target.
    Restore,
    /// Focus the initial element of this layer.
    FocusInitial(LayerId),
}

#[derive(Debug, Clone, Default)]
pub struct LayerStack {
    layers: Vec<Layer>,
    next_id: u64,
}

impl LayerStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Puts a new layer on top.
    pub fn register(&mut self, modal: bool, close_on_escape: bool) -> LayerId {
        self.next_id += 1;
        let id = LayerId(self.next_id);
        self.layers.push(Layer {
            id,
            modal,
            close_on_escape,
        });
        id
    }

    /// `closeOnEscape` is read when Escape arrives, so it can change while open.
    pub fn set_close_on_escape(&mut self, id: LayerId, close_on_escape: bool) {
        if let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == id) {
            layer.close_on_escape = close_on_escape;
        }
    }

    pub fn top(&self) -> Option<LayerId> {
        self.layers.last().map(|layer| layer.id)
    }

    pub fn is_top(&self, id: LayerId) -> bool {
        self.top() == Some(id)
    }

    pub fn contains(&self, id: LayerId) -> bool {
        self.layers.iter().any(|layer| layer.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// The layers from the bottom up.
    pub fn ids(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.layers.iter().map(|layer| layer.id)
    }

    pub fn is_modal(&self, id: LayerId) -> bool {
        self.layers
            .iter()
            .any(|layer| layer.id == id && layer.modal)
    }

    /// The top layer is a modal: everything below it is inert, and Tab stays in it.
    pub fn top_modal(&self) -> Option<LayerId> {
        self.layers
            .last()
            .filter(|layer| layer.modal)
            .map(|layer| layer.id)
    }

    pub fn escape(&self) -> Escape {
        match self.layers.last() {
            None => Escape::NoLayer,
            Some(top) if top.close_on_escape => Escape::Dismiss(top.id),
            Some(top) => Escape::Ignored(top.id),
        }
    }

    /// Whether `id`, one frame after it registered, takes the initial focus: it is a
    /// modal and still on top.
    pub fn initial_focus_due(&self, id: LayerId) -> bool {
        self.layers
            .last()
            .is_some_and(|top| top.id == id && top.modal)
    }

    /// Removes a layer.
    pub fn release(&mut self, id: LayerId) -> Release {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return Release::Unknown;
        };
        let was_top = index + 1 == self.layers.len();
        // A modal above the removed layer owns focus; nonmodal layers above it do not.
        let covered_by_modal = self.layers[index + 1..].iter().any(|layer| layer.modal);
        self.layers.remove(index);
        if covered_by_modal {
            return Release::Covered;
        }
        Release::Pending(PendingRestore {
            layer: id,
            was_top,
            next: self.top(),
            registered_from: self.next_id + 1,
        })
    }

    /// Decides a pending restore one frame after the release.
    ///
    /// `focus_kept`: an element still holds the focus. `target_usable`: the restore
    /// target still exists and no modal covers it.
    pub fn resolve(
        &self,
        pending: &PendingRestore,
        focus_kept: bool,
        target_usable: bool,
    ) -> RestoreAction {
        // Only a modal registered after this release takes focus over the restore.
        if self
            .layers
            .iter()
            .any(|layer| layer.modal && layer.id.0 >= pending.registered_from)
        {
            return RestoreAction::Nothing;
        }
        // Below a nonmodal layer, restore only focus that was lost with this layer.
        if !pending.was_top && focus_kept {
            return RestoreAction::Nothing;
        }
        if target_usable {
            return RestoreAction::Restore;
        }
        match pending.next {
            Some(next) if self.contains(next) => RestoreAction::FocusInitial(next),
            _ => RestoreAction::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_goes_to_the_top_layer_only() {
        let mut stack = LayerStack::new();
        assert_eq!(stack.escape(), Escape::NoLayer);
        let lower = stack.register(true, true);
        let upper = stack.register(true, true);

        assert_eq!(stack.escape(), Escape::Dismiss(upper));
        stack.release(upper);
        assert_eq!(stack.escape(), Escape::Dismiss(lower));
    }

    #[test]
    fn a_locked_top_layer_keeps_lower_layers_from_closing() {
        let mut stack = LayerStack::new();
        stack.register(true, true);
        let locked = stack.register(true, false);

        assert_eq!(stack.escape(), Escape::Ignored(locked));
        stack.set_close_on_escape(locked, true);
        assert_eq!(stack.escape(), Escape::Dismiss(locked));
    }

    #[test]
    fn only_a_modal_still_on_top_takes_the_initial_focus() {
        let mut stack = LayerStack::new();
        let page = stack.register(false, true);
        assert!(!stack.initial_focus_due(page), "a page is not modal");
        let first = stack.register(true, true);
        let second = stack.register(true, true);
        assert!(!stack.initial_focus_due(first), "covered by a newer modal");
        assert!(stack.initial_focus_due(second));
        assert_eq!(stack.top_modal(), Some(second));
    }

    #[test]
    fn releasing_the_top_layer_restores_its_target() {
        let mut stack = LayerStack::new();
        let modal = stack.register(true, true);
        let Release::Pending(pending) = stack.release(modal) else {
            panic!("pending");
        };

        assert!(pending.was_top);
        assert_eq!(stack.resolve(&pending, true, true), RestoreAction::Restore);
        assert_eq!(
            stack.resolve(&pending, false, false),
            RestoreAction::Nothing
        );
    }

    #[test]
    fn a_layer_released_beneath_an_open_modal_restores_nothing() {
        let mut stack = LayerStack::new();
        let settings = stack.register(false, true);
        stack.register(true, true);

        assert_eq!(stack.release(settings), Release::Covered);
        assert_eq!(stack.release(settings), Release::Unknown);
    }

    #[test]
    fn a_newer_modal_wins_over_the_restore() {
        let mut stack = LayerStack::new();
        let bulk = stack.register(false, true);
        let settings = stack.register(false, true);
        let Release::Pending(pending) = stack.release(settings) else {
            panic!("pending");
        };
        stack.register(true, true);

        assert_eq!(stack.resolve(&pending, true, true), RestoreAction::Nothing);
        assert_eq!(pending.next, Some(bulk));
    }

    #[test]
    fn a_nonmodal_layer_registered_meanwhile_does_not_stop_the_restore() {
        let mut stack = LayerStack::new();
        let settings = stack.register(false, true);
        let bulk = stack.register(false, true);
        let Release::Pending(pending) = stack.release(settings) else {
            panic!("pending");
        };

        assert!(!pending.was_top, "bulk registered above it");
        assert_eq!(
            stack.resolve(&pending, false, true),
            RestoreAction::Restore,
            "the focus was lost with the page"
        );
        assert_eq!(
            stack.resolve(&pending, true, true),
            RestoreAction::Nothing,
            "focus that stayed below a nonmodal layer is not stolen"
        );
        assert_eq!(pending.next, Some(bulk));
    }

    #[test]
    fn a_missing_target_falls_back_to_the_next_layer() {
        let mut stack = LayerStack::new();
        let lower = stack.register(true, true);
        let upper = stack.register(true, true);
        let Release::Pending(pending) = stack.release(upper) else {
            panic!("pending");
        };

        assert_eq!(
            stack.resolve(&pending, false, false),
            RestoreAction::FocusInitial(lower)
        );
        stack.release(lower);
        assert_eq!(
            stack.resolve(&pending, false, false),
            RestoreAction::Nothing
        );
    }
}
