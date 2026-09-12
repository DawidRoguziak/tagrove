#!/usr/bin/env bash
set -euo pipefail
[[ -f /.dockerenv && -d /bundle ]]
mkdir -p /evidence /tmp/tagrove-package /tmp/tagrove-data /tmp/tagrove-config /tmp/tagrove-cache
: > /evidence/linkage.txt
rm -f /evidence/result.txt
cd /tmp/tagrove-package
/bundle/Tagrove-*.AppImage --appimage-extract > /evidence/extracted-files.txt
appdir="$PWD/squashfs-root"
package_libraries="$appdir/usr/lib:$appdir/usr/lib/x86_64-linux-gnu"
for binary in "$appdir/usr/bin/media_tagger" "$appdir/usr/bin/ffmpeg" "$appdir/usr/bin/ffprobe"; do
  LD_LIBRARY_PATH="$package_libraries" /lib64/ld-linux-x86-64.so.2 --list "$binary" >> /evidence/linkage.txt
done
if grep 'not found' /evidence/linkage.txt; then exit 1; fi
LD_LIBRARY_PATH="$package_libraries" "$appdir/usr/bin/ffmpeg" -v error -f lavfi -i color=c=red:s=64x64 -frames:v 1 -y /evidence/frame.png
LD_LIBRARY_PATH="$package_libraries" "$appdir/usr/bin/ffprobe" -v error -show_entries stream=width,height /evidence/frame.png > /evidence/probe.txt
bash /tests/media-fixtures.sh env "LD_LIBRARY_PATH=$package_libraries" "$appdir/usr/bin/ffmpeg"
export DISPLAY=:97 GDK_BACKEND=x11
export XDG_DATA_HOME=/tmp/tagrove-data XDG_CONFIG_HOME=/tmp/tagrove-config XDG_CACHE_HOME=/tmp/tagrove-cache
export XDG_RUNTIME_DIR=/tmp/tagrove-runtime
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
unset WAYLAND_DISPLAY
Xvfb "$DISPLAY" -screen 0 1440x900x24 -nolisten tcp -fbdir /evidence > /evidence/xvfb.log 2>&1 &
xvfb_pid=$!
app_pid=""
cleanup() {
  cp /evidence/Xvfb_screen0 /evidence/final-screen.xwd 2>/dev/null || true
  if [[ -n "$app_pid" ]]; then kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true; fi
  kill "$xvfb_pid" 2>/dev/null || true
  wait "$xvfb_pid" 2>/dev/null || true
}
trap cleanup EXIT
sleep 1
"$appdir/AppRun" > /evidence/application.log 2>&1 &
app_pid=$!
for attempt in $(seq 1 120); do
  [[ -s "$XDG_DATA_HOME/com.example.mediatagger/media.db" ]] && break
  kill -0 "$app_pid"
  sleep 0.5
done
test -s "$XDG_DATA_HOME/com.example.mediatagger/media.db"
sleep 3
kill -0 "$app_pid"
xwininfo -root -tree > /evidence/windows.txt
python3 - <<'PY'
import sqlite3
connection = sqlite3.connect('/tmp/tagrove-data/com.example.mediatagger/media.db')
assert connection.execute('pragma integrity_check').fetchone()[0] == 'ok'
PY
cp /evidence/Xvfb_screen0 /evidence/window.xwd
echo 'PASS: clean runtime linkage, packaged tools, application startup and database initialization' > /evidence/result.txt
python3 /tests/media-ui.py "$XDG_DATA_HOME/com.example.mediatagger/media.db" >> /evidence/result.txt

if [[ "${TAGROVE_PACKAGE_INTERACTIVE:-}" == 1 ]]; then wait "$app_pid"; fi
