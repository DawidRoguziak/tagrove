#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
format=all
commit=HEAD
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help) echo 'Usage: build-linux-docker.sh [--format all|flatpak|appimage|native] [--commit REF]'; exit 0 ;;
    --format|--commit)
      [[ $# -ge 2 ]] || { echo "Missing value for $1" >&2; exit 2; }
      if [[ "$1" == --format ]]; then format="$2"; else commit="$2"; fi
      shift 2 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done
case "$format" in all|flatpak|appimage|native) ;; *) echo "Unknown format: $format" >&2; exit 2 ;; esac

mkdir -p "$project_root/artifacts/linux"
# Serialize exports in this checkout. Builds never install or touch app profiles.
exec 9>"$project_root/artifacts/.linux-packaging.lock"
flock -n 9 || { echo 'Another Linux package build is running in this checkout' >&2; exit 1; }
temporary_output_dir="$(mktemp -d "$project_root/artifacts/.linux.XXXXXX")"
container_id=""
volume=""
publishing=()
export_complete=false
cleanup() {
  if [[ -n "$container_id" ]]; then docker rm --force "$container_id" >/dev/null 2>&1 || true; fi
  if [[ -n "$volume" ]]; then docker volume rm "$volume" >/dev/null 2>&1 || true; fi
  if [[ "$export_complete" == false ]]; then
    for ((index=${#publishing[@]}-1; index>=0; index--)); do
      selected="${publishing[index]}"
      output="$project_root/artifacts/linux/$selected"
      previous="$temporary_output_dir/previous-$selected"
      if [[ ! -d "$temporary_output_dir/$selected" && -e "$output" ]]; then
        if ! mv -- "$output" "$temporary_output_dir/$selected"; then
          echo "Rollback failed; recovery files retained in $temporary_output_dir" >&2
          return
        fi
      fi
      if [[ -e "$previous" ]]; then
        if ! mv -- "$previous" "$output"; then
          echo "Rollback failed; recovery files retained in $temporary_output_dir" >&2
          return
        fi
      fi
    done
  fi
  rm -rf -- "$temporary_output_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

build_context="$temporary_output_dir/context"
python3 "$project_root/packaging/linux/prepare-source.py" --commit "$commit" "$build_context"

formats=("$format")
if [[ "$format" == all ]]; then formats=(flatpak appimage); fi
for selected in "${formats[@]}"; do
  image_name="tagrove-linux-${selected}:local"
  stage="$temporary_output_dir/$selected"
  image_id_file="$temporary_output_dir/$selected-image-id"
  mkdir "$stage"
  if [[ "$selected" == flatpak ]]; then
    docker build --platform linux/amd64 --iidfile "$image_id_file" --file "$build_context/Dockerfile.flatpak" --tag "$image_name" "$build_context"
    image_id="$(cat "$image_id_file")"
    volume="$(docker volume create)"
    docker volume create tagrove-flatpak-downloads-x86_64 >/dev/null
    state_cache="tagrove-flatpak-state-$(printf %s "$project_root" | sha256sum | cut -c1-16)-x86_64"
    docker volume create "$state_cache" >/dev/null
    container_id="$(docker create --platform linux/amd64 --privileged \
      --mount "type=volume,src=$volume,dst=/work" \
      --mount "type=volume,src=$state_cache,dst=/work/state" \
      --mount type=volume,src=tagrove-flatpak-downloads-x86_64,dst=/work/state/downloads "$image_id" prepare)"
    docker start --attach "$container_id"
    [[ "$(docker inspect --format '{{.State.ExitCode}}' "$container_id")" == 0 ]]
    docker rm "$container_id" >/dev/null
    container_id="$(docker create --platform linux/amd64 --privileged --network none \
      --mount "type=volume,src=$volume,dst=/work" \
      --mount "type=volume,src=$state_cache,dst=/work/state" \
      --mount type=volume,src=tagrove-flatpak-downloads-x86_64,dst=/work/state/downloads "$image_id" build)"
    docker start --attach "$container_id"
    [[ "$(docker inspect --format '{{.State.ExitCode}}' "$container_id")" == 0 ]]
  elif [[ "$selected" == appimage ]]; then
    docker build --platform linux/amd64 --iidfile "$image_id_file" --file "$build_context/Dockerfile.linux" --tag "$image_name" "$build_context"
    image_id="$(cat "$image_id_file")"
    container_id="$(docker create --platform linux/amd64 --network none "$image_id")"
    docker start --attach "$container_id"
    [[ "$(docker inspect --format '{{.State.ExitCode}}' "$container_id")" == 0 ]]
  else
    docker build --platform linux/amd64 --iidfile "$image_id_file" --file "$build_context/Dockerfile.native" --target artifacts --tag "$image_name" "$build_context"
    image_id="$(cat "$image_id_file")"
    container_id="$(docker create --platform linux/amd64 "$image_id" /out/media_tagger)"
  fi
  docker cp "$container_id:/out/." "$stage"
  docker rm "$container_id" >/dev/null
  container_id=""
  if [[ -n "$volume" ]]; then docker volume rm "$volume" >/dev/null; volume=""; fi
  bash "$project_root/packaging/linux/validate-export.sh" "$selected" "$stage"
  cmp "$build_context/.release-source/application-source.tar.gz" "$stage/application-source.tar.gz"
  cmp "$build_context/.release-source/source-commit.txt" "$stage/source-commit.txt"
  [[ ! -e "$stage/docker-image-id.txt" ]]
  printf '%s\n' "$image_id" > "$stage/docker-image-id.txt"
  (cd "$stage" && sha256sum ./docker-image-id.txt >> SHA256SUMS)
done

# All requested formats must validate before any previous output is replaced.
# Keep all previous directories until every rename succeeds. The exit trap
# rolls back the entire requested set on an error or handled signal.
for selected in "${formats[@]}"; do
  publishing+=("$selected")
  output="$project_root/artifacts/linux/$selected"
  previous="$temporary_output_dir/previous-$selected"
  if [[ -e "$output" ]]; then mv -- "$output" "$previous"; fi
  mv -- "$temporary_output_dir/$selected" "$output"
done
export_complete=true
for selected in "${formats[@]}"; do
  echo "Exported $selected to $project_root/artifacts/linux/$selected"
done
