# 0041: The GPUI library model sends metadata writes as queued data, puts maintenance behind a begin/end barrier, and refuses a tag edit without the complete tag list

- Status: Accepted
- Date: 2026-11-21
- Areas: GPUI port, metadata coordination, tags, vocabulary, settings maintenance
- Related: [search, tags and media groups](../subsystems/search-tags-and-media-groups.md#gpui-port-metadata-mutations-and-the-vocabulary), [frontend conventions](../frontend/architecture-and-ui-conventions.md#gpui-port-runtime-window-and-shell), [GPUI port plan](../plans/gpui-port/README.md) (epic E08, decision D4), [ADR 0001](0001-per-asset-metadata-write-queue.md), [ADR 0033](0033-feff-is-tag-whitespace-in-rust-and-legacy-rows-are-split-on-read.md), [ADR 0037](0037-gpui-client-backend-port-and-model-convention.md), [ADR 0040](0040-gpui-query-model-tickets-and-timer-throttle.md)

## Context

`createAssetTagState`, `createDetailsCache`, `createMutations`, `createKnownTags` and the rest of `createLibrary` (`src/library`) are the behaviour specification of who may write metadata and what is published afterwards ([ADR 0001](0001-per-asset-metadata-write-queue.md)). They lean on things a client model does not have ([ADR 0037](0037-gpui-client-backend-port-and-model-convention.md): state machines, no async):

- **Promises as queue turns.** `enqueueMutation` returns a turn or a promise of one; `runWithMutationBarrier` awaits the earlier barrier and the drain of admitted writes around a caller's async operation.
- **Closures in commands.** `saveTags(id, canonical => next)` computes the replacement from the canonical tags when the write reaches the head of its queue; `write` takes `request`, `changes` and `after` callbacks.
- **Four objects that call each other.** A write settles tokens in the tag state, patches the details cache and the query's rows, and refreshes the query and the vocabulary. The query model of [ADR 0040](0040-gpui-query-model-tickets-and-timer-throttle.md) was embedded in the runtime on its own, and `RuntimeInput::RefreshLibrary` was the query's refresh only.
- **A second tag rule.** `src/utils/media.ts` repeats the core's rule in TypeScript. The client crate may name `media_tagger_core::{models, error}` only, outside `backend/core_backend.rs` (`architecture_boundaries`).

## Options considered

1. **Sub-models next to the query in `AppRuntime`, wired by the runtime.** Every write would cross four models through the runtime, which would have to know the order of settle, patch, release and refresh.
2. **One library model that owns the query, the tag state, the details cache and the vocabulary (chosen).** The order lives in one `handle`; the runtime only wraps inputs and effects.
3. **Tag edits as closures in the input.** Not printable, not comparable, and they would run inside the library model on behalf of an editor.
4. **Tag edits as data (chosen):** a complete replacement, or an add/remove change applied to the canonical tags when the write starts.
5. **A replacement of an unknown tag list is sent, as `createMutations` does.** The guard stays in every caller.
6. **Every tag edit needs the known complete list (chosen).** The model refuses it without a backend call.
7. **Maintenance as a callback run inside the barrier.** A callback in an input, and the operation is another model's backend call.
8. **Maintenance as begin and end inputs (chosen).** The owner of the operation waits for the grant, runs it, and ends the barrier with the reset the operation calls for.
9. **Loosen the boundary rule so the client may name `media_tagger_core::utils::tags`.** One more allowed path for every file of the crate.
10. **Reach the tag rule through the backend adapter file (chosen).** `backend/core_backend.rs` stays the only file that names anything of the core beyond models and errors.

## Decision

- `media_tagger_client::library::Library` (`library/library_model.rs`) is the port of `createLibrary` and `createMutations`. It embeds `AssetQuery` and `KnownTags` (`library/vocabulary.rs`, a model with its own inputs, effects and ticketed events) and owns `AssetTagState` (`library/tag_state.rs`) and `DetailsCache` (`library/details.rs`), which have no effects. `AppRuntime` embeds `Library` instead of the query: `RuntimeInput::Query` and `RuntimeEffect::Query` stay and pass through the library, every other input travels as `RuntimeInput::Library` (boxed) and every other effect as `RuntimeEffect::Library`. `RuntimeInput::RefreshLibrary` is now `refreshLibrary`: the query's refresh and a pass over the known tags, and the startup scan ends only after both answered. This amends the last sentence of the decision in ADR 0040.
- `AssetTagState` keeps the queue of ADR 0001 as explicit state: a turn remembers the admitted turns it waits for as linked at admission, `release` returns the turns that became ready, and a barrier is requested, polled and released. Same rules, same order, same 512-record bound; no promise.
- A write is the input `LibraryInput::Mutate { ticket, mutation }` and one `LibraryEffect::Write` with a `WriteRequest`; the backend's `WriteReply` comes back as `LibraryInput::WriteFinished` and is always applied, also after a reset or the disposal, because it releases the queue turn. The caller is answered once with `LibraryEvent::MutationSettled`; what was published travels as `LibraryEvent::Committed`. Patches are built from the reply only (`mutations::patches`): an asset the backend does not name gets nothing.
- `TagEdit::Replace(tags)` and `TagEdit::Change { add, remove }` replace the function form of `saveTags`. Either is refused with `MutationError::TagsNotLoaded` when the asset's complete tag list is not known at the moment the write starts; `Library::can_edit_tags` tells an editor beforehand.
- Details reads, refreshes, idle waits and maintenance are ticketed as in ADR 0040. A details read overtaken by a new query epoch, a reset or the disposal is answered with nothing at that moment. A deletion is answered after the query refresh and the vocabulary pass it started have ended.
- `LibraryInput::BeginMaintenance` requests the barrier, `LibraryEvent::MaintenanceReady` grants it once every admitted write has ended, and `LibraryInput::EndMaintenance { reset }` applies `LibraryReset::{Identity, Restored, Cleared}` inside the barrier and takes it down.
- `library::tags::normalize_tags` and `is_valid_tag` call the core's `utils::tags` through `backend::core_backend::tag_rules`. The client adds only what `normalizeTags` adds: a tag that breaks the rule is dropped from a list instead of failing it.

## Consequences

- The cases of `createAssetTagState.test.ts`, `createKnownTags.test.ts` and `mutations.test.ts` run on `ManualExecutor` with replies in a chosen order (`library/tag_state/tests.rs`, `library/vocabulary/tests.rs`, `library/library_model/tests.rs`), and one case runs the library on `ThreadExecutor` against the real core.
- Stricter than Solid in one place: a complete replacement of an asset whose tags were never loaded is refused by the model, where Solid leaves that guard to the lightbox and the bulk dock. No Solid test relies on the looser behaviour.
- One difference in tag normalization is the core's to decide: the core trims U+0085 at a tag's edge, JavaScript's `trim()` does not and `isValidTag` then drops the tag. Inside a tag both sides reject it.
- An editor that used `saveTags` with a function (the lightbox's rebase of a failed intent, the bulk dock's add or remove) sends `TagEdit::Change` instead; `MutationOutcome::TagsSaved` carries the list that was sent.
- Launch now reads the known tags (`list_tags` in pages of 200) with the first query, as `createAppRuntime().start` does.
- The settings operations (CSV import, restore, clear; a later epic) use begin/end maintenance; nothing in the client can run such an operation outside the barrier by going through `Library`, but the `Backend` port itself does not enforce it, as the Tauri commands do not.
- The Tauri application, its wire contract and the core are untouched.
