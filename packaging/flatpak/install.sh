#!/usr/bin/env bash
set -euo pipefail
export BUN_INSTALL_CACHE_DIR="$PWD/bun-cache"
export CARGO_HOME="$PWD/cargo-home"
# All tarballs are already unpacked from the lockfile-derived sources.
# The enclosing Flatpak build sandbox has no network access.
bun install --frozen-lockfile --offline
bun run tauri build --no-bundle --config src-tauri/tauri.flatpak.generated.json -- --locked --offline
app_id="$(python3 -c 'import json; print(json.load(open("publisher.json"))["appId"])')"
install -Dm755 src-tauri/target/release/media_tagger /app/bin/media_tagger
install -Dm644 tagrove.desktop "/app/share/applications/$app_id.desktop"
install -Dm644 tagrove.metainfo.xml "/app/share/metainfo/$app_id.metainfo.xml"
for size in 32 64 128 256 512; do
  install -Dm644 "src-tauri/icons/${size}x${size}.png" "/app/share/icons/hicolor/${size}x${size}/apps/$app_id.png"
done
install -Dm644 LICENSE /app/share/licenses/tagrove/LICENSE
python3 packaging/linux/notices.py /app/share/licenses/dependencies
desktop-file-validate "/app/share/applications/$app_id.desktop"
if python3 -c 'import json; raise SystemExit(0 if json.load(open("publisher.json")).get("repository") else 1)'; then
  appstreamcli validate --no-net "/app/share/metainfo/$app_id.metainfo.xml"
else
  # The local publisher intentionally has no public homepage. All other
  # validation severities are unchanged; publication validation has no override.
  appstreamcli validate --no-net --override=url-homepage-missing=info "/app/share/metainfo/$app_id.metainfo.xml"
fi
bash packaging/linux/check-runtime.sh /app/bin/media_tagger /app/bin/ffmpeg /app/bin/ffprobe
