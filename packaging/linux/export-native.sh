#!/usr/bin/env bash
set -euo pipefail
mkdir -p /out/licenses /tmp/tagrove-native
version="$(python3 -c 'import json; print(json.load(open("package.json"))["version"])')"
cp src-tauri/target/release/media_tagger /tmp/tagrove-native/
cp src-tauri/icons/512x512.png /tmp/tagrove-native/tagrove.png
cp src-tauri/linux/com.example.mediatagger.desktop /tmp/tagrove-native/
cp LICENSE /out/LICENSE
python3 packaging/linux/notices.py /out/licenses/dependencies
cp -aL /usr/share/licenses /out/licenses/arch
{
  echo 'format=native'
  echo 'architecture=x86_64'
  echo "version=$version"
  rustc -vV
  cargo --version
  bun --version
  node --version
  pacman -Q
  cat /tmp/media-tagger-ldd.txt
} > /out/build-manifest.txt
tar -czf "/out/Tagrove-${version}-x86_64-native.tar.gz" -C /tmp/tagrove-native .
bash packaging/linux/source-info.sh /out
cd /out
find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
