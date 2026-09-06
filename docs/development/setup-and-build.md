# Local setup and builds

Implementation entry points: [package scripts](../../package.json), [Rust manifest](../../src-tauri/Cargo.toml), [Linux build](../../Dockerfile.linux), [artifact export](../../scripts/build-linux-docker.sh).

This page describes the setup and build behavior implemented by the current repository. Run the commands from the repository root. Configuration and lockfiles are authoritative; this page does not imply support for platforms or tool versions that the repository does not declare.

## Prerequisites

The packaged application supports Arch/CachyOS x86-64 as an unbundled native executable. The Docker workflow forces `linux/amd64` and builds the release executable without running test suites.

The supported Linux release workflow requires a working Docker daemon plus Bash and the standard GNU utilities used by `scripts/build-linux-docker.sh` on the host. `Dockerfile.linux` installs the Arch build dependencies, Bun, Node.js, Rust/Cargo, WebKitGTK, and media runtime packages inside the image. Those language toolchains do not need to be installed on the host.

The repository-level runtime requirements are:

| Tool | Declared requirement | Notes |
| --- | --- | --- |
| Bun | `1.4.0` | Pinned by `packageManager` in `package.json`, CI, and the Linux image. Bun installs dependencies and runs package scripts. |
| Node.js | `22.22.0` | Pinned by `.nvmrc`, CI, and the Linux image. The broader `engines` range is a compatibility declaration, not the release/CI version. |
| Rust and Cargo | `1.98.0` | Pinned with Clippy and rustfmt in `rust-toolchain.toml`, used by CI and the Linux image, and recorded as the minimum in `Cargo.toml`. |

Linux builds use the system media, GTK, WebKit, and graphics stack. Install the runtime dependencies with `sudo pacman -S --needed webkit2gtk-4.1 gtk3 mpv libglvnd ffmpeg`. libmpv performs interactive playback; ffmpeg and ffprobe handle duration probing, E2E fixtures, and thumbnails. Building on rolling Arch targets current Arch/CachyOS systems and does not guarantee compatibility with older distributions or older glibc ABIs.

### Toolchain update policy

CI and release builds use the exact versions above. Update Node, Bun, and Rust in one reviewed dependency change: change `.nvmrc`, `package.json`, `rust-toolchain.toml`, `Cargo.toml`, `Dockerfile.linux`, and `.github/workflows/quality.yml` as applicable; regenerate lockfiles only when resolution changes; then run frozen Bun installation, all quality gates, `test:all`, and the Linux release build. Security fixes may trigger an immediate update. Otherwise review toolchains at least quarterly. The Arch base image and system packages remain rolling, so the build manifest records their resolved versions for each Linux artifact.

### Containerized Arch Linux release

Run the build directly so the host does not need Bun or Node.js:

```bash
./scripts/build-linux-docker.sh
```

The script builds the native release executable without running Vitest, Rust tests, or desktop E2E. It copies only `/out/.` into a temporary host directory, validates exactly one x86-64 ELF binary, one 512x512 PNG icon, one desktop launcher with matching name/icon/executable metadata, and the build manifest, rejects missing shared libraries reported by `ldd`, verifies generated checksums, and only then replaces `artifacts/linux/`. The output contains `media_tagger`, `tagrove.png`, `com.example.mediatagger.desktop`, `SHA256SUMS`, and `build-manifest.txt` with toolchain and package versions.

If Bun is already available on the host, `bun run build:linux:docker` is an equivalent convenience command. Both forms use the ordinary `docker build`/`docker create`/`docker cp` interface and do not require Buildx.

### CachyOS release artifact

Build the artifact on CachyOS/Arch x86-64:

```bash
./scripts/release-linux-user.sh
```

`bun run release:linux:user` is an equivalent wrapper. The script only invokes `scripts/build-linux-docker.sh`. It does not run tests, install files, launch the application, inspect or copy the production database, or modify anything under `~/.local/` or `/usr/local/`.

The resulting files are written to `artifacts/linux/`: `media_tagger`, `tagrove.png`, `com.example.mediatagger.desktop`, `build-manifest.txt`, and `SHA256SUMS`. Installation is separate from building; the local reinstall helper is described below.

### Local installation

