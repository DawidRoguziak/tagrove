#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd -- "$script_dir/.." && pwd)"
install_prefix="${1:-$HOME/.local}"
artifact_dir="$project_root/artifacts/linux"

if pgrep -x media_tagger >/dev/null; then
  echo "Close Tagrove before reinstalling it." >&2
  exit 1
fi

# Check the complete release before changing any installed file.
(
  cd "$artifact_dir"
  sha256sum --check SHA256SUMS
  test -s media_tagger
  test -s tagrove.png
  test -s com.example.mediatagger.desktop
)

mkdir -p -- "$install_prefix/bin"
temporary_binary="$(mktemp "$install_prefix/bin/.media_tagger.XXXXXX")"
trap 'rm -f -- "$temporary_binary"' EXIT
install -m 0755 "$artifact_dir/media_tagger" "$temporary_binary"
mv -f -- "$temporary_binary" "$install_prefix/bin/media_tagger"
install -Dm0644 "$artifact_dir/tagrove.png" "$install_prefix/share/icons/hicolor/512x512/apps/tagrove.png"
install -Dm0644 "$artifact_dir/com.example.mediatagger.desktop" "$install_prefix/share/applications/com.example.mediatagger.desktop"

if command -v gtk-update-icon-cache >/dev/null && [[ -f "$install_prefix/share/icons/hicolor/index.theme" ]]; then
  gtk-update-icon-cache -f "$install_prefix/share/icons/hicolor"
fi
if command -v update-desktop-database >/dev/null; then
  update-desktop-database "$install_prefix/share/applications"
fi

echo "Installed Tagrove in $install_prefix. Its bin directory must be on PATH for the launcher."
