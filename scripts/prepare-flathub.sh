#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$project_root/artifacts/flathub"
exec 8>"$project_root/artifacts/.flathub-preparation.lock"
flock -n 8 || { echo 'Another Flathub preparation is running in this checkout' >&2; exit 1; }
stage="$(mktemp -d "$project_root/artifacts/.flathub.XXXXXX")"
container_id=""
cleanup() {
  if [[ -n "$container_id" ]]; then docker rm --force "$container_id" >/dev/null 2>&1 || true; fi
  if [[ -d "$stage/previous" && ! -d "$project_root/artifacts/flathub/prepared" ]]; then
    if ! mv "$stage/previous" "$project_root/artifacts/flathub/prepared"; then
      echo "Recovery files retained in $stage" >&2
      return
    fi
  fi
  rm -rf "$stage"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
docker build --platform linux/amd64 --iidfile "$stage/image-id.txt" -f "$project_root/Dockerfile.flatpak" -t tagrove-flathub-preparation:local "$project_root"
container_id="$(docker create --platform linux/amd64 --entrypoint bash "$(cat "$stage/image-id.txt")" \
  /inputs/packaging/flatpak/prepare-publication.sh)"
docker start --attach "$container_id"
[[ "$(docker inspect --format '{{.State.ExitCode}}' "$container_id")" == 0 ]]
docker cp "$container_id:/submission/." "$stage/submission"
if [[ -e "$project_root/artifacts/flathub/prepared" ]]; then
  mv "$project_root/artifacts/flathub/prepared" "$stage/previous"
fi
if ! mv "$stage/submission" "$project_root/artifacts/flathub/prepared"; then
  if [[ -d "$stage/previous" ]]; then mv "$stage/previous" "$project_root/artifacts/flathub/prepared"; fi
  exit 1
fi
echo "Publication files: $project_root/artifacts/flathub/prepared"
