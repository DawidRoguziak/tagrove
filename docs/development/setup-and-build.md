# Local setup and builds

This page describes the setup and build behavior implemented by the current repository. Run the commands from the repository root. Configuration and lockfiles are authoritative; this page does not imply support for platforms or tool versions that the repository does not declare.

## Prerequisites

The packaged application supports Windows x86-64 and Arch/CachyOS x86-64. Platform-specific Tauri configs select MSI/NSIS with locally supplied Windows media-tool sidecars on Windows and an unbundled native executable on Linux. The Docker workflow forces `linux/amd64` and runs Linux desktop E2E with WebKitWebDriver.

The supported Linux release workflow requires a working Docker daemon plus Bash and the standard GNU utilities used by `scripts/build-linux-docker.sh` on the host. `Dockerfile.linux` installs the Arch build dependencies, Bun, Node.js, Rust/Cargo, WebKitGTK, and media runtime packages inside the image. It runs the frontend and backend tests before producing artifacts, so those language toolchains do not need to be installed on the host.

The repository-level runtime requirements are:

| Tool | Declared requirement | Notes |
| --- | --- | --- |
| Bun | `1.4.0` | Pinned by `packageManager` in `package.json`, CI, and the Linux image. Bun installs dependencies and runs package scripts. |
| Node.js | `22.22.0` | Pinned by `.nvmrc`, CI, and the Linux image. The broader `engines` range is a compatibility declaration, not the release/CI version. |
| Rust and Cargo | `1.98.0` | Pinned with Clippy and rustfmt in `rust-toolchain.toml`, used by CI and the Linux image, and recorded as the minimum in `Cargo.toml`. |

The repository does not include ffmpeg/ffprobe sidecar executables. A complete Windows package requires the pinned Windows x86-64 files described in `src-tauri/binaries/README.md`; their expected hashes are tracked in `src-tauri/binaries/SHA256SUMS`. Linux builds intentionally use the host media and WebKit stack. Install the runtime dependencies with `sudo pacman -S --needed webkit2gtk-4.1 ffmpeg gst-plugins-base-libs gst-plugins-good gst-plugins-bad gst-libav`. `gst-plugins-ugly` is not required by the tested formats and is intentionally omitted. Building on rolling Arch targets current Arch/CachyOS systems and does not guarantee compatibility with older distributions or older glibc ABIs.

### Toolchain update policy

CI and release builds use the exact versions above. Update Node, Bun, and Rust in one reviewed dependency change: change `.nvmrc`, `package.json`, `rust-toolchain.toml`, `Cargo.toml`, `Dockerfile.linux`, and `.github/workflows/quality.yml` as applicable; regenerate lockfiles only when resolution changes; then run frozen Bun installation, all quality gates, `test:all`, and the Linux release build. Security fixes may trigger an immediate update. Otherwise review toolchains at least quarterly. The Arch base image and system packages remain rolling, so the build manifest records their resolved versions for each Linux artifact.

### Containerized Arch Linux release

Run the build directly so the host does not need Bun or Node.js:

```bash
./scripts/build-linux-docker.sh
```

The script builds the `artifacts` stage after frontend build, Vitest, all Rust tests, Linux desktop E2E, and the native release build. It copies only `/out/.` into a temporary host directory, validates exactly one x86-64 ELF binary and its manifest, rejects missing shared libraries reported by `ldd`, verifies generated checksums, and only then replaces `artifacts/linux/`. The output contains `media_tagger`, `SHA256SUMS`, and `build-manifest.txt` with toolchain and package versions.

If Bun is already available on the host, `bun run build:linux:docker` is an equivalent convenience command. Both forms use the ordinary `docker build`/`docker create`/`docker cp` interface and do not require Buildx.

## Install dependencies

Install frontend and tooling dependencies from the repository root:

```powershell
bun install
```

`bun.lock` records the resolved JavaScript dependency graph and should remain committed. `bunfig.toml` sets `[install].exact = false`, so newly added dependencies are normally written as compatible ranges rather than exact versions. Existing `package.json` dependencies likewise use ranges, while the lockfile supplies reproducible resolutions. For a verification or CI install that must not update the lockfile, use:

