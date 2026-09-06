#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd -- "$script_dir/.." && pwd)"
artifacts_dir="$project_root/artifacts"
output_dir="$artifacts_dir/linux"
mkdir -p -- "$artifacts_dir"
temporary_output_dir="$(mktemp -d "$artifacts_dir/.linux.XXXXXX")"
image_name="media-tagger-linux-artifacts:local"
container_id=""

cleanup() {
  if [[ -n "$container_id" ]]; then
    docker rm --force "$container_id" >/dev/null 2>&1 || true
  fi
  rm -rf -- "$temporary_output_dir"
}

trap cleanup EXIT

docker build \
  --platform linux/amd64 \
  --file "$project_root/Dockerfile.linux" \
  --target artifacts \
  --tag "$image_name" \
  "$project_root"

# The artifacts image is based on scratch and intentionally has no default
# command. docker create still needs one, although this container is never run.
container_id="$(docker create --platform linux/amd64 "$image_name" /out/media_tagger)"

case "$output_dir" in
  "$project_root/artifacts/linux") ;;
  *)
    echo "Refusing to replace unexpected artifact directory: $output_dir" >&2
    exit 1
    ;;
esac

docker cp "$container_id:/out/." "$temporary_output_dir"

mapfile -t binaries < <(find "$temporary_output_dir" -maxdepth 1 -type f -name media_tagger)
mapfile -t manifests < <(find "$temporary_output_dir" -maxdepth 1 -type f -name build-manifest.txt)
mapfile -t icons < <(find "$temporary_output_dir" -maxdepth 1 -type f -name tagrove.png)
mapfile -t launchers < <(find "$temporary_output_dir" -maxdepth 1 -type f -name com.example.mediatagger.desktop)
mapfile -t exported_files < <(find "$temporary_output_dir" -maxdepth 1 -type f)
if [[ ${#binaries[@]} -ne 1 || ${#manifests[@]} -ne 1 || ${#icons[@]} -ne 1 || ${#launchers[@]} -ne 1 || ${#exported_files[@]} -ne 4 ]]; then
  echo "Expected one media_tagger binary, one Tagrove icon, one desktop launcher, and one build manifest" >&2
  exit 1
fi
file "${binaries[0]}" | grep -q 'ELF 64-bit.*x86-64'
file "${icons[0]}" | grep -q 'PNG image data, 512 x 512'
[[ -x "${binaries[0]}" && -s "${manifests[0]}" && -s "${icons[0]}" ]]
grep -qx 'Name=Tagrove' "${launchers[0]}"
grep -qx 'Icon=tagrove' "${launchers[0]}"
grep -qx 'Exec=media_tagger' "${launchers[0]}"
for required_key in gtk webkitgtk mpv egl gl glx; do
  grep -q "^${required_key}=" "${manifests[0]}"
done
(
  cd "$temporary_output_dir"
  sha256sum media_tagger tagrove.png com.example.mediatagger.desktop build-manifest.txt > SHA256SUMS
  sha256sum --check SHA256SUMS
)

rm -rf -- "$output_dir"
mv -- "$temporary_output_dir" "$output_dir"
temporary_output_dir=""

echo "Linux artifacts exported to: $output_dir"
