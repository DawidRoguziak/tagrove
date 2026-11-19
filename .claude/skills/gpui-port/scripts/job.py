#!/usr/bin/env python3
"""Run a long gpui-port job detached and wait for it in bounded steps (one agent command stops at 10 min).

  job.py start EPIC checks                     The epic's check commands from epics.json, all of them, in order.
  job.py start EPIC test-all                   `bun run test:all` on a private Xvfb/dbus session with scratch XDG dirs.
  job.py start EPIC compare --scenario ID...   `bun run compare:ui ID...` (exists after epic E01).
  job.py wait  EPIC JOB [--timeout 540]        {"status": "running"|"exited", "code", "steps", "failed", "tail"}.
  job.py tail  EPIC JOB [-n 80]                Last lines of the job log.

Only these three jobs exist and their commands come from the manifest, never from the caller.
State and logs live in the ignored docs/review/gpui-port/<EPIC>/jobs/. Jobs that need a display
(test-all, compare) run one at a time and never while an app:control session holds its lock.
"""
from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import port_state as port  # noqa: E402

JOBS = ("checks", "test-all", "compare")
DISPLAY_JOBS = ("test-all", "compare")
APP_CONTROL_LOCK = "artifacts/app-control/active.lock"
XVFB_SCREEN = "-screen 0 1440x1100x24 -nolisten tcp"
TEST_ALL = 'export MEDIATAGGER_NATIVE_DISPLAY="$DISPLAY"; exec bun run test:all'


def alive(pid: int) -> bool:
    try:
        with open(f"/proc/{pid}/stat", encoding="utf-8") as handle:
            return handle.read().rsplit(")", 1)[1].split()[0] != "Z"
    except OSError:
        return False


def job_dir(root: Path, epic_id: str) -> Path:
    return port.run_dir(root, epic_id) / "jobs"


def steps_for(root: Path, epic_id: str, job: str, scenarios: list[str]) -> list[list[str]]:
    manifest = port.load_manifest(root)
    epic = port.find_epic(manifest, epic_id)
    if job == "checks":
        commands = port.resolve_checks(manifest, epic)
        for command in commands:
            problem = port.command_problem(command)
            if problem:
                raise port.PortError(f"check '{command}' is not allowed: {problem}")
        return [shlex.split(command) for command in commands]
    if job == "test-all":
        return [["dbus-run-session", "--", "xvfb-run", "-a", "-s", XVFB_SCREEN, "sh", "-c", TEST_ALL]]
    package = port.read_json(root / "package.json")
    if "compare:ui" not in package.get("scripts", {}):
        raise port.PortError("compare:ui does not exist yet; epic E01 builds the comparison harness")
    if not scenarios:
        raise port.PortError("compare needs at least one --scenario")
    for scenario in scenarios:
        if scenario not in manifest["scenarios"]:
            raise port.PortError(f"unknown scenario {scenario}")
    return [["bun", "run", "compare:ui", *scenarios]]


def display_lock(root: Path) -> Path:
    return port.run_dir(root, "_display") / "display.lock"


def display_busy(root: Path) -> str | None:
    if (root / APP_CONTROL_LOCK).exists():
        return f"{APP_CONTROL_LOCK} exists: an app:control session is active; stop it first"
    lock = display_lock(root)
    if lock.exists():
        holder = port.read_json(lock)
        if alive(holder.get("pid", -1)):
            return f"display job {holder.get('job')} of {holder.get('epic')} is still running"
    return None


def start(root: Path, epic_id: str, job: str, scenarios: list[str]) -> dict:
    steps_for(root, epic_id, job, scenarios)  # validate before anything is spawned
    directory = job_dir(root, epic_id)
    state_path = directory / f"{job}.json"
    if state_path.exists():
        state = port.read_json(state_path)
        if state.get("code") is None and alive(state["pid"]):
            return {"ok": True, "status": "running", "pid": state["pid"], "note": "already running"}
    if job in DISPLAY_JOBS:
        busy = display_busy(root)
        if busy:
            return {"ok": False, "status": "busy", "error": busy}
    directory.mkdir(parents=True, exist_ok=True)
    for suffix in ("code", "steps.json"):
        (directory / f"{job}.{suffix}").unlink(missing_ok=True)
    log = open(directory / f"{job}.log", "w", encoding="utf-8")
    wrapper = [sys.executable, "-I", str(Path(__file__).resolve()), "_child", epic_id, job,
               *[item for scenario in scenarios for item in ("--scenario", scenario)]]
    process = subprocess.Popen(wrapper, cwd=root, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                               start_new_session=True)
    log.close()  # the detached child keeps its own handle
    port.write_json(state_path, {"job": job, "epic": epic_id, "pid": process.pid, "started": time.time(), "code": None})
    return {"ok": True, "status": "running", "pid": process.pid, "log": (directory / f"{job}.log").relative_to(root).as_posix()}


