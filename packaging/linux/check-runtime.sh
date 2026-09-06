#!/usr/bin/env bash
set -euo pipefail
for binary in "$@"; do
  file "$binary" | grep -q 'ELF 64-bit.*x86-64'
  dependencies="$(ldd "$binary")"
  printf '%s\n%s\n' "$binary" "$dependencies"
  if grep -q 'not found' <<< "$dependencies"; then exit 1; fi
done
ldd "$1" | grep 'libmpv'
ldd "$1" | grep 'libgtk-3'
ldd "$1" | grep 'libwebkit2gtk-4.1'
"$2" -v error -f lavfi -i color=c=red:s=64x64 -frames:v 1 -y /tmp/tagrove-package-frame.png
"$3" -v error -show_entries stream=width,height /tmp/tagrove-package-frame.png
python3 - <<'PY'
import ctypes
for library, symbol in [("libEGL.so.1", "eglGetProcAddress"), ("libGL.so.1", "glXGetProcAddressARB"), ("libGLESv2.so.2", "glGetString")]:
    assert getattr(ctypes.CDLL(library), symbol)
PY
