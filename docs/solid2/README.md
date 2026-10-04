# React to Solid 2 migration plan

## Purpose and current status

Rewrite the frontend using Solid 2 while preserving the current appearance, interactions,
accessibility, persistence, and backend behavior. The working React application is the
behavioral reference. Its directory structure does not constrain the new implementation.

Implementation has reached stage 5. React remains the functional reference and default
entry; Solid has a separately compiled shell, platform boundary, shared controls and
application-owned library models, virtual gallery, search, tag discovery, shared tag
editing and bulk actions. Lightbox selection and images are next. Track verification and completion in
[PROGRESS.md](PROGRESS.md), and test ownership in [BEHAVIOR-INVENTORY.md](BEHAVIOR-INVENTORY.md).

The planning baseline is commit `f11126dfafd40f877ecf5e496abc37d77a435e78` on
`feature/rewrite-react-to-solid2`. During planning, all 699 frontend tests across 84 files
passed, and TypeScript checks passed. Rust and desktop checks were not run during planning.

## Working rules

- Work in the existing checkout and feature branch. Never create a worktree.
- Never push to GitHub or merge to `main`.
- Keep React runnable through a separate entry point until Solid passes parity checks.
- Every implementation stage includes writing or porting the tests for that stage,
  agent self-review, fixing findings, verification, and a local commit.
- Review staged content for private information before every future commit. Use synthetic
  media and disposable databases; keep screenshots and raw logs in ignored artifacts.
- Keep brief contract and path corrections with earlier stages when needed for accuracy.
  The final stage completes the documentation rewrite.
- Treat executable code and tests as the source of current behavior. Consult the relevant
  pages in the repository documentation index before implementing each stage.

## Dependency choices

The following versions were verified against public npm metadata on 4 October 2026:

| Package | Version |
| --- | --- |
| `solid-js` | `2.0.0-rc.13` |
| `@solidjs/web` | `2.0.0-rc.13` |
| `@solidjs/compiler` | `2.0.0-rc.13` |
| `@solidjs/vite-plugin` | `3.0.0-next.47` |
| `@solidjs/testing-library` | `1.0.0-beta.3` |

Recheck the latest Solid 2 RC when implementation starts, then pin the compatible runtime,
compiler, plugin, and testing package set and commit the lockfile. Do not assume npm tag
names identify the newest compatible package: the runtime used `next`, while the newest
Vite plugin used `latest`. Further RC upgrades require their own review and verification.

Use Node `>=22.12.0`, the Solid 2 runtime and plugin minimum. Keep Bun, Vite, Vitest,
Tailwind, DaisyUI, i18next, Fuse.js, and Zod. There is no switch to Jest, SolidStart, SSR,
or a new router.

Use a narrow Solid adapter over `@tanstack/virtual-core`, initially matching the core
version resolved by the working React virtualizer. The published
`@tanstack/solid-virtual@3.13.40` declares Solid 1 compatibility, so it is not the migration
target. Retain the existing row geometry and test it through the real core implementation.

Sources:

