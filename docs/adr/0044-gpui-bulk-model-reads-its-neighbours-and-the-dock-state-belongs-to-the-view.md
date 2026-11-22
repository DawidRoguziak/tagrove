# 0044: The GPUI bulk model reads the library and the selection through a borrowed environment, takes its library answers by a ticket range, and the dock's local state belongs to the view

- Status: Accepted
- Date: 2026-11-22
- Areas: GPUI port, bulk dock, runtime
- Related: [search, tags and media groups](../subsystems/search-tags-and-media-groups.md#gpui-port-bulk-dock-and-group-order), [GPUI port plan](../plans/gpui-port/README.md) (epic E14, decision D4), [ADR 0035](0035-bulk-actions-floating-dock.md), [ADR 0037](0037-gpui-client-backend-port-and-model-convention.md), [ADR 0041](0041-gpui-library-model-queued-writes-as-data.md), [ADR 0043](0043-gpui-selection-synced-by-the-runtime-and-gestures-as-a-state-machine.md)

## Context

The Solid bulk controller (`createBulk.ts`) sits on top of two other models. It reads many of their reactive getters (the selected IDs and the spec, the selection key, the identity epoch, the canonical tags of one asset, the tag clock and the details generation, the query's cached rows, whether the settings page is shown) and awaits their promises (`mutations.*`, `details.load`, `tags.waitForIdle`). Its feedback is guarded by what the selection and the identity are when a promise resolves. The dock component keeps more state in signals that die with the gallery view (the tab, the tag draft, the inline order, the larger view's draft).

A model of the client crate has no getters to subscribe to and no promises ([ADR 0037](0037-gpui-client-backend-port-and-model-convention.md)). The selection solved the same problem with a snapshot that the runtime sends it ([ADR 0043](0043-gpui-selection-synced-by-the-runtime-and-gestures-as-a-state-machine.md)); for the bulk model the snapshot would have to carry the selected IDs, the tag list of an asset, two clocks and a lookup into the page cache, and it would have to be rebuilt after every input.

## Options considered

1. **A snapshot input like the selection's.** One convention, but a wide struct copied after every input, plus a second round trip for the cached rows of selected IDs.
2. **Handles to the library and the selection inside the model.** Closest to the Solid code; two models would mutate each other outside `handle`.
3. **A borrowed, read-only environment per input (chosen).** The runtime lends `BulkEnv { library, selection, settings_open }` to `Bulk::handle`. The model reads what it needs at that moment and returns every change it wants as an effect.
4. **The dock's local state in the runtime as well.** It would survive the settings page, which the Solid dock's signals do not, and tests of the remount behaviour would no longer mean anything.

## Decision

- **Reading.** `media_tagger_client::bulk::Bulk` is embedded in `AppRuntime`. `Bulk::handle(input, env)` takes a `BulkEnv` with shared references to the library and the selection. It never holds them and cannot change them; `AssetTagState::peek_generation` exists so the tag clock can be read without creating one.
- **Changing.** What the model wants of its neighbours is an effect the runtime applies in the same turn: `BulkEffect::Library` (a write, a details read, a pin, a wait for the idle queue) and `BulkEffect::Selection` (unselecting IDs a favorite toggle did not reach). Only `BulkEffect::LoadSummaries` reaches the runner (`RuntimeEffect::BulkSummaries`).
- **Sync.** After every input of any model the runtime sends `BulkInput::Sync`, and repeats that until the model asks for nothing more. `Sync` is where the Solid model's effects run.
- **Tickets.** The bulk model's library tickets are `BULK_TICKET_BASE` (2^61) up to `SELECTION_TICKET_BASE` (2^62). The runtime drains the library's events after every input, hands the model the answers with such a ticket and a copy of every `MetadataCommit`, in the order they were published, and keeps every other event for `AppRuntime::take_library_events`.
- **Answers to the dock.** The promises of `onAddTag`, `onRemoveTag` and `onApplyGroup` are `BulkEvent`s with the request id the dock chose (`AppRuntime::take_bulk_events`). Request ids are unique in the process.
- **Dock state.** `bulk::Dock` (tab, tag draft and its focus, inline order, larger view) is owned by the GPUI view `BulkDock`, which the gallery view makes while selection mode is on. It reads the model through `BulkView` and returns `DockEffect`s.

## Consequences

- The bulk model is tested through the runtime with the scripted backend and with the real core on a temporary profile (`tests/bulk_core.rs`); the dock state and the board geometry are tested without a window, the view on GPUI's test platform (`tests/bulk_dock_window.rs`).
- `Bulk` does not implement the `Model` trait: its `handle` has the environment as a second argument. No runner drives it alone.
- A new reader of ticketed library commands (the lightbox, epic E15) keeps its tickets below `BULK_TICKET_BASE`; a ticket in the bulk range is taken for the bulk model's and its event is delivered to nobody else.
- `AppRuntime::take_library_events` no longer empties the library's own queue but a list the runtime fills. Its owner has to take it after every input. Until a view reads those events, the GPUI `Runtime` entity drops them when it publishes, so they do not pile up with every write.
- The dock's state is lost with the gallery view, as in the Solid interface; the drafts of the model (group key, order, added tags, feedback) survive it.
