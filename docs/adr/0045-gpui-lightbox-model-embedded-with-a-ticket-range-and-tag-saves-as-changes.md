# 0045: The GPUI lightbox model is embedded in the runtime with a ticket range of its own, saves tags as changes, and its presentation state belongs to the view

- Status: Accepted
- Date: 2026-11-22
- Areas: GPUI port, lightbox, runtime
- Related: [lightbox](../subsystems/lightbox.md#gpui-port-the-lightbox-model), [GPUI port plan](../plans/gpui-port/README.md) (epic E15, decision D4), [ADR 0022](0022-lightbox-delete-fails-fast-during-a-scan.md), [ADR 0037](0037-gpui-client-backend-port-and-model-convention.md), [ADR 0040](0040-gpui-query-model-tickets-and-timer-throttle.md), [ADR 0041](0041-gpui-library-model-queued-writes-as-data.md), [ADR 0044](0044-gpui-bulk-model-reads-its-neighbours-and-the-dock-state-belongs-to-the-view.md)

## Context

`createLightbox.ts` owns the selected asset for the lifetime of the application. It reads reactive getters of the query, the tag state and the details cache, awaits their promises, and guards every continuation with three request counters. Its tag save passes a callback to `mutations.saveTags`, which the write queue calls when the write reaches the head of the asset's queue; the callback merges the editor's intent onto tags another writer stored meanwhile. The modal around it keeps more state in owners that die with it: the side panel, the activity timer, the clipboard confirmation and the delete confirmation with its typed word.

A model of the client crate has no getters to subscribe to, no promises and no callbacks ([ADR 0037](0037-gpui-client-backend-port-and-model-convention.md)). The library model sends writes as data ([ADR 0041](0041-gpui-library-model-queued-writes-as-data.md)), so nothing of the lightbox can run at the moment a queued write is sent. The bulk model already established how a model next to the library reads and changes it ([ADR 0044](0044-gpui-bulk-model-reads-its-neighbours-and-the-dock-state-belongs-to-the-view.md)).

## Options considered

1. **A start hook in the library** (an event when a queued write is sent, answered with the list to write). It keeps the Solid shape, but the write queue would wait for another model inside one turn, and the list sent would no longer be data.
2. **Tag saves as `TagEdit::Replace`.** Simple, but a save queued behind another writer would undo that writer's tags.
3. **Tag saves as `TagEdit::Change` computed when the save is handed over (chosen).** The library applies what the editor added and removed to the canonical tags of the moment it sends. The model follows the tag revision and shows the same merge.
4. **Presentation and confirmation state in the runtime.** It would survive a close, which the Solid owners do not, and the lightbox would reopen with a typed word or a collapsed panel.

## Decision

- **Embedding.** `media_tagger_client::lightbox::Lightbox` lives in `AppRuntime` and has the form of the bulk model: `Lightbox::handle(input, env)` with a borrowed `LightboxEnv { library }`, no `Model` implementation, no runner. Every effect is `LightboxEffect::Library`, an input of the library model applied by the runtime in the same turn. Commands arrive as `RuntimeInput::Lightbox(LightboxCommand)`.
- **Tickets.** `LIGHTBOX_TICKET_BASE` (2^60) up to `BULK_TICKET_BASE` (2^61), one counter for query reads and library commands, which are answered on separate channels. The runtime hands the model `QueryEvent`s and `LibraryEvent`s with such a ticket and a copy of every `MetadataCommit`; nobody else sees them.
- **Order within a turn.** The model applies an answer or a commit as it is and looks at the rest of the library only on `LightboxInput::Sync`, which the runtime sends after the answers of a turn (and the model runs itself after a command). The library publishes saved tags and answers the write in one turn; the answer must be applied before the published tags are compared with the editor's intent, or the model would take its own result for another writer's.
- **Tag saves.** One `Mutation::SaveTags` with `TagEdit::Change { add, remove }` per asset at a time; the difference is taken between the canonical tags the edit was made on and the wanted list. While the write is pending and the canonical tags change, the intent is merged onto them from the lists the write was handed over with, which is what the library sends. The intent is saved when the wanted list still equals that merged list at the answer; otherwise one following write saves the rest.
- **Details reads.** As in the bulk model, an answer without details is a failure only when the tag clock, the details generation and the query epoch are unchanged; a discarded read waits for a pending query refresh, then for the idle write queue. A read gives up after eight discarded answers in a row (`MAX_DISCARDED_READS`); the Solid model retries without a bound.
- **Query refresh from Retry.** `RetryNavigation` without a failed target sends `QueryInput::Refresh` and does not wait for it: the sync after the refresh resolves the position, and a failed refresh ends in `failed` because a position lookup without a session is answered with an error at once.
- **Deletion.** Only `Mutation::DeleteAsset` through the library's queue, answered with `LightboxEvent::DeleteSettled`. The model publishes nothing for it; the selection closes on the library's `MetadataCommit::Deleted`. Every delete command gets exactly one answer: a model that is disposed answers the deletions that still wait, and any asked for later, with `MutationError::Refused(TagStateError::Unavailable)`, as the bulk model answers its pending commands on disposal ([ADR 0044](0044-gpui-bulk-model-reads-its-neighbours-and-the-dock-state-belongs-to-the-view.md)).
- **View-owned state.** `Sidebar`, `Activity`, `GroupClipboard` and `DeleteConfirm` are plain state machines owned by the lightbox view (epic E16). They take the current time as an argument and report their next deadline instead of starting timers; `deadline(now)` is a moment after `now` or nothing, so a timer armed from it never fires for a moment that has passed. `DeleteConfirm` takes request ids from a process-wide counter, so a reopened lightbox never takes the answer to a deletion of the one before.
- **Confirmation word.** No normalization crate is added (the epic allows no new dependency): the comparison composes vowels with a grave or acute accent and Hangul jamo, lowercases with the Unicode default mapping, and trims as JavaScript does. A test types every language's word decomposed.

## Consequences

- The model is tested through `AppRuntime` with the scripted backend on the deterministic executor and with the real core on a temporary profile (`tests/lightbox_core.rs`); the view-owned machines are tested on a fake clock without a window.
- The list a tag save sends is the canonical list with the edit applied, so its order can differ from the Solid request. The backend stores tags sorted; the stored result is the same.
- An edit made while a save waits behind another writer is saved by a second write, where the Solid callback would send the newest list in the first. The stored result is the same.
- A caller of ticketed library commands or query reads that is not an embedded model keeps its tickets below 2^60.
- The GPUI `Runtime` entity still drops `AppRuntime::take_library_events` when it publishes: the embedded models get their answers in the runtime, and no view has tickets of its own.
- A language whose confirmation word needs another composition than the two covered needs that composition added, or a normalization dependency with its own decision.
- Follow-up in epic E16: the lightbox view owns the four machines, schedules a redraw for their deadlines, takes `AppRuntime::take_lightbox_events`, and shows `DeleteNotice`.
