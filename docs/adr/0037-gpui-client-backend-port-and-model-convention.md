# 0037: The GPUI client reaches the core through a blocking `Backend` port run on an executor, and models are state machines

- Status: Accepted
- Date: 2026-11-20
- Areas: architecture, GPUI port, thumbnails, scanning, testing
- Related: [system overview](../architecture/system-overview.md#gpui-port-a-second-adapter-in-its-own-workspace), [IPC contract](../architecture/ipc-contract.md#the-same-contract-without-ipc-the-backend-port), [thumbnails](../subsystems/thumbnails.md#gpui-port-the-thumbnail-pipeline), [testing](../development/testing.md#gpui-workspace), [GPUI port plan](../plans/gpui-port/README.md) (decisions D3, D4), [ADR 0034](0034-tauri-independent-backend-core.md), [ADR 0036](0036-gpui-second-adapter-in-its-own-workspace.md), [ADR 0006](0006-thumb-uri-scheme.md)

## Context

[ADR 0036](0036-gpui-second-adapter-in-its-own-workspace.md) put presentation logic into `media_tagger_client`, a crate without a GUI that may name the core's use cases in one file only. That file needs a shape, and everything the Tauri adapter adds around the core needs a counterpart that does not use Tauri:

- The core is synchronous ([ADR 0034](0034-tauri-independent-backend-core.md)): a use case blocks on SQLite, on files and on the workflow locks. The Tauri commands run them on `spawn_blocking`; a GPUI view must never wait for one.
- The Solid models order their replies by hand (request IDs, generations, settle timers). Their tests are the behaviour specification, and they are only portable if the port's models are deterministic.
- `thumb://`, the `process-progress` event and the single-flight startup scan live in the Tauri adapter (`adapter/thumb_scheme.rs`, `adapter/events.rs`, `adapter/startup_scan.rs`) and stand on `tauri::async_runtime`.

## Options considered

1. **An async port (futures, tokio or GPUI tasks in the client).** Natural for GPUI, but it puts a runtime into a crate that must not know one, and reply order in tests would depend on a scheduler.
2. **A blocking port, called on an executor; models as state machines.** The port stays a plain trait over the use cases. Ordering is decided by whoever runs the tasks, so a test can choose it.
3. **Thumbnails through `thumb_protocol::serve`.** It is the code path of the Tauri handler, but its publisher is one per process and publishes a whole batch against the state of the host that started it, and its `ThumbHost::spawn_blocking` has no instance. Two backends in one process (every test binary) would publish each other's renders. Its replies are HTTP status codes and CORS headers that a GPUI view has no use for.
4. **Thumbnails through `thumb_service::request_thumbnail` and `finish_thumbnail_requests`.** The two phases `serve` is built on, with typed answers and a publisher owned by the backend instance.
5. **Progress as a per-call sink argument.** Correlated, but a view that joins a running startup scan would see no progress, unlike with the shared Tauri event.

## Decision

- **Port.** `media_tagger_client::backend::Backend` has one blocking method per use case the interface needs, named after the Tauri command it replaces and returning the `media_tagger_core::models` type, plus `request_thumbnail` (answers through a responder) and `subscribe_progress`. No video and no window methods. `rename_asset_file`, which the Tauri adapter does not register, is not part of it. Errors are `BackendError`: the core's text unchanged, a kind (`Rejected`, `WorkerFailed`, `NotReady`) and the locale key of an `i18n:<key>` text. Nothing branches on wording.
- **Adapter.** `backend/core_backend.rs` (`CoreBackend`) is the only client file that names `usecases`, `services` and `app`. Every method is the body of the Tauri command without IPC. It takes no lock itself: validation, SQL and the lock order maintenance gate → `scan_lock` → `thumb_lock` stay in the use cases, inside one call. The binary ID range is decoded to `Vec<i64>` there.
- **Models (plan decision D4).** A model implements `model::Model`: one input gives a new state and a list of effects. Inputs are commands, backend replies and timer firings. Each reply and timer carries the `RequestToken` (or generation) it was started with, and a model that has moved on drops it. State is published only from the backend's result. A `Runner` turns effects into port calls on an `exec::Executor` and into timers; results come back through the model's `Inbox` and are applied by `Driver::pump` on the model thread. A call that panics, or whose task the executor dropped, still answers, with a `WorkerFailed` error.
- **Executors.** `ThreadExecutor` (an elastic pool of at most 64 background threads whose idle workers end after 10 s, and one timer thread) runs the application; the GPUI adapter only supplies the wake callback into its event loop. `ManualExecutor` runs tasks when a test says so, in the order the test chooses, on a virtual clock. No tokio and no async in the client or the core.
- **Thumbnails.** `CoreBackend::request_thumbnail` uses option 4: it serves an existing file or enqueues a render and never waits for one; finished renders go to a publisher thread owned by the backend, which publishes what is waiting in one transaction (at most 64) and answers. Answers are `Ready` (bytes and version), `NotFound`, `Unavailable` and `BadRequest`, with the meaning of the `thumb://` status codes. Before the backend is attached to its `AppState` the answer is `Unavailable`, the counterpart of the 503 window. `thumbs::ThumbnailPipeline` holds the tile rules of `ThumbnailImage` (retry backoff, cached-only, prefetch promotion) and cancels by token. Its runner, `thumbs::ThumbnailFetcher`, sends at most 16 requests at once (`MAX_CONCURRENT_FETCHES`) from its own queue.
- **Progress.** One `ProgressHub` per backend replaces the `process-progress` event, with the same contract: shared, best effort, uncorrelated, filtered by phase. A model subscribes before it starts the operation.
- **Startup scan.** `startup_scan::StartupScan` is the single flight of `adapter/startup_scan.rs` on a mutex and a condition variable: the first caller scans the launch snapshot, callers meanwhile wait for that scan, and the result (also a failure or a panic) is kept and replayed.
- **Test doubles.** The feature `test-support` of the client crate holds `FakeBackend`, `ManualExecutor` and `TempCore` (the real core on a temporary profile). It is enabled for the crate's own tests through a dev-dependency on itself and never in a normal build. It makes the existing workspace dependency `tempfile` an optional dependency of the client; `src-gpui/Cargo.lock` gains these two edges on the client package and no package.
- **Core.** The only change is additive: `PartialEq` and `Eq` on the input types `DuplicateResolutionBatchInput`, `DuplicateResolutionChangeInput` and `DbRootMapping`, so that recorded port calls can be compared.

## Consequences

- Reply order is a test input. The acceptance case "a late reply with a stale token does not change state" is a unit test on `ManualExecutor` and `FakeBackend` (`model::tests`, `thumbs::tests`).
- Every port method is tested on the real core through `TempCore` (`backend/core_tests.rs`), including the destructive ones on their own temporary directories.
- A blocking call occupies a thread for its whole run. A scan or a bulk render therefore holds one pool thread; the pool grows instead of queueing behind it, up to 64 threads (tokio's blocking pool, which the Tauri commands use, stops at 512). Past that every task and every timer firing waits, so a runner that can start any number of blocking tasks applies a bound itself. The thumbnail fetcher does: a request checks its source with `Path::exists`, which hangs on an unreachable network mount, and a gallery screen with its overscan rows can ask for more than 64 tiles. With at most 16 requests inside the backend, stuck checks stop thumbnails only and leave the other threads to queries, writes and timers.
- The scheduler cannot take a queued render back, so cancelling a thumbnail saves the request only while it has not reached the backend; afterwards the render is published and the bytes are dropped.
- `request_thumbnail` repeats about sixty lines of `thumb_protocol::serve` and its publisher. A change to the serving rules of `thumb://` has to be made in both places; the rules themselves (`thumb_service`) are shared.
- Neither executor threads nor the publisher thread are joined when the backend is dropped: they finish what they run and end. The adapter that owns `AppState` and the instance lock (epic E06) decides how long to wait before it releases the profile.
- A not-ready backend answers `NotReady` with `i18n:common.loading`; worker failures carry English text in the form of the Tauri commands (`<operation> worker failed: ...`), which are shown as they are there too.
- The wire contract of the Tauri application is untouched: `architecture_boundaries` passes unchanged.
