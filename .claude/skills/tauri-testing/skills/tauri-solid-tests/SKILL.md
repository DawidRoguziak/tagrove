---
name: tauri-solid-tests
description: Write and fix Tagrove frontend tests - Vitest + jsdom + @solidjs/testing-library on Solid 2 RC (flush/settle scheduling, renderHook models, renderSolid prop updates), Tauri IPC mocked with vi.mock of @tauri-apps/api/core or src/platform/tauri modules, wrapper contract tests in src/platform/tauri/__tests__, binary id-range mocks, i18n and storage hygiene. Use for any *.test.ts(x) work under src/.
---

# Frontend tests in Tagrove (Solid 2 RC, Vitest)

Read `tauri-test-strategy` first if the layer is not decided. Owning docs:
`docs/development/testing.md` → "Frontend tests and Vitest", and
`docs/frontend/architecture-and-ui-conventions.md` for state ownership and layer rules.
Everything below is already set up; do not add packages or a second Vitest config.

## 1. Setup you get for free

- `vitest.config.ts`: jsdom, `globals: false` (import `describe/it/expect/vi` explicitly),
  `css: true`, forks pool, 2 workers, 8 GiB heap, includes `src/**/*.{test,spec}.{ts,tsx}`.
- `src/test/setup.ts`: DOM mocks from `setup-dom.ts` (jest-dom matchers, observers, animation
  frames, storage, the controlled suggestion worker), English i18n, and `cleanup` after each test.
  The setup is not rerun per test: restore language or persisted storage you change.
- Helpers in `src/test/`:
  - `settle.ts` → `settle(op)` flushes Solid after a sync or async model command.
  - `renderSolid.tsx` → `renderSolid(Component, props)`; `rerender(next)` updates props on the
    same owner (keeps DOM and local state) and wraps in `LanguageProvider`.
  - `idRange.ts` → `idRangeBuffer(ids)`, `idRangeResult(ids)`, `sequentialIds(start, end)` for the
    binary `get_asset_query_id_range` response.

Tests live in `__tests__/` next to the owning module, named `*.test.ts` / `*.test.tsx`.

## 2. Solid 2 scheduling (not Solid 1)

- Signal writes are **batched until a flush**. After `setX(...)` in a test, call `flush()`
  (from `solid-js`) or wrap the command in `settle(...)` before asserting. Do not assume
  synchronous updates as in Solid 1, and do not add flushes to production code.
- Create JSX **inside** the render callback: `render(() => <Comp />)`, so it belongs to the test owner.
- Models: `renderHook(() => createX(...))` from `@solidjs/testing-library`; drive inputs with
  signals created inside the hook callback (see `src/features/gallery/__tests__/createSelection.test.ts`).
- Prop changes on a component: `renderSolid(...).rerender(nextProps)`. There is no React-style
  `rerender(<Comp prop={...} />)`.
- Async ordering: use deferred promises you resolve manually to force response order (stale
  responses, supersession), then `settle`. Fake timers: `vi.useFakeTimers()` +
  `vi.advanceTimersByTimeAsync(ms)`; restore in `afterEach`.
- Assert listener/observer/timer cleanup directly when that lifetime is the behavior.
- Check the installed `solid-js` / `@solidjs/testing-library` versions (exact RC pins in
  `package.json`) before relying on any API you have not seen in existing tests.

## 3. Mocking Tauri (project convention)

Only `src/platform/tauri` calls IPC. Mock at the layer under test with `vi.mock`, not `mockIPC`:

- **Wrapper contract tests** (`src/platform/tauri/__tests__/api.test.ts`): mock
  `@tauri-apps/api/core` (`invoke`, `convertFileSrc`, `Channel`) with `vi.hoisted`, call the
  wrapper, and assert the exact command name and **camelCase** argument object, plus how the
  snake_case response is passed through or decoded. Every new/changed command gets a case here.
- **Model and component tests**: mock the platform module the code imports, e.g.
  `vi.mock("../../../platform/tauri/assets", () => ({ ... }))`, `.../tags`, `.../scanning`,
  `.../maintenance`, `.../thumbnails`; events through `@tauri-apps/api/event`; dialogs through
  `@tauri-apps/plugin-dialog`.
- Responses in mocks must match `src/platform/tauri/contracts/` types (snake_case fields, tagged
  `status`). Type the mock return values with those contract types so `bun run typecheck` catches drift.
- Binary id ranges: mocking `core.invoke` → resolve `idRangeBuffer(ids)` (empty `ArrayBuffer`
  for `stale`); mocking the `assets` module → `idRangeResult(ids)` or `{ status: "stale" }`.
  Edge cases of `decodeAssetIdRange` stay in the wrapper tests.
- Reset mock implementations and timers in `beforeEach`/`afterEach`; clear `localStorage` keys
  (`media-tagger.*`) you set.
- A mocked `invoke` assertion is not proof that Rust accepts the payload: pair it with a Rust
  serde test (`tauri-rust-tests`) and, for critical paths, desktop E2E.

## 4. Components, a11y and i18n

- Query by role/label/text (`getByRole("button", { name: ... })`), `findBy…`/`waitFor` for async
  content, `userEvent.setup()` for interactions, keyboard tests for focus/a11y contracts.
- Text comes from i18next (English in tests). If you change copy or aria labels, update the
  locale JSONs per `docs/development/localization.md`, the E2E selectors, and the
  `verify-mediatagger` recipes in the same change.
- Respect import direction: tests may import from anywhere, but don't make production `ui/`
  import `features/`/`library/` to ease a test.

## 5. Running

```bash
bun run test -- src/features/gallery/__tests__/createSelection.test.ts
bun run test -- <file> -t "test name"
bun run test                       # whole suite
bun run test -- --maxWorkers=1     # if a fork runs out of memory
bun run typecheck && bun run lint
```

Never `npx vitest` or plain `vitest run` (it skips the parent heap limit).

## 6. Pitfalls

- Asserting before a flush (Solid 2) and getting the old DOM; or hiding that with `waitFor` when a `flush()`/`settle` is the real boundary.
- Destructuring props in a component (kills reactivity); `renderSolid.rerender` exposes it.
- Calling a platform wrapper at module top level; it runs at import time, before `vi.mock` setup.
- Forgetting that `vi.mock` paths are relative to the test file and must match the module the code under test imports.
- Snapshotting large DOM trees; assert what the user sees.
