# Local setup and builds

Implementation entry points: [package scripts](../../package.json), [Rust manifest](../../src-tauri/Cargo.toml), [Linux build](../../Dockerfile.linux), [artifact export](../../scripts/build-linux-docker.sh).

This page describes the setup and build behavior implemented by the current repository. Run the commands from the repository root. Configuration and lockfiles are authoritative; this page does not imply support for platforms or tool versions that the repository does not declare.

## Prerequisites

Linux packages target x86_64. The Docker workflow builds Flatpak against GNOME 50 and AppImage on Debian 12. The Arch-native executable remains an explicit build option. The host needs Docker, Bash, GNU file/coreutils/findutils/diffutils, `flock`, Python 3.12 or later, and Gitleaks 8.30.1. Bun, Node, Rust, and media libraries are installed inside Docker. Install the checksum-pinned scanner with `bash scripts/install-gitleaks.sh /path/to/tools` and add that directory to `PATH`. Package privacy inspection also needs binutils, Flatpak and OSTree.

Toolchains remain Bun `1.4.0`, Node `22.22.0`, and Rust `1.98.0`. Portable builds verify the checksums in [toolchains.json](../../packaging/linux/toolchains.json). Keep `.nvmrc`, `packageManager`, `rust-toolchain.toml`, `Cargo.toml`, and packaging pins aligned when updating a toolchain. Lockfiles change only when dependency resolution changes.

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

[`public/tagrove.svg`](../../public/tagrove.svg) is the Woven T master used by the header and HTML favicon. It recreates the selected proposal with flat `#2D5A2D` and `#7CB87C` fills and transparent gaps. Run `bun run icons:generate` after editing it to regenerate the 16, 32, 64, 128, 256, and 512px RGBA PNGs under `src-tauri/icons/`. Tauri embeds the configured native icons, and packages retain those icons. The launcher source is [`src-tauri/linux/com.example.mediatagger.desktop`](../../src-tauri/linux/com.example.mediatagger.desktop). Linux is the supported packaging target; ICO and ICNS assets are not generated.

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

The local development profile already has a collection of approximately 30,000 images. Use it for manual testing with `bun run tauri:dev` when a large library is useful. Continue to use temporary media fixtures for destructive tests.

Do not use an unqualified debug Tauri launch with the base configuration. In debug builds, `src-tauri/src/lib.rs` permits startup only with `com.example.mediatagger.dev` or `com.example.mediatagger.e2e`. This protects both the local release profile and future publisher identities. Use the established dev or E2E overlay; arbitrary custom identifiers are rejected.

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
| `.nvmrc` / `package.json` / `rust-toolchain.toml` | Exact development/release Node, Bun, and Rust versions plus the Node compatibility range. |
| `biome.json` | JavaScript/TypeScript lint rules and formatting policy for configuration and quality scripts. |
| `.github/workflows/privacy.yml` | Full-history privacy scan and privacy regression tests on pull requests and pushes to `main`. |
| `vite.config.ts` | React/Tailwind integration and the fixed, strict development port. Vite defaults own `dist/` and the preview port because they are not overridden. |
| `src-tauri/tauri.conf.json` | Shared product metadata, frontend hooks/locations, release window/security settings, and icons. |
| `src-tauri/tauri.linux.conf.json` | Disables bundling for ordinary native Linux builds. |
| `src-tauri/tauri.appimage.conf.json` / `tauri.flatpak.conf.json` | Package-specific bundling, resources and installation configuration; the Flatpak generator supplies the configured release identity. |
| `src-tauri/tauri.conf.dev.json` | Development identifier, product name, window title, and development window overlay. |
| `src-tauri/tauri.conf.e2e.json` | E2E identifier/title and suppression of Tauri's automatic frontend build. |
| `src-tauri/capabilities/default.json` | Permissions granted to the `main` window. |
| `src-tauri/Cargo.toml` / `Cargo.lock` | Rust crate/binary identity, edition, features, dependency ranges, and exact Rust dependency resolutions. |
| `src-tauri/build.rs` | Calls `tauri_build::build()`; generated context, application metadata, and resources come from the effective Tauri configuration. |
| `e2e/build-e2e.js` | E2E build safety assertions, isolated Cargo target directory, debug/no-bundle flags, and expected executable path. |

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
- React and `react-dom` should remain mutually compatible, along with `@types/react` and `@types/react-dom`.
- Tailwind CSS and `@tailwindcss/vite` should be updated together; verify the DaisyUI integration after either changes.
- A changed Rust target or native-media dependency requires an explicit system-package policy and release-link verification.

Run `bun install` for intentional JavaScript updates and an appropriate Cargo update for intentional Rust updates, then inspect `bun.lock` and `src-tauri/Cargo.lock`. Most manifest entries are semver ranges, so frozen/locked resolution and the pinned toolchains are both required. Neither lockfile pins rolling Linux system packages.

## Known limitations

- only Linux x86-64 packaging is configured;
- the rolling `archlinux:base-devel` image and system packages are not digest/version pinned, so a later clean container build can use newer native dependencies;
- compatible-range manifests mean lockfiles must be preserved for repeatable resolution;
- the debug guard permits only the established dev/E2E identifiers; custom debug profiles require an explicit isolation review and allowlist change;
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
2. Run `./scripts/build-linux-docker.sh --format native`.
3. Inspect `artifacts/linux/native/`, then run `(cd artifacts/linux/native && sha256sum --check SHA256SUMS)`.
4. Install or launch the artifact manually when needed. The release script does not do either.

## Troubleshooting

**Vite reports that port 1420 is already in use.** Stop the process using the port. `strictPort` intentionally prevents changing ports because Tauri's `devUrl` is fixed to `http://localhost:1420`.

**A debug desktop process refuses its identifier.** Launch with `bun run tauri:dev`, or pass the E2E overlay only for the isolated E2E workflow. An unqualified `tauri dev` inherits the unsafe production identifier from the base configuration.

**The app appears to have an empty library.** Check the selected identifier/profile before changing files. Release, dev, and E2E intentionally use different app-data directories.

**`tauri:build:e2e` has missing or stale UI assets.** Run `bun run build` first. The E2E overlay deliberately disables `beforeBuildCommand`; only the full desktop E2E test orchestration builds the frontend automatically.

**Video probing or thumbnails fail while images still work.** Confirm `ffmpeg` and `ffprobe` are on `PATH`. Application startup succeeding does not prove that both tools were found.

**Linux video does not play.** Install `webkit2gtk-4.1`, `gtk3`, `mpv`, and `libglvnd`; verify `ldd /path/to/extracted/media_tagger` has no missing dependencies and includes libmpv/GTK. Check that `libEGL.so.1` exports `eglGetProcAddress` or `libGL.so.1` exports `glXGetProcAddressARB`, then inspect player/surface stderr errors. For portable packages, inspect the packaged tools and runtime rather than the host libraries.

**The Docker command cannot connect to `/var/run/docker.sock`.** Confirm the current login session has access to the Docker daemon. The build script deliberately does not elevate through `sudo`.

**A dependency resolves differently on another machine.** Confirm both machines use the committed `bun.lock` and `src-tauri/Cargo.lock`, use frozen/locked install modes, and match `.nvmrc`, `packageManager`, and `rust-toolchain.toml`. Compare external system package versions when the pinned language toolchains match.
