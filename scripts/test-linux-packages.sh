#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
[[ "$#" == 2 || "$#" == 4 ]] && [[ "$1" == --format ]] || { echo 'Usage: test-linux-packages.sh --format flatpak|appimage [--distribution debian12|ubuntu24|arch]' >&2; exit 2; }
format="$2"
case "$format" in flatpak|appimage) ;; *) exit 2 ;; esac
distribution=debian12
if [[ "$#" == 4 ]]; then
  [[ "$format" == appimage && "$3" == --distribution ]] || exit 2
  distribution="$4"
fi
case "$distribution" in debian12|ubuntu24|arch) ;; *) exit 2 ;; esac
bundle_dir="$project_root/artifacts/linux/$format"
bash "$project_root/packaging/linux/validate-export.sh" "$format" "$bundle_dir"
mkdir -p "$project_root/artifacts/package-tests"
evidence="$(mktemp -d "$project_root/artifacts/package-tests/$format-$distribution.XXXXXX")"
container_id=""
cleanup() {
  if [[ -n "$container_id" ]]; then
    docker cp "$container_id:/evidence/." "$evidence/" >/dev/null 2>&1 || true
    docker rm --force "$container_id" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT
if [[ "$format" == flatpak ]]; then
  docker build --platform linux/amd64 --iidfile "$evidence/image-id.txt" --file "$project_root/packaging/tests/Dockerfile.flatpak-smoke" --tag tagrove-flatpak-tests:local "$project_root"
  container_id="$(docker create --platform linux/amd64 --privileged \
    --mount "type=bind,src=$bundle_dir,dst=/bundle,readonly" \
    --entrypoint dbus-run-session "$(cat "$evidence/image-id.txt")" -- bash /inputs/packaging/tests/flatpak-lifecycle.sh)"
else
  dockerfile="$project_root/packaging/tests/Dockerfile.appimage-smoke"
  build_arguments=()
  case "$distribution" in
    ubuntu24) build_arguments=(--build-arg BASE=ubuntu:24.04) ;;
    arch) dockerfile="$project_root/packaging/tests/Dockerfile.appimage-arch" ;;
  esac
  docker build --platform linux/amd64 --iidfile "$evidence/image-id.txt" --file "$dockerfile" "${build_arguments[@]}" \
    --tag "tagrove-appimage-tests:$distribution" "$project_root"
  container_id="$(docker create --platform linux/amd64 --network none \
    --mount "type=bind,src=$bundle_dir,dst=/bundle,readonly" "$(cat "$evidence/image-id.txt")")"
fi
docker start --attach "$container_id" 2>&1 | tee "$evidence/container.log"
[[ "$(docker inspect --format '{{.State.ExitCode}}' "$container_id")" == 0 ]]
echo "Package test evidence: $evidence"