[`scripts/reinstall-linux-user.sh`](../../scripts/reinstall-linux-user.sh) reads `artifacts/linux/` relative to its checkout. It refuses replacement while `pgrep -x media_tagger` finds a process and verifies artifact checksums before writing installed files. It installs the executable through a temporary sibling followed by rename, then copies the Woven T icon and Tagrove launcher. It refreshes desktop/icon caches when the corresponding tools and icon-theme index are available.

The default prefix is `$HOME/.local`. The executable goes in `bin/media_tagger`, the icon in `share/icons/hicolor/512x512/apps/tagrove.png`, and the launcher in `share/applications/com.example.mediatagger.desktop`. Its `StartupWMClass=media_tagger` matches the native X11 window class. Pass a different prefix as the first argument for a custom or temporary installation. Its `bin` directory must be on `PATH` for the launcher's `Exec=media_tagger` to resolve. The script creates missing directories, replaces existing files without backups, and does not rebuild or launch the app. Application data remains under the existing Tauri identifier.

`bun run test:linux-install` exercises installation and replacement under temporary prefixes, verifies the icon and launcher metadata, and checks checksum failure and running-process refusal.

### Branding assets

[`public/tagrove.svg`](../../public/tagrove.svg) is the Woven T master used by the header and HTML favicon. It recreates the selected proposal with flat `#2D5A2D` and `#7CB87C` fills and transparent gaps. Run `bun run icons:generate` after editing it to regenerate the 16, 32, 64, 128, 256, and 512px RGBA PNGs under `src-tauri/icons/`. Tauri embeds the configured native icons, and Docker exports the 512px icon as `tagrove.png`. The launcher source is [`src-tauri/linux/com.example.mediatagger.desktop`](../../src-tauri/linux/com.example.mediatagger.desktop). Linux is the supported packaging target; ICO and ICNS assets are not generated.

## Install dependencies

Install frontend and tooling dependencies from the repository root:

```bash
bun install
```

`bun.lock` records the resolved JavaScript dependency graph and should remain committed. `bunfig.toml` sets `[install].exact = false`, so newly added dependencies are normally written as compatible ranges rather than exact versions. Existing `package.json` dependencies likewise use ranges, while the lockfile supplies reproducible resolutions. For a verification or CI install that must not update the lockfile, use:

```bash
bun install --frozen-lockfile
```

Cargo resolves the backend from `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`. Tauri and Cargo commands fetch missing Rust crates automatically; an optional explicit fetch is:

```bash
cargo fetch --manifest-path src-tauri/Cargo.toml --locked
```

Do not replace either lockfile as an incidental setup step. Update it only as part of an intentional dependency change.

## Package scripts