```powershell
bun install --frozen-lockfile
```

Cargo resolves the backend from `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`. Tauri and Cargo commands fetch missing Rust crates automatically; an optional explicit fetch is:

```powershell
cargo fetch --manifest-path src-tauri/Cargo.toml --locked
```

Do not replace either lockfile as an incidental setup step. Update it only as part of an intentional dependency change.

## Package scripts

The following are all current non-test scripts in `package.json`:

| Command | What it does |
| --- | --- |
| `bun run dev` | Starts only the Vite development server. It does not compile or launch the Rust/Tauri application. |
| `bun run build` | Runs the standalone TypeScript project build, including Vite and Vitest configuration, then runs `vite build`. |
| `bun run typecheck` | Runs no-emit TypeScript checks for application code plus `vite.config.ts` and `vitest.config.ts`. |
| `bun run lint` | Runs the repository Biome lint rules over TypeScript, JavaScript, and locale tooling. |
| `bun run format:check` | Checks deterministic formatting for JavaScript configuration and quality/locale scripts. |
| `bun run locale:check` | Validates every `AppLanguage` resource against `en.json`, including paths, types, and placeholders, without network access. |
| `bun run rust:fmt` / `rust:clippy` / `rust:check` | Runs locked Rust formatting, lint, or compile gates. Clippy treats warnings as errors. |
| `bun run quality` | Runs locale validation/tests, TypeScript, Biome, Rust formatting, and locked Clippy checks. |
| `bun run preview` | Serves an existing Vite production build for browser inspection. It does not build first and does not launch Tauri. |
| `bun run tauri:dev` | Runs `tauri dev --config src-tauri/tauri.conf.dev.json`. This is the canonical desktop development command and selects the isolated development identifier and title. Tauri starts `bun run dev` through `beforeDevCommand`. |
| `bun run tauri:build:release` | Runs `tauri build`. Tauri automatically merges the host platform config, producing MSI/NSIS on Windows or only the native executable on Linux. |
| `bun run tauri:build:linux` | Creates the unbundled native Linux release executable. This is the inner container command; use `./scripts/build-linux-docker.sh` from the host. |
| `bun run tauri:build:e2e` | Runs `node ./e2e/build-e2e.js`, validates the E2E identifier/title/target, and creates an unbundled debug executable in `src-tauri/target-e2e/debug/`. It does not build the frontend itself. |
| `bun run build:linux:docker` | Convenience wrapper for `scripts/build-linux-docker.sh`; requires Bun on the host, unlike invoking the shell script directly. |
| `bun run tauri` | Exposes the local Tauri CLI directly for explicit subcommands. It does not select the safe development overlay on its own. |

The repository does not define a coverage threshold. Coverage output is diagnostic, not a merge contract.

## Development modes

### Vite-only browser development

Use `bun run dev` when work only needs the frontend development server. `vite.config.ts` fixes the server to port `1420`, sets `strictPort: true`, and disables screen clearing. If port 1420 is occupied, startup fails instead of silently choosing a different port. The React and Tailwind Vite plugins are enabled.

Vite-only mode is not a complete application runtime. Code paths that invoke Tauri commands, use Tauri dialogs, or load files through the Tauri asset protocol need a desktop process and will not behave like the packaged app in an ordinary browser.

### Canonical Tauri development

Use:

```powershell
bun run tauri:dev
```

The base Tauri configuration expects the development URL `http://localhost:1420`; the strict Vite port ensures that URL cannot drift. The development overlay changes the product/window title to `Image Viewer 3000 Dev` and the identifier to `com.example.mediatagger.dev`.

Do not use an unqualified debug Tauri launch with the base configuration. In debug builds, `src-tauri/src/lib.rs` refuses startup when the effective identifier is the production identifier `com.example.mediatagger`. This protects production app data from a common configuration mistake. The guard requires the dev or E2E overlay, but it only rejects that one production identifier; it does not prove that every other identifier is safe.

