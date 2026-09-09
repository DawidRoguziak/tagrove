# Linux packaging and release preparation

[Setup and builds](setup-and-build.md#docker-packages) owns the build and direct Flatpak installation commands. The package build never installs application files on the host. Docker packaging CI builds Flatpak and AppImage, validates exports, and uploads workflow artifacts without publishing a release.

## Sources and identity

[The publisher configuration](../../packaging/flatpak/publisher.json) holds the app ID, repository URL, release archive URL and SHA-256, release reference/date, developer ID/name and screenshot URLs. Local builds use `com.example.mediatagger`. The manifest generator rejects development/E2E IDs for release packaging. Debug startup permits only the established development and E2E identities, protecting future publisher IDs as well.

Flatpak keeps data under `~/.var/app/<app-id>/`. Its XDG data directory contains the Tauri identifier-specific library. A new Flatpak install starts empty even with a populated native library. Changing the app ID creates a separate installation and data profile. No migration is implicit. AppImage retains the existing native release identity.

Use Flatpak 1.16.6 or later for the direct bundle reinstall workflow. Debian 12's Flatpak 1.14.10 rejected reinstalling the same bundle in the package test; [the supported bookworm backport](https://packages.debian.org/bookworm-backports/flatpak) at 1.16.6 accepted the same command. The disposable Debian test image installs that backport.

The application code is GPL-3.0-or-later, with [LICENSE](../../LICENSE) at the repository root. AppStream metadata is CC0-1.0. Dependency licenses remain attached to their respective code. Build outputs retain dependency manifests and discovered license texts, source URLs/checksums, resolved distribution package versions, and the application source used for that build. Inspect `SOURCE-NOTICE.txt` before redistribution and provide matching corresponding sources, including distribution patches, for bundled GPL/LGPL libraries. The dependency inventory is not a written source offer.

## Privacy gate and release inputs

Run `bun run privacy:check` or `python3 scripts/privacy-check.py` with Gitleaks 8.30.1 on `PATH`. `scripts/install-gitleaks.sh` downloads the pinned Linux x86_64 release and checks its SHA-256. Python 3.12 or later is required for source preparation.

The gate scans every complete reachable Git object across local branches, remote-tracking branches and tags, including commit/tag metadata. It also checks tracked working files and staged blobs. It rejects shallow history, grafts and replacement refs. Gitleaks uses its default rules with decoding/archive traversal enabled, an explicit configuration and an empty ignore file. Local ignore settings and inline allow comments cannot suppress findings. Untracked and ignored working materials are outside this command's scope; release inputs come solely from a clean committed tree.

The filename policy blocks environment/registry credentials, private-key filenames, databases, local archives and application working directories at any depth. Home-path detection covers Linux, macOS and Windows user directories. Two existing illustrative paths are allowed only in their owning documentation/test file. Container paths under `/root` are retained. This does not classify every possible personal identifier or approve screenshots; public images still need review. Example configuration requires an explicit reviewed change to both the ignore and scanner policies.

`./scripts/build-linux-docker.sh --commit HEAD` audits history before preparing the Docker context. `application-source.tar.gz` contains exactly the selected commit's files, with neutral Git archive ownership. It excludes untracked/ignored data by construction. Archive attribute substitutions, generated source trees, symlinks and submodules are rejected. The recorded commit, archive bytes and checksums must match the export. Builds never archive the compiled working directory.

After building each format, run:

```bash
python3 packaging/linux/audit-package.py appimage artifacts/linux/appimage
python3 packaging/linux/audit-package.py flatpak artifacts/linux/flatpak
```

This validates the explicit export inventory and checksums, extracts the bundle into temporary storage, and scans its filenames, contents, ELF strings and corresponding sources. It requires binutils, Flatpak and OSTree. Findings print filenames and hashes, with raw secrets confined to temporary files removed on exit. The reviewed library findings file permits only exact combinations of library path, rule and finding digest. Its GnuTLS keys were compared with public 3.7.9 self-tests; the other entries are format markers and an error message. GTK SVG export paths from upstream assets are allowed only for a reviewed binary SHA-256. A changed dependency finding or binary fails until reviewed. The mpv text `fs-root/home/dos-drive` is a comment, not an absolute home directory.

CI fetches full history and runs the repository gate and its regression tests. Packaging CI also audits each actual package before uploading an explicit list of bundles, corresponding sources, notices, manifests and checksums. A failed gate prevents that release artifact upload. Runtime evidence is generated using synthetic collections and retained separately only after the repository gate passes. Local `artifacts/` directories, session logs and historical backup archives are never a release upload list.

## Preparing a Flathub submission

1. Choose an app ID you control and fill every field in `packaging/flatpak/publisher.json`. Use actual public screenshots and developer details.
2. Publish a source release that includes the application, lockfiles and these packaging definitions. Set `releaseUrl`, `releaseSha256`, `releaseRef`, and `releaseDate` to that release. Verify that the release archive has the same lockfiles used to generate the manifest.
3. Run `./scripts/prepare-flathub.sh`. It uses Docker and writes `artifacts/flathub/prepared/<app-id>.json` plus `flathub.json`, which restricts builds to x86_64. AppStream validation and the Flathub manifest linter must pass. Validation rejects placeholder identities, invalid ID syntax, mismatched code-hosting IDs, missing metadata, local source references, and unpinned release archives. The preparation command downloads and verifies the release archive and rejects a mismatch with the checkout's lockfiles, version or required packaging files. `releaseUrl` must include the chosen release reference. The publication manifest shares toolchain/media modules, permissions, dependency generation, and installation commands with the local manifest.
4. Build that generated manifest with Flatpak Builder, run AppStream and desktop validation, and resolve Flathub linter findings. Verify screenshots, the owned publisher identity, source availability and distribution rights before submission.
5. A maintainer must write and submit the application request. [Flathub's current policy](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy) requires disclosure of generated material and prohibits agents from authoring or opening submission interactions. Use the factual preparation notes in `packaging/flatpak/SUBMISSION.md` for review, not as a generated PR description.

Public submission remains pending the publishing identity and public release. The eventual public archive and publication build have not been exercised by local packaging CI. A custom domain's ownership requires maintainer verification. This repository does not supply invented publisher details or screenshot links, and the preparation command does not submit anything.

## Container boundaries

The GNOME 50 image is pinned by digest in `Dockerfile.flatpak`. Flatpak Builder needs Docker `--privileged` for its nested sandbox, following [Flatpak's documented container workflow](https://github.com/flatpak/flatpak-github-actions/blob/master/README.md). Only build inputs and dedicated Docker build/cache volumes are available. The host home, session bus, Docker socket, installed launchers and libraries are not mounted. A per-checkout Flatpak Builder state cache and a dedicated download cache named `tagrove-flatpak-downloads-x86_64` survive builds. Delete the corresponding dedicated cache volumes to check a completely fresh build; never prune unrelated Docker state.

Flatpak compilation runs with Docker `--network none`, Flatpak `--unshare=network`, Bun `--offline` and Cargo `--locked --offline`. The generated manifest contains all npm/crate tarballs with lockfile digests. Cargo uses an unpacked vendor directory; Bun uses its populated package cache. Missing sources fail the build. Neither host dependency directories nor prebuilt frontend assets enter the build context.

AppImage uses Debian 12, a pinned Bun/Node/Rust toolchain and checksum-pinned linuxdeploy, launcher, GTK/GStreamer plugins and AppImage output plugin. The compiler/bundler runs in a separate Docker container with no network access. Debian security packages can change; the output records their versions and source package names. The optional native target retains its rolling Arch base and runtime requirements.

## Verification

With the pinned Gitleaks on `PATH`, run `bun run test:privacy` for synthetic secrets, staged/history-only leaks, nested private files, shallow histories and exact source archives. Run `bun run test:packaging` or `python3 -m unittest discover -s packaging/tests -v` for source generation, publication validation, checksum inventory, duplicate-bundle rejection, failed-build preservation, and rollback after a later format fails to publish. Test fixtures use temporary directories.

For each actual package, verify the SHA256SUMS file in its format directory. Flatpak export imports the generated bundle into a disposable OSTree repository and runs `ostree fsck`. Package builds check x86_64 ELF architecture, GTK/WebKit/libmpv linkage, EGL/GL entry points, ffmpeg/ffprobe execution and a generated image frame. AppImage checks its extracted WebKit helper processes. These checks do not establish GTK rendering, audio output or file-dialog behavior.

Run the actual exported packages in disposable containers:

```bash
./scripts/test-linux-packages.sh --format flatpak
./scripts/test-linux-packages.sh --format appimage
./scripts/test-linux-packages.sh --format appimage --distribution ubuntu24
./scripts/test-linux-packages.sh --format appimage --distribution arch
```

The media test drives the native folder chooser, scans generated fixtures, waits for gallery thumbnails, saves a tag and checks the database. A video distinct from the GIF changes from blue to yellow; seek assertions require each color, and fullscreen must cover the former sidebar. Evidence goes into a unique `artifacts/package-tests/` directory, including failure screenshots. The Flatpak test uses Debian 12 with GNOME 50 runtimes and captures PulseAudio output to an isolated null sink. It additionally tests direct installation, same-version reinstall, a changed-deployment upgrade fixture, retained database bytes and an untouched native-profile fixture. The upgrade fixture changes metadata and adds a payload marker; it is not a second released application version. Installation can fetch the runtime repository descriptor. Compilation remains offline.

The AppImage runtime baseline includes the host C library, desktop font configuration and graphics drivers. GTK/WebKit, media libraries, tools, MIME data and icon resources come from the AppImage. The pinned GTK launcher selects X11, including XWayland on Wayland desktops. Native Wayland is provided by the Flatpak.

The video surface selects mpv's basic rendering mode on llvmpipe to avoid the black frames observed with Mesa 26.1 and mpv 0.41. This trades advanced scaling/shader processing for working software playback. Hardware drivers retain the normal mode. The package test does not force that option externally; it exercises the application's renderer detection.

Use a disposable OS account or an isolated container/VM for additional release-package testing. Never point destructive workflows at a personal media collection. Keep the existing development/E2E guards unchanged. A release-package smoke test supplements the E2E debug binary; it does not replace it.

Record results for these checks:

| Check | Required evidence |
| --- | --- |
| Clean Docker builds | Build logs, manifest tool versions and source digests for both formats |
| Offline Flatpak compilation | Successful compile in the Docker container with network mode `none` |
| Direct Flatpak install | Install the exported bundle with `flatpak install --user`, then launch it |
| Same-version reinstall | Close the app, reinstall the same bundle with `--reinstall`, reopen and check retained tags/settings |
| Upgrade | Install a higher-version bundle over the same app ID and check retained library data |
| Profile separation | Fresh Flatpak library and untouched native library/profile |
| Basic media | Temporary PNG/JPEG/GIF/video fixtures; scan, thumbnails, tag persistence and image/GIF viewing |
| Native video | Visible decoded picture, audible sound, play/pause, seeking and fullscreen on a real graphics session |
| File dialogs | Open a temporary media directory and save a disposable backup |
| Filesystem/backup | Rename/delete temporary copies; backup/restore only the disposable library |
| Distribution/session coverage | Debian 12, Ubuntu 24.04 and Arch/CachyOS; X11 and Wayland |

Build-time linkage and metadata checks are automated. The full distribution/session matrix and visible/audible playback require installed-package execution and must be reported separately. Do not infer those results from an unbundled E2E run or a successful build.