Build and development entry points are listed below. [Testing and quality commands](testing.md#package-scripts) own validation, audits, and test runners; [localization](localization.md#locale-generator) owns `locale:generate`.

| Command | What it does |
| --- | --- |
| `bun run dev` | Starts only the Vite development server. It does not compile or launch the Rust/Tauri application. |
| `bun run build` | Runs both no-emit TypeScript checks, including Vite/Vitest configuration, then `vite build`. |
| `bun run icons:generate` | Rasterizes the Woven T SVG master into the checked-in native PNG icon sizes using the local Tauri CLI. |
| `bun run preview` | Serves an existing Vite production build for browser inspection. It does not build first and does not launch Tauri. |
| `bun run tauri:dev` | Runs `tauri dev --config src-tauri/tauri.conf.dev.json`. This is the canonical desktop development command and selects the isolated development identifier and title. Tauri starts `bun run dev` through `beforeDevCommand`. |
| `bun run tauri:build:release` | Runs `tauri build` for the supported Linux target. The Docker release command below is preferred for artifact production. |
| `bun run tauri:build:linux` | Creates the unbundled native Linux release executable. This is the inner container command; use `./scripts/build-linux-docker.sh` from the host. |
| `bun run tauri:build:e2e` | Runs `node ./e2e/build-e2e.js`, validates the E2E identifier/title/target, and creates an unbundled debug executable in `src-tauri/target-e2e/debug/`. It does not build the frontend itself. |
| `bun run build:linux:docker` | Convenience wrapper for `scripts/build-linux-docker.sh`; requires Bun on the host, unlike invoking the shell script directly. |
| `bun run release:linux:user` | Convenience wrapper for the CachyOS/Arch Docker build. It only creates files under `artifacts/linux/`. |
| `bun run tauri` | Exposes the local Tauri CLI directly for explicit subcommands. It does not select the safe development overlay on its own. |

## Development modes

### Vite-only browser development

Use `bun run dev` when work only needs the frontend development server. `vite.config.ts` fixes the server to port `1420`, sets `strictPort: true`, and disables screen clearing. If port 1420 is occupied, startup fails instead of silently choosing a different port. The React and Tailwind Vite plugins are enabled.

Vite-only mode is not a complete application runtime. Code paths that invoke Tauri commands, use Tauri dialogs, or load files through the Tauri asset protocol need a desktop process and will not behave like the packaged app in an ordinary browser.

### Canonical Tauri development

Use:

```bash
bun run tauri:dev
```

The base Tauri configuration expects the development URL `http://localhost:1420`; the strict Vite port ensures that URL cannot drift. The development overlay changes the product/window title to `Tagrove Dev` and the identifier to `com.example.mediatagger.dev`.

Do not use an unqualified debug Tauri launch with the base configuration. In debug builds, `src-tauri/src/lib.rs` refuses startup when the effective identifier is the production identifier `com.example.mediatagger`. This protects production app data from a common configuration mistake. The guard requires the dev or E2E overlay, but it only rejects that one production identifier; it does not prove that every other identifier is safe.

## Frontend build contract

`bun run build` runs `bun run typecheck && vite build`:

1. Two no-emit `tsc` passes read `tsconfig.json` and `tsconfig.node.json`. They type-check `src/`, `vite.config.ts`, and `vitest.config.ts`. The application configuration is strict and rejects unused locals, unused parameters, and switch fallthrough.
2. `vite build` bundles the frontend into `dist/`, Vite's default output directory. The base Tauri configuration points `frontendDist` at `../dist` relative to `src-tauri/`.

`bun run typecheck` exposes the TypeScript pass separately. Vite development transpiles modules but is not a substitute for this gate.

## Desktop builds and profiles

The [architecture profile table](../architecture/system-overview.md#profiles-identifiers-and-data-isolation) owns identifiers, names, and app-data boundaries. Use `bun run tauri:dev` for development; a debug launch with the production identifier is rejected.

The standalone `bun run tauri:build:e2e` consumes existing `dist/` because its overlay disables the frontend build. Run `bun run build` first or use `bun run test:e2e:tauri`, which builds the frontend automatically. The [desktop test guide](testing.md#real-desktop-e2e) owns target/title assertions, cleanup guards, and driver setup.

## Configuration ownership

| File | Owns |
| --- | --- |
| `package.json` | Node compatibility, JavaScript dependency ranges, and command entry points. |
| `bunfig.toml` / `bun.lock` | Bun install policy and exact resolved JavaScript dependency graph. |
| `tsconfig.json` | Frontend language target, strictness, module resolution, included source, and no-emit type checking. |
| `tsconfig.node.json` | TypeScript project settings for `vite.config.ts` and `vitest.config.ts`. |
| `.nvmrc` / `package.json` / `rust-toolchain.toml` | Exact CI/release Node, Bun, and Rust versions plus the Node compatibility range. |
| `biome.json` | JavaScript/TypeScript lint rules and formatting policy for configuration and quality scripts. |
| `.github/workflows/quality.yml` | Frozen frontend/configuration checks and locked Rust checks under non-production profiles. |
| `vite.config.ts` | React/Tailwind integration and the fixed, strict development port. Vite defaults own `dist/` and the preview port because they are not overridden. |
| `src-tauri/tauri.conf.json` | Shared product metadata, frontend hooks/locations, release window/security settings, and icons. |
| `src-tauri/tauri.linux.conf.json` | Disables bundling on Linux; Linux intentionally declares no media-tool sidecars. |
| `src-tauri/tauri.conf.dev.json` | Development identifier, product name, window title, and development window overlay. |
| `src-tauri/tauri.conf.e2e.json` | E2E identifier/title and suppression of Tauri's automatic frontend build. |
| `src-tauri/capabilities/default.json` | Permissions granted to the `main` window. |
| `src-tauri/Cargo.toml` / `Cargo.lock` | Rust crate/binary identity, edition, features, dependency ranges, and exact Rust dependency resolutions. |
| `src-tauri/build.rs` | Calls `tauri_build::build()`; generated context, application metadata, and resources come from the effective Tauri configuration. |
| `e2e/build-e2e.js` | E2E build safety assertions, isolated Cargo target directory, debug/no-bundle flags, and expected executable path. |

## Native media dependencies

The release links system libmpv/GTK and loads EGL/GLX entry points dynamically. The Docker build checks those symbols and verifies libmpv/GTK linkage with no missing dependencies inside the container. The host export script validates artifact shape, manifest fields, and checksums; it does not rerun `ldd` on the host. See [thumbnail tool discovery](../subsystems/thumbnails.md#ffmpeg-and-ffprobe-discovery) for ffmpeg/ffprobe precedence and timeouts.

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
- React and `react-dom` should remain mutually compatible, along with `@types/react` and `@types/react-dom`.
- Tailwind CSS and `@tailwindcss/vite` should be updated together; verify the DaisyUI integration after either changes.
- A changed Rust target or native-media dependency requires an explicit system-package policy and release-link verification.

Run `bun install` for intentional JavaScript updates and an appropriate Cargo update for intentional Rust updates, then inspect `bun.lock` and `src-tauri/Cargo.lock`. Most manifest entries are semver ranges, so frozen/locked resolution and the pinned toolchains are both required. Neither lockfile pins rolling Linux system packages.

## Known limitations

- only Linux x86-64 packaging is configured;
- the rolling `archlinux:base-devel` image and system packages are not digest/version pinned, so a later clean container build can use newer native dependencies;
- compatible-range manifests mean lockfiles must be preserved for repeatable resolution;
- the debug guard rejects only the production identifier and cannot certify arbitrary custom overlays;
- a standalone E2E build can package stale or missing frontend assets because its automatic frontend build is disabled;
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
2. Run `./scripts/release-linux-user.sh`.
3. Confirm `artifacts/linux/` contains `media_tagger`, `tagrove.png`, `com.example.mediatagger.desktop`, `build-manifest.txt`, and `SHA256SUMS`, then run `(cd artifacts/linux && sha256sum --check SHA256SUMS)`.
4. Install or launch the artifact manually when needed. The release script does not do either.

## Troubleshooting

**Vite reports that port 1420 is already in use.** Stop the process using the port. `strictPort` intentionally prevents changing ports because Tauri's `devUrl` is fixed to `http://localhost:1420`.

**A debug desktop process says it refuses the production identifier.** Launch with `bun run tauri:dev`, or pass the E2E overlay only for the isolated E2E workflow. An unqualified `tauri dev` inherits the unsafe production identifier from the base configuration.

**The app appears to have an empty library.** Check the selected identifier/profile before changing files. Release, dev, and E2E intentionally use different app-data directories.

**`tauri:build:e2e` has missing or stale UI assets.** Run `bun run build` first. The E2E overlay deliberately disables `beforeBuildCommand`; only the full desktop E2E test orchestration builds the frontend automatically.

**Video probing or thumbnails fail while images still work.** Confirm `ffmpeg` and `ffprobe` are on `PATH`. Application startup succeeding does not prove that both tools were found.

**Linux video does not play.** Install `webkit2gtk-4.1`, `gtk3`, `mpv`, and `libglvnd`; verify `ldd artifacts/linux/media_tagger` has no missing dependencies and includes libmpv/GTK. Check that `libEGL.so.1` exports `eglGetProcAddress` or `libGL.so.1` exports `glXGetProcAddressARB`, then inspect player/surface stderr errors. The supported artifact is the native executable, not an AppImage.

**The Docker command cannot connect to `/var/run/docker.sock`.** Confirm the current login session has access to the Docker daemon. The build script deliberately does not elevate through `sudo`.

**A dependency resolves differently on another machine.** Confirm both machines use the committed `bun.lock` and `src-tauri/Cargo.lock`, use frozen/locked install modes, and match `.nvmrc`, `packageManager`, and `rust-toolchain.toml`. Compare external system package versions when the pinned language toolchains match.
