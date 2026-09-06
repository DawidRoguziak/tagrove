#!/usr/bin/env bash
set -euo pipefail
mkdir -p /out/licenses
version="$(python3 -c 'import json; print(json.load(open("package.json"))["version"])')"
mapfile -t bundles < <(find src-tauri/target/release/bundle/appimage -maxdepth 1 -name '*.AppImage' -type f)
[[ ${#bundles[@]} == 1 ]]
cp "${bundles[0]}" "/out/Tagrove-${version}-x86_64.AppImage"
bundle="/out/Tagrove-${version}-x86_64.AppImage"
mkdir /tmp/tagrove-appimage-validation
cd /tmp/tagrove-appimage-validation
"$bundle" --appimage-extract >/dev/null
appdir="$PWD/squashfs-root"
export LD_LIBRARY_PATH="$appdir/usr/lib:$appdir/usr/lib/x86_64-linux-gnu"
bash /inputs/packaging/linux/check-runtime.sh "$appdir/usr/bin/media_tagger" "$appdir/usr/bin/ffmpeg" "$appdir/usr/bin/ffprobe" > /out/runtime-check.txt
test -f "$appdir/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess"
test -f "$appdir/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitNetworkProcess"
find "$appdir" -name '*.desktop' -exec desktop-file-validate '{}' +
test -s "$appdir/usr/share/licenses/tagrove/dependencies/index.json"
unset LD_LIBRARY_PATH
cd /inputs
cp LICENSE /out/LICENSE
cp -a packaging/linux/licenses /out/licenses/appimage
cp -a .packaging/dependencies /out/licenses/dependencies
mkdir /out/licenses/debian
find /usr/share/doc -name copyright -type f -exec cp --parents '{}' /out/licenses/debian/ \;
dpkg-query -W -f='${binary:Package}\t${source:Package}\t${source:Version}\n' > /out/debian-sources.tsv
{
  echo 'format=appimage'
  echo 'architecture=x86_64'
  echo 'compile-network=none'
  echo "version=$version"
  cat /etc/os-release
  rustc -vV
  cargo --version
  bun --version
  node --version
  dpkg-query -W
} > /out/build-manifest.txt
bash packaging/linux/source-info.sh /out
cd /out
find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
