#!/usr/bin/env bash
set -euo pipefail
format="$1"
cd "$2"
test -s build-manifest.txt
test -s LICENSE
test -s application-source.tar.gz
test -s SOURCE-NOTICE.txt
test -s source-commit.txt
grep -Ex '[0-9a-f]{40}' source-commit.txt >/dev/null
grep -qx "source-commit=$(cat source-commit.txt)" build-manifest.txt
# Only release files and dependency notices may be exported or uploaded.
while IFS= read -r -d '' entry; do
  case "${entry#./}" in
    licenses|LICENSE|SOURCE-NOTICE.txt|SHA256SUMS|build-manifest.txt|application-source.tar.gz|source-commit.txt|toolchains.json|appimage-tools.json|media-sources.json|docker-image-id.txt) ;;
    Tagrove-*.flatpak|flatpak-manifest.json|publisher.json) [[ "$format" == flatpak ]] ;;
    Tagrove-*.AppImage|runtime-check.txt|debian-sources.tsv) [[ "$format" == appimage ]] ;;
    Tagrove-*-native.tar.gz) [[ "$format" == native ]] ;;
    *) echo "Unexpected release entry: $entry" >&2; exit 1 ;;
  esac
done < <(find . -mindepth 1 -maxdepth 1 -print0)
test -d licenses
test -s licenses/dependencies/index.json
test -s SHA256SUMS
# Refuse symlinks and escaping checksum paths before sha256sum follows them.
[[ -z "$(find . -type l -print -quit)" ]]
if grep -Eq ' (\*| )?(/|.*\.\./)' SHA256SUMS; then exit 1; fi
sha256sum --check --strict SHA256SUMS >/dev/null
actual="$(mktemp)"
recorded="$(mktemp)"
trap 'rm -f "$actual" "$recorded"' EXIT
find . -type f ! -name SHA256SUMS -printf '%p\n' | sort > "$actual"
sed -E 's/^[0-9a-f]{64} [ *]//' SHA256SUMS | sort > "$recorded"
diff -u "$actual" "$recorded"
grep -qx "format=$format" build-manifest.txt
grep -qx 'architecture=x86_64' build-manifest.txt
case "$format" in
  flatpak)
    bundles=(Tagrove-*.flatpak)
    [[ ${#bundles[@]} == 1 && -s "${bundles[0]}" ]]
    test -s flatpak-manifest.json
    grep -qx 'compile-network=none' build-manifest.txt
    for license in libass/LICENSE ffmpeg/LICENSE mpv/LICENSE libplacebo/LICENSE \
      libplacebo/3rdparty/fast_float/LICENSE-MIT libplacebo/3rdparty/glad/LICENSE \
      libplacebo/3rdparty/jinja/LICENSE.txt libplacebo/3rdparty/markupsafe/LICENSE.txt; do
      test -s "licenses/$license"
    done
    ;;
  appimage)
    bundles=(Tagrove-*.AppImage)
    [[ ${#bundles[@]} == 1 && -x "${bundles[0]}" ]]
    file "${bundles[0]}" | grep -q 'ELF 64-bit.*x86-64'
    test -s runtime-check.txt
    test -s debian-sources.tsv
    test -s toolchains.json
    test -s appimage-tools.json
    for license in AppImage-runtime.txt AppImageKit.txt SOURCES.json; do
      test -s "licenses/appimage/$license"
    done
    ;;
  native)
    bundles=(Tagrove-*-native.tar.gz)
    [[ ${#bundles[@]} == 1 && -s "${bundles[0]}" ]]
    python3 - "${bundles[0]}" <<'PY'
import sys, tarfile
with tarfile.open(sys.argv[1], 'r:gz') as archive:
    files = [member for member in archive.getmembers() if member.name != '.']
    expected = {'./media_tagger', './tagrove.png', './com.example.mediatagger.desktop'}
    assert len(files) == len(expected) and {member.name for member in files} == expected
    assert all(member.isfile() for member in files)
    binary = archive.extractfile('./media_tagger').read(20)
    assert binary[:6] == b'\x7fELF\x02\x01' and binary[18:20] == b'\x3e\x00'
    assert archive.getmember('./media_tagger').mode & 0o111
    assert archive.extractfile('./tagrove.png').read(8) == b'\x89PNG\r\n\x1a\n'
    desktop = archive.extractfile('./com.example.mediatagger.desktop').read().decode()
    for field in ['[Desktop Entry]', 'Name=Tagrove', 'Exec=media_tagger', 'Icon=tagrove']:
        assert field in desktop.splitlines()
PY
    ;;
  *) exit 2 ;;
esac
