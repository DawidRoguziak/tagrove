#!/usr/bin/env bash

cd /path/to/tagrove

if pgrep -x media_tagger >/dev/null; then
  echo "Najpierw zamknij aplikację."
else
  (cd artifacts/linux && sha256sum --check SHA256SUMS) &&
  tmp="$(mktemp "$HOME/.local/bin/.media_tagger.XXXXXX")" &&
  install -m 0755 artifacts/linux/media_tagger "$tmp" &&
  mv -f "$tmp" "$HOME/.local/bin/media_tagger"
fi
