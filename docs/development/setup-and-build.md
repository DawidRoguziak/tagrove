# Local setup and builds

Implementation entry points: [package scripts](../../package.json), [Rust workspace manifest](../../src-tauri/Cargo.toml), [core crate manifest](../../src-tauri/crates/media_tagger_core/Cargo.toml), [Linux build](../../Dockerfile.linux), [artifact export](../../scripts/build-linux-docker.sh).

This page describes the setup and build behavior implemented by the current repository. Run the commands from the repository root. Configuration and lockfiles are authoritative; this page does not imply support for platforms or tool versions that the repository does not declare.

## Prerequisites

Linux packages target x86_64. The Docker workflow builds Flatpak against GNOME 50 and AppImage on Debian 12. The Arch-native executable remains an explicit build option. The host needs Docker, Bash, GNU file/coreutils/findutils/diffutils, `flock`, Python 3.12 or later, and Gitleaks 8.30.1. Bun, Node, Rust, and media libraries are installed inside Docker. Install the checksum-pinned scanner with `bash scripts/install-gitleaks.sh /path/to/tools` and add that directory to `PATH`. Package privacy inspection also needs binutils, Flatpak and OSTree.

Node must satisfy `>=22.12.0`. Toolchains remain Bun `1.4.0`, Node `22.22.0`, and Rust `1.98.0`. Portable builds verify the checksums in [toolchains.json](../../packaging/linux/toolchains.json). Keep `.nvmrc`, `packageManager`, `rust-toolchain.toml`, `Cargo.toml`, and packaging pins aligned when updating a toolchain. Lockfiles change only when dependency resolution changes.

### Docker packages

```bash
# Both Flatpak and AppImage from a clean, committed checkout
./scripts/build-linux-docker.sh --commit HEAD
# A single format
./scripts/build-linux-docker.sh --format flatpak
./scripts/build-linux-docker.sh --format appimage
# Arch/CachyOS native executable, for systems with matching libraries
./scripts/build-linux-docker.sh --format native
```

The build requires a clean checkout at the requested `--commit`, which defaults to `HEAD`, and a complete Git history. It runs the privacy gate, prepares a temporary Docker context from that Git tree, and creates `application-source.tar.gz` before compilation. Ignored local inputs never enter that context. Source preparation checks every archive file against Git, rejects symlinks/submodules and generated release inputs, and records `source-commit.txt`. The export must contain the same archive and commit record.

Each successful export replaces only its format directory under `artifacts/linux/`. It contains one versioned application bundle, `SHA256SUMS`, a build manifest, application source, and dependency notices. Every requested format must build and validate before export starts. Failed builds and validation leave previous successful output in place. The build script never installs packages or changes launchers, executables, or application profiles. It uses ordinary Docker build/create/start/cp commands and does not require Buildx.

