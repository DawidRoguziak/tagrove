#!/usr/bin/env bash
# Run only inside the dedicated disposable Flatpak test container.
set -euo pipefail
[[ -f /.dockerenv && -d /inputs && "$EUID" == 0 ]]
app_id="$(python3 -c 'import json; print(json.load(open("/inputs/packaging/flatpak/publisher.json"))["appId"])')"
[[ "$app_id" == com.example.mediatagger ]]
[[ ! -e "/root/.var/app/$app_id" ]]
[[ ! -e "/root/.local/share/$app_id" ]]
mkdir -p "/root/.local/share/$app_id"
printf 'existing native library fixture\n' > "/root/.local/share/$app_id/media.db"
printf 'existing native settings fixture\n' > "/root/.local/share/$app_id/settings.json"
native_before="$(sha256sum /root/.local/share/"$app_id"/*)"
export XDG_DATA_HOME=/tmp/tagrove-install-test/data
export XDG_CONFIG_HOME=/tmp/tagrove-install-test/config
export XDG_CACHE_HOME=/tmp/tagrove-install-test/cache
export XDG_RUNTIME_DIR=/tmp/tagrove-install-test/runtime
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR" /evidence
chmod 700 "$XDG_RUNTIME_DIR"
flatpak install --user --noninteractive /bundle/Tagrove-*.flatpak
flatpak info --user "$app_id" > /evidence/installed.txt
flatpak info --user --show-metadata "$app_id" > /evidence/metadata.ini
export DISPLAY=:97
export GDK_BACKEND=x11 XDG_CURRENT_DESKTOP=GNOME
unset WAYLAND_DISPLAY
Xvfb "$DISPLAY" -screen 0 1440x900x24 -nolisten tcp -fbdir /evidence > /evidence/xvfb.log 2>&1 &
xvfb_pid=$!
trap 'cp /evidence/Xvfb_screen0 /evidence/final-screen.xwd 2>/dev/null || true; flatpak kill "$app_id" >/dev/null 2>&1 || true; kill "$xvfb_pid" >/dev/null 2>&1 || true; wait "$xvfb_pid" 2>/dev/null || true' EXIT
sleep 1
dbus-update-activation-environment DISPLAY GDK_BACKEND XDG_CURRENT_DESKTOP XDG_RUNTIME_DIR \
  XDG_DATA_HOME XDG_CONFIG_HOME XDG_CACHE_HOME
flatpak run --user --env=LIBGL_ALWAYS_SOFTWARE=1 "$app_id" > /evidence/application.log 2>&1 &
app_pid=$!
app_data="/root/.var/app/$app_id/data/$app_id"
for attempt in $(seq 1 120); do
  [[ -s "$app_data/media.db" ]] && break
  kill -0 "$app_pid"
  sleep 0.5
done
test -s "$app_data/media.db"
sleep 3
bash /inputs/packaging/tests/media-fixtures.sh flatpak run --user --command=ffmpeg "$app_id"
python3 /inputs/packaging/tests/media-ui.py "$app_data/media.db" > /evidence/media-result.txt
if [[ "${TAGROVE_PACKAGE_INTERACTIVE:-}" == 1 ]]; then wait "$app_pid"; fi
flatpak kill "$app_id"
wait "$app_pid" || true
python3 - "$app_data" <<'PY'
from pathlib import Path
import sqlite3, sys
root = Path(sys.argv[1])
connection = sqlite3.connect(root / 'media.db')
assert connection.execute('pragma integrity_check').fetchone()[0] == 'ok'
assert connection.execute("select count(*) from sqlite_master where type='table'").fetchone()[0] > 0
(root / 'reinstall-retention.txt').write_text('retained application data\n')
PY
sha256sum "$app_data/media.db" > /evidence/database-before.sha256
flatpak install --user --noninteractive --reinstall /bundle/Tagrove-*.flatpak
sha256sum --check /evidence/database-before.sha256
[[ "$(cat "$app_data/reinstall-retention.txt")" == 'retained application data' ]]
# Exercise a new deployment of the same application ID with a disposable
# metadata/payload fixture. This is not a new application release.
before_commit="$(flatpak info --user --show-commit "$app_id")"
location="$(flatpak info --user --show-location "$app_id")"
mkdir /tmp/tagrove-upgrade
cp -a "$location/files" "$location/metadata" /tmp/tagrove-upgrade/
chmod -R u+w /tmp/tagrove-upgrade
printf 'upgrade fixture\n' > /tmp/tagrove-upgrade/files/share/tagrove-upgrade-fixture.txt
python3 - "/tmp/tagrove-upgrade/files/share/metainfo/$app_id.metainfo.xml" <<'PYTHON'
import sys, xml.etree.ElementTree as ET
path = sys.argv[1]
tree = ET.parse(path)
release = tree.find('./releases/release')
release.set('version', release.get('version') + '.99')
tree.write(path, encoding='utf-8', xml_declaration=True)
PYTHON
flatpak build-finish /tmp/tagrove-upgrade
flatpak build-export --disable-sandbox /tmp/tagrove-upgrade-repo /tmp/tagrove-upgrade stable
flatpak build-bundle /tmp/tagrove-upgrade-repo /tmp/tagrove-upgrade.flatpak "$app_id" stable
flatpak install --user --noninteractive /tmp/tagrove-upgrade.flatpak
[[ "$(flatpak info --user --show-commit "$app_id")" != "$before_commit" ]]
[[ "$(flatpak run --user --command=cat "$app_id" /app/share/tagrove-upgrade-fixture.txt)" == 'upgrade fixture' ]]
sha256sum --check /evidence/database-before.sha256
[[ "$(cat "$app_data/reinstall-retention.txt")" == 'retained application data' ]]

flatpak run --user --env=LIBGL_ALWAYS_SOFTWARE=1 "$app_id" >> /evidence/application.log 2>&1 &
app_pid=$!
sleep 5
kill -0 "$app_pid"
flatpak kill "$app_id"
wait "$app_pid" || true
[[ "$(sha256sum /root/.local/share/"$app_id"/*)" == "$native_before" ]]
echo 'PASS: direct install, real application database initialization, same-version reinstall, fixture upgrade, data retention, relaunch, native-profile separation' > /evidence/result.txt
