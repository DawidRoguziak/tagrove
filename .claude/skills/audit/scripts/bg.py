#!/usr/bin/env python3
"""Run a long audit job detached and wait for it in bounded steps (the Bash tool stops at 10 min).

  bg.py --run R start tools|sweep [--perf]   Start run_tools.sh or run_e2e_sweep.py in a new session.
  bg.py --run R wait tools|sweep [--timeout 540]
                                             Return {"status": "running"|"exited", "code": N}.
Only these two jobs exist; nothing else can be started through this script. State lives in
<run>/bg/<job>.json, output in <run>/bg/<job>.log.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_lib as lib  # noqa: E402

HERE = Path(__file__).resolve().parent
JOBS = ("tools", "sweep")


def job_argv(job: str, run_rel: str, perf: bool) -> list[str]:
    if job == "tools":
        return ["sh", str(HERE / "run_tools.sh"), run_rel]
    argv = [sys.executable, "-I", str(HERE / "run_e2e_sweep.py"), "--run", run_rel]
    return [*argv, "--perf"] if perf else argv


def alive(pid: int) -> bool:
    try:
        with open(f"/proc/{pid}/stat", encoding="utf-8") as handle:
            return handle.read().rsplit(")", 1)[1].split()[0] != "Z"
    except OSError:
        return False


def start(repo: Path, run_dir: Path, job: str, perf: bool) -> dict:
    lib.require_not_halted(lib.load_run(run_dir))
    state_path = run_dir / "bg" / f"{job}.json"
    if state_path.exists():
        state = lib.read_json(state_path)
        if state.get("code") is None and alive(state["pid"]):
            return {"ok": True, "status": "running", "pid": state["pid"], "note": "already running"}
    state_path.parent.mkdir(exist_ok=True)
    code_path = run_dir / "bg" / f"{job}.code"
    code_path.unlink(missing_ok=True)
    log = open(run_dir / "bg" / f"{job}.log", "w", encoding="utf-8")
    argv = job_argv(job, run_dir.relative_to(repo).as_posix(), perf)
    wrapper = [sys.executable, "-I", str(HERE / "bg.py"), "--run", str(run_dir), "_child", job, "--", *argv]
    process = subprocess.Popen(wrapper, cwd=repo, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                               start_new_session=True)
    lib.write_json(state_path, {"job": job, "pid": process.pid, "argv": argv, "started": lib.now(), "code": None})
    return {"ok": True, "status": "running", "pid": process.pid}


def child(repo: Path, run_dir: Path, job: str, argv: list[str]) -> int:
    code = subprocess.run(argv, cwd=repo).returncode
    (run_dir / "bg" / f"{job}.code").write_text(str(code))
    return code


def wait(run_dir: Path, job: str, timeout: int) -> dict:
    state_path = run_dir / "bg" / f"{job}.json"
    if not state_path.exists():
        raise lib.AuditError(f"{job} was never started")
    state = lib.read_json(state_path)
    code_path = run_dir / "bg" / f"{job}.code"
    deadline = time.time() + max(1, min(timeout, 540))
    while True:
        if code_path.exists():
            code = int(code_path.read_text().strip() or "1")
            state["code"] = code
            lib.write_json(state_path, state)
            return {"ok": code == 0, "status": "exited", "code": code}
        if not alive(state["pid"]):
            return {"ok": False, "status": "exited", "code": None, "note": "runner died without an exit code"}
        if time.time() >= deadline:
            return {"ok": True, "status": "running", "hint": "call wait again"}
        time.sleep(5)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--run")
    sub = parser.add_subparsers(dest="command", required=True)
    start_parser = sub.add_parser("start")
    start_parser.add_argument("job", choices=JOBS)
    start_parser.add_argument("--perf", action="store_true")
    wait_parser = sub.add_parser("wait")
    wait_parser.add_argument("job", choices=JOBS)
    wait_parser.add_argument("--timeout", type=int, default=540)
    child_parser = sub.add_parser("_child")
    child_parser.add_argument("job", choices=JOBS)
    child_parser.add_argument("argv", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    try:
        repo = lib.repo_root(HERE)
        run_dir = lib.resolve_run(args.run, repo)
        if args.command == "_child":
            command = args.argv[1:] if args.argv[:1] == ["--"] else args.argv
            if command not in (job_argv(args.job, run_dir.relative_to(repo).as_posix(), False),
                               job_argv(args.job, run_dir.relative_to(repo).as_posix(), True)):
                raise lib.AuditError("refusing an unexpected command")
            return child(repo, run_dir, args.job, command)
        result = start(repo, run_dir, args.job, args.perf) if args.command == "start" else wait(run_dir, args.job, args.timeout)
    except (lib.AuditError, OSError) as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        return 2
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