## Frontend build contract

`bun run build` runs `bun run typecheck && vite build`:

1. Two no-emit `tsc` passes read `tsconfig.json` and `tsconfig.node.json`. They type-check `src/`, `vite.config.ts`, and `vitest.config.ts`. The application configuration is strict and rejects unused locals, unused parameters, and switch fallthrough.
2. `vite build` bundles the frontend into `dist/`, Vite's default output directory. The base Tauri configuration points `frontendDist` at `../dist` relative to `src-tauri/`.

`bun run typecheck` exposes the TypeScript pass separately. Vite development transpiles modules but is not a substitute for this gate.

## Desktop builds and profiles

The user-facing product name is `Image Viewer 3000`. `MediaTagger` remains the internal project/crate identity, and the existing `com.example.mediatagger` identifiers remain unchanged because they own app-data locations. Correcting display text must never be treated as an identifier migration.

| Profile | Command/configuration | Title | Identifier | Output and data boundary |
| --- | --- | --- | --- | --- |
| Release | `bun run tauri:build:release`; base plus host platform config | `Image Viewer 3000` | `com.example.mediatagger` | Normal `src-tauri/target` output and production app data. Tauri bundles MSI/NSIS on Windows; Linux produces a native executable. The Docker workflow exports the Linux executable to `artifacts/linux/`. |
| Development | `bun run tauri:dev`; dev overlay merged over the base | `Image Viewer 3000 Dev` | `com.example.mediatagger.dev` | Debug build and a separate identifier-specific app-data directory. |
| Desktop E2E | `bun run tauri:build:e2e`; E2E overlay merged over the base | `Image Viewer 3000 E2E` | `com.example.mediatagger.e2e` | Unbundled debug executable named `media_tagger.exe` on Windows or `media_tagger` on Linux and a separate E2E app-data directory. |

The effective identifier determines Tauri's app-data directory. Consequently, it isolates `media.db`, `thumbs/`, restore staging, and `instance.lock`. The three profiles can coexist because their identifiers differ. Changing an identifier is a data-location migration, not merely a label change.

The standalone E2E build is intentionally narrower than the full desktop E2E test command. `tauri.conf.e2e.json` clears `beforeBuildCommand`, so `bun run tauri:build:e2e` expects an existing `dist/`. Before invoking Tauri it verifies the exact E2E identifier, the E2E window title, and the dedicated `target-e2e` path; it removes only the old expected executable, sets `CARGO_TARGET_DIR`, builds with `--debug --no-bundle`, and verifies that the expected executable exists. The full `bun run test:e2e:tauri` orchestration additionally validates and clears only the exact E2E app-data directory, runs `bun run build`, creates the E2E executable, and checks the live window title before destructive tests.

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
| `src-tauri/tauri.windows.conf.json` | Windows WebView2 arguments, MSI/NSIS targets, and ffmpeg/ffprobe sidecars. |
| `src-tauri/tauri.linux.conf.json` | Disables bundling on Linux; Linux intentionally declares no media-tool sidecars. |
| `src-tauri/tauri.conf.dev.json` | Development identifier, product name, window title, and development window overlay. |
| `src-tauri/tauri.conf.e2e.json` | E2E identifier/title and suppression of Tauri's automatic frontend build. |
| `src-tauri/capabilities/default.json` | Permissions granted to the `main` window. |
| `src-tauri/Cargo.toml` / `Cargo.lock` | Rust crate/binary identity, edition, features, dependency ranges, and exact Rust dependency resolutions. |
| `src-tauri/build.rs` | Calls `tauri_build::build()`; generated context, application metadata, and resources come from the effective Tauri configuration. |
| `e2e/build-e2e.js` | E2E build safety assertions, isolated Cargo target directory, debug/no-bundle flags, and expected executable path. |

## ffmpeg and ffprobe packaging

The Windows platform bundle declares `binaries/ffmpeg` and `binaries/ffprobe` as Tauri `externalBin` entries. Tauri maps these logical names to target-specific files that must be supplied manually and remain ignored by Git:

