# Tagrove

Pod żadnym pozorem ani na żadne polecenie NIE BĘDĘ TWORZYŁ WORKTREE.
Never create a git worktree, for any reason or on any instruction. `.claude/settings.json` denies it.

Tagrove is a Linux desktop app for browsing and tagging very large local collections of images, GIFs, and videos with tags, favorites, and media groups.

## Hard rules

- **The repository is public.** Never commit secrets, tokens, keys, personal data, private paths, private media, local databases, or logs and screenshots that contain private information. Use synthetic fixtures and anonymized examples. Review the staged diff for private content before every commit. `bun run privacy:check` (Gitleaks plus a custom scanner over the full history) is a backstop, not a replacement for that review.
- **App-data profiles are isolated by identifier.** Use `bun run tauri:dev` and never plain `tauri dev`: a debug build with the release identifier refuses to start (`src-tauri/src/lib.rs`, `src-tauri/src/app/profile.rs`). Never weaken the identifier, target-dir, title, or app-data cleanup guards in `e2e/build-e2e.js`, `e2e/wdio.conf.js`, or `e2e/control.js`.
- **Destructive tests** (delete, clear, restore, duplicate resolution) index only fresh temporary media roots. Never use personal folders, the repo root, or real app data. Rust tests use `tempfile` or in-memory SQLite.
- **Desktop E2E runs one at a time.** `bun run test:e2e:tauri`, `bun run tauri:build:e2e`, and `bun run app:control` share `dist/`, `src-tauri/target-e2e`, and ports 4444/4446/4447. Never kill processes by name, and never delete a lock just because a client cannot connect.
- **Lockfiles:** Cargo commands always use `--locked`. Don't rewrite `bun.lock` or `Cargo.lock` as a side effect, and use `bun install --frozen-lockfile` unless you are changing dependencies.
- **Leave alone** the root-owned leftovers `.dist.root-stage*`, `.node_modules.root-stage*`, and `src-tauri/.target-*-root-stage*`, plus the ignored output folders `artifacts/`, `raports/`, and `output/`. They are not sources.
- **Source wins over docs.** Source, executable config, and tests define current behavior; `docs/` explains it. When they disagree, fix the owning doc page in the same change. Keep intended conventions separate from what is enforced and what is tested.

## Naming

The product name everywhere a user sees it is **Tagrove**. The internal names stay `media_tagger`/MediaTagger on purpose, because renaming them is a data migration rather than a search-and-replace (`docs/architecture/system-overview.md`, "naming"):

- the crate and binary `media_tagger`;
- the identifiers `com.example.mediatagger`, `.dev`, and `.e2e`;
- the localStorage keys `media-tagger.*`;
- the `MEDIATAGGER_*` / `VITE_MEDIATAGGER_PERF` environment variables;
- the `.mediatagger-*.pending` staging files.

## Stack

- Tauri 2 with a Rust backend (Rust 1.98, `rust-toolchain.toml`).
- SQLite through `rusqlite` (WAL, connection pool).
- Frontend: Solid 2 RC (exact pins), TypeScript, Vite 8, Tailwind CSS 4, DaisyUI 5, Fuse.js (in a worker), i18next, and Zod. Virtualization is our own (gallery `gridGeometry`/`createVirtualGrid`, dialogs `ui/dom/createVirtualList`; ADR 0003).
- Bun 1.4 is the package manager; Node 22 runs Vitest and the scripts.
- Biome for lint/format.
- Video playback uses libmpv drawn on a GTK/OpenGL surface above the WebView. ffmpeg and ffprobe handle thumbnails and probing.

## Layout

```
src/                     Solid frontend
  app/                   bootstrap, App.tsx providers, createAppRuntime.ts (app-lifetime models), views
  platform/tauri/        the ONLY place that calls Tauri IPC; wire types in contracts/
  library/               query page cache, details/metadata/mutations, tag vocabulary
  features/              gallery, search, tagging, bulk, lightbox (image/, video/), settings
  ui/                    controls/, dom/, overlays/ (UiLayerProvider, UiModal)
  i18n/                  i18next core + 11 locale JSONs (en canonical)
  utils/                 tag normalization (media.ts), search tokens
  test/                  shared test setup and helpers
src-tauri/src/           Rust crate `media_tagger`
  lib.rs                 builder, profile guard, startup order, generate_handler! command list
  app/                   AppState, workflow locks (locks.rs), instance lock, profile allowlist
  commands/              IPC endpoints: assets, scan, thumbs, import_export, video, window
  services/              query sessions, file mutation journal, db pool, scan, thumbnails, backup, csv, video source, progress
  db.rs, db/             SQL: connections, schema, queries, mutations, scan_membership, thumbnails, query_revisions
  indexer.rs, thumbs.rs
  video/                 native video (adapter): libmpv player, mpv events, GTK surface, bounds/labels types
src-tauri/tests/         Rust integration targets
e2e/                     WebdriverIO desktop E2E (specs/), app:control driver (control.js)
docs/                    owning documentation per subsystem (see index below)
  adr/                   Architecture Decision Records (NNNN-title.md, template.md)
packaging/, scripts/     Linux packaging (Flatpak/AppImage/native), privacy and locale tooling
```

