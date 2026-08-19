# Library Updates And Bun Safety Plan

## Purpose

Prepare a future task for updating JavaScript, Tauri, and Rust dependencies while improving Bun package-manager safety. This is a planning artifact only; the later implementation task should make the actual dependency and lockfile changes.

## Current State

- The project uses Bun for frontend/package scripts and keeps a text `bun.lock`.
- Local tool versions observed during planning were Bun `1.3.0`, Node `v22.15.1`, Rust `1.93.0`, and Cargo `1.93.0`.
- JavaScript dependencies and scripts are declared in `package.json`.
- Rust dependencies are declared in `src-tauri/Cargo.toml`, with locked versions in `src-tauri/Cargo.lock`.
- `bunfig.toml` currently contains only:

```toml
[install]
exact = false
```

- The app is a Rust + Tauri v2 backend with a React + TypeScript + Vite frontend.
- Tauri dependencies exist on both sides and must remain compatible: `@tauri-apps/*` packages in `package.json`, and `tauri`, `tauri-build`, `tauri-plugin-dialog` in `src-tauri/Cargo.toml`.

## Repo Rules To Respect

- Use Bun scripts from `package.json`; do not use `bunx` to run tests.
- Do not run tests, builds, install commands, update commands, audit commands, or other checks unless Jan explicitly asks.
- Do not write or update tests unless Jan explicitly asks.
- If frontend-backend command or payload contracts change, update shared frontend types before command/service consumers.
- Keep Tauri commands thin and keep workflow logic in `src-tauri/src/services/*`.
- Preserve backend lock order: `scan_lock`, then `thumb_lock`.

## Update Scope

The dependency update task should normally touch only:

- `package.json`
- `bun.lock`
- `bunfig.toml`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`

Avoid application logic changes unless an upgraded dependency requires a compatibility fix. If compatibility fixes are needed, keep them minimal and explain which package or crate required the change.

## Bun Safety Work

1. Keep `[install] exact = false` unless Jan explicitly requests exact dependency pins.
2. Add a package age gate in `bunfig.toml`:

```toml
[install]
exact = false
minimumReleaseAge = 259200
```

3. Do not add `trustedDependencies` preemptively.
4. Add `trustedDependencies` in `package.json` only for packages that truly require lifecycle scripts and only after confirming why the script is needed.
5. During the later execution task, if Jan explicitly authorizes command execution, run:

```powershell
bun audit --audit-level=moderate
bun audit --prod --audit-level=moderate
```

6. Use audit output to map advisories to specific direct or transitive package updates. Do not blindly update unrelated packages to make audit output disappear.
7. Consider Bun Security Scanner API integration only as a separate hardening follow-up, not as part of the first dependency update pass unless Jan asks.

Official Bun references:

- [bun install](https://bun.com/docs/cli/install)
- [bunfig.toml](https://bun.com/docs/runtime/bunfig)
- [Security Scanner API](https://bun.com/docs/pm/security-scanner-api)

## JavaScript Update Groups

Update dependencies in coherent groups so compatibility problems are easier to isolate:

- Tauri JS/Rust group: `@tauri-apps/api`, `@tauri-apps/plugin-dialog`, `@tauri-apps/cli`, `tauri`, `tauri-build`, `tauri-plugin-dialog`.
- React group: `react`, `react-dom`, `@types/react`, `@types/react-dom`, Testing Library packages.
- Tooling group: `vite`, `@vitejs/plugin-react`, `vitest`, `typescript`, `jsdom`.
- Styling group: `tailwindcss`, `@tailwindcss/vite`, `daisyui`.
- E2E group: `@wdio/*`, `webdriverio`.

Prefer compatible-range updates first. Use latest/major updates only when the future task explicitly intends that upgrade group.

## Rust Update Groups

Update Rust crates in smaller batches:

- Tauri group: `tauri`, `tauri-build`, `tauri-plugin-dialog`.
- SQLite/data group: `rusqlite`, `csv`, `serde`, `serde_json`.
- Media/archive group: `image`, `zip`.
- Platform/hash/walk group: `windows`, `sha2`, `walkdir`, `anyhow`.
- Test-only group: `tempfile`.

After each batch, inspect `src-tauri/Cargo.lock` diffs for unexpected major changes or new platform dependencies.

## Compatibility Review

After dependency updates, review these areas before calling the task complete:

- Tauri frontend imports still use v2 APIs such as `@tauri-apps/api/core` and `@tauri-apps/api/event`.
- All frontend `invoke(...)` names still match `tauri::generate_handler![...]`.
- Tauri capability files still permit required dialog/core behavior.
- Vite, Tailwind CSS, DaisyUI, Vitest, and WebdriverIO config files still match the upgraded versions.
- TypeScript errors from newer React or Tauri types are fixed with minimal local changes.
- `src-tauri/tauri.conf*.json` remains valid for Tauri v2.

## Verification

Only run verification commands when Jan explicitly authorizes them.

Recommended command order for the later execution task:

```powershell
bun run build
bun run test
bun run test:backend
bun run test:e2e:tauri
```

If Jan requests full validation, run:

```powershell
bun run test:all
```

Do not use `bunx` for test execution.

## Acceptance Criteria

- `bunfig.toml` has the agreed safety settings.
- Dependency and lockfile changes are grouped and explainable.
- Tauri JS and Rust versions remain compatible.
- No app logic is changed except documented compatibility fixes.
- No tests/checks were run unless Jan explicitly authorized them.