- `src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe`
- `src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe`

When both files are present, release MSI/NSIS packages include both tools. A fresh checkout does not contain them and cannot build a complete Windows package until they are supplied. Linux release builds do not include them: they rely on system `ffmpeg` and `ffprobe` from `PATH`. Development and unbundled builds may resolve tools from Tauri resource/binary locations or adjacent resource directories. Missing tools do not block application startup, but video duration probing and thumbnail generation can fail or degrade.

`src-tauri/binaries/README.md` pins the Gyan.dev FFmpeg `8.0.1-full_build` archive, origin, license, extraction names, update procedure, and version check. `src-tauri/binaries/SHA256SUMS` records the expected SHA-256 hashes for both extracted executables. Update the pair and both metadata files together, preserve the exact target-triple filenames, and smoke-test an installed package rather than relying only on a `PATH` fallback.

## Performance diagnostics

Frontend and backend performance logging use different environment variables and activation rules.

Enable frontend User Timing instrumentation at Vite build/dev-server time with:

```powershell
$env:VITE_MEDIATAGGER_PERF = "1"
bun run tauri:dev
```

The frontend enables it only when the value is exactly `1`. It emits browser-console `[perf]` entries, including measured asset-query refreshes.

Enable backend timing output with:

```powershell
$env:MEDIATAGGER_PERF = "1"
bun run tauri:dev
```

The Rust backend checks only whether `MEDIATAGGER_PERF` exists, so even an empty value or `0` enables its stderr timings for scans, asset queries, and thumbnail pages. Remove the variable to disable it:

```powershell
Remove-Item Env:MEDIATAGGER_PERF -ErrorAction SilentlyContinue
Remove-Item Env:VITE_MEDIATAGGER_PERF -ErrorAction SilentlyContinue
```

A non-debug Windows build hides its console window, so backend stderr is easiest to observe during `tauri:dev` or under an explicit process/log capture.

## Dependency updates

Keep dependency families aligned rather than updating one manifest in isolation:

- Tauri changes may span `@tauri-apps/cli`, `@tauri-apps/api`, `@tauri-apps/plugin-dialog`, Rust `tauri`, `tauri-build`, and `tauri-plugin-dialog`. Check their compatibility together and regenerate both lockfiles as applicable.
- React and `react-dom` should remain mutually compatible, along with `@types/react` and `@types/react-dom`.
- Tailwind CSS and `@tailwindcss/vite` should be updated together; verify the DaisyUI integration after either changes.
- A changed Rust target or packaging platform requires an explicit media-tool policy and corresponding bundle verification. Windows uses matching sidecars; Linux uses the system tools.

Run `bun install` for intentional JavaScript updates and an appropriate Cargo update for intentional Rust updates, then inspect `bun.lock` and `src-tauri/Cargo.lock`. Most manifest entries are semver ranges, so frozen/locked resolution and the pinned toolchains are both required. Neither lockfile pins system prerequisites such as WebView2, the Windows SDK, Microsoft build tools, NSIS/MSI tooling, or rolling Linux system packages.

## Guarantees and limitations

Current build guarantees:

- the canonical dev command selects a non-production identifier and fixed Vite URL;
- a debug application refuses to start with the production identifier;
- the frontend production build must pass the configured TypeScript checks before Vite bundles it;
- release builds invoke the frontend build and merge the appropriate Windows or Linux bundle configuration;
- the Docker Linux workflow installs its toolchain and declared runtime media stack in the image, runs frontend/backend/desktop tests, and exports the native release binary, manifest, and checksums;
- the E2E builder asserts its identifier, title, isolated target directory, and resulting executable; and
- committed Bun and Cargo lockfiles capture exact dependency resolutions.

Current limitations:

