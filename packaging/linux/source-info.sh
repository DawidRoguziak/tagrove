#!/usr/bin/env bash
set -euo pipefail
destination="$1"
# Prepared on the host from the audited Git tree, before any compiler runs.
[[ -s .release-source/source-commit.txt ]]
grep -Ex '[0-9a-f]{40}' .release-source/source-commit.txt >/dev/null
(cd .release-source && sha256sum --check --strict SHA256SUMS)
cp .release-source/application-source.tar.gz .release-source/source-commit.txt "$destination/"
cp packaging/linux/toolchains.json "$destination/"
cp packaging/linux/appimage-tools.json "$destination/"
cp packaging/flatpak/media.json "$destination/media-sources.json"
cp packaging/linux/SOURCE-NOTICE.txt "$destination/"
printf 'source-commit=%s\n' "$(cat .release-source/source-commit.txt)" >> "$destination/build-manifest.txt"
sha256sum bun.lock src-tauri/Cargo.lock >> "$destination/build-manifest.txt"
