#!/bin/sh
set -eu
bunx vite build --config e2e/gallery-scale/vite.config.ts
# XTest and screenshots must never target the user's desktop.
exec env -u WAYLAND_DISPLAY GDK_BACKEND=x11 NO_AT_BRIDGE=1 GIO_USE_VFS=local GTK_OVERLAY_SCROLLING=0 \
  xvfb-run -a -s '-screen 0 1900x900x24 -nolisten tcp' \
  sh -c 'export MEDIATAGGER_SCALE_DISPLAY="$DISPLAY"; python3 e2e/gallery-scale/webkit.py'
