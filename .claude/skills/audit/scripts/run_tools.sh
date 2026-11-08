#!/bin/sh
# Static-stage tool runner for /audit. Runs every check serially and records raw results.
#   sh .claude/skills/audit/scripts/run_tools.sh docs/review/<run>
# Writes <run>/tools/<name>.log and <run>/tools/raw-results.json. The clerk turns failures into
# `audit_state.py put tools` (file/line findings); a missing optional tool is `unavailable`.
# Read-only on sources; never starts an app session or a desktop E2E build.
set -u

run_dir=${1:?usage: run_tools.sh docs/review/<run>}
case "$run_dir" in
  docs/review/*) ;;
  *) echo "run directory must be docs/review/<run>" >&2; exit 2 ;;
esac
[ -f "$run_dir/run.json" ] || { echo "missing $run_dir/run.json" >&2; exit 2; }
[ -f .git/HEAD ] && [ -d src-tauri ] || { echo "run from the repository root" >&2; exit 2; }
if [ -e artifacts/app-control/active.lock ]; then
  echo "an app:control session owns the checkout; run the tools later" >&2
  exit 2
fi

tools="$run_dir/tools"
mkdir -p "$tools"
results="$tools/raw-results.jsonl"
: > "$results"

record() {
  # name status exit_code seconds
  printf '{"name":"%s","status":"%s","exit_code":%s,"seconds":%s,"log":"tools/%s.log"}\n' \
    "$1" "$2" "$3" "$4" "$1" >> "$results"
}

check() {
  name=$1
  limit=$2
  shift 2
  start=$(date +%s)
  timeout --kill-after=30 "$limit" "$@" > "$tools/$name.log" 2>&1
  code=$?
  seconds=$(( $(date +%s) - start ))
  if [ "$code" -eq 0 ]; then status=pass
  elif [ "$code" -eq 124 ] || [ "$code" -eq 137 ]; then status=timeout
  else status=fail
  fi
  record "$name" "$status" "$code" "$seconds"
  echo "$name: $status ($code, ${seconds}s)"
}

unavailable() {
  printf '%s\n' "$2" > "$tools/$1.log"
  record "$1" unavailable null 0
  echo "$1: unavailable"
}

check typecheck 900 bun run typecheck
check lint 600 bun run lint
check format 600 bun run format:check
check rust-fmt 600 bun run rust:fmt
check clippy 3600 bun run rust:clippy
check clippy-extra 3600 cargo clippy --manifest-path src-tauri/Cargo.toml --lib --bins --locked \
  --message-format=json -- -W clippy::unwrap_used -W clippy::expect_used \
  -W clippy::indexing_slicing -W clippy::await_holding_lock
if command -v cargo-audit > /dev/null 2>&1; then
  check audit-rust 900 bun run audit:rust -- --no-fetch
else
  unavailable audit-rust "cargo-audit is not installed"
fi
check locale-check 600 bun run locale:check
check locale-tools 600 bun run test:locale-tools
check vitest 3600 bun run test
check backend 5400 bun run test:backend
check app-control 600 bun run test:app-control
check e2e-build 600 bun run test:e2e-build
if command -v gitleaks > /dev/null 2>&1; then
  check privacy 900 bun run privacy:check
else
  unavailable privacy "gitleaks is not installed; privacy:check cannot run"
fi

python3 - "$results" "$tools/raw-results.json" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1], encoding="utf-8") if line.strip()]
json.dump({"results": rows}, open(sys.argv[2], "w", encoding="utf-8"), indent=2)
PY
echo "raw results: $tools/raw-results.json"