Flatpak Builder runs in a digest-pinned GNOME 50 container with `--privileged`, as documented by [Flatpak's container workflow](https://github.com/flatpak/flatpak-github-actions/blob/master/README.md). Inputs come from the audited commit tree and pass through Docker's additional context exclusions. Mounts are limited to that build's working/repository volume, a per-checkout Builder state cache, and the dedicated `tagrove-flatpak-downloads-x86_64` download cache. Neither the host home nor Docker socket is mounted. The working volume and containers are removed on exit; the dedicated build and checksum-verified download caches survive.

The first Flatpak container downloads checksum-verified sources generated from `bun.lock` and `Cargo.lock`. The second compiles and exports with Docker `--network none`, Flatpak network isolation, Bun `--offline`, and Cargo `--locked --offline`. It uses the populated Bun cache and Cargo vendor directory. AppImage likewise compiles and bundles in a container with networking disabled, after frozen dependency installation and checksum verification of its packaging tools. Host `node_modules`, Cargo targets, `dist`, app data and artifacts are excluded from Docker inputs.

Flatpak packages ffmpeg, ffprobe and libmpv under `/app`, using the runtime for GTK, WebKit, graphics and audio dependencies. AppImage includes the executable, adjacent ffmpeg/ffprobe, shared libraries, GTK/GStreamer resources and WebKit helper processes. Both exports check x86_64 architecture, linkage, graphics entry points and a generated media frame. These checks do not prove visible video or audible playback; see [package verification](linux-packaging.md).

### Direct Flatpak installation

Use Flatpak 1.16.6 or later. Debian 12 users need its supported Flatpak backport for same-version bundle reinstall; see [the tested version requirement](linux-packaging.md#sources-and-identity).

```bash
flatpak install --user ./artifacts/linux/flatpak/Tagrove-*.flatpak
# Close Tagrove before replacing it.
flatpak install --user --reinstall ./artifacts/linux/flatpak/Tagrove-*.flatpak
flatpak run com.example.mediatagger
```

Flatpak installs and exports its own desktop entry and icons. Reinstalling retains application data. The first Flatpak launch starts a separate library under `~/.var/app/com.example.mediatagger/`, leaving the native library under `~/.local/share/com.example.mediatagger/` untouched. The build and installation workflow does not migrate either library or replace an existing native launcher. Do not use `flatpak uninstall --delete-data` when you intend to retain the library.

The local bundle includes GNOME runtime repository information; Flatpak may ask to fetch that runtime during installation. Filesystem access includes home, `/mnt`, `/media`, and `/run/media`. Grant a different media location explicitly, for example:

```bash
flatpak override --user --filesystem=/srv/photos com.example.mediatagger
```

Wayland, fallback X11, IPC, graphics devices and PulseAudio access are declared in the manifest. Access is writable because rename/delete are application features. Always use disposable fixtures when testing them.

The AppImage can be launched after `chmod +x artifacts/linux/appimage/Tagrove-*.AppImage`. It uses the existing native release identity and library. Use isolated XDG directories for package testing. Native builds need the matching Arch packages `webkit2gtk-4.1 gtk3 mpv libglvnd ffmpeg`; they do not promise Debian/Ubuntu ABI compatibility.

See [release preparation and verification](linux-packaging.md) for publishing identity, corresponding sources, CI, and the test matrix.

### Branding assets

[`public/tagrove.svg`](../../public/tagrove.svg) is the Obsidian Grove master used by the header and HTML favicon. It traces the three tag shapes and central triangle from `docs/frontend/new_logo.png`, with an emerald gradient, transparent holes and no glow or background. The cropped square viewBox keeps the mark readable at small sizes. Run `bun run icons:generate` after editing it to regenerate the 16, 32, 64, 128, 256, and 512px RGBA PNGs under `src-tauri/icons/`. Tauri embeds the configured native icons, and packages retain those icons. The launcher source is [`src-tauri/linux/com.example.mediatagger.desktop`](../../src-tauri/linux/com.example.mediatagger.desktop). Linux is the supported packaging target; ICO and ICNS assets are not generated.

Local Inter and JetBrains Mono variable fonts, including SIL OFL licenses, are in `public/fonts/`. They are copied into the application bundle by Vite; rendering does not contact a font CDN. The source is the `ofl/inter` and `ofl/jetbrainsmono` directories in the Google Fonts repository.

## Install dependencies

Install frontend and tooling dependencies from the repository root:

```bash
bun install
```

`bun.lock` records the resolved JavaScript dependency graph and should remain committed. `bunfig.toml` sets `[install].exact = false`, so newly added dependencies are normally written as compatible ranges rather than exact versions. Most `package.json` dependencies use ranges. The Solid toolchain packages are exact pins; the lockfile supplies reproducible transitive resolutions. For a verification or CI install that must not update the lockfile, use:

```bash
bun install --frozen-lockfile
```

Cargo resolves the backend from the workspace manifest `src-tauri/Cargo.toml`, the core crate manifest `src-tauri/crates/media_tagger_core/Cargo.toml` and the one shared `src-tauri/Cargo.lock`. Both packages are default workspace members, so commands on `src-tauri/Cargo.toml` build and test both; add `-p media_tagger` (adapter) or `-p media_tagger_core` (core) to narrow one. The core builds without the GUI stack: `cargo build --manifest-path src-tauri/Cargo.toml --locked -p media_tagger_core`. Tauri and Cargo commands fetch missing Rust crates automatically; an optional explicit fetch is:

```bash
cargo fetch --manifest-path src-tauri/Cargo.toml --locked
```

Do not replace either lockfile as an incidental setup step. Update it only as part of an intentional dependency change.

## Package scripts

Build and development entry points are listed below. [Testing and quality commands](testing.md#package-scripts) own validation, audits, and test runners; [localization](localization.md#locale-generator) owns `locale:generate`.

| Command | What it does |
| --- | --- |
| `bun run dev` | Starts only the Vite development server. It does not compile or launch the Rust/Tauri application. |
| `bun run build` | Checks application/tests and Vite/Vitest TypeScript contexts, then builds Solid. |
| `bun run icons:generate` | Rasterizes the Obsidian Grove SVG master into the checked-in native PNG icon sizes using the local Tauri CLI. |
| `bun run preview` | Serves an existing Vite production build for browser inspection. It does not build first and does not launch Tauri. |
| `bun run tauri:dev` | Runs `tauri dev --config src-tauri/tauri.conf.dev.json`. This is the canonical desktop development command and selects the isolated development identifier and title. Tauri starts `bun run dev` through `beforeDevCommand`. |
| `bun run tauri:build:release` | Runs `tauri build` for the supported Linux target. The Docker release command below is preferred for artifact production. |
| `bun run tauri:build:linux` | Creates the unbundled native Linux release executable. This is the inner container command; use `./scripts/build-linux-docker.sh` from the host. |
| `bun run tauri:build:e2e` | Runs `node ./e2e/build-e2e.js`, validates the E2E identifier/title/target, and creates an unbundled debug executable in `src-tauri/target-e2e/debug/`. It first builds the frontend into `dist/`. |
| `bun run build:linux:docker` | Convenience wrapper for `scripts/build-linux-docker.sh`; requires Bun on the host, unlike invoking the shell script directly. |
| `bun run tauri` | Exposes the local Tauri CLI directly for explicit subcommands. It does not select the safe development overlay on its own. |
| `bun run gpui:dev` | Runs `cargo run --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`: the GPUI adapter on its development profile. It needs no Vite server. See [GPUI adapter](#gpui-adapter). |
| `bun run gpui:build:e2e` | Runs `node ./scripts/gpui-build-e2e.mjs`: checks the profile list and the target path, builds the GPUI adapter for the E2E identifier into `src-gpui/target-e2e/debug/`, and verifies the identifier the binary reports. |
| `bun run gpui:check` | `cargo check` of every target of the GPUI workspace, with `--locked`. |
| `bun run gpui:control` | Runs `node ./e2e/compare/gpui-control.js`: builds the GPUI E2E binary and keeps it open on a private Xvfb display with scratch XDG directories. See [comparison harness](testing.md#comparison-harness-tauri-and-gpui). |
| `bun run compare:ui` | Runs `node ./e2e/compare/compare.js <scenario...>`: screenshots of the same named state in the Tauri and the GPUI application under `artifacts/gpui-compare/`. It builds both E2E binaries. |

## Development modes

### Vite-only browser development

Use `bun run dev` when work only needs the frontend development server. `vite.config.ts` fixes the server to port `1420`, sets `strictPort: true`, and disables screen clearing. If port 1420 is occupied, startup fails instead of silently choosing a different port. Tailwind and the Solid plugin are enabled.

Vite-only mode is not a complete application runtime. Code paths that invoke Tauri commands, use Tauri dialogs, or load files through the Tauri asset protocol need a desktop process and will not behave like the packaged app in an ordinary browser.

### Solid frontend

Solid 2 is the only frontend. `index.html` loads `src/app/main.tsx`; Vite uses the
Solid compiler and Tailwind. Normal build, desktop and E2E commands share this entry.
There is no renderer-selection environment variable.

`bun run test:build` builds the application and checks the emitted module graph and
package manifest for the Solid entry and absence of React or legacy feature trees.
It writes the module list to ignored `artifacts/build/modules.json`.
Desktop runs remain serial because they share `dist/` and the E2E target.

The installed family is pinned in `package.json` and `bun.lock`:

| Package | Exact version |
| --- | --- |
| `solid-js`, `@solidjs/web`, `@solidjs/compiler` | `2.0.0-rc.13` |
| `@solidjs/vite-plugin` | `3.0.0-next.47` |
| `@solidjs/testing-library` | `1.0.0-beta.3` |

These are repository pins, not a claim about the latest public releases. Vite and Vitest
both use `@solidjs/vite-plugin`; JSX stays preserved for the compiler with
`jsxImportSource: @solidjs/web`. Vitest uses browser/development export conditions and
inlines the Solid dependencies. No separate renderer projects or compiler contexts remain.

Virtualization has no third-party dependency
([ADR 0003](../adr/0003-custom-gallery-virtualizer.md)). The gallery uses
`features/gallery/createVirtualGrid.ts` over the pure geometry in `gridGeometry.ts`; bulk order
and duplicate dialogs use `ui/dom/createVirtualList.ts`. Upgrading the Solid RC family requires
its own compatibility and behavior checks, including the row-pool order of `For`.

### Canonical Tauri development

Use:

```bash
bun run tauri:dev
```

The base Tauri configuration expects the development URL `http://localhost:1420`; the strict Vite port ensures that URL cannot drift. The development overlay changes the product/window title to `Tagrove Dev` and the identifier to `com.example.mediatagger.dev`.

Use generated or copied synthetic media in an isolated profile for reproducible large-library checks. Keep destructive tests in temporary roots.

Do not use an unqualified debug Tauri launch with the base configuration. In debug builds, `src-tauri/src/lib.rs` permits startup only with `com.example.mediatagger.dev` or `com.example.mediatagger.e2e`. This protects both the local release profile and future publisher identities. Use the established dev or E2E overlay; arbitrary custom identifiers are rejected.

### GPUI adapter

The GPUI port ([ADR 0036](../adr/0036-gpui-second-adapter-in-its-own-workspace.md)) is a second Cargo workspace, `src-gpui/`. Every Cargo command for it names the manifest and uses the committed lock:

```bash
bun run gpui:dev                  # window "Tagrove GPUI Dev", profile com.example.mediatagger.gpui.dev
cargo check --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client
cargo tree --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client -e normal   # no GUI crate
```

The first build fetches the pinned Zed commit and the repositories it pins into Cargo's Git cache and compiles about 450 crates. The build needs the development files of `libxkbcommon` (with its X11 part), `libxcb`, Wayland and Fontconfig/FreeType; at run time GPUI draws through Vulkan and falls back to EGL, so a Mesa or vendor driver for either is enough. The repository toolchain from `rust-toolchain.toml` builds it; no other toolchain is installed for the port.

The identifier is part of the binary. `src-gpui/crates/media_tagger_gpui/build.rs` reads `MEDIATAGGER_GPUI_IDENTIFIER` at build time (default `com.example.mediatagger.gpui.dev`), and the binary refuses to start unless that value is one of the two GPUI profiles in the [profile table](../architecture/system-overview.md#profiles-identifiers-and-data-isolation). `bun run gpui:build:e2e` is the only supported way to build the E2E profile: it sets the variable and a separate target directory, so the development binary is never replaced. `media_tagger_gpui --identifier` prints what a binary was built for without starting it. `media_tagger_gpui --scenario <id>` starts the E2E build in a named state of the [comparison harness](testing.md#comparison-harness-tauri-and-gpui); the development build refuses it, and every argument other than these two is an error.

The GPUI and Tauri applications use different identifiers, data directories and instance locks, so `bun run gpui:dev` and `bun run tauri:dev` can run at the same time. Before it opens the profile, the GPUI binary checks that the display server `WAYLAND_DISPLAY` or `DISPLAY` names answers; without one it exits with an error instead of starting headless, and when only the X display answers it starts on X11.

## Frontend build contract

`bun run build` runs `bun run typecheck && vite build`:

1. Two no-emit `tsc` passes read `tsconfig.json` and `tsconfig.node.json`. They check Solid application/tests and Vite/Vitest configuration. The application configuration is strict and rejects unused locals, unused parameters, and switch fallthrough.
2. `vite build` bundles the frontend into `dist/`, Vite's default output directory. The base Tauri configuration points `frontendDist` at `../dist` relative to `src-tauri/`.

`bun run typecheck` exposes the TypeScript pass separately. Vite development transpiles modules but is not a substitute for this gate.

## Desktop builds and profiles

The [architecture profile table](../architecture/system-overview.md#profiles-identifiers-and-data-isolation) owns identifiers, names, and app-data boundaries. Use `bun run tauri:dev` for development; a debug launch with the production identifier is rejected.

Both standalone `bun run tauri:build:e2e` and `bun run test:e2e:tauri` use the shared helper to build fresh frontend assets before the native executable. The overlay disables the redundant Tauri build hook. The [desktop test guide](testing.md#real-desktop-e2e) owns target/title assertions, cleanup guards, and driver setup.

## Configuration ownership

| File | Owns |
| --- | --- |
| `package.json` | Node compatibility, JavaScript dependency ranges/exact pins, and command entry points. |
| `bunfig.toml` / `bun.lock` | Bun install policy and exact resolved JavaScript dependency graph. |
| `tsconfig.json` | Application/test sources, strictness, preserved JSX and the `@solidjs/web` JSX context. |
| `tsconfig.node.json` | TypeScript project settings for `vite.config.ts` and `vitest.config.ts`. |
| `.nvmrc` / `package.json` / `rust-toolchain.toml` | Exact development/release Node, Bun, and Rust versions plus the Node compatibility range. |
| `biome.json` | JavaScript/TypeScript lint rules and formatting policy for configuration and quality scripts. |
| `.github/workflows/privacy.yml` | Full-history privacy scan and privacy regression tests on pull requests and pushes to `main`. |
| `vite.config.ts` | Solid compilation, Tailwind, and the fixed, strict development port. Vite defaults own `dist/` and the preview port because they are not overridden. |
| `src-tauri/tauri.conf.json` | Shared product metadata, frontend hooks/locations, release window/security settings, and icons. |
| `src-tauri/tauri.linux.conf.json` | Disables bundling for ordinary native Linux builds. |
| `src-tauri/tauri.appimage.conf.json` / `tauri.flatpak.conf.json` | Package-specific bundling, resources and installation configuration; the Flatpak generator supplies the configured release identity. |
| `src-tauri/tauri.conf.dev.json` | Development identifier, product name, window title, and development window overlay. |
| `src-tauri/tauri.conf.e2e.json` | E2E identifier/title and suppression of Tauri's automatic frontend build. |
| `src-tauri/capabilities/default.json` | Permissions granted to the `main` window. |
| `src-tauri/Cargo.toml` / `Cargo.lock` | Workspace members and shared `[workspace.dependencies]`; the adapter crate/binary `media_tagger` identity, edition, features and dependency ranges; exact Rust dependency resolutions for both packages. |
| `src-tauri/crates/media_tagger_core/Cargo.toml` | The core crate `media_tagger_core`: GUI-free dependencies, `publish = false`, and the `test-support` feature that only the adapter's dev-dependencies enable. |
| `src-tauri/build.rs` | Calls `tauri_build::build()`; generated context, application metadata, and resources come from the effective Tauri configuration. |
| `e2e/build-e2e.js` | E2E build safety assertions, isolated Cargo target directory, debug/no-bundle flags, and expected executable path. |
| `src-gpui/Cargo.toml` / `Cargo.lock` | The GPUI workspace: members, shared dependency declarations, the pinned Zed `rev` of `gpui` and `gpui_platform`, the path dependency on the core, and the exact resolutions of the GPUI tree. Independent of `src-tauri/Cargo.lock` except for the core's dependencies, which `gpui:lock-parity` keeps identical. |
| `src-gpui/crates/media_tagger_client/Cargo.toml` | The client crate: no GUI dependency (`architecture_boundaries`). |
| `src-gpui/crates/media_tagger_gpui/Cargo.toml` / `build.rs` | The GPUI adapter crate and binary `media_tagger_gpui`; `build.rs` bakes `MEDIATAGGER_GPUI_IDENTIFIER` into it. |
| `scripts/gpui-build-e2e.mjs` | GPUI E2E build safety assertions, the isolated target directory `src-gpui/target-e2e`, and the identifier check of the finished binary. |
| `e2e/compare/scenarios/*.js` | One comparison scenario per file: theme, seed and the `app:control` steps of the Tauri side. The GPUI start state of the same id is in `src-gpui/crates/media_tagger_client/src/scenario.rs`. |
| `scripts/gpui-lock-parity.py` | The two lock-file guards: core parity between `src-tauri/Cargo.lock` and `src-gpui/Cargo.lock`, and the allowed package sources of the GPUI lock (`PINNED_BY_ZED`). |

## Native media dependencies

Native releases link host libmpv/GTK and load EGL/GLX entry points dynamically. AppImage supplies its media and GTK libraries; Flatpak supplies them through `/app` and its runtime. Docker builds check those symbols and verify libmpv/GTK linkage inside the package environment. The host export script validates artifact shape, manifest fields, and checksums; it does not rerun `ldd` on the host. See [thumbnail tool discovery](../subsystems/thumbnails.md#ffmpeg-and-ffprobe-discovery) for ffmpeg/ffprobe precedence and timeouts.

## Performance diagnostics

Frontend and backend performance logging use different environment variables and activation rules.

Enable frontend User Timing instrumentation at Vite build/dev-server time with:

```bash
VITE_MEDIATAGGER_PERF=1 \
bun run tauri:dev
```

The frontend enables it only when the value is exactly `1`. It emits browser-console `[perf]` entries, including measured asset-query refreshes.

Enable backend timing output with:

```bash
MEDIATAGGER_PERF=1 \
bun run tauri:dev
```

The Rust backend checks only whether `MEDIATAGGER_PERF` exists, so even an empty value or `0` enables stderr timings for scans, asset queries, and thumbnail pages. Native playback errors are written separately by the player/surface code. Remove the variables to disable diagnostics:

```bash
unset MEDIATAGGER_PERF VITE_MEDIATAGGER_PERF
```

## Dependency updates

Keep dependency families aligned rather than updating one manifest in isolation:

- Tauri changes may span `@tauri-apps/cli`, `@tauri-apps/api`, `@tauri-apps/plugin-dialog`, Rust `tauri`, `tauri-build`, and `tauri-plugin-dialog`. Check their compatibility together and regenerate both lockfiles as applicable.
- Keep the pinned Solid runtime, web renderer, compiler, Vite plugin and testing library compatible. RC upgrades require explicit verification.
- Tailwind CSS and `@tailwindcss/vite` should be updated together; verify the DaisyUI integration after either changes.
- A changed Rust target or native-media dependency requires an explicit system-package policy and release-link verification.

Run `bun install` for intentional JavaScript updates and an appropriate Cargo update for intentional Rust updates, then inspect `bun.lock` and `src-tauri/Cargo.lock`. Most manifest entries are semver ranges, so frozen/locked resolution and the pinned toolchains are both required. Neither lockfile pins rolling Linux system packages.

There are two Cargo lock files, and `media_tagger_core` is compiled from both. After changing a dependency of the core, bring `src-gpui/Cargo.lock` to the same versions (`cargo update --manifest-path src-gpui/Cargo.toml -p <name> --precise <version>`) and run `bun run gpui:lock-parity`. Nothing else in `src-gpui/Cargo.lock` has to match the Tauri lock, and `src-tauri/Cargo.lock` never takes a package because the GPUI tree wants it.

### Updating the pinned Zed commit

GPUI has no semver and comes from a Git repository, so a bump is a reviewed change ([ADR 0036](../adr/0036-gpui-second-adapter-in-its-own-workspace.md)):

1. Choose the commit of a stable Zed release tag (`git ls-remote --tags https://github.com/zed-industries/zed`; tags ending in `-pre` are previews) and set the same full SHA as `rev` of every Zed dependency in `src-gpui/Cargo.toml`.
2. Resolve the lock starting from the lock file of that Zed commit, so the GPUI tree uses the versions Zed tested, and then align the core's dependencies with `src-tauri/Cargo.lock`. Keep deviations from Zed's lock to those the core closure, a RustSec advisory or a yanked release requires, and list them in the change.
3. Read the Git dependencies of that commit (`Cargo.toml` of the Zed workspace and of `crates/gpui_linux`) and update `PINNED_BY_ZED` in `scripts/gpui-lock-parity.py`. Repeat an entry of Zed's `[patch.crates-io]` only when the build or a test needs it, and say why.
4. List the packages that are in neither lock yet with their licences (`cargo metadata --manifest-path src-gpui/Cargo.toml --locked --format-version 1`), and run `bun run audit:gpui`. A new audit exception needs the owner's approval.
5. Fetch with the network (`cargo fetch --manifest-path src-gpui/Cargo.toml --locked`), then make the first build of the new dependencies offline in a sandbox that sees only the repository (read-only, with `src-gpui/target` writable), the Cargo home and the toolchain: `bwrap --unshare-all` with `cargo build --manifest-path src-gpui/Cargo.toml --locked --offline`. Build scripts and procedural macros of new crates run there first.
6. Run `bun run gpui:lock-parity`, `bun run gpui:clippy`, `bun run test:client`, `bun run test:gpui`, and check that the build still works on the toolchain of `rust-toolchain.toml`.

## Known limitations

- only Linux x86-64 packaging is configured;
- the rolling `archlinux:base-devel` image and system packages are not digest/version pinned, so a later clean container build can use newer native dependencies;
- compatible-range manifests mean lockfiles must be preserved for repeatable resolution;
- the debug guard permits only the established dev/E2E identifiers; custom debug profiles require an explicit isolation review and allowlist change;
- the GPUI adapter builds only as a development or E2E binary; it is not packaged and has no release profile;
- Vite-only development cannot validate native IPC, dialogs, asset-protocol loading, app-data paths, or packaged resource layout;
- unbundled E2E builds do not prove the release artifact's exact shared-library environment.

## Build checklist

For a clean local desktop development setup:

1. Confirm Node satisfies `package.json` and that `bun`, `rustc`, and `cargo` are on `PATH`.
2. Install the documented Linux GTK, WebKitGTK, libmpv, OpenGL, ffmpeg, and ffprobe prerequisites.
3. Run `bun install --frozen-lockfile` for an unchanged checkout, or `bun install` when intentionally updating dependencies.
4. Run `bun run build` once to validate TypeScript and produce `dist/`.
5. Run `bun run tauri:dev` for normal desktop development.

For an Arch Linux x86-64 release without host build dependencies:

1. Confirm `docker version` can reach the daemon as the current user.
2. Run `./scripts/build-linux-docker.sh --format native`.
3. Inspect `artifacts/linux/native/`, then run `(cd artifacts/linux/native && sha256sum --check SHA256SUMS)`.
4. Install or launch the artifact manually when needed. The release script does not do either.

## Troubleshooting

**Vite reports that port 1420 is already in use.** Stop the process using the port. `strictPort` intentionally prevents changing ports because Tauri's `devUrl` is fixed to `http://localhost:1420`.

**A debug desktop process refuses its identifier.** Launch with `bun run tauri:dev`, or pass the E2E overlay only for the isolated E2E workflow. An unqualified `tauri dev` inherits the unsafe production identifier from the base configuration.

**The app appears to have an empty library.** Check the selected identifier/profile before changing files. Release, dev, and E2E intentionally use different app-data directories.

**The GPUI binary refuses its identifier or reports no display server.** `media_tagger_gpui --identifier` shows what the binary was built for; only the two GPUI profiles start, so rebuild with `bun run gpui:dev` or `bun run gpui:build:e2e` rather than setting `MEDIATAGGER_GPUI_IDENTIFIER` by hand. The binary needs `WAYLAND_DISPLAY` or `DISPLAY`, and a server that answers there: "does not answer" means the variable is left over from a session, a compositor or an Xvfb that no longer runs (a relative `WAYLAND_DISPLAY` also needs `XDG_RUNTIME_DIR`).

**`cargo` reports that `src-gpui/Cargo.lock` needs to be updated.** A manifest of the GPUI workspace or of the core changed without its lock. Update the lock intentionally (see [dependency updates](#dependency-updates)) instead of dropping `--locked`.

**`tauri:build:e2e` fails before compiling Rust.** Check the frontend build output. The build helper produces fresh frontend assets before invoking Tauri; the overlay disables the redundant Tauri `beforeBuildCommand`.

**Video probing or thumbnails fail while images still work.** Confirm `ffmpeg` and `ffprobe` are on `PATH`. Startup resolves ffmpeg once (`resolve_ffmpeg_path` in `src-tauri/crates/media_tagger_core/src/app/bootstrap.rs`): an executable copy in the resource directory or beside the executable wins over `/usr/bin/ffmpeg` and `PATH`; see [tool discovery](../subsystems/thumbnails.md#ffmpeg-and-ffprobe-discovery). Application startup succeeding does not prove that both tools were found.

**Linux video does not play.** Install `webkit2gtk-4.1`, `gtk3`, `mpv`, and `libglvnd`; verify `ldd /path/to/extracted/media_tagger` has no missing dependencies and includes libmpv/GTK. Check that `libEGL.so.1` exports `eglGetProcAddress` or `libGL.so.1` exports `glXGetProcAddressARB`, then inspect player/surface stderr errors. For portable packages, inspect the packaged tools and runtime rather than the host libraries.

**The Docker command cannot connect to `/var/run/docker.sock`.** Confirm the current login session has access to the Docker daemon. The build script deliberately does not elevate through `sudo`.

**A dependency resolves differently on another machine.** Confirm both machines use the committed `bun.lock`, `src-tauri/Cargo.lock` and `src-gpui/Cargo.lock`, use frozen/locked install modes, and match `.nvmrc`, `packageManager`, and `rust-toolchain.toml`. Compare external system package versions when the pinned language toolchains match.
