# 0039: GPUI overlays route Escape and Tab through a layer stack at the window root, and controls are controlled

- Status: Accepted
- Date: 2026-11-21
- Areas: GPUI port, frontend conventions, keyboard, accessibility
- Related: [frontend conventions](../frontend/architecture-and-ui-conventions.md#gpui-port-theme-controls-and-overlays), [testing](../development/testing.md#gpui-workspace), [GPUI port plan](../plans/gpui-port/README.md) (epic E05), [ADR 0037](0037-gpui-client-backend-port-and-model-convention.md), [ADR 0038](0038-gpui-text-field-own-element-and-key-context.md)

## Context

The Solid interface gets three things from the browser that GPUI does not have:

- **One Escape for the whole page.** `UiLayerProvider` listens on `window`: Escape reaches the top registered layer wherever the focus is, a modal traps Tab, and a released layer gives the focus back one animation frame later (`src/ui/overlays/UiLayerProvider.tsx`).
- **A body that always has the focus.** With nothing focused, key events still go to the document. GPUI dispatches a keystroke along the focused element's ancestry; with no focused element it uses the window's own root node, above every element, where no binding of the application matches.
- **Native controls.** Range inputs and radio groups bring their keys (arrows, Page keys, Home, End, one tab stop per group) and `UiSlider` and `UiSegmentedControl` only restyle them. Both are controlled: they ask the owner and keep showing its value.

Every later screen of the port (dialogs, settings, lightbox, bulk dock) builds on these.

## Options considered

1. **Escape and Tab bound without a key context.** Fires wherever the focus is, but [ADR 0038](0038-gpui-text-field-own-element-and-key-context.md) forbids it: such a binding counts as deep as the focused element and takes Escape away from a text field's owner.
2. **Escape bound in each modal's own key context.** Works while the focus is inside the modal, which a trapped modal ensures, but not for nonmodal layers (the settings page, bulk mode), whose Escape must work with the focus anywhere.
3. **Bindings in one context at the window root, and a focusable root.** The root element of every window carries the key context `UiLayers` with Escape, Tab and Shift+Tab and their handlers, and tracks a focus handle of its own that takes the focus whenever nothing else has it. Every other context is deeper, so a field or control keeps the keys it binds, and an Escape it does not handle falls through to the root.
4. **Controls as GPUI elements that own their value.** Simpler call sites, but the owner could no longer refuse a value, which the Solid tests require ("keeps the committed slider value when its parent rejects an input").

## Decision

Option 3 for the keys and focus, and controlled controls.

- `media_tagger_client::overlay::LayerStack` decides: Escape goes to the top layer only (a top layer that does not close on Escape lets the key go on and closes nothing below it); a modal still on top one frame after it registered takes the initial focus; a released layer's restore is decided one frame later from whether a modal covered it, whether a modal registered since, whether it was on top and whether the focus is still somewhere. The adapter (`media_tagger_gpui::ui::layers`) keeps the focus handles and Escape callbacks per layer in a GPUI global and runs those decisions with `Window::on_next_frame`.
- `layers::root(cx)` is the root element of every window; `layers::focus_root` gives it the focus when the window opens and from `on_focus_lost`. "The focus is still somewhere" means on an element drawn inside the root other than the root.
- Tab inside a top modal walks the window's tab order until it reaches a stop inside the modal, so it wraps and comes back from outside; without a stop inside, the modal's container keeps the focus. Outside a modal, Tab is `Window::focus_next`.
- A modal is a `ModalState` entity (presence, layer, focus handles) plus a `Modal` element the owner renders every frame. Escape and a backdrop press emit `ModalEvent::Dismiss`; the owner closes. The layer is released when the modal closes, while it still fades out; its presence (`ModalPresence`) fades only opacity and ignores the timer of an older transition.
- The slider and the segmented control are entities that emit what the user asks for (`SliderEvent::Input`, `SegmentedEvent::Change`) and draw the value the owner sets. Their keys are bound in their own contexts (`UiSlider`, `UiSegmented`) by `ui::init`. Buttons and switches are elements with an `on_click`/`on_change`; their focus handle lives in element state under their id unless the owner passes one.

## Consequences

- Every window's content goes inside `layers::root`, and the window keeps the root focused when nothing else is. A view rendered outside it gets no Escape and no Tab handling.
- New overlays register with `layers::register` (or use `ModalState`); no view handles Escape on its own for closing. Shortcuts keep following ADR 0038; Escape, Tab and Shift+Tab stay bound only in `UiLayers`.
- What is not modelled: `inert` and `aria-hidden` of the background (a modal occludes the pointer and traps Tab instead), body scroll locking, and Enter activating a focused segment (GPUI's keyboard click fires on Enter and Space for every clickable element; a radio reacts to Space only).
- Tests: `media_tagger_client::overlay` and `controls` unit tests, `media_tagger_gpui` targets `overlays` and `controls` on GPUI's test platform.
