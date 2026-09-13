#!/usr/bin/env bash
set -euo pipefail
cd /work
case "$1" in
  prepare)
    cp -a /inputs source
    python3 source/packaging/flatpak/generate.py source manifest.json
    flatpak-builder --arch=x86_64 --download-only --state-dir=/work/state build manifest.json
    ;;
  build)
    # This entire container runs with Docker --network none. Compilation uses
    # only sources downloaded in the preceding container and its GNOME SDK.
    flatpak-builder --arch=x86_64 --disable-download --disable-rofiles-fuse --force-clean \
      --state-dir=/work/state --repo=/work/repo build manifest.json
    mkdir /out
    app_id="$(python3 -c 'import json; print(json.load(open("manifest.json"))["app-id"])')"
    version="$(python3 -c 'import json; print(json.load(open("source/package.json"))["version"])')"
    flatpak build-bundle --arch=x86_64 --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo \
      repo "/out/Tagrove-${version}-x86_64.flatpak" "$app_id" stable
    ostree init --repo=/work/validation-repo --mode=archive-z2
    flatpak build-import-bundle /work/validation-repo /out/*.flatpak
    ostree --repo=/work/validation-repo fsck
    cp manifest.json /out/flatpak-manifest.json
    python3 - <<'PYTHON'
import json
from pathlib import Path
manifest = json.loads(Path("manifest.json").read_text())
publisher = next(source["contents"] for source in manifest["modules"][-1]["sources"]
                 if source.get("dest-filename") == "publisher.json")
Path("/out/publisher.json").write_text(publisher + "\n")
PYTHON
    cp source/LICENSE /out/LICENSE
    cp -a build/files/share/licenses /out/licenses
    {
      echo 'format=flatpak'
      echo 'architecture=x86_64'
      echo 'compile-network=none'
      echo "app-id=$app_id"
      echo "version=$version"
      flatpak-builder --version
      flatpak list --columns=application,arch,branch,active
      cat build/metadata
    } > /out/build-manifest.txt
    cd source
    bash packaging/linux/source-info.sh /out
    cd /out
    find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
    ;;
  *) exit 2 ;;
esac
