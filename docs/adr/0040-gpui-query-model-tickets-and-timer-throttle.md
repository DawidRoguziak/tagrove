# 0040: The GPUI query model answers reads by ticket, throttles stale refreshes with two timers, and is embedded in the runtime

- Status: Accepted
- Date: 2026-11-21
- Areas: GPUI port, library query, gallery
- Related: [library query and gallery](../subsystems/library-query-and-gallery.md#gpui-port-the-query-model), [frontend conventions](../frontend/architecture-and-ui-conventions.md#gpui-port-runtime-window-and-shell), [GPUI port plan](../plans/gpui-port/README.md) (epic E07, decision D4), [ADR 0037](0037-gpui-client-backend-port-and-model-convention.md), [ADR 0005](0005-binary-id-ranges-and-anchored-refresh.md), [ADR 0029](0029-range-selection-keeps-its-session-alive.md)

## Context

`createAssetQuery` (`src/library/query/createAssetQuery.ts`) is the behaviour specification of the query model. It leans on three things the client crate does not have ([ADR 0037](0037-gpui-client-backend-port-and-model-convention.md): models are state machines, no async, no runtime):

- **Promises.** `getAssetAtAsync`, `getAssetPosition`, `getIdsRangeAsync` and `keepSessionAlive` return a promise to the lightbox and the selection. A model's `handle` returns effects only.
- **A clock.** The stale refresh waits `max(250 ms, lastStaleRefreshAt + 1 s - Date.now())`. A model that reads a clock is not deterministic on `ManualExecutor`.
- **A reactive owner.** `createLibrary` holds the gallery range in a signal and an effect demands it again whenever the range or the query epoch changes; `crypto.randomUUID()` names the owner.

The runtime of epic E06 also refreshed the library itself, with one unfiltered query under a fixed owner ID, until this model existed.

## Options considered

1. **Reads by callback or channel handed in with the input.** Close to a promise, but an input would carry a closure: not comparable, not printable, and the answer would run inside the query model's `handle` on behalf of another model.
2. **Reads by ticket, answered as events (chosen).** The caller picks a `Ticket`, the model queues one `QueryEvent` per read, and the owner of both models hands it on. Plain data in both directions.
3. **A clock port for the stale interval.** One more port and a second notion of time next to the executor's timers.
4. **Two timers (chosen).** The debounce is a timer effect; the minimum interval is a second timer effect that starts when a stale-driven refresh runs. The scheduled refresh runs when both have fired.
5. **The query as a separate driver next to the runtime.** Two inboxes and two wake paths for state that the startup sequence has to read together (the startup scan ends only after the library refresh answered).
6. **The query embedded in `AppRuntime` (chosen).** One inbox, one pump; the runtime wraps the query's inputs and effects.

## Decision

- `media_tagger_client::library::AssetQuery` (`src-gpui/crates/media_tagger_client/src/library/query.rs`) is the model; `library::PageCache` is its cache without I/O. It holds the applied filters and the gallery's viewport itself and demands the viewport when it changes and when the query epoch changes.
- A read for another model is an input with a `Ticket` and is answered exactly once with a `QueryEvent`. A read that a refresh, a reset or the disposal overtakes is answered at that moment with the value the Solid model resolves to when the late reply arrives: no row, `Stale`, `QueryError::Cancelled`, `KeepAlive::Skipped`. The late reply is then dropped by its token.
- The stale throttle is `QueryEffect::StaleRefreshTimer` (250 ms) and `QueryEffect::StaleCooldownTimer` (1 s, started when a stale-driven refresh runs). An explicit refresh cancels the scheduled refresh and leaves the cooldown running, as the Solid model keeps `lastStaleRefreshAt`.
- The owner ID comes from a process-wide counter (`gpui-asset-query-<n>`). The backend's per-owner generation table lives in the same process, so uniqueness inside the process is what the contract needs.
- `assets()` lists cached rows in index order. `QueryError` is an enum; nothing branches on text.
- `AppRuntime` embeds the model: `RuntimeInput::Query`, `RuntimeEffect::Query`, and `run_query_effect(backend, effect, dispatch, wrap)` in the runner. `RuntimeInput::RefreshLibrary` is the query's refresh, and the fixed owner `LIBRARY_OWNER_ID` and `LibraryState` are gone.

## Consequences

- The cases of `createAssetQuery.test.ts` and `queryRaces.test.ts` run on `ManualExecutor` with the virtual clock; reply order is chosen by the test (`library/query/tests.rs`).
- A caller learns earlier than in Solid that its read is obsolete. Nothing can act on the difference: the value is the same and the epoch has already changed.
- Events of ticketed reads stay in the model until its owner takes them (`AppRuntime::take_query_events`). The selection and lightbox models (epics E11 and E15) are embedded next to the query and are handed those events by the runtime; until then nothing asks, so nothing queues.
- A second runtime on the same core no longer gets `Superseded` for its library refresh: each query model is its own owner, as each `createAssetQuery` is.
- A model that keeps data per query (details cache, selection) compares `query_epoch()` instead of receiving `onQueryChanged`.
- Page demand is not bounded by the runner: a viewport asks for the pages it intersects, as in Solid. The grid of epic E09 must hand in the visible range with its overscan, never the whole result.
- The Tauri application and its wire contract are untouched.