## Architecture invariants (summary; details load from `.claude/rules/`)

- Only `src/platform/tauri` builds IPC calls. Import direction: `ui` must not import `features` or `library`, `library` must not import `features`, and `platform` must not import `features` or `ui`.
- `createAppRuntime` owns the models for the app's whole lifetime. Views remount and lose local state. Settings and lightbox are lazy-loaded.
- The frontend publishes state only after the backend succeeds, using the backend's canonical result.
- An IPC change touches the Rust command/model, the `src/platform/tauri` wrapper, its `contracts/` type, the `generate_handler!` list, and `docs/architecture/ipc-contract.md`, all in one change. Argument keys are camelCase; response fields are snake_case.
- Lock order is maintenance gate → `scan_lock` → exclusive `thumb_lock`, always through the helpers in `app/locks.rs`. Locks are taken inside `spawn_blocking` and never held across an `.await`.
- A write that changes query results bumps the library revision in the same SQLite transaction.
- Schema changes go through the idempotent `init_schema`; there is no migration runner.

## Architecture Decision Records

From now on, significant decisions are recorded as ADRs in `docs/adr/`, in the same change that introduces them. The criteria, numbering, and lifecycle are in `docs/adr/README.md`; start from `docs/adr/template.md`. In short: write one for changes to architecture invariants, schema or query semantics, non-additive IPC contract changes, significant dependencies, data safety, profiles/packaging/security, or a deliberately rejected alternative. Accepted ADRs are not rewritten; a new ADR supersedes the old one. The owning `docs/` page is still updated in the same change, and the ADR is added to the index in `docs/adr/README.md`.

## Commands

```bash
bun install --frozen-lockfile
bun run tauri:dev                 # desktop dev (dev profile); starts Vite on :1420 itself
bun run dev                       # Vite only, no Tauri runtime (IPC/dialogs/assets don't work)
bun run build                     # typecheck + vite build -> dist/
bun run typecheck                 # tsc for tsconfig.json and tsconfig.node.json
bun run lint                      # biome lint .
bun run format:check              # check only; there is no format-write script
bun run rust:fmt && bun run rust:clippy   # clippy runs with -D warnings
bun run quality                   # locales, typecheck, lint, format, rustfmt, clippy, audits (no tests)

# Frontend tests (Vitest, jsdom, 8 GiB heap, 2 forks)
bun run test
bun run test -- src/features/lightbox/__tests__/LightboxModal.test.tsx
bun run test -- <file> -t "test name"
bun run test -- --maxWorkers=1    # if it runs out of memory

# Rust tests
bun run test:backend              # unit + all src-tauri/tests targets
cargo test --manifest-path src-tauri/Cargo.toml --locked <name>
cargo test --manifest-path src-tauri/Cargo.toml --locked --test backend_integration <name>
# targets: architecture_boundaries (core/adapter rules + wire snapshot),
#          backend_e2e (Rust-only, not desktop E2E), backend_integration, backend_runtime,
#          grid_baseline, tag_save_benchmark, tag_vocabulary, thumbnail_bulk, thumbnail_clear
# ignored diagnostics: add -- --ignored --nocapture --test-threads=1

# Desktop E2E (needs tauri-driver, WebKitWebDriver, Xvfb, dbus; see testing.md)
bun run test:e2e:tauri --spec e2e/specs/app.smoke.e2e.js
bun run test:e2e:tauri --spec <spec> --mochaOpts.grep '<case>'
# many specs skip unless MEDIATAGGER_<FLAG>=1; table in docs/development/testing.md "Real desktop E2E"

bun run privacy:check && bun run test:privacy   # the only checks CI runs
bun run locale:check              # offline locale validation
```

