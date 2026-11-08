#!/usr/bin/env python3
"""Run every desktop E2E spec once, serially, each on a fresh private display and XDG root.

  run_e2e_sweep.py --run <dir> [--list] [--only a,b] [--perf] [--timeout 3600]

Items: every spec in e2e/specs with its opt-in flag from the testing.md table (plus
MEDIATAGGER_PERF=1 for grid-baseline), an extra reduced-motion pass for the specs that read
MEDIATAGGER_REDUCED_MOTION, the gallery-scale harness (default, GALLERY_MILLION,
GALLERY_PERFORMANCE), and with --perf the ignored grid_baseline and tag_save_benchmark Rust
diagnostics. Results: <run>/qa/sweep/<item>.json and summary.json; logs next to them.
Refuses to start while artifacts/app-control/active.lock exists (shared dist/, target-e2e and
ports). Temporary XDG roots live in /tmp/mediatagger-sweep-*; FUSE mounts are unmounted and the
root removed after every item. Never touches the audit dataset.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402

TABLE_ROW = re.compile(r"^\|\s*(?P<specs>`[^|]+)\|\s*`(?P<flag>MEDIATAGGER_[A-Z0-9_]+)`")
EXTRA_FLAGS = {"grid-baseline": {"MEDIATAGGER_PERF": "1"}}
XVFB = "-screen 0 1440x1100x24 -nolisten tcp"


def spec_flags(testing_md: str) -> dict[str, str]:
    flags = {}
    for line in testing_md.splitlines():
        match = TABLE_ROW.match(line)
        if match:
            for spec in re.findall(r"`([a-z0-9.-]+)`", match.group("specs")):
                flags[spec] = match.group("flag")
    return flags


def items(repo: Path, perf: bool) -> list[dict]:
    flags = spec_flags((repo / "docs" / "development" / "testing.md").read_text(encoding="utf-8"))
    result = []
    for spec_path in sorted((repo / "e2e" / "specs").glob("*.e2e.js")):
        spec = spec_path.name.removesuffix(".e2e.js")
        env = {flags[spec]: "1"} if spec in flags else {}
        env.update(EXTRA_FLAGS.get(spec, {}))
        result.append({"item": spec, "kind": "spec", "spec": f"e2e/specs/{spec_path.name}", "env": env})
        if "MEDIATAGGER_REDUCED_MOTION" in spec_path.read_text(encoding="utf-8"):
            result.append({"item": f"{spec}+reduced-motion", "kind": "spec", "spec": f"e2e/specs/{spec_path.name}",
                           "env": {**env, "MEDIATAGGER_REDUCED_MOTION": "1"}})
    for mode in ("default", "GALLERY_MILLION", "GALLERY_PERFORMANCE"):
        result.append({"item": f"gallery-scale-{mode.lower().replace('_', '-')}", "kind": "gallery-scale",
                       "env": {} if mode == "default" else {mode: "1"}})
    if perf:
        result.append({"item": "rust-grid-baseline", "kind": "cargo",
                       "argv": ["cargo", "test", "--manifest-path", "src-tauri/Cargo.toml", "--locked", "--release",
                                "--test", "grid_baseline", "--", "--ignored", "--nocapture", "--test-threads=1"],
                       "env": {"TMPDIR": "/var/tmp"}})
        result.append({"item": "rust-tag-save-benchmark", "kind": "cargo",
                       "argv": ["cargo", "test", "--manifest-path", "src-tauri/Cargo.toml", "--locked",
                                "--test", "tag_save_benchmark", "--", "--ignored", "--nocapture", "--test-threads=1"],
                       "env": {}})
    return result


def command_for(item: dict) -> list[str]:
    if item["kind"] == "cargo":
        return item["argv"]
    if item["kind"] == "gallery-scale":
        return ["sh", "e2e/gallery-scale/run.sh"]
    inner = ('export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; '
             f"exec bun run test:e2e:tauri --spec {item['spec']}")
    return ["dbus-run-session", "--", "xvfb-run", "-a", "-s", XVFB, "sh", "-c", inner]


def private_env(root: Path, extra: dict) -> dict:
    env = {k: v for k, v in os.environ.items()
           if k not in ("WAYLAND_DISPLAY", "DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS")
           and not k.startswith("MEDIATAGGER_") and k != "VITE_MEDIATAGGER_PERF"}
    for name in ("data", "config", "cache", "runtime"):
        (root / name).mkdir(mode=0o700)
    env.update(XDG_DATA_HOME=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"),
               XDG_CACHE_HOME=str(root / "cache"), XDG_RUNTIME_DIR=str(root / "runtime"),
               GDK_BACKEND="x11", NO_AT_BRIDGE="1")
    env.update(extra)
    return env


def unmount_fuse(root: Path) -> None:
    for mount in (root / "runtime" / "doc", root / "runtime" / "gvfs"):
        if os.path.ismount(mount):
            subprocess.run(["fusermount", "-u", str(mount)], check=False)


def run_item(repo: Path, out: Path, item: dict, timeout: int) -> dict:
    if (repo / "artifacts" / "app-control" / "active.lock").exists():
        return {**item, "status": "blocked", "summary": "app:control active.lock present"}
    root = Path(tempfile.mkdtemp(prefix="mediatagger-sweep-", dir="/tmp"))
    log = out / f"{item['item']}.log"
    started = time.time()
    try:
        with open(log, "w", encoding="utf-8") as handle:
            try:
                code = subprocess.run(command_for(item), cwd=repo, env=private_env(root, item.get("env", {})),
                                      stdout=handle, stderr=subprocess.STDOUT, timeout=timeout,
                                      start_new_session=True).returncode
                status = "passed" if code == 0 else "failed"
            except subprocess.TimeoutExpired:
                code, status = None, "timeout"
    finally:
        unmount_fuse(root)
        shutil.rmtree(root, ignore_errors=True)
    text = log.read_text(encoding="utf-8", errors="replace")
    if status == "passed" and re.search(r"\b0 passing\b", text) and re.search(r"\b\d+ pending\b", text):
        status = "skipped"
    return {**item, "status": status, "exit_code": code, "seconds": round(time.time() - started, 1),
            "command": " ".join(command_for(item)), "log": f"qa/sweep/{log.name}",
            "summary": "\n".join(text.splitlines()[-25:])[-3000:]}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run")
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--only")
    parser.add_argument("--perf", action="store_true")
    parser.add_argument("--timeout", type=int, default=3600)
    args = parser.parse_args(argv)
    try:
        repo = lib.repo_root()
        run_dir = lib.resolve_run(args.run, repo)
        planned = items(repo, args.perf)
        if args.only:
            wanted = set(args.only.split(","))
            planned = [item for item in planned if item["item"] in wanted]
        if args.list:
            print(json.dumps(planned, indent=2))
            return 0
        lib.require_not_halted(lib.load_run(run_dir))
        out = run_dir / "qa" / "sweep"
        out.mkdir(parents=True, exist_ok=True)
        results = []
        for item in planned:
            target = out / f"{item['item']}.json"
            if target.exists():
                results.append(lib.read_json(target))
                continue
            result = run_item(repo, out, item, args.timeout)
            if result["status"] != "blocked":
                lib.write_json(target, result)
            results.append(result)
            print(f"{item['item']}: {result['status']}", flush=True)
        if not args.only and all(r["status"] != "blocked" for r in results):
            counts: dict[str, int] = {}
            for result in results:
                counts[result["status"]] = counts.get(result["status"], 0) + 1
            lib.write_json(out / "summary.json", {"generated": lib.now(), "counts": counts,
                                                  "items": [{k: r[k] for k in ("item", "status")} for r in results]})
    except lib.AuditError as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
