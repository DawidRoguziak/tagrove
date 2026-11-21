//! The overlay layer of `src/ui/overlays` without a GUI: the stack of layers that
//! decides which one Escape closes and where the focus goes when one closes
//! (`UiLayerProvider`), and the presence of a modal that fades in and out
//! (`createModalPresence`, `createModalSnapshot`, the sizes of `UiModal`).

mod layers;
mod presence;

pub use layers::{Escape, LayerId, LayerStack, PendingRestore, Release, RestoreAction};
pub use presence::{ModalMotion, ModalPresence, ModalSize, ModalSnapshot, Phase};
