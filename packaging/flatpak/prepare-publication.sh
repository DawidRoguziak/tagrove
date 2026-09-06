#!/usr/bin/env bash
set -euo pipefail
python3 /inputs/packaging/flatpak/verify-release.py /inputs
app_id="$(python3 -c 'import json; print(json.load(open("/inputs/packaging/flatpak/publisher.json"))["appId"])')"
mkdir /submission
python3 /inputs/packaging/flatpak/generate.py /inputs "/submission/$app_id.json" \
  --publication --metadata-output /metainfo.xml
printf '{\n    "only-arches": ["x86_64"]\n}\n' > /submission/flathub.json
appstreamcli validate --no-net --pedantic /metainfo.xml
flatpak-builder-lint manifest "/submission/$app_id.json"
