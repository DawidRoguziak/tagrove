# 0036: GPUI is a second UI adapter in its own Cargo workspace, taken from the Zed repository

- Status: Accepted
- Date: 2026-11-20
- Areas: architecture, dependencies, profiles, packaging, testing
- Related: [system overview](../architecture/system-overview.md#gpui-port-a-second-adapter-in-its-own-workspace), [setup and build](../development/setup-and-build.md#gpui-adapter), [testing](../development/testing.md#gpui-workspace), [GPUI port plan](../plans/gpui-port/README.md) (decisions D1, D2, D5), [ADR 0034](0034-tauri-independent-backend-core.md), [ADR 0032](0032-rusqlite-0-41-for-bundled-sqlite-3-54.md)

## Context

[ADR 0034](0034-tauri-independent-backend-core.md) made the backend core `media_tagger_core` independent of Tauri. The GPUI port builds a second interface on that core while the Tauri/Solid application keeps working and stays the behaviour reference. Three forces shape how the port is added:

- **Where GPUI comes from.** The `gpui` crate on crates.io is more than a year behind the Zed repository. The newer `gpui-pre` snapshots and `gpui-component` on crates.io are published by a third party, not by Zed.
- **The released application's dependency tree.** `packaging/flatpak/generate.py` rejects every Cargo source that is not crates.io, and `src-tauri/Cargo.lock` has no Git source today. A Git dependency in that lock would break packaging and put several hundred GPUI packages into the audit of the shipped application.
- **The user's library.** A new interface must not open the release, dev or E2E profile of the Tauri application while it is being built.

## Options considered

1. **Add the GPUI crates to the `src-tauri` workspace.** One lock file and one `cargo` invocation, but the lock gains Git sources and the whole GPUI tree, Tauri builds and E2E pay for it, and Flatpak generation fails.
2. **A second workspace, `src-gpui/`, with the core as a path dependency.** Two lock files that can drift apart in the core's dependencies; in exchange the Tauri workspace, its lock, audit and packaging do not change at all.
3. **GPUI from crates.io** (`gpui` 0.2.2, or `gpui-pre` with `gpui-component`). No Git dependency, but either stale code or code whose publisher is not Zed.
4. **GPUI from the Zed repository, pinned to a commit.** Provenance is one repository; the cost is a Git dependency, a manual check on every bump, and no semver.
5. **Reuse the Tauri profiles.** Both interfaces would see the same library, and a bug in the port could damage it; the two applications could also not run side by side (`instance.lock`).

## Decision

- **Workspace.** The port lives in `src-gpui/`: its own `Cargo.toml` with `[workspace]`, its own `Cargo.lock`, and the target directories `src-gpui/target` and `src-gpui/target-e2e`. `media_tagger_core` is the path dependency `../src-tauri/crates/media_tagger_core`; it stays a member of the Tauri workspace, so its `*.workspace = true` keys still resolve from `src-tauri/Cargo.toml`. `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock` never gain a GPUI or Git dependency.
- **Two crates, one direction.** `media_tagger_gpui` → `media_tagger_client` → `media_tagger_core`. `media_tagger_client` holds presentation logic and names no GUI crate. It reaches the core beyond `models` and `error` only in `backend/core_backend.rs`, the adapter of its `Backend` port. `media_tagger_gpui` holds views and executor glue; it reaches the core only in `boot.rs` (startup composition) and never touches SQLite or the workflow locks.
- **GPUI source.** `gpui` and `gpui_platform` are Git dependencies on `https://github.com/zed-industries/zed`, both pinned with `rev` to the full commit SHA of a stable Zed release tag (first pin: `v1.29.3`, `d01b134cddf5e4f71964deb2d6f81cdb38fcf1e7`). No `gpui-pre`, no `gpui-component`. The only other Git sources in `src-gpui/Cargo.lock` are repositories this Zed commit pins itself: `gfx-rs/wgpu` and the Zed forks `font-kit`, `oo7`, `scap` and `xim-rs`. None of Zed's `[patch.crates-io]` entries is repeated, because the workspace builds and runs without them.
- **Core parity.** `bun run gpui:lock-parity` (`scripts/gpui-lock-parity.py`) fails when a package of the core's dependency closure has a different version, source or checksum in the two locks, when a package of `src-gpui/Cargo.lock` comes from an unlisted source, or when the Zed dependencies do not share one full `rev`. Both applications therefore link the same bundled SQLite ([ADR 0032](0032-rusqlite-0-41-for-bundled-sqlite-3-54.md)).
- **Profiles.** The port has two profiles, `com.example.mediatagger.gpui.dev` ("Tagrove GPUI Dev") and `com.example.mediatagger.gpui.e2e` ("Tagrove GPUI E2E"), and no release identifier until the cutover epic. The identifier is baked in at build time (`MEDIATAGGER_GPUI_IDENTIFIER`, default dev); `profile::resolve` refuses every other identifier in debug and release builds, before anything touches the disk. Data lives in `${XDG_DATA_HOME:-~/.local/share}/<identifier>/` with the same layout as the Tauri profiles.
- **Startup.** `boot::start` runs the core's framework-free phases in the Tauri order: app-data directory → `InstanceLock` → `open_storage` → `resolve_ffmpeg_path` → `start_workers`. Before it, a display check probes the display server the environment names and stops the startup when none answers. The result of `boot::start` is dropped after the event loop returns: the database pool closes, then the instance lock is released. The thumbnail scheduler has no shutdown, so its worker threads end with the process.

## Consequences

- The Tauri application, its lock file, `audit:rust` and the Flatpak generator are unchanged; `bun run test:packaging` keeps proving that the released lock has only crates.io sources.
- There are two lock files. A core dependency change now updates both, and `gpui:lock-parity` is part of `bun run quality`. A package that only one closure contains (Cargo feature unification, dev-dependencies) is listed by the check and does not fail it.
- A Zed bump is a reviewed change: pick the commit of a stable release tag, re-derive `PINNED_BY_ZED` in `scripts/gpui-lock-parity.py` from that commit's manifests, list the packages new to both locks with their licences, run `bun run audit:gpui`, and do the first build of new dependencies offline in a sandbox ([setup](../development/setup-and-build.md#updating-the-pinned-zed-commit)). `src-gpui/Cargo.lock` deliberately differs from Zed's own lock where the core closure, a RustSec advisory or a yanked release requires it.
- Zed develops on a newer patch toolchain than `rust-toolchain.toml` pins. The pinned commit builds on the repository toolchain; a commit that does not is not adopted without a toolchain decision for both workspaces.
- `src-tauri/tests/architecture_boundaries.rs` scans `../src-gpui/crates`: the client manifest (the `GUI_CRATES` list plus `gpui*`, `wgpu*`, `blade*`), the client's and the adapter's use of `media_tagger_core`, and the dependency direction. The wire snapshot of the Tauri adapter is untouched.
- The two applications and all five profiles can run at the same time: every identifier has its own `media.db`, `thumbs/` and `instance.lock`.
- The GPUI adapter is not packaged, and `src-gpui` is not part of the release build. Packaging, a release identifier and removing Tauri are the cutover epic's decisions and need a new ADR.
- Tests: `cargo test --manifest-path src-gpui/Cargo.toml --locked` (profile guard, display check, startup composition on temporary profiles, the startup guards in order, the window in GPUI's test platform and through the in-process headless renderer), `scripts/test_gpui_lock_parity.py`, `scripts/gpui-build-e2e.test.mjs`, and the GPUI rules in `architecture_boundaries`.