def private_session() -> tuple[dict, Path]:
    """Environment of the documented headless recipe (docs/development/testing.md)."""
    scratch = Path(tempfile.mkdtemp(prefix="mediatagger-gpui-port-"))
    env = dict(os.environ)
    for name, variable in (("data", "XDG_DATA_HOME"), ("config", "XDG_CONFIG_HOME"), ("cache", "XDG_CACHE_HOME"),
                           ("runtime", "XDG_RUNTIME_DIR")):
        (scratch / name).mkdir()
        env[variable] = str(scratch / name)
    os.chmod(scratch / "runtime", 0o700)
    env.pop("WAYLAND_DISPLAY", None)
    env.update(GDK_BACKEND="x11", LANG="C.UTF-8", LC_ALL="C.UTF-8", DISABLE_MANGOHUD="1")
    return env, scratch


def remove_session(scratch: Path) -> None:
    for name in ("doc", "gvfs"):
        mount = scratch / "runtime" / name
        if os.path.ismount(mount):
            subprocess.run(["fusermount", "-u", str(mount)], check=False)
    shutil.rmtree(scratch, ignore_errors=True)


def child(root: Path, epic_id: str, job: str, scenarios: list[str]) -> int:
    directory = job_dir(root, epic_id)
    steps = steps_for(root, epic_id, job, scenarios)
    env, scratch, lock = None, None, None
    if job in DISPLAY_JOBS:
        lock = display_lock(root)
        port.write_json(lock, {"pid": os.getpid(), "epic": epic_id, "job": job})
    if job == "test-all":
        env, scratch = private_session()
    results, worst = [], 0
    try:
        for argv in steps:
            print(f"\n===== {shlex.join(argv)} =====", flush=True)
            began = time.time()
            code = subprocess.run(argv, cwd=root, env=env).returncode
            results.append({"command": shlex.join(argv), "code": code, "seconds": round(time.time() - began)})
            port.write_json(directory / f"{job}.steps.json", results)
            print(f"===== exit {code} =====", flush=True)
            worst = worst or code
    finally:
        if scratch is not None:
            remove_session(scratch)
        if lock is not None:
            lock.unlink(missing_ok=True)
        (directory / f"{job}.code").write_text(str(0 if worst == 0 else 1))
    return 0 if worst == 0 else 1


def tail(root: Path, epic_id: str, job: str, lines: int) -> str:
    log = job_dir(root, epic_id) / f"{job}.log"
    if not log.exists():
        return ""
    return "\n".join(log.read_text(encoding="utf-8", errors="replace").splitlines()[-lines:])


def wait(root: Path, epic_id: str, job: str, timeout: int) -> dict:
    directory = job_dir(root, epic_id)
    state_path = directory / f"{job}.json"
    if not state_path.exists():
        raise port.PortError(f"{job} was never started for {epic_id}")
    state = port.read_json(state_path)
    code_path = directory / f"{job}.code"
    steps_path = directory / f"{job}.steps.json"
    log = (directory / f"{job}.log").relative_to(root).as_posix()
    deadline = time.time() + max(1, min(timeout, 540))
    while True:
        steps = port.read_json(steps_path) if steps_path.exists() else []
        if code_path.exists():
            code = int(code_path.read_text().strip() or "1")
            state["code"] = code
            port.write_json(state_path, state)
            failed = [step["command"] for step in steps if step["code"] != 0]
            return {"ok": code == 0, "status": "exited", "code": code, "steps": steps, "failed": failed, "log": log,
                    "tail": tail(root, epic_id, job, 40) if code else ""}
        if not alive(state["pid"]):
            return {"ok": False, "status": "exited", "code": None, "steps": steps, "log": log,
                    "note": "runner died without an exit code", "tail": tail(root, epic_id, job, 40)}
        if time.time() >= deadline:
            return {"ok": True, "status": "running", "steps": steps, "log": log, "hint": "call wait again"}
        time.sleep(5)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("start", "_child"):
        item = sub.add_parser(name)
        item.add_argument("epic")
        item.add_argument("job", choices=JOBS)
        item.add_argument("--scenario", action="append", default=[])
    wait_parser = sub.add_parser("wait")
    wait_parser.add_argument("epic")
    wait_parser.add_argument("job", choices=JOBS)
    wait_parser.add_argument("--timeout", type=int, default=540)
    tail_parser = sub.add_parser("tail")
    tail_parser.add_argument("epic")
    tail_parser.add_argument("job", choices=JOBS)
    tail_parser.add_argument("-n", type=int, default=80)
    options = parser.parse_args(argv)
    # The detached child is started with the repository root as its working directory.
    root = Path.cwd() if options.command == "_child" else port.repo_root()
    try:
        if options.command == "_child":
            return child(root, options.epic, options.job, options.scenario)
        if options.command == "tail":
            print(tail(root, options.epic, options.job, options.n))
            return 0
        if options.command == "start":
            result = start(root, options.epic, options.job, options.scenario)
        else:
            result = wait(root, options.epic, options.job, options.timeout)
    except (port.PortError, OSError, json.JSONDecodeError) as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
