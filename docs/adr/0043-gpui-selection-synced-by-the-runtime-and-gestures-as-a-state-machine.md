# 0043: The GPUI selection is told the query's state by the runtime, reads through a ticket range of its own, and takes its gestures from a state machine without a window

- Status: Accepted
- Date: 2026-11-22
- Areas: GPUI port, gallery selection, runtime
- Related: [search, tags and media groups](../subsystems/search-tags-and-media-groups.md#gpui-port-selection-and-gestures), [library query and gallery](../subsystems/library-query-and-gallery.md#gpui-port-the-gallery-grid), [GPUI port plan](../plans/gpui-port/README.md) (epic E11, decision D4), [ADR 0008](0008-selection-spec-resolved-in-rust.md), [ADR 0029](0029-range-selection-keeps-its-session-alive.md), [ADR 0037](0037-gpui-client-backend-port-and-model-convention.md), [ADR 0039](0039-gpui-overlay-layers-and-controls.md), [ADR 0040](0040-gpui-query-model-tickets-and-timer-throttle.md)

## Context

The Solid selection (`createSelection.ts`, ADR 0008 and 0029) reads the query model through reactive getters (query epoch, session, total, failure epoch, identity epoch) and through two promises (`getIdsRangeAsync`, `keepSessionAlive`), and its gestures (`createGridHandlers.ts`) live on DOM events: pointer capture, `pointerup` before `click`, a capturing click listener, `requestAnimationFrame`. A model of the client crate has neither getters to subscribe to nor promises ([ADR 0037](0037-gpui-client-backend-port-and-model-convention.md)), ticketed query reads are answered through one event queue that other models will share ([ADR 0040](0040-gpui-query-model-tickets-and-timer-throttle.md)), and GPUI has none of those DOM mechanisms.

## Options considered

1. **The selection owns a handle to the query model and calls it.** Closest to the Solid code, but two models would then mutate each other outside `handle`, and the selection could not be tested without a query model.
2. **Route query events by a registry of tickets** that every reader registers in. Exact, but one more shared structure for a problem a reserved range solves.
3. **Selection as a model that is told and asks (chosen).** State arrives as an input, reads leave as effects, the runtime connects both in one turn.
4. **Gestures in the view**, as GPUI listeners with their own flags. Works, but the hold delay, the click suppression and the long press are exactly the rules the Solid tests pin down, and they would only be testable through a window.
5. **Take the release in the bubble phase** like every other listener. GPUI fires a tile's click in the bubble phase of the same mouse-up, and listeners of one phase run in reverse paint order, so the gesture could end after the click it has to precede; a Shift-click's range read would then be cancelled by the gesture's own cancel.

## Decision

- **Model.** `media_tagger_client::gallery::Selection` is embedded in `AppRuntime` (`RuntimeInput::Selection`). After every input the library took, the runtime sends it `SelectionInput::Sync(QuerySnapshot)` and only then the answers to its reads that came with that input, so a read a refresh overtook is forgotten instead of reported as a failure. Its reads are effects the runtime feeds to the embedded query model in the same turn; the keepalive interval is a timer effect, the only selection effect that reaches the runner.
- **Tickets.** The selection's tickets start at `SELECTION_TICKET_BASE` (2^62). The runtime routes `IdRangeRead` and `SessionKeptAlive` events with such a ticket to the selection and keeps every other event for `AppRuntime::take_query_events`. Every other caller of ticketed query reads chooses its tickets below that value.
- **Gestures.** `gallery::SelectionGesture` is a state machine without a window: the view gives it pointer positions relative to the viewport, the grid's place in the viewport and times on the executor's clock, and runs its effects. The rectangle's tiles come from global indexes (`selection_geometry`), never from drawn tiles.
- **Event order.** The gallery registers window-wide listeners in every frame; the release is handled in the capture phase, the moves in the bubble phase. A press on a control inside the viewport is marked by the control and starts no gesture.
- **Who cancels what.** A change of selection mode or of the query epoch makes the view end a held gesture; for a released rectangle it only drops the preview, because the model has already cancelled its own read on the same change. The Solid handlers send a cancel in both cases, which is harmless there because nothing can start in between; here a cancel from the view arrives after whatever the same runtime update started (the clicks of a comparison scenario) and would end that.
- **Escape and Ctrl+A.** While selection mode is on, the gallery view holds a layer of the layer stack that is not modal ([ADR 0039](0039-gpui-overlay-layers-and-controls.md)); Ctrl+A is a `Gallery` binding that acts only while that layer is the top one and otherwise propagates.

## Consequences

- The selection is tested alone, with reads answered by hand, and through the runtime with the scripted backend; the gestures are tested without a window and once more through GPUI's test platform (`tests/gallery_selection.rs`).
- A new reader of ticketed query events (the lightbox, epic E15) must keep its tickets below `SELECTION_TICKET_BASE`; a ticket at or above it is taken for the selection's and its event is not delivered to anyone else.
- Resolving the IDs of a range (at most 10,000 for a small selection, the envelope of a new large range when an ID with an unknown position is held) runs on the model thread when the read answers, as in the Solid model; the read itself runs on the executor.
- A move without the primary button down ends a held gesture, which stands in for `lostpointercapture`; so does the window losing its activation.
- The bulk dock (epic E14) reads `AppRuntime::selection` (`count`, `notice`, `capture`) and writes range selections through `SelectionCapture::spec_mutation`.