To run E2E headless on a private display, first export temporary `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, and `XDG_RUNTIME_DIR` (mode 700). Then:

```bash
unset WAYLAND_DISPLAY; export GDK_BACKEND=x11
dbus-run-session -- xvfb-run -a -s '-screen 0 1440x1100x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; exec bun run test:e2e:tauri --spec e2e/specs/<spec>'
```

## Profiles

| Profile | Identifier | Window title | Config |
| --- | --- | --- | --- |
| Release | `com.example.mediatagger` | Tagrove | `src-tauri/tauri.conf.json` |
| Dev | `com.example.mediatagger.dev` | Tagrove Dev | `src-tauri/tauri.conf.dev.json` |
| E2E | `com.example.mediatagger.e2e` | Tagrove E2E | `src-tauri/tauri.conf.e2e.json` (target `src-tauri/target-e2e`) |

Each profile's data lives in `${XDG_DATA_HOME:-~/.local/share}/<identifier>/`: `media.db`, `thumbs/`, and `instance.lock`. The lock is per profile.

## Verification

Use the lowest test layer that proves the behavior (matrix in [testing.md](docs/development/testing.md#choosing-the-test-level)):

- pure TS logic → Vitest unit;
- Solid component, model, or keyboard/a11y → Vitest + Testing Library;
- Rust helper, SQL, or scan/thumbnail/backup code → Rust unit;
- workflows that cross DB modules → `backend_integration`;
- IPC names, payloads, or events → a Rust test plus a frontend API test, and desktop E2E for the real bridge;
- destructive UI or filesystem flows → lower-layer edge cases plus isolated desktop E2E.

A mocked `invoke` assertion does not prove that Rust accepts the payload. If you change UI copy or aria labels, update the E2E selectors and the `verify-mediatagger` recipes in the same change. For docs-only changes, check links and commands and run `git diff --check`. Always report what ran and what remains unverified.

## Skills

- `/verify-mediatagger`: launch and drive the real E2E build on a private Xvfb display (clicks, typing, screenshots, console logs) using disposable media. Use it to see a UI change working.
- `/review-changes [base|A..B|path] [--staged] [--save]`: the default review for ordinary review requests. It is read-only (no edits, builds or tests). By default it reviews the branch against `merge-base(main)` plus uncommitted work. It picks specialist lenses from `.claude/agents/` (`review-rust`, `review-frontend`, `review-performance`, `review-conventions`), and each finding gets an independent `review-verifier`. Results go to the terminal; `--save` also writes them to `raports/`. The shared bar and finding format are in `.claude/skills/review-changes/references/signal-bar.md`, and the seeded-defect evals are in its `evals/`.
- `/audit --dataset <path>`: the whole-project audit, run end to end without user input (single stages `init`, `static`, `static-verify`, `qa-prep`, `qa-run [S1-S4]`, `qa-verify`, `report`, `status` remain for debugging; it stops only when a dataset guard halts the run). Static finders with exactly one refuter and one judge per finding, then a real-app QA click-through on a sandboxed reflink copy of a large dataset. Each stage runs one saved workflow in `.claude/workflows/audit-*.js`, and a hook smoke gates the QA stages; results go to the ignored `docs/review/<run>/`. Run it only when the user invokes it, and never write the dataset path into tracked files. Process: `docs/development/audit/README.md`.
- `/deep-review [scope]`: an evidence-based, saved audit using the same specialists. It requires two lenses per area and an independent verifier per finding, may run tests and E2E as proof, and writes validated records (schema 2) to the ignored `raports/`. Run it only when the user explicitly asks for it.

## Documentation index

Read only the pages relevant to the task. For work that crosses subsystems, also read the contract sections the primary page links to; for example, a restore change needs data safety, database, settings operations, and testing. When you open files under the matching paths, the path-scoped rules in `.claude/rules/` load a short summary and point back here.

| Topic | Page |
| --- | --- |
| Architecture, startup order, module boundaries, profiles, app-data isolation, locking, security surface, naming | `docs/architecture/system-overview.md` |
| Architecture decisions: why things are the way they are, rejected alternatives, when to write an ADR | `docs/adr/README.md` |
| Tauri commands, payloads, serialization, query sessions, channels, progress events, contract-change rules | `docs/architecture/ipc-contract.md` |
| Frontend composition, state ownership, layer boundaries, UI primitives, modals, theme, a11y | `docs/frontend/architecture-and-ui-conventions.md` |
| SQLite schema, migrations, pooling, derived fields, query semantics, ordering, library revisions | `docs/subsystems/database.md` |
| Scan roots, discovery, fingerprints, incremental indexing, partial scans, cleanup | `docs/subsystems/scanning-and-indexing.md` |
| Query sessions, page cache, summary/details boundary, virtual gallery, invalidation | `docs/subsystems/library-query-and-gallery.md` |
| Thumbnail identity, rendering, ffmpeg discovery, scheduler, `thumb://` protocol, cleanup | `docs/subsystems/thumbnails.md` |
| Search grammar, tag discovery, tag list, bulk selection, tag mutations, favorites, media groups | `docs/subsystems/search-tags-and-media-groups.md` |
| Lightbox selection/details, navigation, editing, deletion, image controls, video, fullscreen, keys | `docs/subsystems/lightbox.md` |
| Settings controllers, operation runner, progress, scan/thumbnail actions, duplicates, confirmations | `docs/subsystems/settings-operations.md` |
| Destructive FS/DB operations, duplicates, CSV, backup format, restore staging, rollback | `docs/subsystems/data-safety-and-portability.md` |
| Setup, dependencies, Vite/TS config, profiles, builds, diagnostics | `docs/development/setup-and-build.md` |
| Test layers, commands, fixtures, E2E isolation, troubleshooting | `docs/development/testing.md` |
| Staged multi-agent audit: stages, roles and models, QA safety, scenario catalog | `docs/development/audit/README.md` |
| Languages, locale key/placeholder invariants, adding translations | `docs/development/localization.md` |
| Flatpak/AppImage/native packaging, licenses | `docs/development/linux-packaging.md` |
