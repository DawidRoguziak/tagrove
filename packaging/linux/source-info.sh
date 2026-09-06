#!/usr/bin/env bash
set -euo pipefail
destination="$1"
cp packaging/linux/toolchains.json "$destination/"
cp packaging/linux/appimage-tools.json "$destination/"
cp packaging/flatpak/media.json "$destination/media-sources.json"
cp packaging/linux/SOURCE-NOTICE.txt "$destination/"
tar --exclude='./node_modules' --exclude='./dist' --exclude='./src-tauri/target*' \
  --exclude='./cargo-vendor' --exclude='./cargo-home' --exclude='./bun-cache' \
  --exclude='./.git' --exclude='./artifacts' --exclude='./output' \
  --exclude='./.flatpak-builder' --exclude='./.packaging' \
  -czf "$destination/application-source.tar.gz" .
sha256sum bun.lock src-tauri/Cargo.lock >> "$destination/build-manifest.txt"