- [Solid RC release](https://github.com/solidjs/solid/releases/tag/solid-js@2.0.0-rc.13)
- [Solid 2 migration guide at the selected RC](https://raw.githubusercontent.com/solidjs/solid/solid-js@2.0.0-rc.13/documentation/solid-2.0/MIGRATION.md)
- [Vite plugin](https://www.npmjs.com/package/@solidjs/vite-plugin)
- [Solid testing library](https://www.npmjs.com/package/@solidjs/testing-library/v/1.0.0-beta.3)
- [TanStack Solid adapter manifest](https://raw.githubusercontent.com/TanStack/virtual/main/packages/solid-virtual/package.json)

## Target file structure

```text
src/
  app/
    main.tsx
    App.tsx
    createAppRuntime.ts
    AppRuntimeContext.tsx
    WindowControls.tsx
  platform/
    tauri/
      contracts/
      assets.ts
      tags.ts
      scanning.ts
      thumbnails.ts
      maintenance.ts
      video.ts
      window.ts
  library/
    createLibrary.ts
    query/
    metadata/
    thumbnails/
    vocabulary/
  features/
    gallery/
    search/
    tagging/
      suggestions/
    bulk/
    lightbox/
      image/
      video/
    settings/
      sections/
      dialogs/
  ui/
    controls/
    overlays/
    dom/
  i18n/
  test/
  styles.css
```

Place tests in `__tests__` beside the owning modules. Feature folders contain named
components, `create…Model` functions, and focused helpers where the behavior needs them.
Create directories when their stage introduces the implementation; do not add empty stubs.
Avoid a global hooks directory, generic service layers, or wrappers that only forward calls.

| Area | Responsibility and dependency rules |
| --- | --- |
| `app` | Bootstrap, navigation, lifetime, dependency composition, lazy loading, and window controls. Composes features and shared models. |
| `platform/tauri` | Commands, payloads, channels, events, native dialogs, media URLs, and window operations. Does not import UI or feature state. |
| `library` | Query cache, shared metadata mutations, tag-write coordination, thumbnails, vocabulary, and invalidation. Does not import feature components. |
| `features` | Feature behavior and presentation. Use explicit shared-model contracts; avoid imports into another feature's private state. |
| `ui` | Controls, modal layers, focus, presence, and reusable DOM behavior. Does not import library or feature code. |
| `i18n` | Existing resources, language normalization and persistence, and the Solid binding to i18next. |
| `test` | Shared synthetic fixtures, Tauri mocks, DOM setup, and renderer-specific test helpers. |

The shared tag editor belongs in `features/tagging`, because suggestions and tag behavior
are application features. Lightbox and bulk consume it without owning separate editors.
Keep pure business helpers beside their domain or feature. Share framework-independent
helpers between the two entry points instead of copying them.

## Ownership and interfaces

`createAppRuntime` constructs the library, search, selection, bulk, settings, and suggestion
worker models once under the application owner. They survive Gallery/Settings switching.
The context provides access to those stable models; it does not become another state owner.
Leaf components receive narrow state and command contracts.

DOM measurements, popovers, and temporary interaction state belong to the mounted view.
Native playback belongs to the active media session. Modal presence may retain DOM during
exit, but logical close still releases keyboard layers, focus traps, and background locks
at the same point as the React implementation.

Models expose read-only reactive state or accessors and explicit commands. Remove React
`Dispatch`, `SetStateAction`, refs, and component types from these contracts. Use
discriminated states where loading, failure, and ready states would otherwise contradict
each other. Keep transport types separate from editor drafts and view state.

Tauri command names, wire payloads, channel events, database schema, app-data profiles,
storage keys, and backend behavior remain unchanged. Split the frontend API boundary by
subsystem without changing serialization. Components must not construct invoke payloads
or pass reactive proxies directly across IPC.

## Solid 2 patterns

- Use signals for independent values, stores for structured reactive state, and memos
  for derived values. Avoid effects that copy one reactive value into another.
- Use the RC's compute/apply effect API, owned cleanup, and `onSettled` for DOM setup.
  Ref callbacks capture elements; cleanup belongs to an owned scope.
- Keep reactive prop reads in JSX, memos, or effect computation. Do not destructure
  reactive props into frozen values at component initialization.
- Use `jsx: preserve` and `jsxImportSource: @solidjs/web` for Solid sources. Keep React
  and Solid TypeScript/compiler contexts separate while both entry points exist.
- Use `lazy`, `Loading`, and `Errored` for feature loading. Preserve loading dialogs,
  failure recovery, close animations, and the 1,500 ms settings/lightbox prefetch.
- Keep locks, generation counters, pending request registries, and navigation targets
  synchronous. Solid 2's batched setters must not admit duplicate writes or stale
  same-turn reads. Compute command inputs explicitly rather than setting and rereading
  a signal. Do not scatter forced flushes through production logic.
- Preserve backend-first persistence and canonical-response publication. Do not add
  optimistic writes during the migration.
- Keep session caches, failed-page tracking, and stale-response guards explicit. Generic
  async resources must not replace their retry, retention, or cancellation rules.
- Subscribe to thumbnail changes per asset. One thumbnail completion must not update
  every gallery tile or rebuild row measurements.
- Dispose listeners, observers, timers, workers, and subscriptions with their owners.
  Late async registration must immediately clean up if its owner was already disposed.

## UI preservation and migration setup

Keep the current CSS, fonts, icons, translation resources, and visual tokens. Component
boundaries and markup may change if appearance, semantics, focus, and geometry remain
equivalent. Preserve the existing gallery/settings remount behavior, scroll containers,
breakpoints, keyboard interactions, native-video clearance, and reduced-motion behavior.

Keep existing React sources in place while adding Solid under the target directories.
Temporary API re-exports and a React-specific i18n binding may support the old entry.
Use `MEDIATAGGER_FRONTEND=react|solid` as a temporary build selection, defaulting to React.
Vite selects one module entry and compiler before bundling. Never mount both renderers
in the same application or build temporary component bridges.

The desktop harness passes the selection through its frontend build, preserving the
existing `dist/` contract, Tauri identifiers, window titles, ports, and cleanup guards.
Run renderer comparisons serially with fresh equivalent E2E fixtures. Development still
uses `bun run tauri:dev`. Stage 9 removes the temporary selection and makes Solid the
normal entry.

## Stages

Execute the stages in order. Each stage file includes its implementation, tests, review,
and intended local commit. The status source of truth is [PROGRESS.md](PROGRESS.md).

| Stage | Plan |
| --- | --- |
| 1 | [Reference and separate toolchains](01-reference-and-toolchains.md) |
| 2 | [Platform boundaries and Solid shell](02-platform-and-shell.md) |
| 3 | [Shared library state](03-library-state.md) |
| 4 | [Gallery, search, and tag discovery](04-gallery-and-search.md) |
| 5 | [Shared tag editing and bulk actions](05-tagging-and-bulk.md) |
| 6 | [Lightbox selection and image behavior](06-lightbox-and-images.md) |
| 7 | [Native video integration](07-native-video.md) |
| 8 | [Settings and maintenance workflows](08-settings-and-maintenance.md) |
| 9 | [Parity, cutover, and React removal](09-parity-and-cutover.md) |
| 10 | [Documentation](10-documentation.md) |

## Stage completion gate

1. Write or port the tests that observe the changed behavior. Keep the React behavioral
   tests until their Solid counterparts prove the same contract. Pure tests can move once.
2. Run focused tests while implementing, then the complete affected test layer. Run both
   frontend projects while React remains supported. Typecheck, lint, validate locales,
   and build the affected entries; use the repository test matrix for Rust/desktop checks.
3. Self-review the actual diff and affected callers for behavior, ownership, cleanup,
   races, accessibility, and dependency direction. No delegated review is required.
4. Fix findings and rerun the affected checks. Required failures or unavailable required
   checks keep the stage incomplete; record them instead of treating skips as passes.
5. Update the tracker with exact commands, outcomes, review findings, and limitations.
6. Inspect staged content for privacy and scope, then make the stage's local commit.
   Record its hash when opening the next stage, so the completed commit need not contain
   its own hash. For the final stage, use the commit subject and Git history to resolve
   the hash without making a recursive bookkeeping commit.

During migration, the total test count may change. Every existing behavior must remain
covered. Replace React render-count and StrictMode checks with Solid update-isolation,
ownership, and disposal checks. Do not weaken assertions or skip cases to reach parity.
Use deferred promises and explicit scheduling boundaries instead of arbitrary sleeps.

Final acceptance includes both themes, English and Polish, all locale validation,
narrow layouts, loading and failure states, keyboard/focus behavior, dialogs, reduced
motion, native video, persistence, and existing performance bounds. Treat timing samples
as diagnostics on the same host; retain deterministic cache, DOM, request-count, and
measurement bounds as acceptance gates. Record any remaining platform limitations.