- only Windows and Linux x86-64 packaging are configured; macOS and ARM are not;
- the rolling `archlinux:base-devel` image and system packages are not digest/version pinned, so a later clean container build can use newer native dependencies;
- Windows build tools and WebView2 are external prerequisites and are not pinned by the repository;
- compatible-range manifests mean lockfiles must be preserved for repeatable resolution;
- the debug guard rejects only the production identifier and cannot certify arbitrary custom overlays;
- a standalone E2E build can package stale or missing frontend assets because its automatic frontend build is disabled;
- Vite-only development cannot validate native IPC, dialogs, asset-protocol loading, app-data paths, or packaged resource layout;
- unbundled E2E builds do not prove installed MSI/NSIS resource layout or sidecar launch; and
- there is no numeric coverage threshold or installed Windows package smoke test in CI.

## Build checklist

For a clean local desktop development setup:

1. Confirm Node satisfies `package.json` and that `bun`, `rustc`, and `cargo` are on `PATH`.
2. Install the Windows MSVC/Tauri prerequisites and WebView2.
3. Run `bun install --frozen-lockfile` for an unchanged checkout, or `bun install` when intentionally updating dependencies.
4. Run `bun run build` once to validate TypeScript and produce `dist/`.
5. Run `bun run tauri:dev` for normal desktop development.

Before distributing a Windows release:

1. Inspect intended changes to both manifests and lockfiles.
2. Confirm both target-suffixed media sidecars exist.
3. Run the required frontend/backend tests described in the testing guide.
4. Run `bun run tauri:build:release`.
5. Smoke-test the installed MSI/NSIS result, including image display, video probing, and video thumbnail generation without relying on a system `ffmpeg` or `ffprobe` on `PATH`.

For an Arch Linux x86-64 release without host build dependencies:

1. Confirm `docker version` can reach the daemon as the current user.
2. Run `./scripts/build-linux-docker.sh`.
3. Confirm `artifacts/linux/` contains `media_tagger`, `build-manifest.txt`, and `SHA256SUMS`, then run `(cd artifacts/linux && sha256sum --check SHA256SUMS)`.
4. Install the declared runtime packages on the target Arch/CachyOS system and run `artifacts/linux/media_tagger`.
5. Smoke-test image and GIF display plus video playback with image and audio, probing, and thumbnail generation.

## Troubleshooting

**Vite reports that port 1420 is already in use.** Stop the process using the port. `strictPort` intentionally prevents changing ports because Tauri's `devUrl` is fixed to `http://localhost:1420`.

**A debug desktop process says it refuses the production identifier.** Launch with `bun run tauri:dev`, or pass the E2E overlay only for the isolated E2E workflow. An unqualified `tauri dev` inherits the unsafe production identifier from the base configuration.

**The app appears to have an empty library.** Check the selected identifier/profile before changing files. Release, dev, and E2E intentionally use different app-data directories.

**`tauri:build:e2e` has missing or stale UI assets.** Run `bun run build` first. The E2E overlay deliberately disables `beforeBuildCommand`; only the full desktop E2E test orchestration builds the frontend automatically.

**Video probing or thumbnails fail while images still work.** Confirm the target-suffixed ffmpeg and ffprobe files exist for Windows packages. On Linux, confirm `ffmpeg` and `ffprobe` are on `PATH`. Application startup succeeding does not prove that video tools were found.

**Linux video does not play or emits GStreamer pipeline errors.** Install `webkit2gtk-4.1`, `ffmpeg`, `gst-plugins-base-libs`, `gst-plugins-good`, `gst-plugins-bad`, and `gst-libav`, then verify `ldd artifacts/linux/media_tagger` has no `not found` entries. Playback uses a loopback HTTP stream rather than Tauri's custom asset protocol, so remaining failures should be diagnosed from the logged native `MediaError` category and the installed GStreamer codec support. The supported Linux artifact is the native executable, not an AppImage.

**The Docker command cannot connect to `/var/run/docker.sock`.** Confirm the current login session has access to the Docker daemon. The build script deliberately does not elevate through `sudo`.

**A dependency resolves differently on another machine.** Confirm both machines use the committed `bun.lock` and `src-tauri/Cargo.lock`, use frozen/locked install modes, and match `.nvmrc`, `packageManager`, and `rust-toolchain.toml`. Compare external system package versions when the pinned language toolchains match.
