#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd -- "$script_dir/.." && pwd)"
output_dir="$project_root/artifacts/linux"
temporary_output_dir="$(mktemp -d "$project_root/artifacts/.linux.XXXXXX")"
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
mapfile -t appimages < <(find "$temporary_output_dir" -maxdepth 1 -type f -name '*.AppImage')
if [[ ${#binaries[@]} -ne 1 || ${#appimages[@]} -ne 1 ]]; then
  echo "Expected exactly one media_tagger binary and one AppImage" >&2
  exit 1
fi
file "${binaries[0]}" | grep -q 'ELF 64-bit.*x86-64'
[[ -x "${binaries[0]}" && -x "${appimages[0]}" ]]
(
  cd "$temporary_output_dir"
  sha256sum media_tagger *.AppImage build-manifest.txt > SHA256SUMS
  sha256sum --check SHA256SUMS
)

rm -rf -- "$output_dir"
mv -- "$temporary_output_dir" "$output_dir"
temporary_output_dir=""

echo "Linux artifacts exported to: $output_dir"
